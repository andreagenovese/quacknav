//! The pose's fit — how well what the sensor sees at a stand sits on the
//! map from where the map says the duck is. A book is worth the pose it
//! was written with (rim1, 2026-09-16: seventeen centimetres of pose put
//! thirty phantom drops on the corridor's floor), and maploc gives no
//! measure of it (the still window's residual does not correlate with
//! the truth: `agreecheck.py`, ρ −0.42). This one is quacksat's own:
//! every obstacle the fresh frames saw, projected onto the map at the
//! pose of now, and the distance from it to the nearest mapped wall;
//! the median of those distances is the fit, in metres. Measured
//! against the twin's truth before it is trusted (`fitcheck.py`).

use super::*;

/// An obstacle farther than this is not judged: the sensor's range
/// error grows with distance, and a wall two metres off tells little.
const FIT_RANGE_M: f64 = 1.2;
/// How far around the projected point a wall is looked for; beyond it
/// the point is "unmatched" (a wall the map does not have, or a pose
/// error larger than this).
const FIT_SEARCH_M: f64 = 0.40;

/// The fit at this stand: (median distance to the nearest wall, points
/// matched within [`FIT_SEARCH_M`], points judged), or `None` with too
/// few points to say.
pub(super) fn pose_fit(grid: &Grid, cliff: &crate::cliff::CliffStatus, pose: (f64, f64, f64), now: Instant) -> Option<(f64, usize, usize)> {
    let pts = seen_points(cliff, now);
    if pts.len() < 6 {
        return None;
    }
    let (m, matched) = fit_at(grid, &pts, pose, (0.0, 0.0, 0.0));
    Some((m, matched, pts.len()))
}

/// The shift that fits best — the ray judge's question asked of the
/// stand: over a small search round the pose (±[`SHIFT_M`],
/// ±[`SHIFT_RAD`]), where do the seen obstacles sit best on the mapped
/// walls? The size of that shift is the estimate of the pose's error,
/// what a median distance alone cannot give (the sensor's own noise on
/// a wall is 2–14 cm at a true pose, fit1, 2026-09-19). Returns
/// (shift dx, dy, dyaw, the median there, the median at the pose).
pub(super) fn pose_shift(grid: &Grid, cliff: &crate::cliff::CliffStatus, pose: (f64, f64, f64), now: Instant) -> Option<((f64, f64, f64), f64, f64)> {
    let pts = seen_points(cliff, now);
    if pts.len() < 12 {
        return None;
    }
    let (here, _) = fit_at(grid, &pts, pose, (0.0, 0.0, 0.0));
    let mut best: ((f64, f64, f64), f64) = ((0.0, 0.0, 0.0), here);
    let nxy = (SHIFT_M / SHIFT_STEP_M).round() as i64;
    let nyaw = (SHIFT_RAD / SHIFT_STEP_RAD).round() as i64;
    for iy in -nyaw..=nyaw {
        for ix in -nxy..=nxy {
            for jy in -nxy..=nxy {
                let d = (ix as f64 * SHIFT_STEP_M, jy as f64 * SHIFT_STEP_M, iy as f64 * SHIFT_STEP_RAD);
                let (m, _) = fit_at(grid, &pts, pose, d);
                // Better by a margin, or as good and nearer the pose.
                let closer = (d.0.hypot(d.1) + 0.5 * d.2.abs()) < (best.0.0.hypot(best.0.1) + 0.5 * best.0.2.abs());
                if m < best.1 - 0.004 || (m <= best.1 + 0.001 && closer) {
                    best = (d, m);
                }
            }
        }
    }
    Some((best.0, best.1, here))
}
const SHIFT_M: f64 = 0.30;
const SHIFT_STEP_M: f64 = 0.03;
const SHIFT_RAD: f64 = 0.17;
const SHIFT_STEP_RAD: f64 = 0.035;

/// The obstacles the fresh frames saw, in the body frame (bearing, range).
fn seen_points(cliff: &crate::cliff::CliffStatus, now: Instant) -> Vec<(f64, f64)> {
    cliff
        .recent
        .iter()
        .filter(|f| now.duration_since(f.at).as_secs_f64() <= RECORD_FRESH_S && !f.moving)
        .flat_map(|f| f.obstacles.iter())
        .filter(|o| o.range_m <= FIT_RANGE_M && o.range_m >= 0.15)
        .map(|o| (o.bearing, o.range_m))
        .collect()
}

/// The median distance to the nearest wall, the points projected at
/// `pose` shifted by `d` (dx, dy in the map, dyaw); unmatched points
/// count as the search radius.
fn fit_at(grid: &Grid, pts: &[(f64, f64)], (x, y, yaw): (f64, f64, f64), d: (f64, f64, f64)) -> (f64, usize) {
    let mut dists: Vec<f64> = Vec::with_capacity(pts.len());
    for (bearing, range) in pts {
        let b = yaw + d.2 + bearing;
        let (px, py) = (x + d.0 + range * b.cos(), y + d.1 + range * b.sin());
        dists.push(nearest_wall_m(grid, px, py, FIT_SEARCH_M).unwrap_or(FIT_SEARCH_M));
    }
    let matched = dists.iter().filter(|v| **v < FIT_SEARCH_M).count();
    dists.sort_by(|a, b| a.total_cmp(b));
    (dists[dists.len() / 2], matched)
}

/// The distance from (x, y) to the nearest wall cell within `max_m`.
fn nearest_wall_m(grid: &Grid, x: f64, y: f64, max_m: f64) -> Option<f64> {
    let r = (max_m / grid.cell_m).ceil() as i64;
    let c0 = ((x - grid.x_min) / grid.cell_m).floor() as i64;
    let r0 = ((y - grid.y_min) / grid.cell_m).floor() as i64;
    let mut best: Option<f64> = None;
    for dr in -r..=r {
        for dc in -r..=r {
            let (rr, cc) = (r0 + dr, c0 + dc);
            if rr < 0 || cc < 0 || rr >= grid.rows as i64 || cc >= grid.cols as i64 {
                continue;
            }
            if grid.cells[rr as usize * grid.cols + cc as usize] != crate::map::Cell::Wall {
                continue;
            }
            let (wx, wy) = (grid.x_min + (cc as f64 + 0.5) * grid.cell_m, grid.y_min + (rr as f64 + 0.5) * grid.cell_m);
            let d = (wx - x).hypot(wy - y);
            if d <= max_m && best.is_none_or(|b| d < b) {
                best = Some(d);
            }
        }
    }
    best
}

impl Job {
    /// The fit at this stand, logged and put in the status.
    pub(super) fn measure_fit(&mut self, robot: &dyn Body, handle: &ExploreHandle, grid: &Grid, pose: (f64, f64, f64)) {
        let Some(cliff) = robot.cliff() else { return };
        let fit = pose_fit(grid, &cliff, pose, robot.now());
        if let Some((m, matched, judged)) = fit {
            let shift = pose_shift(grid, &cliff, pose, robot.now());
            let (sx, sy, syaw, best) = shift.map_or((0.0, 0.0, 0.0, m), |(d, b, _)| (d.0, d.1, d.2, b));
            tracing::info!(
                fit_m = format!("{m:.3}"),
                matched,
                judged,
                shift_m = format!("{:.3}", sx.hypot(sy)),
                shift_deg = format!("{:.1}", syaw.to_degrees()),
                shift_xy = ?(format!("{sx:.2}"), format!("{sy:.2}")),
                fit_shifted_m = format!("{best:.3}"),
                at = ?(format!("{:.2}", pose.0), format!("{:.2}", pose.1)),
                "map explore: pose fit"
            );
        }
        self.last_fit = fit.map(|(m, _, _)| m);
        handle.update(|s| s.fit = fit);
    }
}
