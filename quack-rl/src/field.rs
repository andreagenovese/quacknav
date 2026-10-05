//! The true distance to the goal over the floor, from the world (not the
//! map): what the reward measures progress by, and what the expert walks
//! down. Two of them: the plain geodesic, and the expert's, dearer near
//! the holes' rims and the things that stand.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::world::World;

pub const FIELD_CELL: f64 = 0.05;

#[derive(Debug, Clone)]
pub struct Field {
    pub x0: f64,
    pub y0: f64,
    pub cols: usize,
    pub rows: usize,
    /// Metres (or the expert's weighted metres); `f64::INFINITY` where the
    /// body cannot stand or cannot reach.
    pub d: Vec<f64>,
}

const N8: [(isize, isize, f64); 8] = [
    (-1, 0, 1.0),
    (1, 0, 1.0),
    (0, -1, 1.0),
    (0, 1, 1.0),
    (-1, -1, std::f64::consts::SQRT_2),
    (-1, 1, std::f64::consts::SQRT_2),
    (1, -1, std::f64::consts::SQRT_2),
    (1, 1, std::f64::consts::SQRT_2),
];

impl Field {
    /// Dijkstra from `goal` over the cells a body of `body_r` can stand on
    /// (the movers left out); `weighted` prices the cells near a hole's rim
    /// and near what stands.
    pub fn build(world: &World, goal: (f64, f64), body_r: f64, weighted: bool) -> Self {
        Self::build_avoiding(world, goal, body_r, weighted, &[])
    }

    /// [`Field::build`], with patches the field keeps `body_r` and a
    /// step's reach away from (the expert's: the map's unseen floor, where
    /// the pilot's shield refuses to step).
    pub fn build_avoiding(world: &World, goal: (f64, f64), body_r: f64, weighted: bool, avoid: &[(f64, f64, f64)]) -> Self {
        let [bx0, bx1, by0, by1] = world.bounds;
        let cols = ((bx1 - bx0) / FIELD_CELL).ceil() as usize;
        let rows = ((by1 - by0) / FIELD_CELL).ceil() as usize;
        let n = rows * cols;
        let mut price = vec![f64::INFINITY; n];
        for r in 0..rows {
            for c in 0..cols {
                let (x, y) = (bx0 + (c as f64 + 0.5) * FIELD_CELL, by0 + (r as f64 + 0.5) * FIELD_CELL);
                if world.static_collides(x, y, body_r - 0.01) || x - body_r < bx0 || x + body_r > bx1 || y - body_r < by0 || y + body_r > by1 {
                    continue;
                }
                let hole = world.hole_clearance(x, y);
                if hole < 0.03 {
                    continue;
                }
                if avoid.iter().any(|(ax, ay, ar)| crate::dist((x, y), (*ax, *ay)) < ar + body_r + 0.1) && crate::dist((x, y), goal) > 0.3 {
                    continue;
                }
                let mut p = 1.0;
                if weighted {
                    if hole < 0.25 {
                        p += 6.0 * (0.25 - hole) / 0.25;
                    }
                    let near = world.static_clearance(x, y) - body_r;
                    if near < 0.12 {
                        p += 4.0 * (0.12 - near.max(0.0)) / 0.12;
                    }
                }
                price[r * cols + c] = p;
            }
        }
        let mut d = vec![f64::INFINITY; n];
        let mut f = Self { x0: bx0, y0: by0, cols, rows, d: Vec::new() };
        let Some(start) = f.nearest_open(&price, goal, 0.4) else {
            f.d = d;
            return f;
        };
        let mut heap = BinaryHeap::new();
        d[start] = 0.0;
        heap.push(Reverse((0u64, start)));
        while let Some(Reverse((k, i))) = heap.pop() {
            let here = k as f64 / 1e6;
            if here > d[i] + 1e-9 {
                continue;
            }
            let (r, c) = ((i / cols) as isize, (i % cols) as isize);
            for (dr, dc, len) in N8 {
                let (rr, cc) = (r + dr, c + dc);
                if rr < 0 || cc < 0 || rr >= rows as isize || cc >= cols as isize {
                    continue;
                }
                let j = rr as usize * cols + cc as usize;
                if !price[j].is_finite() {
                    continue;
                }
                // No corner cutting past a blocked cell.
                if dr != 0 && dc != 0 {
                    let a = r as usize * cols + cc as usize;
                    let b = rr as usize * cols + c as usize;
                    if !price[a].is_finite() || !price[b].is_finite() {
                        continue;
                    }
                }
                let nd = here + len * FIELD_CELL * 0.5 * (price[i] + price[j]);
                if nd + 1e-9 < d[j] {
                    d[j] = nd;
                    heap.push(Reverse(((nd * 1e6) as u64, j)));
                }
            }
        }
        f.d = d;
        f
    }

    fn idx(&self, x: f64, y: f64) -> Option<usize> {
        let c = ((x - self.x0) / FIELD_CELL).floor();
        let r = ((y - self.y0) / FIELD_CELL).floor();
        (c >= 0.0 && r >= 0.0 && (c as usize) < self.cols && (r as usize) < self.rows).then(|| r as usize * self.cols + c as usize)
    }

    fn centre(&self, i: usize) -> (f64, f64) {
        (self.x0 + ((i % self.cols) as f64 + 0.5) * FIELD_CELL, self.y0 + ((i / self.cols) as f64 + 0.5) * FIELD_CELL)
    }

    fn nearest_open(&self, open: &[f64], p: (f64, f64), reach: f64) -> Option<usize> {
        let k = (reach / FIELD_CELL).ceil() as isize;
        let (pc, pr) = (((p.0 - self.x0) / FIELD_CELL).floor() as isize, ((p.1 - self.y0) / FIELD_CELL).floor() as isize);
        let mut best: Option<(f64, usize)> = None;
        for dr in -k..=k {
            for dc in -k..=k {
                let (r, c) = (pr + dr, pc + dc);
                if r < 0 || c < 0 || r >= self.rows as isize || c >= self.cols as isize {
                    continue;
                }
                let i = r as usize * self.cols + c as usize;
                if !open[i].is_finite() {
                    continue;
                }
                let dd = crate::dist(self.centre(i), p);
                if dd <= reach && best.is_none_or(|b| dd < b.0) {
                    best = Some((dd, i));
                }
            }
        }
        best.map(|b| b.1)
    }

    /// The distance from (x, y): its cell's, or the nearest reached cell's
    /// within 0.3 m plus the way to it.
    pub fn at(&self, x: f64, y: f64) -> f64 {
        if let Some(i) = self.idx(x, y)
            && self.d[i].is_finite()
        {
            return self.d[i];
        }
        match self.nearest_open(&self.d, (x, y), 0.3) {
            Some(i) => self.d[i] + crate::dist(self.centre(i), (x, y)),
            None => f64::INFINITY,
        }
    }

    /// Down the field from (x, y), about `ahead` metres: where the expert
    /// looks.
    pub fn descend(&self, x: f64, y: f64, ahead: f64) -> Option<(f64, f64)> {
        let mut i = match self.idx(x, y).filter(|i| self.d[*i].is_finite()) {
            Some(i) => i,
            None => self.nearest_open(&self.d, (x, y), 0.3)?,
        };
        let mut walked = 0.0;
        while walked < ahead {
            let (r, c) = ((i / self.cols) as isize, (i % self.cols) as isize);
            let mut best = (self.d[i], i);
            for (dr, dc, _) in N8 {
                let (rr, cc) = (r + dr, c + dc);
                if rr < 0 || cc < 0 || rr >= self.rows as isize || cc >= self.cols as isize {
                    continue;
                }
                let j = rr as usize * self.cols + cc as usize;
                if self.d[j] < best.0 {
                    best = (self.d[j], j);
                }
            }
            if best.1 == i {
                break;
            }
            walked += crate::dist(self.centre(i), self.centre(best.1));
            i = best.1;
        }
        Some(self.centre(i))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{Hole, Rect};

    #[test]
    fn the_field_goes_round_a_wall() {
        let mut w = World { bounds: [0.0, 4.0, 0.0, 4.0], ..Default::default() };
        // A wall across the middle with a gap at the top.
        w.rects.push(Rect::new(1.9, 2.1, 0.0, 3.2));
        let f = Field::build(&w, (3.5, 0.5), 0.11, false);
        let d = f.at(0.5, 0.5);
        // Up to the gap (~3.4 high) and back down: well over the 3 m straight.
        assert!(d > 7.0 && d < 9.0, "{d}");
        let look = f.descend(0.5, 0.5, 0.5).unwrap();
        assert!(look.1 > 0.6, "{look:?}");
    }

    #[test]
    fn holes_are_not_floor() {
        let mut w = World { bounds: [0.0, 4.0, 0.0, 2.0], ..Default::default() };
        w.holes.push(Hole { x0: 1.5, x1: 2.5, y0: 0.0, y1: 1.5, booked: false });
        let f = Field::build(&w, (3.5, 0.5), 0.11, true);
        assert!(f.at(2.0, 0.5).is_infinite() || f.at(2.0, 0.5) > f.at(1.0, 0.5));
        assert!(f.at(0.5, 0.5).is_finite());
    }
}
