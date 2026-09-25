//! Regulated Pure Pursuit (Macenski et al. 2023, Nav2's
//! `RegulatedPurePursuitController`), fitted to a gait that has one speed.
//!
//! `QK_RPP=1` (ADR 0009, step 2b). In place of the cascade of aims — straight
//! when the lane is clear, the string pulled at most 0.6 m, a waypoint a
//! stride on, the centred aim, the held aim, never behind the beak — one
//! rule: the aim is the point of the route `L` along it from the point of
//! the route nearest the body, `L` short where the body is near a wall or a
//! rim and long in the open, and the chord to it clear. The leg steers by
//! the arc through that point (curvature `2·y/L²`), and it is regulated as
//! RPP regulates its speed: the duck walks at the gait's one pace, so what
//! shrinks with a tight curve or a near obstacle is the leg's length — a
//! shorter leg is a sooner look.
//!
//! What stays as it was: turning in place to a heading far off the nose
//! (RPP's rotate-to-heading), the passage law beside a drop, and every
//! guard (`plan_step`, `drop_on_path`, off the rim, the turn refused by a
//! rim). They are safety, not following.

use super::*;

/// The lookahead, and the clearance over which it grows from the least to
/// the most.
/// The least lookahead, and the least the chord check may shrink it to:
/// knobs `QK_RPP_L_MIN_M`. An aim nearer than the gait can follow is worse
/// than none — at 0.2 m by the stairwell the heading error swung +31°,
/// −50°, −4°, −44° from one turn to the next (the turn-in-place pulse is
/// 13° ± 10°), the turns ran out and the duck stood (paper twin, bath).
fn rpp_l_min_m() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| knob("QK_RPP_L_MIN_M", 0.40))
}
const RPP_L_MAX_M: f64 = 1.00;
const RPP_CLEAR_NEAR_M: f64 = 0.15;
const RPP_CLEAR_FAR_M: f64 = 0.60;
/// Below this turning radius the leg is shortened in proportion (RPP's
/// `regulated_linear_scaling_min_radius`).
const RPP_MIN_RADIUS_M: f64 = 0.90;
/// Below this clearance the leg is shortened in proportion (RPP's
/// cost-regulated speed, on the distance itself).
const RPP_PROXIMITY_M: f64 = 0.40;
/// A booked drop counts this much nearer than it is when the lookahead is
/// chosen: beside a rim the aim is short, as it is beside nothing else.
/// With a rim weighed like a wall the chord to an aim 0.9 m on cut the
/// route's corner by the stairwell into a pocket between the rim and a
/// thing the sensor saw, and the duck stood there 900 s (paper twin,
/// bath, four seeds in thirty). `QK_RPP_DROP_EXTRA_M`.
fn rpp_drop_extra_m() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| knob("QK_RPP_DROP_EXTRA_M", 0.30))
}
/// The gait's yaw rate per unit of `vyaw`, measured on the twin.
const YAW_PER_UNIT: f64 = 0.65;

pub(super) fn rpp() -> bool {
    static V: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *V.get_or_init(|| switch("QK_RPP").unwrap_or(false))
}

/// `QK_RPP_OPEN=1`: the pursuit only away from the drops (farther than
/// `STRING_NEAR_DROP_M`); beside one, the aims as they were.
pub(super) fn rpp_open_only() -> bool {
    static V: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *V.get_or_init(|| switch("QK_RPP_OPEN").unwrap_or(false))
}

/// The lookahead for a body `clear_m` from the nearest thing.
pub(super) fn lookahead_for(clear_m: f64) -> f64 {
    let t = ((clear_m - RPP_CLEAR_NEAR_M) / (RPP_CLEAR_FAR_M - RPP_CLEAR_NEAR_M)).clamp(0.0, 1.0);
    rpp_l_min_m() + t * (RPP_L_MAX_M - rpp_l_min_m())
}

/// The point `l` along `path` from the path's point nearest `at` (the
/// projection onto its nearest segment), or the path's end.
pub(super) fn pursuit_point(path: &[(f64, f64)], at: (f64, f64), l: f64) -> Option<(f64, f64)> {
    if path.is_empty() {
        return None;
    }
    if path.len() == 1 {
        return Some(path[0]);
    }
    // Nearest segment and the projection on it.
    let (mut best, mut best_d) = ((0usize, 0.0f64), f64::INFINITY);
    for (i, w) in path.windows(2).enumerate() {
        let (a, b) = (w[0], w[1]);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len2 = dx * dx + dy * dy;
        let t = if len2 > 0.0 { (((at.0 - a.0) * dx + (at.1 - a.1) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
        let p = (a.0 + t * dx, a.1 + t * dy);
        let d = dist2(p, at);
        if d < best_d {
            best_d = d;
            best = (i, t);
        }
    }
    let (i, t) = best;
    let seg = |k: usize| (path[k], path[k + 1]);
    let (a, b) = seg(i);
    let mut left = l;
    let mut from = (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1));
    let mut k = i;
    loop {
        let to = path[k + 1];
        let d = dist2(from, to);
        if d >= left {
            let f = if d > 0.0 { left / d } else { 0.0 };
            return Some((from.0 + f * (to.0 - from.0), from.1 + f * (to.1 - from.1)));
        }
        left -= d;
        from = to;
        k += 1;
        if k + 1 >= path.len() {
            return path.last().copied();
        }
    }
}

/// The curvature of the arc from `pose` through `aim` (positive left).
pub(super) fn curvature(pose: (f64, f64, f64), aim: (f64, f64)) -> f64 {
    let (x, y, yaw) = pose;
    let (dx, dy) = (aim.0 - x, aim.1 - y);
    let lateral = -yaw.sin() * dx + yaw.cos() * dy;
    let l2 = dx * dx + dy * dy;
    if l2 < 1e-9 { 0.0 } else { 2.0 * lateral / l2 }
}

/// The regulated leg: `vyaw` for the arc, and how long to walk it — the
/// time to reach the aim, shortened by a tight curve and by a near thing,
/// never under a second (a shorter leg is a pulse the gait does not walk).
pub(super) fn regulated(kappa: f64, l: f64, clear_m: f64, cap_s: f64) -> (f64, f64) {
    let vyaw = (GAIT_M_PER_S * kappa / YAW_PER_UNIT).clamp(-0.7, 0.7);
    let mut scale = 1.0;
    if kappa.abs() > 1e-9 && 1.0 / kappa.abs() < RPP_MIN_RADIUS_M {
        scale *= (1.0 / kappa.abs()) / RPP_MIN_RADIUS_M;
    }
    if clear_m < RPP_PROXIMITY_M {
        scale *= (clear_m / RPP_PROXIMITY_M).max(0.0);
    }
    let walk_s = (l / GAIT_M_PER_S * scale).clamp(1.0, cap_s.max(1.0));
    (vyaw, walk_s)
}

impl Job {
    /// The nearest a wall on the map or anything on the books comes to
    /// `(x, y)`, looked for within a metre.
    pub(super) fn clearance_around(&self, grid: &Grid, (x, y): (f64, f64)) -> f64 {
        let mut best = 1.0f64;
        let r = (1.0 / grid.cell_m).ceil() as isize;
        let (col0, row0) = (((x - grid.x_min) / grid.cell_m).floor() as isize, ((y - grid.y_min) / grid.cell_m).floor() as isize);
        for dr in -r..=r {
            for dc in -r..=r {
                let (rr, cc) = (row0 + dr, col0 + dc);
                if rr < 0 || cc < 0 || grid.cell(rr as usize, cc as usize) != Some(Cell::Wall) {
                    continue;
                }
                let (wx, wy) = (grid.x_min + (cc as f64 + 0.5) * grid.cell_m, grid.y_min + (rr as f64 + 0.5) * grid.cell_m);
                best = best.min((dist2((wx, wy), (x, y)) - grid.cell_m / 2.0).max(0.0));
            }
        }
        for (p, rad) in self.planner_walls() {
            let extra = if rad >= DROP_RADIUS_M { rpp_drop_extra_m() } else { 0.0 };
            best = best.min((dist2(p, (x, y)) - rad - extra).max(0.0));
        }
        best
    }

    /// The pursuit aim on `path` for a body at `pose`, and the lookahead and
    /// clearance it was chosen with.
    pub(super) fn rpp_aim(&self, grid: &Grid, pose: (f64, f64, f64), path: &[(f64, f64)], stand: (f64, f64)) -> ((f64, f64), f64, f64) {
        let (x, y, _) = pose;
        let clear = self.clearance_around(grid, (x, y));
        let mut l = lookahead_for(clear);
        loop {
            let aim = if dist2((x, y), stand) <= l { stand } else { pursuit_point(path, (x, y), l).unwrap_or(stand) };
            let heading = (aim.1 - y).atan2(aim.0 - x);
            let chord = dist2((x, y), aim);
            if chord < 1e-6 || (grid.lane_clear(x, y, heading, chord, lane_half_m()) && self.clear_of_local(x, y, heading, chord, lane_half_m())) {
                return (aim, l, clear);
            }
            if l <= rpp_l_min_m() {
                return (aim, l, clear);
            }
            l = (l * 0.7).max(rpp_l_min_m());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pursuit_point_is_measured_from_the_nearest_point_of_the_route() {
        let path: Vec<(f64, f64)> = (0..=20).map(|i| (i as f64 * 0.1, 0.0)).collect();
        // Beside the route at x = 0.55: the aim is 0.5 on from x = 0.55.
        let p = pursuit_point(&path, (0.55, 0.3), 0.5).unwrap();
        assert!((p.0 - 1.05).abs() < 1e-9 && p.1.abs() < 1e-9, "{p:?}");
        // Past the end: the end.
        assert_eq!(pursuit_point(&path, (1.9, 0.0), 1.0), Some((2.0, 0.0)));
        // Round a corner.
        let l: Vec<(f64, f64)> = vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0)];
        let p = pursuit_point(&l, (0.8, 0.0), 0.5).unwrap();
        assert!((p.0 - 1.0).abs() < 1e-9 && (p.1 - 0.3).abs() < 1e-9, "{p:?}");
    }

    #[test]
    fn the_arc_bends_toward_the_aim_and_a_tight_one_walks_less() {
        // Aim straight ahead: no curvature; to the left: positive.
        assert!(curvature((0.0, 0.0, 0.0), (1.0, 0.0)).abs() < 1e-12);
        let k = curvature((0.0, 0.0, 0.0), (0.5, 0.5));
        assert!((k - 2.0).abs() < 1e-9, "{k}");
        let (vyaw, s_tight) = regulated(k, 0.2, 1.0, 3.0);
        assert!(vyaw > 0.0);
        let (_, s_open) = regulated(0.1, 0.2, 1.0, 3.0);
        assert!(s_tight < s_open, "{s_tight} vs {s_open}");
        // Near a thing, the leg is the shortest there is.
        let (_, s_near) = regulated(0.0, 1.0, 0.1, 3.0);
        assert_eq!(s_near, 1.0);
    }

    #[test]
    fn the_lookahead_grows_with_the_room() {
        assert_eq!(lookahead_for(0.0), rpp_l_min_m());
        assert_eq!(lookahead_for(2.0), RPP_L_MAX_M);
        assert!(lookahead_for(0.4) > rpp_l_min_m() && lookahead_for(0.4) < RPP_L_MAX_M);
    }
}
