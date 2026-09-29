//! Close looks (the user's, 2026-09-29): a wall the map holds but the body
//! never came near has been drawn from afar, and afar is where the map is
//! worst — casa_arredata's and house2's bathrooms, their far walls seen
//! only from 1.6-2.2 m, drawn 15-35 cm off and doubled, the pose right all
//! along. When the frontiers are done the explorer goes and looks at such
//! walls from near: a stand [`LOOK_FROM_M`] off, facing them, the mapper's
//! still window drawing them again from where it draws well.

use super::*;

/// A wall cell no point of the trail came within this of was seen from
/// afar only.
const FAR_ONLY_M: f64 = 1.3;
/// The stand for a close look: this far from the wall's cells.
const LOOK_FROM_M: f64 = 0.8;
/// Far-only wall cells within this of one another are one wall to look at.
const LOOK_GROUP_M: f64 = 0.5;
/// A group this small is not worth the walk.
const LOOK_MIN_CELLS: usize = 6;
/// Close looks in one job at most.
const LOOKS_MAX: u32 = 12;
/// A place looked at (or given up on) is not looked at again within this.
const LOOKED_M: f64 = 0.8;
/// The stand at the close look: a still window and the head's sweep.
const LOOK_STAND_S: f64 = 6.0;

/// While frontiers remain, a close look at most this often: the sessions
/// end on their budget with frontiers left (every MuJoCo session but one),
/// so waiting for the frontiers to be done is waiting for ever.
pub(super) const CLOSE_LOOK_EVERY_S: f64 = 180.0;

/// `QK_CLOSE_LOOK=0`: no close looks.
pub(super) fn close_look_on() -> bool {
    switch("QK_CLOSE_LOOK").unwrap_or(true)
}

impl Job {
    /// A wall seen from afar only, and where to look at it from: the stand
    /// and the wall's centre. The densest group of far-only wall cells; the
    /// stand the reachable free point [`LOOK_FROM_M`] from its centre, in
    /// sight of it, off the drops, nearest by route.
    fn far_wall_look(&self, grid: &Grid, from: (f64, f64)) -> Option<((f64, f64), (f64, f64))> {
        if self.close_looks >= LOOKS_MAX {
            return None;
        }
        let drops: Vec<(f64, f64)> = self.local.iter().filter(|(_, r)| *r >= DROP_RADIUS_M).map(|(p, _)| *p).collect();
        let mut far: Vec<(f64, f64)> = Vec::new();
        for r in 0..grid.rows {
            for c in 0..grid.cols {
                if grid.cells[r * grid.cols + c] != Cell::Wall {
                    continue;
                }
                let p = (grid.x_min + (c as f64 + 0.5) * grid.cell_m, grid.y_min + (r as f64 + 0.5) * grid.cell_m);
                if self.looked.iter().any(|q| dist2(*q, p) < LOOKED_M) || drops.iter().any(|d| dist2(*d, p) < 0.3) {
                    continue;
                }
                if self.trail.iter().all(|t| dist2(*t, p) > FAR_ONLY_M) {
                    far.push(p);
                }
            }
        }
        if far.len() < LOOK_MIN_CELLS {
            return None;
        }
        let (centre, n) = far
            .iter()
            .map(|p| {
                let near: Vec<&(f64, f64)> = far.iter().filter(|q| dist2(**q, *p) <= LOOK_GROUP_M).collect();
                let k = near.len() as f64;
                ((near.iter().map(|q| q.0).sum::<f64>() / k, near.iter().map(|q| q.1).sum::<f64>() / k), near.len())
            })
            .max_by_key(|(_, n)| *n)?;
        if n < LOOK_MIN_CELLS {
            return None;
        }
        let walls = self.planner_walls();
        let lanes = self.lanes();
        (0..16)
            .filter_map(|k| {
                let a = k as f64 * std::f64::consts::TAU / 16.0;
                let stand = (centre.0 + LOOK_FROM_M * a.cos(), centre.1 + LOOK_FROM_M * a.sin());
                let (c, r) = (((stand.0 - grid.x_min) / grid.cell_m) as isize, ((stand.1 - grid.y_min) / grid.cell_m) as isize);
                if c < 0 || r < 0 || c as usize >= grid.cols || r as usize >= grid.rows || grid.cells[r as usize * grid.cols + c as usize] != Cell::Free {
                    return None;
                }
                if drops.iter().any(|d| dist2(*d, stand) < 2.0 * FRONTIER_OFF_DROP_M) {
                    return None;
                }
                let facing = (centre.1 - stand.1).atan2(centre.0 - stand.0);
                if !grid.lane_clear(stand.0, stand.1, facing, LOOK_FROM_M - 0.15, 0.05) {
                    return None;
                }
                let path = path_to(grid, from.0, from.1, stand, &walls, inflate_m(), &lanes)?;
                Some((path.len(), stand))
            })
            .min_by_key(|(len, _)| *len)
            .map(|(_, stand)| (stand, centre))
    }

    /// One close look, if any is left (see the module): `None` nothing to
    /// look at; `Some(None)` looked (or tried), the explorer goes on;
    /// `Some(Some(verdict))` the job ends (stopped).
    pub(super) fn close_look(&mut self, handle: &ExploreHandle, robot: &mut dyn Body, grid: &Grid, from: (f64, f64)) -> Option<Option<(State, String)>> {
        if !close_look_on() {
            return None;
        }
        let (spot, at) = self.far_wall_look(grid, from)?;
        self.close_looks += 1;
        self.looked.push(at);
        tracing::info!(stand = ?spot, wall = ?at, n = self.close_looks, "map explore: a wall seen from afar only; going to look at it from near");
        let deadline = robot.now() + travel_budget(dist2(from, spot) * 1.5);
        match self.travel(handle, robot, spot, deadline, false) {
            (State::Stopped, why) => return Some(Some((State::Stopped, why))),
            (State::Done, _) => {}
            (_, why) => {
                tracing::info!(why, "map explore: the close look's way failed; given up");
                return Some(None);
            }
        }
        if let Some(f) = robot.frame() {
            let err = wrap((at.1 - f.y).atan2(at.0 - f.x) - f.yaw);
            if err.abs() > 0.2 {
                let _ = self.stick_turn(robot, err.signum(), err.abs());
            }
        }
        let _ = stand(robot, LOOK_STAND_S);
        self.record_drops(robot);
        tracing::info!(wall = ?at, "map explore: close look done");
        Some(None)
    }
}
