//! The round of a hole (the user's, 2026-09-29): the map draws a hole's
//! floor as free — the beams pass over it to the walls behind, and a 2-D
//! map knows no floor — so the drop books alone keep the planner off a
//! hole, and they hold only the rim the sensor happened to look at from
//! near (35-100 % of a stairwell's rim after four sessions; the falls of
//! 2026-09-29 were routes along the rest). A hole on the books is looked
//! at from its four sides: a stand [`LOOK_OUT_M`] beyond its outermost
//! booked point that way, facing it, the books written at the stand.

use super::*;

/// Booked drops this near one another are one hole.
const HOLE_GROUP_M: f64 = 0.4;
/// A hole of fewer booked points is not yet a hole to go round.
const HOLE_MIN_POINTS: usize = 3;
/// The stand: this far beyond the hole's outermost booked point on a side.
const LOOK_OUT_M: f64 = 0.6;
/// A hole already met: its centre within this of one on the list.
const SAME_HOLE_M: f64 = 0.6;
/// Looks at holes in one job at most.
const RIM_LOOKS_MAX: u32 = 16;
/// A look at a hole at most this often.
pub(super) const RIM_LOOK_EVERY_S: f64 = 60.0;
/// The stand at a hole: the still frames the books vote on, and the sweep.
const RIM_STAND_S: f64 = 6.0;

/// `QK_RIM_TOUR=0`: no rounds of the holes.
pub(super) fn rim_tour_on() -> bool {
    switch("QK_RIM_TOUR").unwrap_or(true)
}

impl Job {
    /// The holes on the books: their centres and points.
    fn holes(&self) -> Vec<((f64, f64), Vec<(f64, f64)>)> {
        let mut left: Vec<(f64, f64)> = self.local.iter().filter(|(_, r)| *r >= DROP_RADIUS_M).map(|(p, _)| *p).collect();
        let mut out = Vec::new();
        while let Some(seed) = left.pop() {
            let mut group = vec![seed];
            let mut i = 0;
            while i < group.len() {
                let p = group[i];
                let (near, far): (Vec<_>, Vec<_>) = left.into_iter().partition(|q| dist2(*q, p) <= HOLE_GROUP_M);
                group.extend(near);
                left = far;
                i += 1;
            }
            if group.len() >= HOLE_MIN_POINTS {
                let n = group.len() as f64;
                let c = (group.iter().map(|p| p.0).sum::<f64>() / n, group.iter().map(|p| p.1).sum::<f64>() / n);
                out.push((c, group));
            }
        }
        out
    }

    /// The next side of a hole to look from: the stand and the hole's
    /// centre. Sides with no free, reachable stand are marked done.
    fn next_rim_look(&mut self, grid: &Grid, from: (f64, f64)) -> Option<((f64, f64), (f64, f64))> {
        if self.rim_looks >= RIM_LOOKS_MAX {
            return None;
        }
        let walls = self.planner_walls();
        let lanes = self.lanes();
        for (centre, points) in self.holes() {
            let k = match self.rim_sides.iter().position(|(c, _)| dist2(*c, centre) < SAME_HOLE_M) {
                Some(k) => k,
                None => {
                    self.rim_sides.push((centre, 0));
                    self.rim_sides.len() - 1
                }
            };
            for side in 0..4u8 {
                if self.rim_sides[k].1 & (1 << side) != 0 {
                    continue;
                }
                self.rim_sides[k].1 |= 1 << side;
                let a = f64::from(side) * std::f64::consts::FRAC_PI_2;
                let (ux, uy) = (a.cos(), a.sin());
                let out = points.iter().map(|p| (p.0 - centre.0) * ux + (p.1 - centre.1) * uy).fold(f64::NEG_INFINITY, f64::max);
                let spot = (centre.0 + (out + LOOK_OUT_M) * ux, centre.1 + (out + LOOK_OUT_M) * uy);
                let (c, r) = (((spot.0 - grid.x_min) / grid.cell_m) as isize, ((spot.1 - grid.y_min) / grid.cell_m) as isize);
                if c < 0 || r < 0 || c as usize >= grid.cols || r as usize >= grid.rows || grid.cells[r as usize * grid.cols + c as usize] != Cell::Free {
                    continue;
                }
                if path_to(grid, from.0, from.1, spot, &walls, inflate_m(), &lanes).is_none() {
                    continue;
                }
                return Some((spot, centre));
            }
        }
        None
    }

    /// One look at a hole, if a side is left (see the module): `None`
    /// nothing to do; `Some(None)` looked (or tried); `Some(Some(verdict))`
    /// the job ends (stopped).
    pub(super) fn rim_look(&mut self, handle: &ExploreHandle, robot: &mut dyn Body, grid: &Grid, from: (f64, f64)) -> Option<Option<(State, String)>> {
        if !rim_tour_on() {
            return None;
        }
        let (spot, centre) = self.next_rim_look(grid, from)?;
        self.rim_looks += 1;
        let before = self.local.iter().filter(|(_, r)| *r >= DROP_RADIUS_M).count();
        tracing::info!(stand = ?spot, hole = ?centre, n = self.rim_looks, "map explore: round of a hole: looking at it from a side not yet seen");
        let deadline = robot.now() + travel_budget(dist2(from, spot) * 1.5);
        match self.travel(handle, robot, spot, deadline, false) {
            (State::Stopped, why) => return Some(Some((State::Stopped, why))),
            (State::Done, _) => {}
            (_, why) => {
                tracing::info!(why, "map explore: round of a hole: the way failed; given up");
                return Some(None);
            }
        }
        if let Some(f) = robot.frame() {
            let err = wrap((centre.1 - f.y).atan2(centre.0 - f.x) - f.yaw);
            if err.abs() > 0.2 {
                let _ = self.stick_turn(robot, err.signum(), err.abs());
            }
        }
        let _ = stand(robot, RIM_STAND_S);
        self.record_drops(robot);
        let after = self.local.iter().filter(|(_, r)| *r >= DROP_RADIUS_M).count();
        tracing::info!(hole = ?centre, booked = after.saturating_sub(before), "map explore: round of a hole: looked");
        Some(None)
    }
}
