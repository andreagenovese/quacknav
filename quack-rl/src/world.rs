//! A house as the simulator holds it: axis-aligned boxes (walls and
//! furniture), posts (table and chair legs), holes (stairwells), and things
//! that move (pets, feet). Each static thing says whether the map has it —
//! the map is drawn from the world, less what was put down since, plus what
//! has gone since — and how tall it is (what the depth sensor's floor
//! threshold loses while walking).
//!
//! The depth sensor is the paper twin's: eight columns across 0.78 rad
//! around the head's yaw, a ray each for what stands in the way, and eight
//! floor rows per column looking for the floor at 0.25..2.0 m — with the
//! noise, dropouts and phantoms of [`Calib`].

use std::time::Instant;

use quack_nav::cliff::{CliffFrame, Drop, DropKind, Obstacle};
use quack_nav::map::{Cell, Grid};
use serde::{Deserialize, Serialize};

use crate::calib::Calib;
use crate::{Rng, dist};

pub const COLS: usize = 8;
pub const COL_FOV: f64 = 0.78;
/// Floor distances the eight rows look at, metres ahead (head 0.25 m up),
/// when the calibration sets no `tof_pitch_rad`.
pub const ROW_FLOOR_M: [f64; 8] = [0.25, 0.32, 0.42, 0.55, 0.75, 1.0, 1.4, 2.0];
/// With one: the rows evenly across the sensor's field (kinematics'
/// `FOV_DEG`, the zones' centres half a zone in from its edges) ...
pub const ROWS: usize = 8;
pub const ROW_FOV: f64 = 0.785;
/// ... and a return this near the floor, in the sensor's height, is floor.
const TOF_FLOOR_SAFETY: f64 = 0.85;
/// The sensor's height above the floor (the rows' floor distances assume it).
pub const HEAD_H: f64 = 0.25;
/// The map's cells, as the duck's.
pub const CELL: f64 = 0.05;
/// A low box's floor rows read a drop up to this far past its face.
const PHANTOM_DEPTH_M: f64 = 0.5;
/// Kinematics' `Reprojector::MIN_RANGE_M`: a return nearer than this,
/// measured from the sensor, is the sensor's own crosstalk and no hit.
const TOF_MIN_RANGE_M: f64 = 0.10;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rect {
    pub x0: f64,
    pub x1: f64,
    pub y0: f64,
    pub y1: f64,
    pub height: f64,
    /// On the map.
    pub mapped: bool,
    /// In the house (a mapped box that is gone is on the map only).
    pub present: bool,
    /// Low furniture (a bed, a sofa): its floor rows read phantom drops.
    pub low: bool,
}

impl Rect {
    pub fn new(x0: f64, x1: f64, y0: f64, y1: f64) -> Self {
        Self { x0: x0.min(x1), x1: x0.max(x1), y0: y0.min(y1), y1: y0.max(y1), height: 1.0, mapped: true, present: true, low: false }
    }
    pub fn contains(&self, x: f64, y: f64, r: f64) -> bool {
        x + r > self.x0 && x - r < self.x1 && y + r > self.y0 && y - r < self.y1
    }
    pub fn clearance(&self, x: f64, y: f64) -> f64 {
        let dx = (self.x0 - x).max(0.0).max(x - self.x1);
        let dy = (self.y0 - y).max(0.0).max(y - self.y1);
        dx.hypot(dy)
    }
    /// The ray's entry distance, if it meets the box within `max`.
    pub fn ray(&self, x: f64, y: f64, dx: f64, dy: f64, max: f64) -> Option<f64> {
        self.span(x, y, dx, dy, max).map(|(t, _)| t)
    }

    /// Where a ray from (x, y) along (dx, dy) goes in and comes out.
    pub fn span(&self, x: f64, y: f64, dx: f64, dy: f64, max: f64) -> Option<(f64, f64)> {
        let (mut t0, mut t1) = (0.0_f64, max);
        for (o, d, lo, hi) in [(x, dx, self.x0, self.x1), (y, dy, self.y0, self.y1)] {
            if d.abs() < 1e-12 {
                if o < lo || o > hi {
                    return None;
                }
                continue;
            }
            let (mut ta, mut tb) = ((lo - o) / d, (hi - o) / d);
            if ta > tb {
                std::mem::swap(&mut ta, &mut tb);
            }
            t0 = t0.max(ta);
            t1 = t1.min(tb);
        }
        (t0 <= t1).then_some((t0.max(0.0), t1))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post {
    pub x: f64,
    pub y: f64,
    pub r: f64,
    pub height: f64,
    pub mapped: bool,
}

fn ray_circle(x: f64, y: f64, dx: f64, dy: f64, cx: f64, cy: f64, r: f64, max: f64) -> Option<f64> {
    let (fx, fy) = (x - cx, y - cy);
    let b = fx * dx + fy * dy;
    let c = fx * fx + fy * fy - r * r;
    if c <= 0.0 {
        return Some(0.0);
    }
    let disc = b * b - c;
    if disc < 0.0 {
        return None;
    }
    let t = -b - disc.sqrt();
    (t >= 0.0 && t <= max).then_some(t)
}

/// Where a ray goes in a circle and comes out.
fn span_circle(x: f64, y: f64, dx: f64, dy: f64, cx: f64, cy: f64, r: f64, max: f64) -> Option<(f64, f64)> {
    let t = ray_circle(x, y, dx, dy, cx, cy, r, max)?;
    let (fx, fy) = (x - cx, y - cy);
    let b = fx * dx + fy * dy;
    let disc = (b * b - (fx * fx + fy * fy - r * r)).max(0.0);
    Some((t, (-b + disc.sqrt()).min(max)))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hole {
    pub x0: f64,
    pub x1: f64,
    pub y0: f64,
    pub y1: f64,
    /// Its rim on the books (a saved map's ground book).
    pub booked: bool,
}

impl Hole {
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x0 && x <= self.x1 && y >= self.y0 && y <= self.y1
    }
    pub fn clearance(&self, x: f64, y: f64) -> f64 {
        Rect::new(self.x0, self.x1, self.y0, self.y1).clearance(x, y)
    }
    /// The rim every 10 cm, as a ground book has it.
    pub fn rim(&self) -> Vec<(f64, f64)> {
        let mut pts = Vec::new();
        let mut x = self.x0;
        while x <= self.x1 + 1e-9 {
            pts.push((x, self.y0));
            pts.push((x, self.y1));
            x += 0.1;
        }
        let mut y = self.y0 + 0.1;
        while y < self.y1 {
            pts.push((self.x0, y));
            pts.push((self.x1, y));
            y += 0.1;
        }
        pts
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MoverKind {
    /// Walks from one side to the other across the way, and on.
    Crosser,
    /// Wanders, turning now and then.
    Wanderer,
    /// Walks and stops, as feet in a kitchen.
    StopGo,
    /// Stands in a doorway: still until the duck comes within
    /// [`LINGER_NEAR_M`], `timer` seconds more, then steps out into one of
    /// the rooms and wanders (someone talking on the threshold, a pet lying
    /// there). The `passers` family's (see `scenarios`).
    Lingerer,
    /// Walks its `path` there and back (round it when it has more than two
    /// points), pausing `pause` seconds at each end or corner, and waits
    /// while the duck is in its way: the twin's `patrol`
    /// (`scripts/twin/viewer/movers.py`), the people and the cat of
    /// casa_viva and casa_gente, read from the house's truth.
    Patrol,
}

/// The duck this near wakes a [`MoverKind::Lingerer`]'s countdown.
pub const LINGER_NEAR_M: f64 = 0.9;
/// A [`MoverKind::Patrol`] waits while the duck is this near ahead of it
/// (the twin's `YIELD_M`): a person does not walk into a duck. Waited
/// [`WAIT_ROUND_S`], it steps [`ROUND_M`] to the side where the house
/// leaves room, and goes on from there; where there is none (a door, the
/// passage by the stairwell) it steps back to the path's point it came
/// from and pauses there, out of the way (the twin's movers.py, the same
/// numbers).
pub const YIELD_M: f64 = 0.35;
pub const WAIT_ROUND_S: f64 = 3.0;
pub const ROUND_M: f64 = 0.5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mover {
    pub x: f64,
    pub y: f64,
    pub r: f64,
    pub heading: f64,
    pub speed: f64,
    pub kind: MoverKind,
    /// Seconds left standing still (stop-and-go), or before turning.
    pub timer: f64,
    pub still: bool,
    /// How tall it stands (a cat 0.25, a foot and leg well over the head).
    #[serde(default = "mover_height")]
    pub height: f64,
    /// A [`MoverKind::Lingerer`] the duck came near: its countdown runs.
    #[serde(default)]
    pub woken: bool,
    /// A [`MoverKind::Patrol`]'s way: the points it walks between, the
    /// one it walks to now, whether it walks them backwards, and how long
    /// it pauses at each (`timer` counts the pause down).
    #[serde(default)]
    pub path: Vec<(f64, f64)>,
    #[serde(default)]
    pub leg: usize,
    #[serde(default)]
    pub back: bool,
    #[serde(default)]
    pub pause: (f64, f64),
    /// How long it has waited for the duck in its way, and the point it
    /// steps aside to once it has waited [`WAIT_ROUND_S`].
    #[serde(default)]
    pub waited: f64,
    #[serde(default)]
    pub detour: Option<(f64, f64)>,
    /// The detour is a step back to the path's point it came from.
    #[serde(default)]
    pub retreat: bool,
}

fn mover_height() -> f64 {
    0.3
}

impl Mover {
    /// A [`MoverKind::Patrol`] at its path's start.
    pub fn patrol(path: Vec<(f64, f64)>, r: f64, height: f64, speed: f64, pause: (f64, f64)) -> Self {
        let (x, y) = path.first().copied().unwrap_or_default();
        Mover { x, y, r, heading: 0.0, speed, kind: MoverKind::Patrol, timer: 0.0, still: false, height, woken: false, path, leg: 1, back: false, pause, waited: 0.0, detour: None, retreat: false }
    }

    /// Everything else, at `(x, y)` with no path.
    pub fn new(x: f64, y: f64, r: f64, heading: f64, speed: f64, kind: MoverKind, timer: f64, still: bool, height: f64) -> Self {
        Mover { x, y, r, heading, speed, kind, timer, still, height, woken: false, path: Vec::new(), leg: 0, back: false, pause: (0.0, 0.0), waited: 0.0, detour: None, retreat: false }
    }
}

/// How the map draws the house: real maps ink what the rays met — a band
/// of wall cells along each face — and leave unknown what they never saw:
/// the inside of furniture and thick walls, and patches of floor nothing
/// looked at.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapStyle {
    /// Wall cells this deep from a face (the rest of the inside unknown);
    /// `None`: the whole inside wall.
    pub band_m: Option<f64>,
    /// Floor never seen: (x, y, radius), in the world.
    pub blobs: Vec<(f64, f64, f64)>,
}

impl Default for MapStyle {
    fn default() -> Self {
        Self { band_m: None, blobs: Vec::new() }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct World {
    /// [x0, x1, y0, y1]: outside is no floor the duck reaches (walls close
    /// it), and nothing the map knows.
    pub bounds: [f64; 4],
    pub rects: Vec<Rect>,
    pub posts: Vec<Post>,
    pub holes: Vec<Hole>,
    pub movers: Vec<Mover>,
    #[serde(default)]
    pub style: MapStyle,
}

/// What a ray met.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Hit {
    Rect(usize),
    Post(usize),
    Mover(usize),
}

impl World {
    /// Whether a body of radius `r` at (x, y) touches anything there.
    pub fn static_collides(&self, x: f64, y: f64, r: f64) -> bool {
        self.rects.iter().any(|b| b.present && b.contains(x, y, r)) || self.posts.iter().any(|p| dist((x, y), (p.x, p.y)) < r + p.r)
    }

    pub fn mover_collides(&self, x: f64, y: f64, r: f64) -> Option<usize> {
        self.movers.iter().position(|m| dist((x, y), (m.x, m.y)) < r + m.r)
    }

    pub fn in_hole(&self, x: f64, y: f64) -> bool {
        self.holes.iter().any(|h| h.contains(x, y))
    }

    pub fn hole_clearance(&self, x: f64, y: f64) -> f64 {
        self.holes.iter().map(|h| h.clearance(x, y)).fold(f64::INFINITY, f64::min)
    }

    pub fn static_clearance(&self, x: f64, y: f64) -> f64 {
        let r = self.rects.iter().filter(|b| b.present).map(|b| b.clearance(x, y)).fold(f64::INFINITY, f64::min);
        let p = self.posts.iter().map(|p| (dist((x, y), (p.x, p.y)) - p.r).max(0.0)).fold(f64::INFINITY, f64::min);
        r.min(p)
    }

    /// The nearest thing along a ray within `max`. Things lower than
    /// `low_m` are not seen (the floor threshold while walking).
    pub fn ray(&self, x: f64, y: f64, a: f64, max: f64, low_m: f64) -> (f64, Option<Hit>) {
        let (dx, dy) = (a.cos(), a.sin());
        let mut best = max;
        let mut which = None;
        for (i, b) in self.rects.iter().enumerate() {
            if !b.present || b.height < low_m {
                continue;
            }
            if let Some(t) = b.ray(x, y, dx, dy, best)
                && t < best
            {
                best = t;
                which = Some(Hit::Rect(i));
            }
        }
        for (i, p) in self.posts.iter().enumerate() {
            if p.height < low_m {
                continue;
            }
            if let Some(t) = ray_circle(x, y, dx, dy, p.x, p.y, p.r, best)
                && t < best
            {
                best = t;
                which = Some(Hit::Post(i));
            }
        }
        for (i, m) in self.movers.iter().enumerate() {
            if let Some(t) = ray_circle(x, y, dx, dy, m.x, m.y, m.r, best)
                && t < best
            {
                best = t;
                which = Some(Hit::Mover(i));
            }
        }
        (best, which)
    }

    /// Everything a ray meets within `max`, nearest first: (distance,
    /// height, what).
    pub fn ray_all(&self, x: f64, y: f64, a: f64, max: f64) -> Vec<(f64, f64, f64, Hit)> {
        let (dx, dy) = (a.cos(), a.sin());
        let mut out = Vec::new();
        for (i, b) in self.rects.iter().enumerate() {
            if b.present
                && let Some((t, u)) = b.span(x, y, dx, dy, max)
            {
                out.push((t, u, b.height, Hit::Rect(i)));
            }
        }
        for (i, p) in self.posts.iter().enumerate() {
            if let Some((t, u)) = span_circle(x, y, dx, dy, p.x, p.y, p.r, max) {
                out.push((t, u, p.height, Hit::Post(i)));
            }
        }
        for (i, m) in self.movers.iter().enumerate() {
            if let Some((t, u)) = span_circle(x, y, dx, dy, m.x, m.y, m.r, max) {
                out.push((t, u, m.height, Hit::Mover(i)));
            }
        }
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        out
    }

    /// The movers' next `dt`: they keep off the walls, the holes and the
    /// duck at `duck` (radius `duck_r`) — a pet does not walk into a duck,
    /// it stops or goes round; a duck walking into it is the duck's bump.
    pub fn step_movers(&mut self, dt: f64, rng: &mut Rng, duck: (f64, f64), duck_r: f64) {
        for i in 0..self.movers.len() {
            let mut m = self.movers[i].clone();
            if m.kind == MoverKind::Lingerer {
                m.woken |= dist((m.x, m.y), duck) < LINGER_NEAR_M;
                if m.woken {
                    m.timer -= dt;
                }
                if m.timer <= 0.0 {
                    m.kind = MoverKind::Wanderer;
                    m.still = false;
                    // Out of the doorway into one of the rooms.
                    m.heading += if rng.chance(0.5) { 0.0 } else { std::f64::consts::PI } + rng.range(-0.4, 0.4);
                    m.timer = rng.range(2.0, 4.0);
                }
                self.movers[i] = m;
                continue;
            }
            if m.kind == MoverKind::Patrol {
                // As the twin's movers.py: a pause counts down, then the
                // next point of the path, waiting while the duck is near
                // ahead; at the point a pause, and the next leg — back
                // along the path, or round it when it has more than two.
                m.timer -= dt;
                if m.timer <= 0.0
                    && let Some(&goal) = m.path.get(m.leg)
                {
                    let aside = m.detour;
                    let goal = aside.unwrap_or(goal);
                    let (dx, dy) = (goal.0 - m.x, goal.1 - m.y);
                    let d = dx.hypot(dy);
                    let (ux, uy) = if d > 1e-9 { (dx / d, dy / d) } else { (0.0, 0.0) };
                    let (ox, oy) = (duck.0 - m.x, duck.1 - m.y);
                    let yields = ox.hypot(oy) < m.r + YIELD_M && ox * ux + oy * uy > 0.0;
                    m.still = yields;
                    if yields {
                        m.waited += dt;
                        if m.waited >= WAIT_ROUND_S && aside.is_none() {
                            let [bx0, bx1, by0, by1] = self.bounds;
                            let room = |p: (f64, f64)| !self.static_collides(p.0, p.1, m.r) && !self.in_hole(p.0, p.1) && p.0 - m.r > bx0 && p.0 + m.r < bx1 && p.1 - m.r > by0 && p.1 + m.r < by1;
                            m.detour = [1.0, -1.0].into_iter().map(|side| (m.x - side * uy * ROUND_M, m.y + side * ux * ROUND_M)).find(|p| room(*p) && room(((m.x + p.0) / 2.0, (m.y + p.1) / 2.0)));
                            let came_from = if m.back { m.leg + 1 } else { m.leg.wrapping_sub(1) };
                            if m.detour.is_none()
                                && let Some(&from) = m.path.get(came_from)
                            {
                                m.detour = Some(from);
                                m.retreat = true;
                            }
                        }
                    } else {
                        m.waited = 0.0;
                        let step = d.min(m.speed * dt);
                        m.x += ux * step;
                        m.y += uy * step;
                        m.heading = uy.atan2(ux);
                        if step >= d - 1e-9 && aside.is_some() {
                            m.detour = None;
                            if m.retreat {
                                m.retreat = false;
                                m.timer = rng.range(m.pause.0, m.pause.1);
                            }
                        } else if step >= d - 1e-9 {
                            m.timer = rng.range(m.pause.0, m.pause.1);
                            let n = m.path.len();
                            let next = if m.back { m.leg.checked_sub(1) } else { Some(m.leg + 1).filter(|k| *k < n) };
                            m.leg = match next {
                                Some(k) => k,
                                None if n > 2 && m.path[0] != m.path[n - 1] => 0,
                                None => {
                                    m.back = !m.back;
                                    if m.back { m.leg - 1 } else { m.leg + 1 }
                                }
                            };
                        }
                    }
                }
                self.movers[i] = m;
                continue;
            }
            m.timer -= dt;
            match m.kind {
                MoverKind::StopGo if m.timer <= 0.0 => {
                    m.still = !m.still;
                    m.timer = if m.still { rng.range(0.5, 3.0) } else { rng.range(1.0, 4.0) };
                    if !m.still {
                        m.heading += rng.range(-1.2, 1.2);
                    }
                }
                MoverKind::Wanderer if m.timer <= 0.0 => {
                    m.heading += rng.range(-1.5, 1.5);
                    m.timer = rng.range(1.0, 4.0);
                }
                _ => {}
            }
            if !m.still {
                let (nx, ny) = (m.x + m.speed * dt * m.heading.cos(), m.y + m.speed * dt * m.heading.sin());
                let [bx0, bx1, by0, by1] = self.bounds;
                let blocked = self.static_collides(nx, ny, m.r)
                    || self.in_hole(nx, ny)
                    || nx - m.r < bx0
                    || nx + m.r > bx1
                    || ny - m.r < by0
                    || ny + m.r > by1;
                let into_duck = dist((nx, ny), duck) < m.r + duck_r + 0.02;
                if blocked {
                    // Turn about, a little at random.
                    m.heading += std::f64::consts::PI + rng.range(-0.8, 0.8);
                } else if into_duck {
                    // Waits for the duck, or goes round it.
                    if rng.chance(0.5) {
                        m.heading += rng.range(-1.6, 1.6);
                    }
                } else {
                    m.x = nx;
                    m.y = ny;
                }
            }
            self.movers[i] = m;
        }
    }

    /// One depth frame from the body at (x, y, yaw), the head at
    /// `head_yaw`: the sensor's 8 × 8 zones, each row looking down at the
    /// floor `ROW_FLOOR_M` ahead from `HEAD_H` up. A zone returns an
    /// obstacle where its beam meets something before the floor — a thing
    /// of height H at distance D is met by the rows whose floor distance d
    /// has d ≥ D ≥ d (1 − H / HEAD_H), so a wall answers in every row that
    /// reaches it, a low box in one or two — and the floor otherwise, where
    /// a hole is a drop. Spurious near returns (the crosstalk and the feet
    /// the twin's sensor shows), noise, bias and dropouts per [`Calib`].
    #[allow(clippy::too_many_arguments)]
    /// `look`: the head's pitch from its rest, radians, negative down — a
    /// look at the floor ahead (`Body::look_down`) is about −0.69, what
    /// robotd's `robot.look` at (0.30, 0, −0.12) aims it; the rows then
    /// land from about 0.15 m.
    pub fn sense(&self, (x, y, yaw): (f64, f64, f64), head_yaw: f64, look: f64, walking: bool, calib: &Calib, rng: &mut Rng, seq: u64, at: Instant) -> CliffFrame {
        let mut obstacles = Vec::new();
        let mut drops = Vec::new();
        let mut floors = Vec::new();
        let low_m = if walking { calib.tof_low_walk_m } else { 0.0 };
        // The rows, lowest first: each one's slope (rise per metre ahead)
        // and where it meets the floor (none at or above the horizon).
        let angles: Vec<f64> = match calib.tof_pitch_rad {
            Some(p) => (0..ROWS).map(|r| p + ROW_FOV * ((r as f64 + 0.5) / ROWS as f64 - 0.5)).collect(),
            None => ROW_FLOOR_M.iter().map(|d| (-HEAD_H / d).atan()).collect(),
        };
        let rows: Vec<(f64, f64)> = angles
            .iter()
            .map(|a| {
                let slope = (a + look).tan();
                (slope, if slope < 0.0 { HEAD_H / -slope } else { f64::INFINITY })
            })
            .collect();
        for c in 0..COLS {
            let bearing = head_yaw + COL_FOV * ((c as f64 + 0.5) / COLS as f64 - 0.5);
            let a = yaw + bearing;
            let hits = self.ray_all(x, y, a, calib.tof_range_max);
            let mut edge: Option<(f64, f64)> = None;
            let mut beyond: Option<f64> = None;
            let mut floor_rows = Vec::new();
            for (r, (slope, d)) in rows.iter().copied().enumerate() {
                let prev = if r == 0 { 0.0 } else { rows[r - 1].1 };
                // The beam at `t` ahead is `HEAD_H + slope t` up: it meets a
                // thing that tall before the floor, on its face or, coming
                // down over it, on its top (the twin's bag: twice the sim's
                // returns within 0.2 m before this, 2026-10-25) ...
                let met = hits.iter().find_map(|(t, u, h, what)| {
                    if *t > d || *h < low_m {
                        return None;
                    }
                    if HEAD_H + slope * t <= *h {
                        return Some((*t, *h, *what));
                    }
                    let top = (*h - HEAD_H) / slope;
                    (slope < 0.0 && top <= u.min(d)).then_some((top, *h, *what))
                });
                // ... and kinematics reads a return this near the floor as
                // floor (`Reprojector::FLOOR_SAFETY`, 0.85 of the height).
                let met = met.filter(|(t, ..)| calib.tof_pitch_rad.is_none() || HEAD_H + slope * t >= (1.0 - TOF_FLOOR_SAFETY) * HEAD_H);
                if let Some((t, _, what)) = &met {
                    if !rng.chance(if *t > 1.4 { calib.tof_dropout_far } else { calib.tof_dropout }) {
                        // `tof_range_bias` is quack-nav's `range_m` short of
                        // the trunk's distance: kinematics measures it from
                        // the sensor (pollen-robotics/microduck#430).
                        let trunk = (t + calib.tof_range_sd * rng.gauss()).max(0.02);
                        let range = (trunk + calib.tof_range_bias).max(0.02);
                        // Under `TOF_MIN_RANGE_M` kinematics calls the return
                        // `Zone::TooClose`, no hit: nothing reaches quack-nav.
                        if (TOF_MIN_RANGE_M..calib.tof_range_max).contains(&range) {
                            obstacles.push(Obstacle { bearing, range_m: range, trunk_m: trunk });
                        }
                    }
                    // A low box just past the beak: its floor rows read a
                    // drop now and then (the paper twin's phantom).
                    if matches!(what, Hit::Rect(i) if self.rects[*i].low) && d > *t && d <= t + PHANTOM_DEPTH_M && rng.chance(calib.phantom_p) {
                        drops.push(Drop { bearing, range_m: d, edge_min_m: prev, floor_beyond_m: 0.0, kind: DropKind::Deep });
                    }
                    continue;
                }
                // Above the horizon, or the floor out of range: no return.
                if d >= calib.tof_range_max {
                    continue;
                }
                // The floor, or a hole where the floor should be.
                if self.in_hole(x + d * a.cos(), y + d * a.sin()) {
                    if edge.is_none() {
                        edge = Some((d, floor_rows.last().copied().unwrap_or(0.0)));
                    }
                } else {
                    if edge.is_some() && beyond.is_none() {
                        beyond = Some(d);
                    }
                    if edge.is_none() && rng.chance(calib.phantom_any_p) {
                        drops.push(Drop { bearing, range_m: d, edge_min_m: prev, floor_beyond_m: 0.0, kind: DropKind::Deep });
                    }
                    floor_rows.push(d);
                    floors.push((bearing, d));
                }
            }
            if let Some((range_m, edge_min_m)) = edge {
                drops.push(Drop { bearing, range_m, edge_min_m, floor_beyond_m: beyond.unwrap_or(0.0), kind: DropKind::Missing });
            }
        }
        // Spurious near returns, zone by zone.
        if calib.spur_p > 0.0 {
            for z in 0..COLS * ROW_FLOOR_M.len() {
                if rng.chance(calib.spur_p) {
                    let c = z % COLS;
                    let bearing = head_yaw + COL_FOV * ((c as f64 + 0.5) / COLS as f64 - 0.5);
                    let range = (calib.spur_median_m * (calib.spur_sigma * rng.gauss()).exp()).clamp(0.1, calib.tof_range_max - 0.01);
                    obstacles.push(Obstacle { bearing, range_m: range, trunk_m: range - calib.tof_range_bias });
                }
            }
        }
        CliffFrame { moving: walking, seq, at, head_yaw, drops, floors, obstacles, floor_beams: 40, judged: 64, odom: None }
    }

    /// The map as the duck holds it, in the map's frame (the world moved
    /// by `bias`): floor free, what the map has inked a wall, holes and
    /// the outside unknown. Cells as `map.frame` codes them (0 unknown,
    /// 1 free, 2 wall), and the decoded grid.
    pub fn raster(&self, bias: (f64, f64)) -> (Vec<u8>, Grid) {
        let [x0, x1, y0, y1] = self.bounds;
        let margin = 0.3;
        let (gx0, gy0) = (x0 - margin + bias.0, y0 - margin + bias.1);
        let cols = ((x1 - x0 + 2.0 * margin) / CELL).ceil() as usize;
        let rows = ((y1 - y0 + 2.0 * margin) / CELL).ceil() as usize;
        let mut codes = vec![0u8; rows * cols];
        let mut cells = vec![Cell::Unknown; rows * cols];
        for r in 0..rows {
            for c in 0..cols {
                // The cell's centre in the world.
                let (wx, wy) = (gx0 + (c as f64 + 0.5) * CELL - bias.0, gy0 + (r as f64 + 0.5) * CELL - bias.1);
                let inside = wx > x0 && wx < x1 && wy > y0 && wy < y1;
                let deep = |b: &Rect| match self.style.band_m {
                    Some(band) => wx > b.x0 + band && wx < b.x1 - band && wy > b.y0 + band && wy < b.y1 - band,
                    None => false,
                };
                let in_rect = self.rects.iter().filter(|b| b.mapped && b.contains(wx, wy, CELL * 0.5));
                let (mut wall, mut hidden) = (false, false);
                for b in in_rect {
                    if deep(b) {
                        hidden = true;
                    } else {
                        wall = true;
                    }
                }
                wall = wall || self.posts.iter().any(|p| p.mapped && dist((wx, wy), (p.x, p.y)) < p.r + CELL * 0.5);
                let unseen = self.style.blobs.iter().any(|(bx, by, br)| dist((wx, wy), (*bx, *by)) < *br);
                let (code, cell) = if wall {
                    (2, Cell::Wall)
                } else if !inside || hidden || unseen || self.in_hole(wx, wy) {
                    (0, Cell::Unknown)
                } else {
                    (1, Cell::Free)
                };
                codes[r * cols + c] = code;
                cells[r * cols + c] = cell;
            }
        }
        (codes, Grid { rows, cols, x_min: gx0, y_min: gy0, cell_m: CELL, cells })
    }

    /// The booked holes' rims, in the map's frame.
    pub fn books(&self, bias: (f64, f64)) -> Vec<(f64, f64)> {
        self.holes.iter().filter(|h| h.booked).flat_map(|h| h.rim()).map(|(x, y)| (x + bias.0, y + bias.1)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rays_meet_boxes_posts_and_movers() {
        let mut w = World { bounds: [-2.0, 2.0, -2.0, 2.0], ..Default::default() };
        w.rects.push(Rect::new(1.0, 1.2, -1.0, 1.0));
        assert!((w.ray(0.0, 0.0, 0.0, 2.2, 0.0).0 - 1.0).abs() < 1e-9);
        w.posts.push(Post { x: 0.5, y: 0.0, r: 0.03, height: 0.7, mapped: false });
        assert!((w.ray(0.0, 0.0, 0.0, 2.2, 0.0).0 - 0.47).abs() < 1e-9);
        w.movers.push(Mover::new(0.3, 0.0, 0.1, 0.0, 0.0, MoverKind::Wanderer, 1.0, true, 0.3));
        assert!((w.ray(0.0, 0.0, 0.0, 2.2, 0.0).0 - 0.2).abs() < 1e-9);
        // Low things are lost while walking.
        w.movers.clear();
        w.posts[0].height = 0.05;
        assert!((w.ray(0.0, 0.0, 0.0, 2.2, 0.09).0 - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_look_down_sees_the_narrow_hole_a_stride_ahead_that_the_walking_pitch_cannot() {
        // A hole 0.20–0.45 m ahead: nearer than the sensor's nearest row
        // lands at the calibrated pitches (0.52–0.60 m), which reaches the
        // floor beyond it.
        let mut w = World { bounds: [-2.0, 2.0, -2.0, 2.0], ..Default::default() };
        w.holes.push(Hole { x0: 0.20, x1: 0.45, y0: -0.5, y1: 0.5, booked: false });
        let c = Calib { tof_dropout: 0.0, phantom_any_p: 0.0, tof_range_sd: 0.0, tof_pitch_rad: Some(-6.0_f64.to_radians()), ..Calib::default() };
        let level = w.sense((0.0, 0.0, 0.0), 0.0, 0.0, false, &c, &mut Rng::new(1), 1, Instant::now());
        assert!(level.drops.is_empty(), "the level head lands its nearest row past the hole: {:?}", level.drops);
        let down = w.sense((0.0, 0.0, 0.0), 0.0, -0.69, false, &c, &mut Rng::new(1), 2, Instant::now());
        let near = down.drops.iter().map(|d| d.edge_min_m).fold(f64::INFINITY, f64::min);
        assert!(!down.drops.is_empty() && near < 0.25, "the look sees the hole from its near rim: {near:.2} {:?}", down.drops);
    }

    #[test]
    fn a_patrol_walks_its_path_there_and_back_and_waits_for_the_duck() {
        let mut w = World { bounds: [-2.0, 2.0, -2.0, 2.0], ..Default::default() };
        w.movers.push(Mover::patrol(vec![(0.0, 0.0), (1.0, 0.0)], 0.14, 0.9, 0.5, (1.0, 1.0)));
        let mut rng = Rng::new(3);
        let far = (5.0, 5.0);
        // 2 s at 0.5 m/s: at the far end, pausing.
        for _ in 0..20 {
            w.step_movers(0.1, &mut rng, far, 0.11);
        }
        let m = &w.movers[0];
        assert!((m.x - 1.0).abs() < 1e-9 && m.timer > 0.0 && m.back, "{m:?}");
        // The pause over, back toward the start.
        for _ in 0..15 {
            w.step_movers(0.1, &mut rng, far, 0.11);
        }
        let m = &w.movers[0];
        assert!(m.x < 0.9 && m.x > 0.5, "{m:?}");
        // The duck right ahead on its way: it waits; behind it: it walks on.
        let x = m.x;
        w.step_movers(0.1, &mut rng, (x - 0.3, 0.0), 0.11);
        assert!((w.movers[0].x - x).abs() < 1e-9 && w.movers[0].still, "{:?}", w.movers[0]);
        w.step_movers(0.1, &mut rng, (x + 0.3, 0.0), 0.11);
        assert!(w.movers[0].x < x - 0.04, "{:?}", w.movers[0]);
    }

    #[test]
    fn the_sensor_finds_a_hole_ahead() {
        let mut w = World { bounds: [-2.0, 2.0, -2.0, 2.0], ..Default::default() };
        w.holes.push(Hole { x0: 0.5, x1: 0.9, y0: -0.5, y1: 0.5, booked: false });
        let c = Calib { tof_dropout: 0.0, phantom_any_p: 0.0, ..Calib::default() };
        let f = w.sense((0.0, 0.0, 0.0), 0.0, 0.0, true, &c, &mut Rng::new(1), 1, Instant::now());
        assert!(!f.drops.is_empty());
        let d = f.drops.iter().map(|d| d.range_m).fold(f64::INFINITY, f64::min);
        assert!((0.5..=0.75).contains(&d), "{d}");
    }

    #[test]
    fn the_map_is_the_world_moved_by_the_bias() {
        let mut w = World { bounds: [-1.0, 1.0, -1.0, 1.0], ..Default::default() };
        w.rects.push(Rect::new(0.4, 0.6, -1.0, 1.0));
        let mut gone = Rect::new(-0.6, -0.4, -1.0, 1.0);
        gone.mapped = false;
        w.rects.push(gone);
        let (_, g) = w.raster((0.1, 0.0));
        assert_eq!(g.at(0.6, 0.0), Some(Cell::Wall));
        assert_eq!(g.at(0.3, 0.0), Some(Cell::Free));
        assert_eq!(g.at(-0.4, 0.0), Some(Cell::Free));
    }
}
