//! The paper twin: the explorer, run against a kinematic model of the duck
//! in the apartment's boxes, thousands of times an hour.
//!
//!     cargo run --release -p quack-nav --example paper_twin -- \
//!         quack-nav/examples/apartment.world.json OUT_DIR [--seed N] \
//!         [--budget S] [--noise M] [--kidnap P] [--runs K]
//!         [--goto X,Y [--books] [--known]] [--bias DX,DY]
//!
//! What is real: `explore.rs` (the job), `frontier.rs` (the planner) and
//! `tools::plan_step` (every guard of `robot.map_step`). What is modelled,
//! from what the MuJoCo twin measured: the gait (0.114 m/s at vx 0.3,
//! 0.65 rad/s per unit of yaw, a right veer, no turning in place from a
//! standstill below the dead zone and 30–58°/s past it (`in_place_rate`),
//! backing up only with a positive yaw), the depth sensor as
//! eight columns of rays with the head sweep at a stand, the map as cells
//! seen from stands, the pose as truth plus a random walk that the stands
//! pull back — and, if asked, a kidnap now and then to exercise the guards.
//!
//! Per run it writes `runNNN.log` (the watch format: `[   20s] ... truth=(x, y`
//! lines, so `mapshot.py` and `rooms.py` read it) and `runNNN.frame.json`
//! (a `map.frame`, so the same tools read the map), and prints one summary
//! line: coverage, legs, refusals, path, falls.

use std::collections::HashMap;
use std::f64::consts::{PI, TAU};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use quack_nav::cliff::{CliffFrame, CliffStatus, Drop, DropKind, Obstacle, StreamState};
use quack_nav::map::MapFrame;
use quack_duck::gait::GaitConfig;
use quack_nav::explore::{Body, ExploreHandle, Job};
use quack_nav::tools::plan_step;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
struct World {
    bounds: [f64; 4],
    start: [f64; 3],
    holes: Vec<[f64; 4]>,
    rooms: HashMap<String, [f64; 4]>,
    boxes: Vec<(String, f64, f64, f64, f64)>,
    /// Boxes low enough (top within ~0.3 m) that the depth sensor's floor
    /// rows read them as a drop now and then — MuJoCo run 70 put five
    /// drops on the bed and they sealed the bedroom door on the planner.
    /// The mechanism in the simulator is not pinned down; what is modelled
    /// is its signature: a "deep" drop at the row's floor distance, on the
    /// box, in a fraction of the frames that look at it from close by.
    #[serde(default)]
    low: Vec<String>,
}

/// Fraction of (column, floor row) beams landing on a low box within
/// [`PHANTOM_DEPTH_M`] past its face that report a drop, per frame.
const PHANTOM_P: f64 = 0.10;
const PHANTOM_DEPTH_M: f64 = 0.5;

/// A hash in [0, 1) of the beam's identity in this frame: deterministic
/// per seed, so a run replays.
fn beam_hash(seed: u64, seq: u64, col: usize, row: usize) -> f64 {
    let mut z = seed ^ seq.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ ((col as u64) << 8 | row as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64
}

/// Measured on the twin.
const SPEED_AT_03: f64 = 0.114;
const YAW_PER_UNIT: f64 = 0.65;
const STRAIGHT_VEER: f64 = -0.05;
const BACK_SPEED: f64 = 0.08;
const SPIN_RATE: f64 = 0.52;
const BODY_R: f64 = 0.11;
const CELL: f64 = 0.05;
const SENSOR_RANGE: f64 = 2.2;
const HEAD_SWEEP: [f64; 3] = [-0.8, 0.0, 0.8];
const COLS: usize = 8;
const COL_FOV: f64 = 0.78;
/// Floor distances the eight rows look at, metres ahead (head 0.25 m up).
const ROW_FLOOR_M: [f64; 8] = [0.25, 0.32, 0.42, 0.55, 0.75, 1.0, 1.4, 2.0];

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn gauss(&mut self) -> f64 {
        let (u, v) = (self.next().max(1e-12), self.next());
        (-2.0 * u.ln()).sqrt() * (TAU * v).cos()
    }
}

struct PaperTwin {
    /// The leg being walked asked for the heading hold (`hold: true`).
    hold_leg: bool,
    /// `--known`: the map is the world's own, frozen, as a saved map is
    /// on the twin — for the journey benches (the paper twin never maps
    /// the living room on its own, 32 % in 900 s, so it could not measure
    /// the seal or the go-round; point 10, 2026-09-20).
    known: bool,
    world: World,
    seed: u64,
    rng: Rng,
    clock: Instant,
    t0: Instant,
    // truth
    x: f64,
    y: f64,
    yaw: f64,
    fell: bool,
    bumps: u32,
    path_m: f64,
    last_walk: Option<Instant>,
    // reported pose = truth + err + bias
    err: (f64, f64, f64),
    /// `--bias`: a constant offset of the map's frame from the world — the
    /// house2 map on MuJoCo sits 0.16–0.19 m west of the truth along the
    /// stairwell's passage, maploc tracking it confidently (rimG1,
    /// 2026-09-20); the stands do not pull it back, it is the map's.
    bias: (f64, f64),
    noise: f64,
    kidnap_p: f64,
    kidnaps: u32,
    // map
    rows: usize,
    cols: usize,
    x_min: f64,
    y_min: f64,
    cells: Vec<u8>,
    windows: u32,
    submaps: u32,
    seq: u64,
    cliff: CliffStatus,
    gait: GaitConfig,
    // recording
    log: fs::File,
    next_log: Duration,
    truth_track: Vec<(f64, f64)>,
    refusals: u32,
}

impl PaperTwin {
    fn new(world: World, seed: u64, noise: f64, kidnap_p: f64, log: fs::File) -> Self {
        let [x0, x1, y0, y1] = world.bounds;
        let cols = ((x1 - x0) / CELL).ceil() as usize;
        let rows = ((y1 - y0) / CELL).ceil() as usize;
        let t0 = Instant::now();
        let [sx, sy, syaw] = world.start;
        let mut twin = Self {
            world,
            seed,
            rng: Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1),
            clock: t0,
            t0,
            x: sx,
            y: sy,
            yaw: syaw,
            fell: false,
            hold_leg: false,
            known: false,
            bumps: 0,
            path_m: 0.0,
            last_walk: None,
            err: (0.0, 0.0, 0.0),
            bias: (0.0, 0.0),
            noise,
            kidnap_p,
            kidnaps: 0,
            rows,
            cols,
            x_min: x0,
            y_min: y0,
            cells: vec![0; rows * cols],
            windows: 0,
            submaps: 1,
            seq: 0,
            cliff: CliffStatus {
                stream: StreamState::Serving,
                body_seen: true,
                ..CliffStatus::default()
            },
            gait: GaitConfig::default(),
            log,
            next_log: Duration::from_secs(20),
            truth_track: Vec::new(),
            refusals: 0,
        };
        twin.stand_scan();
        twin
    }

    fn elapsed(&self) -> Duration {
        self.clock - self.t0
    }

    /// The whole house on the map at once, as a saved map has it: floor
    /// free, the walls and furniture inked, the holes left unknown (the
    /// floor sensor breaks the ray at a rim, nothing inks a hole); the
    /// map frozen from here on.
    fn know_the_world(&mut self) {
        for r in 0..self.rows {
            for c in 0..self.cols {
                let (x, y) = (self.x_min + (c as f64 + 0.5) * CELL, self.y_min + (r as f64 + 0.5) * CELL);
                self.cells[r * self.cols + c] = if self.in_box(x, y, 0.0) {
                    2
                } else if self.in_hole(x, y) {
                    0
                } else {
                    1
                };
            }
        }
        self.known = true;
    }

    /// Distance along a ray from (x, y) at angle `a` to the first box, or
    /// `max` if none.
    fn ray_to_wall(&self, x: f64, y: f64, a: f64, max: f64) -> f64 {
        self.ray_hit(x, y, a, max).0
    }

    /// The nearest box face along the ray, and which box.
    fn ray_hit(&self, x: f64, y: f64, a: f64, max: f64) -> (f64, Option<usize>) {
        let (dx, dy) = (a.cos(), a.sin());
        let mut best = max;
        let mut which = None;
        for (i, (_, bx0, bx1, by0, by1)) in self.world.boxes.iter().enumerate() {
            let (mut t0, mut t1) = (0.0_f64, best);
            for (o, d, lo, hi) in [(x, dx, *bx0, *bx1), (y, dy, *by0, *by1)] {
                if d.abs() < 1e-9 {
                    if o < lo || o > hi {
                        t0 = f64::INFINITY;
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
            if t0 <= t1 && t0 < best {
                best = t0.max(0.0);
                which = Some(i);
            }
        }
        (best, which)
    }

    fn in_hole(&self, x: f64, y: f64) -> bool {
        self.world
            .holes
            .iter()
            .any(|[x0, x1, y0, y1]| x >= *x0 && x <= *x1 && y >= *y0 && y <= *y1)
    }

    fn in_box(&self, x: f64, y: f64, r: f64) -> bool {
        self.world.boxes.iter().any(|(_, x0, x1, y0, y1)| {
            x + r > *x0 && x - r < *x1 && y + r > *y0 && y - r < *y1
        })
    }

    fn cell_at(&mut self, x: f64, y: f64) -> Option<&mut u8> {
        let c = ((x - self.x_min) / CELL).floor();
        let r = ((y - self.y_min) / CELL).floor();
        if c < 0.0 || r < 0.0 || c as usize >= self.cols || r as usize >= self.rows {
            return None;
        }
        Some(&mut self.cells[r as usize * self.cols + c as usize])
    }

    /// A stand: the head sweeps, the sensor judges, the map grows, the pose
    /// is pulled back toward the truth (a loop closure), the odds of a
    /// kidnap are rolled.
    /// One depth frame from where the duck stands now, the head at
    /// `head_yaw`: obstacles per column, and a drop where the floor rows
    /// find the hole.
    fn sense(&self, head_yaw: f64, seq: u64) -> CliffFrame {
        {
            let mut obstacles = Vec::new();
            let mut drops = Vec::new();
            for c in 0..COLS {
                let bearing = head_yaw + COL_FOV * ((c as f64 + 0.5) / COLS as f64 - 0.5);
                let a = self.yaw + bearing;
                let (hit, which) = self.ray_hit(self.x, self.y, a, SENSOR_RANGE);
                if hit < SENSOR_RANGE {
                    obstacles.push(Obstacle {
                        bearing,
                        range_m: hit,
                    });
                }
                // A low box just past the beak: the floor rows that land on
                // it read a drop now and then.
                let low = which.is_some_and(|i| self.world.low.iter().any(|n| *n == self.world.boxes[i].0));
                if low {
                    for (r, d) in ROW_FLOOR_M.iter().copied().enumerate() {
                        if d <= hit || d > hit + PHANTOM_DEPTH_M {
                            continue;
                        }
                        if beam_hash(self.seed, seq, c, r) < PHANTOM_P {
                            let prev = ROW_FLOOR_M.iter().copied().filter(|p| *p < d).last().unwrap_or(0.0);
                            drops.push(Drop {
                                bearing,
                                range_m: d,
                                edge_min_m: prev,
                                kind: DropKind::Deep,
                            });
                        }
                    }
                }
                // The rows that look at the floor: is the floor there?
                let mut edge: Option<(f64, f64)> = None;
                for d in ROW_FLOOR_M {
                    if d > hit {
                        break;
                    }
                    if self.in_hole(self.x + d * a.cos(), self.y + d * a.sin()) {
                        // The edge is between the last good row and this one.
                        let prev = ROW_FLOOR_M.iter().copied().filter(|p| *p < d).last().unwrap_or(0.0);
                        edge = Some((d, prev));
                        break;
                    }
                }
                if let Some((range_m, edge_min_m)) = edge {
                    drops.push(Drop {
                        bearing,
                        range_m,
                        edge_min_m,
                        kind: DropKind::Missing,
                    });
                }
            }
            CliffFrame {
                moving: false,
                seq,
                at: self.clock,
                head_yaw,
                drops,
                obstacles,
                floor_beams: 40,
                judged: 64,
            }
        }
    }

    fn stand_scan(&mut self) {
        let mut frames = Vec::new();
        for head_yaw in HEAD_SWEEP {
            self.seq += 1;
            frames.push(self.sense(head_yaw, self.seq));
        }
        // Map growth: a dense sweep over everything the head saw, free
        // cells along each ray up to the hit, the hit inked one cell thick
        // on each side — the filled sector a real stand produces.
        let half = HEAD_SWEEP[2] + COL_FOV / 2.0;
        // A known map is frozen: nothing inks (`--known`).
        let n = if self.known { -1 } else { 120 };
        for k in 0..=n {
            let aa = self.yaw - half + 2.0 * half * f64::from(k) / f64::from(n);
            let h = self.ray_to_wall(self.x, self.y, aa, SENSOR_RANGE);
            let mut d = 0.05;
            while d < h {
                let (px, py) = (self.x + d * aa.cos(), self.y + d * aa.sin());
                if self.in_hole(px, py) {
                    break;
                }
                if let Some(cell) = self.cell_at(px, py)
                    && *cell == 0
                {
                    *cell = 1;
                }
                d += CELL / 2.0;
            }
            if h < SENSOR_RANGE {
                for extra in [0.02, 0.06] {
                    let (px, py) = (self.x + (h + extra) * aa.cos(), self.y + (h + extra) * aa.sin());
                    if let Some(cell) = self.cell_at(px, py) {
                        *cell = 2;
                    }
                }
            }
        }
        self.cliff.recent = frames;
        self.cliff.frames += 3;
        self.cliff.odom_yaw = Some(self.yaw + self.err.2);
        self.windows += 3;
        if self.windows % 6 == 0 {
            self.submaps += 1;
        }
        // Loop closure: half the error goes.
        self.err.0 *= 0.5;
        self.err.1 *= 0.5;
        self.err.2 *= 0.5;
        if self.rng.next() < self.kidnap_p {
            self.kidnaps += 1;
            self.err.0 += 1.5 * self.rng.gauss();
            self.err.1 += 1.5 * self.rng.gauss();
            self.err.2 += 0.8 * self.rng.gauss();
        }
    }

    /// Walk for `secs` with the gait's response to (vx, vyaw); the truth
    /// moves, the reported pose drifts, boxes stop the body, holes end it.
    fn walk(&mut self, vx: f64, vyaw: f64, secs: f64) {
        let dt = 0.1;
        let mut t = 0.0;
        let spinning = vx.abs() < 0.05;
        let can_spin = self
            .last_walk
            .is_some_and(|w| self.clock - w < Duration::from_millis(1500));
        // A short walking pulse (the alignment's, 0.6 s) turns by what the
        // gait happens to be doing: measured on the MuJoCo twin
        // (turnprobe, 2026-09-18) +18, +18, +16, +18, +17, +4, −1, +4 to
        // the left and −19, −9, +13, −12, −32, −30, −30, −7 to the right —
        // 13° ± 10° per pulse, the sign wrong about once in ten. The
        // long legs keep the mean.
        let pulse_gain = if vx > 0.05 && secs <= 0.8 && vyaw.abs() > 0.3 {
            (0.85 + 0.65 * self.rng.gauss()).clamp(-0.6, 2.2)
        } else {
            1.0
        };
        while t < secs {
            // The heading hold (`tools::timed_move_held`, 2026-09-18):
            // on a walking leg that is not an arc the taps cancel the
            // veer — measured 1–3 cm of lateral drift per metre in
            // place of 1–14, the heading within ±4°.
            let held = vx > 0.05 && vyaw.abs() < 0.05 && (self.hold_leg || std::env::var("QK_HOLD_HEADING").is_ok_and(|v| v == "1"));
            let (v, w) = if vx > 0.05 {
                (
                    SPEED_AT_03 * vx / 0.3,
                    YAW_PER_UNIT * vyaw * pulse_gain + if vyaw.abs() < 0.1 && !held { STRAIGHT_VEER } else { 0.0 },
                )
            } else if vx < -0.05 {
                // Backing: from a standstill only with a positive yaw; once
                // stepping, any yaw (measured 2026-09-08: +0.7 then -0.7
                // backed 0.23 m turning -87°, then straight -13°).
                if vyaw > 0.3 || can_spin { (-BACK_SPEED, 0.6 * vyaw) } else { (0.0, 0.0) }
            } else if let Some(w) = spinning.then(|| in_place_rate(vyaw)).flatten() {
                // Past the dead zone: a turn in place from a standstill.
                (0.0, w)
            } else if spinning && can_spin && vyaw.abs() > 0.3 {
                // Sign honoured (right turns measured on the twin, 2026-09-06).
                (0.0, SPIN_RATE * vyaw.signum())
            } else {
                (0.0, 0.0)
            };
            let (nx, ny) = (self.x + v * dt * self.yaw.cos(), self.y + v * dt * self.yaw.sin());
            if self.in_box(nx, ny, BODY_R) {
                // The body meets something: the leg ends there, as the real
                // duck's does against a wall (or worse).
                self.bumps += 1;
                self.clock += Duration::from_secs_f64(secs - t);
                break;
            } else {
                self.path_m += ((nx - self.x).powi(2) + (ny - self.y).powi(2)).sqrt();
                self.x = nx;
                self.y = ny;
            }
            self.yaw = (self.yaw + w * dt + PI).rem_euclid(TAU) - PI;
            if !self.fell && self.in_hole(self.x, self.y) {
                self.fell = true;
                eprintln!(
                    "FELL at ({:.2}, {:.2}) yaw {:.2} during vx {vx} vyaw {vyaw} for {secs} s",
                    self.x, self.y, self.yaw
                );
            }
            // Odometry drifts with motion.
            self.err.0 += self.noise * v.abs() * dt * self.rng.gauss();
            self.err.1 += self.noise * v.abs() * dt * self.rng.gauss();
            self.err.2 += self.noise * 0.3 * w.abs() * dt * self.rng.gauss();
            self.clock += Duration::from_secs_f64(dt);
            self.tick();
            t += dt;
            if self.fell {
                break;
            }
        }
        if vx.abs() > 0.05 {
            self.last_walk = Some(self.clock);
        }
        self.cliff.odom_yaw = Some(self.yaw + self.err.2);
    }

    /// The stream as the guards see it: the stand's sweep frames stamped
    /// when they were taken (they age out of the memory window after a
    /// move, as on the robot — a frame carried past a move puts its drops
    /// where they are not, and a phantom 25 cm off the beak froze a twin
    /// for forty minutes), plus one fresh frame from where the duck is
    /// now with the head centred: the real sensor never stops.
    fn cliff_now(&self) -> CliffStatus {
        let mut c = self.cliff.clone();
        c.recent
            .retain(|f| self.clock.duration_since(f.at) <= quack_nav::cliff::MEMORY);
        c.recent.push(self.sense(0.0, self.seq + 1));
        c.odom_yaw = Some(self.yaw + self.err.2);
        c
    }

    fn frame_now(&self) -> MapFrame {
        MapFrame {
            seq: self.seq,
            x: self.x + self.err.0 + self.bias.0,
            y: self.y + self.err.1 + self.bias.1,
            yaw: self.yaw + self.err.2,
            tracking: !self.fell,
            x_min: self.x_min as f32,
            y_min: self.y_min as f32,
            cell_m: CELL as f32,
            rows: self.rows as u32,
            cols: self.cols as u32,
            cells: b64(&self.cells),
            n_submaps: self.submaps,
            n_loops: self.windows / 9,
            windows: self.windows,
            still: true,
            seated: self.fell,
        }
    }

    /// The watch-format line, every 20 s of simulated time.
    fn tick(&mut self) {
        if self.elapsed() >= self.next_log {
            self.next_log += Duration::from_secs(20);
            self.truth_track.push((self.x, self.y));
            let free = self.cells.iter().filter(|c| **c == 1).count();
            let wall = self.cells.iter().filter(|c| **c == 2).count();
            let f = self.frame_now();
            let _ = writeln!(
                self.log,
                "[{:5.0}s] running ref={} cells={}/{} | pose={{'x': {:.2}, 'y': {:.2}, 'yaw': {:.2}}} trk={} | truth=({:.2}, {:.2}, 0.116, {:.2})",
                self.elapsed().as_secs_f64(),
                self.refusals,
                free,
                wall,
                f.x,
                f.y,
                f.yaw,
                if f.tracking { "True" } else { "False" },
                self.x,
                self.y,
                self.yaw
            );
        }
    }

    fn coverage(&self) -> Vec<(String, f64)> {
        let mut out = Vec::new();
        // Specific rooms first, the two big slabs last (they overlap).
        let order = ["kitchen", "bath", "corridor_n", "corridor_s", "west", "east"];
        let mut counted = vec![false; self.cells.len()];
        for name in order {
            let Some([x0, x1, y0, y1]) = self.world.rooms.get(name) else { continue };
            let mut free = 0usize;
            for r in 0..self.rows {
                for c in 0..self.cols {
                    let i = r * self.cols + c;
                    if counted[i] || self.cells[i] != 1 {
                        continue;
                    }
                    let (x, y) = (self.x_min + (c as f64 + 0.5) * CELL, self.y_min + (r as f64 + 0.5) * CELL);
                    if x >= *x0 && x <= *x1 && y >= *y0 && y <= *y1 {
                        counted[i] = true;
                        free += 1;
                    }
                }
            }
            out.push((name.to_string(), free as f64 * CELL * CELL / ((x1 - x0) * (y1 - y0))));
        }
        out
    }
}

impl PaperTwin {
    /// Room stays from the truth track (a sample every 20 s): the longest
    /// stay in minutes and how many stays lasted five minutes or more.
    /// The user's eye on MuJoCo: from one build to the next the duck
    /// went from leaving a room in 5–11 minutes to 15–32.
    fn dwell(&self) -> (f64, usize) {
        let order = ["kitchen", "bath", "corridor_n", "corridor_s", "west", "east"];
        let room = |(x, y): (f64, f64)| {
            order
                .iter()
                .find(|n| {
                    self.world
                        .rooms
                        .get(**n)
                        .is_some_and(|[x0, x1, y0, y1]| x >= *x0 && x <= *x1 && y >= *y0 && y <= *y1)
                })
                .copied()
                .unwrap_or("hall")
        };
        let (mut longest, mut long_stays, mut cur, mut n) = (0usize, 0usize, "", 0usize);
        for p in &self.truth_track {
            let r = room(*p);
            if r == cur {
                n += 1;
            } else {
                if n * 20 >= 300 {
                    long_stays += 1;
                }
                longest = longest.max(n);
                cur = r;
                n = 1;
            }
        }
        if n * 20 >= 300 {
            long_stays += 1;
        }
        longest = longest.max(n);
        (longest as f64 * 20.0 / 60.0, long_stays)
    }
}

impl Body for PaperTwin {
    fn step(&mut self, args: &Value) -> Result<Value, String> {
        if self.fell {
            // A fallen twin stays down; time passes, so the job's patience
            // runs out instead of the loop spinning on a frozen clock.
            self.clock += Duration::from_secs(1);
            return Err("the duck is seated or fallen: stand it up first (sit_toggle)".into());
        }
        let frame = self.frame_now();
        let cliff = self.cliff_now();
        let plan = match plan_step(args, &frame, Some(&cliff), &self.gait, self.clock) {
            Ok(p) => p,
            Err(e) => {
                self.refusals += 1;
                return Err(e);
            }
        };
        if plan.walk_s > 0.0 {
            self.hold_leg = args.get("hold").and_then(Value::as_bool).unwrap_or(false);
            self.walk(plan.params.vx, plan.params.vyaw, plan.walk_s);
            self.hold_leg = false;
        }
        let before = self.windows;
        if plan.stop_s >= 3.0 {
            self.clock += Duration::from_secs_f64(plan.stop_s);
            self.tick();
            self.stand_scan();
        } else {
            self.clock += Duration::from_secs_f64(plan.stop_s);
            self.tick();
        }
        let f = self.frame_now();
        Ok(json!({
            "walked_s": plan.walk_s,
            "stood_s": plan.stop_s,
            "new_windows": self.windows - before,
            "windows": self.windows,
            "tracking": f.tracking,
            "pose": {"x": f.x, "y": f.y, "yaw": f.yaw},
            "shortened": plan.shortened,
            "steered": plan.steered,
            "hint": if self.fell { "the duck fell" } else { "this stop reached the map" },
        }))
    }

    fn blind_move(&mut self, args: &Value) -> Result<Value, String> {
        let vx = args.get("vx").and_then(Value::as_f64).unwrap_or(0.0);
        let vyaw = args.get("vyaw").and_then(Value::as_f64).unwrap_or(0.0);
        let secs = args.get("duration_s").and_then(Value::as_f64).unwrap_or(0.0);
        self.walk(vx, vyaw, secs);
        Ok(json!({"moved": true}))
    }

    fn frame(&self) -> Option<MapFrame> {
        Some(self.frame_now())
    }

    fn pose_trusted(&self) -> bool {
        !self.fell
    }

    fn frozen_map(&self) -> bool {
        self.known
    }

    fn cliff(&self) -> Option<CliffStatus> {
        Some(self.cliff_now())
    }

    fn now(&self) -> Instant {
        self.clock
    }

    fn sleep(&mut self, d: Duration) {
        self.clock += d;
        self.tick();
    }
}

fn b64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |acc, (i, b)| acc | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn main() -> anyhow::Result<()> {
    // `RUST_LOG=info` shows the explorer's own decisions (targets, refusals,
    // guards) on stderr — the diagnostic view; quiet by default.
    if std::env::var_os("RUST_LOG").is_some() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .with_ansi(false)
            .with_writer(std::io::stderr)
            .try_init();
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let world_path = args.first().cloned().ok_or_else(|| anyhow::anyhow!("usage: paper_twin WORLD.json OUT_DIR [--seed N] [--budget S] [--noise M] [--kidnap P] [--runs K]"))?;
    let out_dir = PathBuf::from(args.get(1).cloned().unwrap_or_else(|| "paper-out".into()));
    let opt = |name: &str, default: f64| -> f64 {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    };
    let seed0 = opt("--seed", 1.0) as u64;
    let budget = opt("--budget", 1800.0);
    let noise = opt("--noise", 0.05);
    let kidnap = opt("--kidnap", 0.0);
    let runs = opt("--runs", 1.0) as u64;
    // `--goto x,y`: map first, then walk to that point and report the
    // planned route against the one the body walked — the point-to-point
    // simulation.
    let pair = |name: &str| -> Option<(f64, f64)> {
        args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).and_then(|v| {
            let mut it = v.split(',').map(|p| p.trim().parse::<f64>());
            match (it.next(), it.next()) {
                (Some(Ok(x)), Some(Ok(y))) => Some((x, y)),
                _ => None,
            }
        })
    };
    let bias = pair("--bias").unwrap_or((0.0, 0.0));
    let goto: Option<(f64, f64)> = args.iter().position(|a| a == "--goto").and_then(|i| args.get(i + 1)).and_then(|v| {
        let mut it = v.split(',').map(|p| p.trim().parse::<f64>());
        match (it.next(), it.next()) {
            (Some(Ok(x)), Some(Ok(y))) => Some((x, y)),
            _ => None,
        }
    });
    fs::create_dir_all(&out_dir)?;
    let world_text = fs::read_to_string(&world_path)?;
    println!("run  seed  outcome            legs  refus  served  cover%  kitchen  bath  corrN  corrS  west  east  path_m  bumps  kidnaps  stay_max  stays5  end_s");
    for k in 0..runs {
        let seed = seed0 + k;
        let mut world: World = serde_json::from_str(&world_text)?;
        let holes = world.holes.clone();
        // Things thinner than six centimetres (the dock's panel and strips
        // under the duck) or smaller than ten (balls and toys, parked at the
        // origin in the model) the duck steps over or kicks aside; a
        // circle-and-box model would stand on them for ever. Walls are 12 cm
        // and stay.
        world
            .boxes
            .retain(|(_, x0, x1, y0, y1)| {
                (x1 - x0).min(y1 - y0) >= 0.06 && (x1 - x0).max(y1 - y0) >= 0.10
            });
        let log = fs::File::create(out_dir.join(format!("run{seed:03}.log")))?;
        let mut twin = PaperTwin::new(world, seed, noise, kidnap, log);
        twin.bias = bias;
        let handle = ExploreHandle::new();
        // `--known`: no mapping — the world is the map, frozen; the
        // budget then only seeds each run's starting point (a short
        // guarded walk, so the journeys do not all begin on one spot).
        let known = args.iter().any(|a| a == "--known");
        let (state, reason, mut status) = if known && goto.is_some() {
            let mut job = Job::new(Vec::new(), budget.min(60.0), false, -1.0, twin.clock);
            let (state, reason) = job.run(&handle, &mut twin);
            twin.know_the_world();
            (state, reason, handle.status())
        } else {
            let mut job = Job::new(Vec::new(), budget, false, -1.0, twin.clock);
            let (state, reason) = job.run(&handle, &mut twin);
            (state, reason, handle.status())
        };
        // The point-to-point leg: plan on the map just built, walk it, and
        // keep both routes for the picture.
        let mut goto_out = json!(null);
        if let Some(goal) = goto {
            // `--start x,y`: the journey begins there, whatever the mapping
            // left (a spot the mapping rarely ends on — beside the hole).
            if let Some((sx, sy)) = pair("--start") {
                twin.x = sx;
                twin.y = sy;
            }
            let start = (twin.x, twin.y);
            let mapped_at = twin.elapsed().as_secs_f64();
            let planned: Vec<(f64, f64)> = twin
                .frame_now()
                .grid()
                .ok()
                .and_then(|g| quack_nav::frontier::path_to(&g, twin.x + twin.err.0, twin.y + twin.err.1, goal, &[], quack_nav::frontier::INFLATE_M, &[]))
                .unwrap_or_default();
            let handle2 = ExploreHandle::new();
            // `--books`: the journey starts with every hole's rim on the
            // books, as a saved map with its ground book would have it.
            let books: Vec<(f64, f64)> = if args.iter().any(|a| a == "--books") {
                holes
                    .iter()
                    .flat_map(|h| {
                        // `holes` is [x0, x1, y0, y1] (see `in_hole`).
                        let (x0, x1, y0, y1) = (h[0].min(h[1]), h[0].max(h[1]), h[2].min(h[3]), h[2].max(h[3]));
                        let mut pts = Vec::new();
                        let mut x = x0;
                        while x <= x1 + 1e-9 {
                            pts.push((x, y0));
                            pts.push((x, y1));
                            x += 0.1;
                        }
                        let mut y = y0 + 0.1;
                        while y < y1 {
                            pts.push((x0, y));
                            pts.push((x1, y));
                            y += 0.1;
                        }
                        pts
                    })
                    .collect()
            } else {
                Vec::new()
            };
            let mut goto_job = Job::to_goal(goal, 900.0, -1.0, twin.clock).with_books(books);
            let before = twin.path_m;
            let t0 = twin.elapsed().as_secs_f64();
            let (gstate, greason) = goto_job.run(&handle2, &mut twin);
            let gs = handle2.status();
            let straight = ((goal.0 - start.0).powi(2) + (goal.1 - start.1).powi(2)).sqrt();
            let planned_m = planned.windows(2).map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt()).sum::<f64>();
            let walked = twin.path_m - before;
            let err = ((twin.x - goal.0).powi(2) + (twin.y - goal.1).powi(2)).sqrt();
            eprintln!(
                "goto seed {seed}: {gstate:?} — {greason}; from ({:.2}, {:.2}) to ({:.2}, {:.2}): straight {straight:.2} m, planned {planned_m:.2} m, walked {walked:.2} m in {:.0} s, {} legs, {} refusals, ended {err:.2} m away{}",
                start.0, start.1, goal.0, goal.1, twin.elapsed().as_secs_f64() - t0, gs.legs, gs.refusals,
                if twin.fell { ", FELL" } else { "" }
            );
            goto_out = json!({"goal": [goal.0, goal.1], "start": [start.0, start.1], "mapped_s": mapped_at,
                              "planned": planned, "planned_m": planned_m, "walked_m": walked, "straight_m": straight,
                              "secs": twin.elapsed().as_secs_f64() - t0, "legs": gs.legs, "refusals": gs.refusals,
                              "error_m": err, "fell": twin.fell, "state": format!("{gstate:?}"), "reason": greason});
            status = gs;
        }
        let cov = twin.coverage();
        let total: f64 = twin.cells.iter().filter(|c| **c == 1).count() as f64 * CELL * CELL / 62.22;
        let frame = twin.frame_now();
        fs::write(
            out_dir.join(format!("run{seed:03}.frame.json")),
            serde_json::to_string(&json!({"frame": frame, "outcome": format!("{state:?}: {reason}"), "local": status.local, "truth": [twin.x, twin.y, twin.yaw], "goto": goto_out}))?,
        )?;
        let by: HashMap<_, _> = cov.into_iter().collect();
        let pct = |n: &str| by.get(n).copied().unwrap_or(0.0) * 100.0;
        let (stay_max, stays5) = twin.dwell();
        println!(
            "{k:>3}  {seed:>4}  {:<18} {:>4}  {:>5}  {:>6}  {:>5.1}  {:>7.0}  {:>4.0}  {:>5.0}  {:>5.0}  {:>4.0}  {:>4.0}  {:>6.1}  {:>5}  {:>7}  {stay_max:>8.1}  {stays5:>6}  {:>5.0}",
            format!("{state:?}{}", if twin.fell { " FELL" } else { "" }),
            status.legs,
            status.refusals,
            status.visited,
            total * 100.0,
            pct("kitchen"), pct("bath"), pct("corridor_n"), pct("corridor_s"), pct("west"), pct("east"),
            twin.path_m,
            twin.bumps,
            twin.kidnaps,
            twin.elapsed().as_secs_f64(),
        );
    }
    Ok(())
}

/// Turning from a standstill past the gait's dead zone, as measured on the
/// MuJoCo twin (scripts/twin/turnprobe.py, 2026-09-23, fork and
/// daemon-v0.14.4 alike): nothing below it, 30°/s at +1.2, ~51°/s at +1.5,
/// ~58°/s at −1.5, while −1.2 barely turns (5°/s) — the right side's
/// threshold is higher.
fn in_place_rate(vyaw: f64) -> Option<f64> {
    match vyaw {
        v if v >= 1.45 => Some(51f64.to_radians()),
        v if v >= 1.15 => Some(30f64.to_radians()),
        v if v <= -1.45 => Some(-58f64.to_radians()),
        _ => None,
    }
}
