//! Houses by the thousand. Each scenario is a piece of a house, a start, a
//! goal 2-6 m away, and what the map does not know on the way:
//!
//! - `clutter`: a room with furniture, and things on the floor the map has
//!   never seen — boxes, bags, chair legs, some too low for the sensor
//!   while walking;
//! - `doorway`: two rooms and a wall between, one or two doors 0.42-0.80 m,
//!   a door half closed by something put down since;
//! - `stairwell`: a corridor with a hole in it, passages of 0.42-0.70 m
//!   beside it, its rim on the books or not, the map a little off across
//!   the passage (house2's 0.16-0.19 m, MuJoCo 2026-09-20);
//! - `corners`: narrow corridors, 0.55-0.90 m, turning at right angles;
//! - `movers`: pets and feet crossing the way, wandering, stopping;
//! - `low`: beds and sofas beside the way, their floor rows reading
//!   phantom drops;
//! - `mixed`: a doorway, clutter, a hole and something moving at once.
//!
//! The levels are the curriculum: 0 the plain ones, 3 everything with the
//! map off and the sensor at its noisiest.

use serde::{Deserialize, Serialize};

use crate::field::Field;
use crate::world::{Hole, Mover, MoverKind, Post, Rect, World};
use crate::{Rng, dist};

pub const FAMILIES: [&str; 7] = ["clutter", "doorway", "stairwell", "corners", "movers", "low", "mixed"];
const WALL: f64 = 0.12;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scenario {
    pub family: String,
    pub level: u32,
    pub seed: u64,
    pub world: World,
    pub start: (f64, f64, f64),
    pub goal: (f64, f64),
    /// The map's frame against the world.
    pub bias: (f64, f64),
}

fn walls_around(w: &mut World) {
    let [x0, x1, y0, y1] = w.bounds;
    w.rects.push(Rect::new(x0 - WALL, x1 + WALL, y0 - WALL, y0));
    w.rects.push(Rect::new(x0 - WALL, x1 + WALL, y1, y1 + WALL));
    w.rects.push(Rect::new(x0 - WALL, x0, y0, y1));
    w.rects.push(Rect::new(x1, x1 + WALL, y0, y1));
}

/// A free spot, `clear` from anything standing and further from a hole.
fn free_spot(w: &World, rng: &mut Rng, area: [f64; 4], clear: f64) -> Option<(f64, f64)> {
    for _ in 0..200 {
        let p = (rng.range(area[0] + clear, area[1] - clear), rng.range(area[2] + clear, area[3] - clear));
        if !w.static_collides(p.0, p.1, clear) && w.hole_clearance(p.0, p.1) > clear + 0.25 {
            return Some(p);
        }
    }
    None
}

fn furniture(w: &mut World, rng: &mut Rng, area: [f64; 4], n: usize, low_p: f64) {
    for _ in 0..n {
        let (sx, sy) = (rng.range(0.3, 1.2), rng.range(0.3, 0.9));
        let (sx, sy) = if rng.chance(0.5) { (sx, sy) } else { (sy, sx) };
        let x = rng.range(area[0], area[1] - sx);
        let y = rng.range(area[2], area[3] - sy);
        let mut r = Rect::new(x, x + sx, y, y + sy);
        if rng.chance(low_p) {
            r.low = true;
            r.height = rng.range(0.3, 0.45);
        }
        w.rects.push(r);
    }
    // A table on four legs, now and then; its legs on the map or not.
    if rng.chance(0.4) {
        let (tx, ty) = (rng.range(area[0] + 0.2, area[1] - 1.0), rng.range(area[2] + 0.2, area[3] - 0.8));
        let (lx, ly) = (rng.range(0.5, 0.9), rng.range(0.4, 0.7));
        let mapped = rng.chance(0.5);
        for (dx, dy) in [(0.0, 0.0), (lx, 0.0), (0.0, ly), (lx, ly)] {
            w.posts.push(Post { x: tx + dx, y: ty + dy, r: rng.range(0.015, 0.03), height: 0.7, mapped });
        }
    }
}

/// Things put down since the map was made, near the straight way from
/// `a` to `b`: boxes, bags, a chair's legs; some under the sensor's floor
/// threshold while walking.
fn unmapped_on_the_way(w: &mut World, rng: &mut Rng, a: (f64, f64), b: (f64, f64), n: usize) {
    for _ in 0..n {
        let t = rng.range(0.25, 0.8);
        let (mx, my) = (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1));
        let off = rng.range(-0.35, 0.35);
        let len = dist(a, b).max(1e-6);
        let (nx, ny) = (-(b.1 - a.1) / len, (b.0 - a.0) / len);
        let (cx, cy) = (mx + off * nx, my + off * ny);
        if dist((cx, cy), a) < 0.45 || dist((cx, cy), b) < 0.45 {
            continue;
        }
        let height = if rng.chance(0.2) { rng.range(0.04, 0.085) } else { rng.range(0.1, 0.6) };
        if rng.chance(0.25) {
            // A chair: four thin legs.
            let s = rng.range(0.35, 0.45);
            for (dx, dy) in [(0.0, 0.0), (s, 0.0), (0.0, s), (s, s)] {
                w.posts.push(Post { x: cx - s / 2.0 + dx, y: cy - s / 2.0 + dy, r: 0.015, height: 0.45, mapped: false });
            }
        } else {
            let (sx, sy) = (rng.range(0.08, 0.4), rng.range(0.08, 0.4));
            let mut r = Rect::new(cx - sx / 2.0, cx + sx / 2.0, cy - sy / 2.0, cy + sy / 2.0);
            r.mapped = false;
            r.height = height;
            w.rects.push(r);
        }
    }
}

fn movers(w: &mut World, rng: &mut Rng, a: (f64, f64), b: (f64, f64), n: usize) {
    for _ in 0..n {
        let kind = [MoverKind::Crosser, MoverKind::Wanderer, MoverKind::StopGo][rng.pick(3)];
        let t = rng.range(0.3, 0.7);
        let (mx, my) = (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1));
        let along = (b.1 - a.1).atan2(b.0 - a.0);
        let (r, speed) = match kind {
            MoverKind::StopGo => (rng.range(0.12, 0.2), rng.range(0.25, 0.7)),
            _ => (rng.range(0.07, 0.18), rng.range(0.12, 0.5)),
        };
        // A crosser starts off to one side and walks across the way.
        let side = if rng.chance(0.5) { 1.0 } else { -1.0 };
        let off = if kind == MoverKind::Crosser { rng.range(0.6, 1.4) } else { rng.range(0.3, 1.0) };
        let (x, y) = (mx - side * off * along.sin(), my + side * off * along.cos());
        let heading = if kind == MoverKind::Crosser { along - side * std::f64::consts::FRAC_PI_2 + rng.range(-0.3, 0.3) } else { rng.range(-3.14, 3.14) };
        if w.static_collides(x, y, r) || w.in_hole(x, y) || dist((x, y), a) < 0.5 {
            continue;
        }
        let [bx0, bx1, by0, by1] = w.bounds;
        if x - r < bx0 || x + r > bx1 || y - r < by0 || y + r > by1 {
            continue;
        }
        w.movers.push(Mover { x, y, r, heading, speed, kind, timer: rng.range(0.5, 3.0), still: kind == MoverKind::StopGo && rng.chance(0.5) });
    }
}

fn room(rng: &mut Rng) -> World {
    let (wd, ht) = (rng.range(3.0, 6.0), rng.range(2.6, 5.0));
    let mut w = World { bounds: [0.0, wd, 0.0, ht], ..Default::default() };
    walls_around(&mut w);
    w
}

fn two_rooms(rng: &mut Rng, unmapped_door: bool) -> (World, [f64; 4], [f64; 4]) {
    let (wd, ht) = (rng.range(4.0, 7.0), rng.range(2.5, 4.5));
    let mut w = World { bounds: [0.0, wd, 0.0, ht], ..Default::default() };
    walls_around(&mut w);
    let xm = rng.range(wd * 0.35, wd * 0.65);
    let doors = if rng.chance(0.35) { 2 } else { 1 };
    let mut cuts: Vec<(f64, f64)> = Vec::new();
    for k in 0..doors {
        let width = rng.range(0.42, 0.8);
        let lo = if doors == 1 { rng.range(0.2, ht - width - 0.2) } else if k == 0 { rng.range(0.2, ht / 2.0 - width - 0.1) } else { rng.range(ht / 2.0 + 0.1, ht - width - 0.2) };
        cuts.push((lo, lo + width));
    }
    let mut y = 0.0;
    for (lo, hi) in &cuts {
        if *lo > y {
            w.rects.push(Rect::new(xm - WALL / 2.0, xm + WALL / 2.0, y, *lo));
        }
        y = *hi;
    }
    w.rects.push(Rect::new(xm - WALL / 2.0, xm + WALL / 2.0, y, ht));
    if unmapped_door {
        // Something put down in a doorway since: half closing it, or all
        // of it when there is another door.
        let (lo, hi) = cuts[rng.pick(cuts.len())];
        let closed = doors == 2 && rng.chance(0.4);
        let keep = if closed { 0.0 } else { rng.range(0.0, (hi - lo - 0.25).max(0.0)) };
        let (b0, b1) = if rng.chance(0.5) { (lo, hi - keep) } else { (lo + keep, hi) };
        if b1 - b0 > 0.05 {
            let mut r = Rect::new(xm - rng.range(0.1, 0.3), xm + rng.range(0.1, 0.3), b0, b1);
            r.mapped = false;
            r.height = rng.range(0.15, 0.6);
            w.rects.push(r);
        }
    }
    let left = [0.0, xm - WALL, 0.0, ht];
    let right = [xm + WALL, wd, 0.0, ht];
    (w, left, right)
}

fn stairwell(rng: &mut Rng, booked_p: f64, bias_p: f64) -> (World, (f64, f64), (f64, f64), (f64, f64)) {
    let len = rng.range(3.0, 6.0);
    let p1 = rng.range(0.42, 0.7);
    let p2 = if rng.chance(0.4) { 0.0 } else { rng.range(0.42, 0.7) };
    let hw = rng.range(0.3, 0.6);
    let width = p1 + hw + p2;
    let mut w = World { bounds: [0.0, len, 0.0, width], ..Default::default() };
    walls_around(&mut w);
    let hx = rng.range(1.0, len - 1.5);
    let hl = rng.range(0.4, 0.9);
    w.holes.push(Hole { x0: hx, x1: hx + hl, y0: p2, y1: p2 + hw, booked: rng.chance(booked_p) });
    let start = (rng.range(0.3, 0.7), rng.range(0.2, width - 0.2));
    let goal = (rng.range(len - 0.7, len - 0.3), rng.range(0.2, width - 0.2));
    let bias = if rng.chance(bias_p) { (rng.range(-0.05, 0.05), rng.range(-0.2, 0.2)) } else { (0.0, 0.0) };
    (w, start, goal, bias)
}

fn corners(rng: &mut Rng) -> (World, (f64, f64), (f64, f64)) {
    // An L of corridors, or a U: a block in the middle of a room leaves
    // the corridors round it.
    let cw = rng.range(0.55, 0.9);
    let (wd, ht) = (rng.range(2.5, 5.0), rng.range(2.5, 4.0));
    let mut w = World { bounds: [0.0, wd, 0.0, ht], ..Default::default() };
    walls_around(&mut w);
    let u = rng.chance(0.5);
    if u {
        w.rects.push(Rect::new(cw, wd - cw, 0.0, ht - cw));
    } else {
        w.rects.push(Rect::new(cw, wd, 0.0, ht - cw));
    }
    let start = (cw / 2.0, rng.range(0.3, 1.0));
    let goal = if u { (wd - cw / 2.0, rng.range(0.3, 1.0)) } else { (wd - rng.range(0.3, 0.8), ht - cw / 2.0) };
    (w, start, goal)
}

/// Whether the map's planner finds a way and the truth has one.
fn passable(s: &Scenario) -> bool {
    let truth = Field::build(&s.world, s.goal, 0.11, false);
    if !truth.at(s.start.0, s.start.1).is_finite() {
        return false;
    }
    let (_, grid) = s.world.raster(s.bias);
    let books: Vec<quack_nav::frontier::ExtraWall> = s.world.books(s.bias).into_iter().map(|p| (p, 0.10)).collect();
    let (sx, sy) = (s.start.0 + s.bias.0, s.start.1 + s.bias.1);
    quack_nav::frontier::path_to(&grid, sx, sy, (s.goal.0 + s.bias.0, s.goal.1 + s.bias.1), &books, 0.12, &[]).is_some()
}

/// A scenario of `family` (or one the level allows, at random) at `level`.
pub fn generate(seed: u64, level: u32, family: Option<&str>) -> Scenario {
    let mut rng = Rng::new(seed ^ 0x5EED_CAFE);
    for attempt in 0..50 {
        let fam = family.map(str::to_string).unwrap_or_else(|| {
            let allowed: &[&str] = match level {
                0 => &["clutter", "doorway", "corners"],
                1 => &["clutter", "doorway", "corners", "stairwell"],
                2 => &["clutter", "doorway", "corners", "stairwell", "movers", "low"],
                _ => &FAMILIES,
            };
            allowed[rng.pick(allowed.len())].to_string()
        });
        let unmapped = if level == 0 { 0 } else { rng.pick(4 + level as usize) };
        let bias_p = match level {
            0 | 1 => 0.0,
            2 => 0.3,
            _ => 0.5,
        };
        let mut bias = (0.0, 0.0);
        let (mut world, start, goal) = match fam.as_str() {
            "doorway" => {
                let half = level >= 1 && rng.chance(0.5);
                let (w, l, r) = two_rooms(&mut rng, half);
                let (Some(a), Some(b)) = (free_spot(&w, &mut rng, l, 0.3), free_spot(&w, &mut rng, r, 0.3)) else { continue };
                let (a, b) = if rng.chance(0.5) { (a, b) } else { (b, a) };
                let mut w = w;
                if level >= 1 {
                    unmapped_on_the_way(&mut w, &mut rng, a, b, unmapped / 2);
                }
                (w, a, b)
            }
            "stairwell" => {
                let (w, a, b, bi) = stairwell(&mut rng, if level <= 1 { 1.0 } else { 0.7 }, bias_p);
                bias = bi;
                let (a, b) = if rng.chance(0.5) { (a, b) } else { (b, a) };
                (w, a, b)
            }
            "corners" => {
                let (mut w, a, b) = corners(&mut rng);
                if level >= 1 && rng.chance(0.4) {
                    unmapped_on_the_way(&mut w, &mut rng, a, b, 1);
                }
                let (a, b) = if rng.chance(0.5) { (a, b) } else { (b, a) };
                (w, a, b)
            }
            "movers" => {
                let mut w = room(&mut rng);
                let area = w.bounds;
                let n = rng.pick(3);
                furniture(&mut w, &mut rng, area, n, 0.2);
                let (Some(a), Some(b)) = (free_spot(&w, &mut rng, area, 0.3), free_spot(&w, &mut rng, area, 0.3)) else { continue };
                let n = 1 + rng.pick(3);
                movers(&mut w, &mut rng, a, b, n);
                (w, a, b)
            }
            "low" => {
                let mut w = room(&mut rng);
                let area = w.bounds;
                let n = 2 + rng.pick(3);
                furniture(&mut w, &mut rng, area, n, 0.9);
                let (Some(a), Some(b)) = (free_spot(&w, &mut rng, area, 0.3), free_spot(&w, &mut rng, area, 0.3)) else { continue };
                (w, a, b)
            }
            "mixed" => {
                let half = rng.chance(0.5);
                let (mut w, l, r) = two_rooms(&mut rng, half);
                let n = rng.pick(3);
                furniture(&mut w, &mut rng, l, n, 0.4);
                let n = rng.pick(3);
                furniture(&mut w, &mut rng, r, n, 0.4);
                if rng.chance(0.5) {
                    // A stairwell-like hole in one room.
                    let room = if rng.chance(0.5) { l } else { r };
                    let (hw, hl) = (rng.range(0.3, 0.6), rng.range(0.4, 0.8));
                    let hx = rng.range(room[0] + 0.5, (room[1] - hl - 0.5).max(room[0] + 0.5));
                    let hy = rng.range(room[2] + 0.45, (room[3] - hw - 0.45).max(room[2] + 0.45));
                    w.holes.push(Hole { x0: hx, x1: hx + hl, y0: hy, y1: hy + hw, booked: rng.chance(0.7) });
                }
                let (Some(a), Some(b)) = (free_spot(&w, &mut rng, l, 0.3), free_spot(&w, &mut rng, r, 0.3)) else { continue };
                let (a, b) = if rng.chance(0.5) { (a, b) } else { (b, a) };
                unmapped_on_the_way(&mut w, &mut rng, a, b, unmapped / 2);
                let n = rng.pick(3);
                movers(&mut w, &mut rng, a, b, n);
                if rng.chance(bias_p) {
                    bias = (rng.range(-0.12, 0.12), rng.range(-0.12, 0.12));
                }
                (w, a, b)
            }
            _ => {
                let mut w = room(&mut rng);
                let area = w.bounds;
                let n = 1 + rng.pick(5);
                furniture(&mut w, &mut rng, area, n, 0.3);
                let (Some(a), Some(b)) = (free_spot(&w, &mut rng, area, 0.3), free_spot(&w, &mut rng, area, 0.3)) else { continue };
                unmapped_on_the_way(&mut w, &mut rng, a, b, unmapped);
                if rng.chance(bias_p) {
                    bias = (rng.range(-0.1, 0.1), rng.range(-0.1, 0.1));
                }
                (w, a, b)
            }
        };
        // Furniture moved since the map: on the map where it was, in the
        // house a little further on.
        if level >= 2 && rng.chance(0.3) {
            let movable: Vec<usize> = (4..world.rects.len()).filter(|i| world.rects[*i].mapped && world.rects[*i].height < 1.0).collect();
            if !movable.is_empty() {
                let i = movable[rng.pick(movable.len())];
                let mut moved = world.rects[i].clone();
                world.rects[i].present = false;
                let (dx, dy) = (rng.range(-0.3, 0.3), rng.range(-0.3, 0.3));
                moved.x0 += dx;
                moved.x1 += dx;
                moved.y0 += dy;
                moved.y1 += dy;
                moved.mapped = false;
                world.rects.push(moved);
            }
        }
        // The map's own way of drawing: thin-inked furniture with unknown
        // insides most of the time, and patches of floor never seen.
        world.style.band_m = if rng.chance(0.7) { Some(rng.range(0.05, 0.15)) } else { None };
        if level >= 1 {
            for _ in 0..rng.pick(4) {
                let [bx0, bx1, by0, by1] = world.bounds;
                let (x, y, r) = (rng.range(bx0, bx1), rng.range(by0, by1), rng.range(0.05, 0.2));
                if dist((x, y), start) > r + 0.35 && dist((x, y), goal) > r + 0.35 {
                    world.style.blobs.push((x, y, r));
                }
            }
        }
        if dist(start, goal) < 1.8 || world.static_collides(start.0, start.1, 0.13) || world.static_collides(goal.0, goal.1, 0.13) || world.mover_collides(start.0, start.1, 0.2).is_some() {
            continue;
        }
        let yaw = (goal.1 - start.1).atan2(goal.0 - start.0) + rng.range(-1.6, 1.6);
        let s = Scenario { family: fam, level, seed, world, start: (start.0, start.1, yaw), goal, bias };
        if passable(&s) {
            return s;
        }
        let _ = attempt;
    }
    // Never in practice; an empty room as the last word.
    let mut w = World { bounds: [0.0, 4.0, 0.0, 3.0], ..Default::default() };
    walls_around(&mut w);
    Scenario { family: "clutter".into(), level, seed, world: w, start: (0.5, 0.5, 0.0), goal: (3.5, 2.5), bias: (0.0, 0.0) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_family_generates_passable_scenarios() {
        for fam in FAMILIES {
            for seed in 0..15 {
                let s = generate(seed, 3, Some(fam));
                assert_eq!(s.family, fam, "seed {seed}");
                assert!(passable(&s), "{fam} seed {seed}");
                assert!(dist((s.start.0, s.start.1), s.goal) >= 1.8);
            }
        }
    }

    #[test]
    fn generation_is_deterministic() {
        let a = serde_json::to_string(&generate(7, 2, None)).unwrap();
        let b = serde_json::to_string(&generate(7, 2, None)).unwrap();
        assert_eq!(a, b);
    }
}
