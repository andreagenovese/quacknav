//! A local planner (ADR 0009): the dynamic window, on the gait we have.
//!
//! The guard judges a blind leg along a straight lane ahead of the nose,
//! whatever the leg's arc: in house2's passage east of the stairwell the
//! jamb 0.36 m off at 15° refused every leg, 29 in a row, until the goal's
//! time ran out (MuJoCo, 2026-09-26) — legs that curved away from it
//! included. Here each candidate arc is rolled out through the gait's own
//! model (forward [`GAIT_M_PER_S`], 0.65 of the yaw asked) and judged as it
//! would be walked: the body's centre and nose against what the sensor
//! sees now, the map's walls and the books' drops; the admissible arc that
//! best closes on a point of the route ahead is walked. `QK_LOCAL=dwa`;
//! off until MuJoCo says so.

use super::*;
use crate::passage::BODY_HALF_M;

/// `QK_LOCAL=dwa`: when the guard refuses the route's leg for something
/// the sensor sees, the local planner's leg instead.
pub(super) fn local_planner() -> bool {
    std::env::var("QK_LOCAL").is_ok_and(|v| v == "dwa")
}

/// The yaw rates tried, as the leg asks them (the gait turns 0.65 of it).
const ARC_VYAW: [f64; 9] = [-0.7, -0.52, -0.35, -0.17, 0.0, 0.17, 0.35, 0.52, 0.7];
/// The walks tried, seconds.
const ARC_S: [f64; 3] = [1.0, 1.5, 2.0];
/// Nearest a sensed point may come to the body's centre or nose along
/// the arc: half a body's width and a margin for the pitching trunk.
const SENSED_CLEAR_M: f64 = 0.12;
/// Nearest a mapped wall cell may come to the body's centre.
const WALL_CLEAR_M: f64 = 0.12;
/// Nearest the body's centre may come to a booked drop's rim: half a
/// body and a margin. The first MuJoCo rounds (house2, 2026-09-27) judged
/// arcs with the step back's margin, 0.05 m past the rim point's radius,
/// and one took the body along the stairwell's east side 0.05 m from the
/// rim; the next leg put it on the corner and it fell.
const DROP_CLEAR_M: f64 = BODY_HALF_M + 0.05;
/// How far along the route the arc aims.
const LOOK_M: f64 = 0.6;
/// What the sensor saw within this long counts.
const SENSED_WITHIN: Duration = Duration::from_millis(1500);
/// ... and within this range of the body.
const SENSED_RANGE_M: f64 = 1.2;

/// A leg the local planner chose.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ArcLeg {
    pub vyaw: f64,
    pub walk_s: f64,
    /// Metres closed on the look-ahead point.
    pub progress_m: f64,
    /// Nearest approach to anything along the arc, metres.
    pub clear_m: f64,
}

/// The route's point [`LOOK_M`] ahead of `at`: the first farther than
/// that, else the route's end.
pub(super) fn look_ahead(path: &[(f64, f64)], at: (f64, f64)) -> Option<(f64, f64)> {
    path.iter().copied().find(|p| dist2(*p, at) >= LOOK_M).or_else(|| path.last().copied())
}

/// The best admissible arc from `pose` toward `look`, or none. Pure: the
/// sensed points (world frame), the drops on the books and the grid are
/// what it knows.
pub(super) fn best_arc(grid: &Grid, drops: &[((f64, f64), f64)], sensed: &[(f64, f64)], pose: (f64, f64, f64), look: (f64, f64)) -> Option<ArcLeg> {
    let wall_cells = walls_near(grid, (pose.0, pose.1), GAIT_M_PER_S * 2.5 + WALL_CLEAR_M + 0.2);
    let mut best: Option<(f64, ArcLeg)> = None;
    for &vyaw in &ARC_VYAW {
        for &secs in &ARC_S {
            let (v, w) = (GAIT_M_PER_S, 0.65 * vyaw);
            let (mut px, mut py, mut h) = pose;
            let mut clear = f64::INFINITY;
            let mut ok = true;
            let mut t = 0.0;
            while t < secs - 1e-9 && ok {
                px += v * 0.1 * h.cos();
                py += v * 0.1 * h.sin();
                h += w * 0.1;
                t += 0.1;
                let nose = (px + BODY_HALF_M * h.cos(), py + BODY_HALF_M * h.sin());
                for p in sensed {
                    let d = dist2(*p, (px, py)).min(dist2(*p, nose));
                    clear = clear.min(d);
                    ok &= d >= SENSED_CLEAR_M;
                }
                for c in &wall_cells {
                    let d = dist2(*c, (px, py));
                    clear = clear.min(d);
                    ok &= d >= WALL_CLEAR_M;
                }
                for ((dx, dy), r) in drops {
                    if *r >= DROP_RADIUS_M {
                        ok &= dist2((*dx, *dy), (px, py)) >= r + DROP_CLEAR_M && dist2((*dx, *dy), nose) >= r + DROP_PATH_MARGIN_M;
                    }
                }
                ok &= grid.at(px, py).is_some_and(|c| c != Cell::Wall);
            }
            if !ok {
                continue;
            }
            let progress = dist2((pose.0, pose.1), look) - dist2((px, py), look);
            if progress < 0.02 {
                continue;
            }
            let heading_err = wrap((look.1 - py).atan2(look.0 - px) - h).abs();
            let score = 2.0 * progress - 0.15 * heading_err + 0.5 * clear.min(0.4);
            if best.is_none_or(|(s, _)| score > s) {
                best = Some((score, ArcLeg { vyaw, walk_s: secs, progress_m: progress, clear_m: clear }));
            }
        }
    }
    best.map(|(_, a)| a)
}

/// The mapped wall cells' centres within `radius` of `at`.
fn walls_near(grid: &Grid, at: (f64, f64), radius: f64) -> Vec<(f64, f64)> {
    let r = (radius / grid.cell_m).ceil() as i64;
    let c0 = ((at.0 - grid.x_min) / grid.cell_m).floor() as i64;
    let r0 = ((at.1 - grid.y_min) / grid.cell_m).floor() as i64;
    let mut out = Vec::new();
    for dr in -r..=r {
        for dc in -r..=r {
            let (rr, cc) = (r0 + dr, c0 + dc);
            if rr < 0 || cc < 0 || rr >= grid.rows as i64 || cc >= grid.cols as i64 {
                continue;
            }
            if grid.cells[rr as usize * grid.cols + cc as usize] == Cell::Wall {
                out.push((grid.x_min + (cc as f64 + 0.5) * grid.cell_m, grid.y_min + (rr as f64 + 0.5) * grid.cell_m));
            }
        }
    }
    out
}

impl Job {
    /// What the sensor saw in the last [`SENSED_WITHIN`], walking frames
    /// included, within [`SENSED_RANGE_M`]: obstacle points in the map's
    /// frame, from the pose of now.
    pub(super) fn sensed_points(&self, robot: &dyn Body, (x, y, yaw): (f64, f64, f64)) -> Vec<(f64, f64)> {
        let Some(cliff) = robot.cliff() else { return Vec::new() };
        let now = robot.now();
        cliff
            .recent
            .iter()
            .filter(|f| now.duration_since(f.at) <= SENSED_WITHIN)
            .flat_map(|f| f.obstacles.iter())
            .filter(|o| o.range_m <= SENSED_RANGE_M)
            .map(|o| (x + o.range_m * (yaw + o.bearing).cos(), y + o.range_m * (yaw + o.bearing).sin()))
            .collect()
    }

    /// The local planner's leg toward the route ahead, as the gait takes
    /// it, marked `local` for the guard (see [`Job::guarded_step`]).
    pub(super) fn local_leg(&self, robot: &dyn Body, grid: &Grid, pose: (f64, f64, f64), path: &[(f64, f64)]) -> Option<(Value, ArcLeg)> {
        let look = look_ahead(path, (pose.0, pose.1))?;
        let sensed = self.sensed_points(robot, pose);
        let Some(arc) = best_arc(grid, &self.local, &sensed, pose, look) else {
            tracing::info!(at = ?(pose.0, pose.1, pose.2), look = ?look, sensed = sensed.len(), "map explore: local planner: no admissible arc");
            return None;
        };
        Some((json!({"vx": 0.3, "vyaw": arc.vyaw, "walk_s": arc.walk_s, "stop_s": 0.0, "local": true, "phase": "local"}), arc))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open(w: usize, h: usize) -> Grid {
        Grid { rows: h, cols: w, x_min: -1.0, y_min: -1.0, cell_m: 0.05, cells: vec![Cell::Free; w * h] }
    }

    /// The passage east of house2's stairwell: a jamb 0.36 m off at 15°
    /// left, the route straight on. The straight leg would graze it; an
    /// arc to the right clears it and still closes on the route.
    #[test]
    fn an_arc_clears_the_jamb_the_straight_lane_refuses() {
        let grid = open(60, 60);
        let b = 15f64.to_radians();
        let jamb = (0.36 * b.cos(), 0.36 * b.sin());
        let arc = best_arc(&grid, &[], &[jamb], (0.0, 0.0, 0.0), (0.6, 0.0)).expect("an arc");
        assert!(arc.vyaw < 0.0, "turns away from the jamb: {arc:?}");
        assert!(arc.clear_m >= SENSED_CLEAR_M);
    }

    /// A wall of sensed points across the way: nothing admissible.
    #[test]
    fn no_arc_through_a_wall() {
        let grid = open(60, 60);
        let wall: Vec<(f64, f64)> = (-10..=10).map(|i| (0.2, 0.05 * f64::from(i))).collect();
        assert!(best_arc(&grid, &[], &wall, (0.0, 0.0, 0.0), (0.6, 0.0)).is_none());
    }

    /// The arc of the fall (house2, 2026-09-27): along the stairwell's
    /// east side, the rim a body's half away. Not admissible now.
    #[test]
    fn no_arc_along_the_rim() {
        let grid = open(60, 60);
        let rim: Vec<((f64, f64), f64)> = (0..8).map(|i| ((-0.25 + 0.05 * f64::from(i), -0.25), DROP_RADIUS_M)).collect();
        if let Some(a) = best_arc(&grid, &rim, &[], (0.0, 0.0, -0.3), (0.3, -0.3)) {
            panic!("an arc along the rim: {a:?}");
        }
    }

    /// A drop on the books beside the way: no arc passes within its margin.
    #[test]
    fn no_arc_over_a_booked_drop() {
        let grid = open(60, 60);
        let drops = [((0.2, -0.1), DROP_RADIUS_M)];
        if let Some(a) = best_arc(&grid, &drops, &[], (0.0, 0.0, 0.0), (0.6, 0.0)) {
            assert!(a.vyaw > 0.0, "only away from it: {a:?}");
        }
    }
}
