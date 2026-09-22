//! Threading a narrow passage: the geometry of two side boundaries and the
//! steering that keeps a body between them.
//!
//! The house has a 0.54 m lane between a wall and the stairwell, and the
//! duck — 0.19 m wide — walks it by hand every time and by itself almost
//! never (2026-09-15: 158 refusals in eight minutes). Not for want of room:
//! for want of a model. The step guard kept a lane and a margin sized for
//! open floor, and the centring between walls looked only at *mapped*
//! walls, which a hole never is — beside the stairwell nothing centred the
//! body, it drifted to the edge, and the edge was refused. This module is
//! the model: a passage is two lateral boundaries, wherever they come from
//! (a mapped wall, a drop the sensor saw, an obstacle it sees), and a
//! passage leg is a short straight step steered back to the middle and
//! along the axis, with margins the size of the body and the drift of one
//! leg, no more.
//!
//! In symbols, with `L`, `R` the free lateral distances to the left and
//! right boundary at the body (`near`) and `s` metres ahead (`far`):
//!
//! - width `W = min(L_near + R_near, L_far + R_far)`;
//! - feasible iff `W ≥ 2 (b + τ)` — the body's half-width `b` plus the
//!   lateral drift `τ` of one leg (heading error over the leg's length,
//!   odometry, the gait's wander), each side;
//! - offset from the centreline `e = (L_near − R_near) / 2`, positive when
//!   the middle is to the left;
//! - skew of the heading against the passage axis
//!   `θ = ½ (atan2(ΔL, s) − atan2(ΔR, s))` with `ΔL = L_far − L_near`,
//!   `ΔR = R_far − R_near`: a left gap that shrinks ahead and a right one
//!   that grows say the heading points left of the axis, and `θ < 0` turns
//!   it right;
//! - the yaw to walk with: `vyaw = k_e · e / (W/2) + k_θ · θ`, clamped.

/// Half the body's width plus a hair: 0.19 m across, measured.
pub const BODY_HALF_M: f64 = 0.10;
/// Lateral drift over one short leg: the gait's wander and a few degrees of
/// heading over 15–25 cm of walking.
pub const LEG_DRIFT_M: f64 = 0.05;
/// How far ahead the far boundary is sampled.
pub const AHEAD_M: f64 = 0.30;
/// Yaw at a full offset (the body against one boundary), rad/s.
pub const GAIN_OFFSET: f64 = 0.4;
/// Yaw per radian of skew, rad/s per rad: enough to cancel the skew within
/// one 1.5 s leg at the gait's 95 % yaw response, no more — at 1.0 the
/// heading swung ±20° from leg to leg through the stairwell lane.
pub const GAIN_SKEW: f64 = 0.6;
/// The most a passage leg steers: gentle, or the arc itself leaves the lane.
pub const MAX_YAW: f64 = 0.5;
/// A gap narrower than this, boundaries on both sides, is a passage.
pub const PASSAGE_MAX_M: f64 = 1.2;

/// Free lateral distance to the boundary on each side, metres. `None`
/// means nothing bounds that side within the look.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sides {
    pub left: Option<f64>,
    pub right: Option<f64>,
}

impl Sides {
    pub fn open() -> Self {
        Self { left: None, right: None }
    }
    /// The nearer of two boundaries on each side.
    pub fn nearest(self, other: Sides) -> Sides {
        fn min(a: Option<f64>, b: Option<f64>) -> Option<f64> {
            match (a, b) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, None) => a,
                (None, b) => b,
            }
        }
        Sides { left: min(self.left, other.left), right: min(self.right, other.right) }
    }
}

/// What a passage leg needs to know, and the yaw to walk it with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Passage {
    /// The narrowest width along the leg, metres.
    pub width_m: f64,
    /// Offset of the middle from the body, metres, positive to the left.
    pub offset_m: f64,
    /// The heading's skew against the axis, radians, positive when the
    /// heading points left of it.
    pub skew_rad: f64,
    /// Yaw to walk with, rad/s: toward the middle and along the axis.
    pub vyaw: f64,
    /// Room on the tighter side once the body and one leg's drift are
    /// paid for, metres. Negative: the leg does not fit.
    pub spare_m: f64,
}

impl Passage {
    /// The leg fits: the body plus one leg's drift on each side.
    pub fn fits(&self) -> bool {
        // a millimetre of grace for the arithmetic, not for the body
        self.spare_m >= -1e-3
    }
}

/// Judge a passage from its boundaries at the body and `ahead_m` further
/// on. `None` when either side is open on either sample — no passage,
/// nothing to centre between.
pub fn passage(near: Sides, far: Sides, ahead_m: f64) -> Option<Passage> {
    let (ln, rn, lf, rf) = (near.left?, near.right?, far.left?, far.right?);
    let width_m = (ln + rn).min(lf + rf);
    if width_m > PASSAGE_MAX_M {
        return None;
    }
    let offset_m = (ln - rn) / 2.0;
    let skew_rad = 0.5 * ((lf - ln).atan2(ahead_m) - (rf - rn).atan2(ahead_m));
    let half = (width_m / 2.0).max(1e-3);
    let vyaw = (GAIN_OFFSET * offset_m / half + GAIN_SKEW * skew_rad).clamp(-MAX_YAW, MAX_YAW);
    // Whether it fits is a question about the leg as steered, not about
    // the sides as they stand: a body skewed toward a wall has that wall
    // close ahead, and the steered leg curves away from it — refusing it
    // for the side it is leaving is the deadlock the stairwell lane fell
    // into from the south (2026-09-15). Roll the steered leg out over the
    // sampled stretch and take the least clearance to either boundary,
    // the boundaries interpolated between the near and the far sample.
    let spare_m = spare_along(ln, rn, lf, rf, ahead_m, vyaw) - (BODY_HALF_M + LEG_DRIFT_M);
    Some(Passage { width_m, offset_m, skew_rad, vyaw, spare_m })
}

/// Forward speed and yaw response the roll-out assumes (measured).
const ROLL_SPEED: f64 = 0.12;
const ROLL_YAW_GAIN: f64 = 0.95;

/// The least clearance to either boundary along a leg walked at `vyaw`
/// over the sampled stretch: the body's lateral offset at forward `s` is
/// integrated from the yaw, the boundaries are lines from the near to the
/// far sample.
fn spare_along(ln: f64, rn: f64, lf: f64, rf: f64, ahead_m: f64, vyaw: f64) -> f64 {
    let (mut l_min, mut r_min) = (f64::INFINITY, f64::INFINITY);
    let (mut s, mut lat, mut h) = (0.0_f64, 0.0_f64, 0.0_f64);
    let dt = 0.1;
    let mut t = 0.0;
    // the roll-out is the leg's own length in time: a yaw that turns the
    // body across the passage stops advancing `s`, and the body must not
    // be walked sideways for longer than the leg lasts
    let leg_s = ahead_m / ROLL_SPEED;
    let (mut l_last, mut r_last) = (ln, rn);
    while s <= ahead_m && t < leg_s {
        t += dt;
        let f = (s / ahead_m).clamp(0.0, 1.0);
        let l_here = ln + f * (lf - ln) - lat;
        let r_here = rn + f * (rf - rn) + lat;
        if s > 0.05 {
            l_min = l_min.min(l_here);
            r_min = r_min.min(r_here);
        }
        l_last = l_here;
        r_last = r_here;
        h += vyaw * ROLL_YAW_GAIN * dt;
        s += ROLL_SPEED * dt * h.cos();
        lat += ROLL_SPEED * dt * h.sin();
    }
    // The question is where the body goes, not where it stands: a duck
    // brushing a door jamb has 6 cm on that side *now*, and a leg steered
    // away from it is the right leg. A side the body starts inside the
    // band of is judged by where the leg leaves it — it must gain ground
    // — and not by the stretch it spends leaving.
    // The leg may never come closer to that side than the body stands
    // now, and must end at least 3 cm further from it. (A drop that close
    // is the drop guard's to refuse, on the same rolled-out leg: brushing
    // a wall is a bump, brushing a drop is a fall.)
    let band = BODY_HALF_M + LEG_DRIFT_M;
    let judged = |start: f64, min: f64, last: f64| {
        if start < band {
            if min >= start - 1e-6 && last > start + 0.03 { last } else { min.min(start) }
        } else {
            min
        }
    };
    judged(ln, l_min, l_last).min(judged(rn, r_min, r_last))
}

/// Lateral free distance implied by a thing seen at polar `(range_m,
/// bearing)` relative to the heading, if it lies beside the leg: ahead of
/// the body, within `ahead_m` forward. Returns which side and how far.
pub fn side_of(range_m: f64, bearing: f64, ahead_m: f64) -> Option<(bool, f64)> {
    let forward = range_m * bearing.cos();
    if forward < -BODY_HALF_M || forward > ahead_m + BODY_HALF_M {
        return None;
    }
    let lateral = range_m * bearing.sin();
    // A thing on the heading line is in the way, not beside it: the
    // front guards judge it. Read as a side it made a hole dead ahead into
    // a "passage" 0.00 m wide (2026-09-15).
    if lateral.abs() < BODY_HALF_M / 2.0 {
        return None;
    }
    Some((lateral >= 0.0, lateral.abs()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sides(l: f64, r: f64) -> Sides {
        Sides { left: Some(l), right: Some(r) }
    }

    /// The stairwell lane: 0.54 m between a wall and a drop, the body
    /// centred and aligned — it fits, with 0.12 m to spare, and walks
    /// straight.
    #[test]
    fn the_stairwell_lane_fits_a_centred_duck() {
        let p = passage(sides(0.27, 0.27), sides(0.27, 0.27), AHEAD_M).expect("a passage");
        assert!((p.width_m - 0.54).abs() < 1e-9);
        assert!(p.fits(), "{p:?}");
        assert!((p.spare_m - 0.12).abs() < 1e-9);
        assert!(p.vyaw.abs() < 1e-9 && p.skew_rad.abs() < 1e-9);
    }

    /// Off-centre toward the drop on the right: steer left, in proportion.
    #[test]
    fn an_offset_steers_back_to_the_middle() {
        let p = passage(sides(0.42, 0.16), sides(0.42, 0.16), AHEAD_M).unwrap();
        assert!((p.offset_m - 0.13).abs() < 1e-9);
        assert!(p.vyaw > 0.15, "left, and firmly: {p:?}");
        assert!(p.fits(), "0.16 m to the drop is the body plus a leg's drift, just: {p:?}");
        // 0.10 m off the boundary — the body's own half-width — and steered
        // away: the passage law lets it go (it leaves); whether that side
        // is a drop is the drop guard's question, asked on the same leg.
        let p = passage(sides(0.44, 0.10), sides(0.44, 0.10), AHEAD_M).unwrap();
        assert!(p.fits(), "{p:?}");
        // steered the wrong way it does not
        assert!(spare_along(0.44, 0.10, 0.44, 0.10, AHEAD_M, -0.3) < BODY_HALF_M + LEG_DRIFT_M);
    }

    /// Heading a few degrees toward the wall: the left gap shrinks ahead,
    /// the right grows, and the skew turns the heading right even from
    /// the exact middle.
    #[test]
    fn a_skewed_heading_is_turned_back_along_the_axis() {
        let p = passage(sides(0.27, 0.27), sides(0.22, 0.32), AHEAD_M).unwrap();
        assert!(p.skew_rad < 0.0 && p.vyaw < 0.0, "{p:?}");
        assert!((p.skew_rad + (0.05_f64).atan2(0.30)).abs() < 1e-6);
    }

    /// Skewed 15° toward the wall in the stairwell lane, from the middle:
    /// the wall is 0.19 m off the heading line 0.3 m ahead, inside the
    /// body's drift band, yet the steered leg turns away and clears it.
    #[test]
    fn a_skewed_body_fits_because_the_steered_leg_turns_away() {
        let p = passage(sides(0.27, 0.27), sides(0.19, 0.35), AHEAD_M).unwrap();
        assert!(p.vyaw < -0.1, "turn right, away from the wall: {p:?}");
        assert!(p.fits(), "{p:?}");
        // straight on, the wall would come within the body and its drift
        assert!(spare_along(0.27, 0.27, 0.19, 0.35, AHEAD_M, 0.0) < BODY_HALF_M + LEG_DRIFT_M + 0.05);
        // 25° toward the wall still steers out
        let p = passage(sides(0.27, 0.27), sides(0.13, 0.41), AHEAD_M).unwrap();
        assert!(p.fits(), "{p:?}");
        // but not from beside the wall already: 0.12 m off it, skewed in
        let p = passage(sides(0.12, 0.42), sides(0.02, 0.52), AHEAD_M).unwrap();
        assert!(!p.fits(), "{p:?}");
    }

    /// Brushing a door jamb — 6 cm on the left, 0.68 m on the right, as a
    /// doorway looks from its edge: the leg steered right is the right
    /// leg and fits, though the body stands inside the band now. Walked
    /// straight, it does not.
    #[test]
    fn brushing_a_jamb_fits_when_the_leg_leaves_it() {
        let p = passage(sides(0.06, 0.68), sides(0.06, 0.68), AHEAD_M).unwrap();
        assert!(p.vyaw < -0.2, "away from the jamb: {p:?}");
        assert!(p.fits(), "{p:?}");
        assert!(spare_along(0.06, 0.68, 0.06, 0.68, AHEAD_M, 0.0) < BODY_HALF_M + LEG_DRIFT_M);
    }

    /// Open on one side: not a passage; a wide hall: not a passage.
    #[test]
    fn open_floor_is_not_a_passage() {
        assert!(passage(Sides { left: Some(0.3), right: None }, sides(0.3, 0.3), AHEAD_M).is_none());
        assert!(passage(sides(1.0, 1.0), sides(1.0, 1.0), AHEAD_M).is_none());
    }

    /// A drop seen 0.35 m ahead and 0.27 m to the right while the leg is
    /// 0.3 m: beside the leg, on the right, 0.27 m off. One at 1.2 m dead
    /// ahead is not beside it.
    #[test]
    fn things_seen_become_sides() {
        let (fwd, lat) = (0.35_f64, -0.27_f64);
        let (left, d) = side_of(fwd.hypot(lat), lat.atan2(fwd), AHEAD_M).unwrap();
        assert!(!left && (d - 0.27).abs() < 1e-6);
        assert!(side_of(1.2, 0.0, AHEAD_M).is_none());
        assert!(side_of(0.3, 0.05, AHEAD_M).is_none(), "dead ahead is not a side");
    }
}
