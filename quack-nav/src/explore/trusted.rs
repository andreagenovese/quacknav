//! Trusted floor — the user's idea (2026-09-18): "in exploration the
//! stands come first; once the duck has mapped walls and holes
//! correctly, follow the route as the blind journeys do." A route to the
//! next frontier runs mostly over floor the duck already knows: cells it
//! has walked, and cells the depth sensor judged as floor from a stand
//! (up to the drop edge or the obstacle it saw along each bearing). A
//! leg whose path, flank included, lies on such floor walks blind — no
//! guard, no refusal; the last stretch into the unknown keeps the guard
//! and its stand. Kicks and pulses are never blind by this: they turn
//! toward what no leg has judged.
//!
//! Measured on the paper bench before anything (thirty runs, explore
//! then the living room past the stairwell): blind legs with the rim on
//! the books fell 3 in 30, all on kicks; the guard 0 in 30, 14 arrived.

use super::*;
use std::collections::HashSet;

/// The cell of the trusted floor: 5 cm.
const CELL_M: f64 = 0.05;
/// How far along a bearing a stand's frame vouches for the floor, at
/// most: the sensor sees the floor well to about a metre.
const SEEN_FAR_M: f64 = 0.9;
/// Kept back from a drop's edge and from an obstacle along the bearing.
const SEEN_BACK_M: f64 = 0.12;
/// Half of the sensor's cone, and the step between bearings inside it.
const CONE_STEP_RAD: f64 = 0.05;
/// A trail point vouches for the floor this far around it.
const TRAIL_TRUST_M: f64 = 0.10;
/// The flank a leg's path must have trusted floor on, either side.
const FLANK_M: f64 = 0.11;
/// No cell this near a booked drop is trusted, whatever a frame said.
const NEAR_DROP_M: f64 = 0.15;

#[derive(Default)]
pub(super) struct TrustedFloor {
    cells: HashSet<(i32, i32)>,
    /// The cells the body walked, apart: a drop cannot be booked there
    /// (a hole is not where the duck has stood), and unlike the seen
    /// floor these do not share the pose error of the frame that sees
    /// the drop — rim1's thirty phantoms lay on floor walked minutes
    /// before (2026-09-16).
    walked: HashSet<(i32, i32)>,
}

impl TrustedFloor {
    fn key(x: f64, y: f64) -> (i32, i32) {
        ((x / CELL_M).floor() as i32, (y / CELL_M).floor() as i32)
    }
    fn mark(&mut self, x: f64, y: f64) {
        self.cells.insert(Self::key(x, y));
    }
    pub(super) fn has(&self, x: f64, y: f64) -> bool {
        self.cells.contains(&Self::key(x, y))
    }
    pub(super) fn walked_at(&self, x: f64, y: f64) -> bool {
        self.walked.contains(&Self::key(x, y))
    }
    pub(super) fn len(&self) -> usize {
        self.cells.len()
    }
    /// The cells' centres, for the overlay.
    pub(super) fn cells(&self) -> Vec<(f64, f64)> {
        self.cells.iter().map(|(i, j)| ((*i as f64 + 0.5) * CELL_M, (*j as f64 + 0.5) * CELL_M)).collect()
    }

    /// A stretch the body walked, `from` to `to`, and a little around.
    pub(super) fn walked(&mut self, from: (f64, f64), to: (f64, f64)) {
        let d = dist2(from, to);
        let n = (d / (CELL_M / 2.0)).ceil().max(1.0) as usize;
        for k in 0..=n {
            let t = k as f64 / n as f64;
            let (px, py) = (from.0 + t * (to.0 - from.0), from.1 + t * (to.1 - from.1));
            let mut ox = -TRAIL_TRUST_M;
            while ox <= TRAIL_TRUST_M + 1e-9 {
                let mut oy = -TRAIL_TRUST_M;
                while oy <= TRAIL_TRUST_M + 1e-9 {
                    self.mark(px + ox, py + oy);
                    self.walked.insert(Self::key(px + ox, py + oy));
                    oy += CELL_M / 2.0;
                }
                ox += CELL_M / 2.0;
            }
        }
    }

    /// What one stand's frames vouch for: along every bearing the
    /// frame looked at, floor from the body out to [`SEEN_FAR_M`], or to
    /// [`SEEN_BACK_M`] short of the nearest drop edge or obstacle the
    /// frame saw within a few degrees of that bearing.
    pub(super) fn seen(&mut self, (x, y, yaw): (f64, f64, f64), frame: &crate::cliff::CliffFrame) {
        let mut b = frame.head_yaw - crate::cliff::HALF_FOV_RAD;
        let end = frame.head_yaw + crate::cliff::HALF_FOV_RAD;
        while b <= end + 1e-9 {
            let mut far = SEEN_FAR_M;
            for d in &frame.drops {
                if wrap(d.bearing - b).abs() < 0.12 {
                    far = far.min(d.edge_min_m - SEEN_BACK_M);
                }
            }
            for o in &frame.obstacles {
                if wrap(o.bearing - b).abs() < 0.12 {
                    far = far.min(o.range_m - SEEN_BACK_M);
                }
            }
            let dir = yaw + b;
            let mut r = 0.0;
            while r <= far {
                self.mark(x + r * dir.cos(), y + r * dir.sin());
                r += CELL_M / 2.0;
            }
            b += CONE_STEP_RAD;
        }
    }
}

impl Job {
    /// The stand's fresh frames, onto the trusted floor.
    pub(super) fn trust_seen(&mut self, robot: &dyn Body) {
        let (Some(frame), Some(cliff)) = (robot.frame(), robot.cliff()) else { return };
        let pose = frame.pose();
        let now = robot.now();
        let frames: Vec<crate::cliff::CliffFrame> = cliff
            .recent
            .iter()
            .filter(|f| now.duration_since(f.at).as_secs_f64() <= RECORD_FRESH_S && !f.moving)
            .cloned()
            .collect();
        for f in &frames {
            self.trusted.seen(pose, f);
        }
    }

    /// Whether a step back of `secs` to `side` (see `back_path`) lies on
    /// trusted floor, clear of the books: the retreat and the backing
    /// pulses beside a drop are allowed on floor the body knows.
    pub(super) fn back_on_trusted(&self, pose: (f64, f64, f64), side: f64, secs: f64) -> bool {
        let (_, samples) = self.back_path(pose, side, secs, 0.0);
        let drops: Vec<(f64, f64)> = self.local.iter().filter(|(_, r)| *r >= DROP_RADIUS_M).map(|(p, _)| *p).collect();
        !samples.is_empty()
            && samples
                .iter()
                .all(|s| self.trusted.has(s.0, s.1) && !drops.iter().any(|d| dist2(*d, *s) < NEAR_DROP_M))
    }

    /// Whether this leg's path, played through the gait model with the
    /// flank on both sides, lies on trusted floor from end to end.
    pub(super) fn leg_on_trusted_floor(&self, (x, y, yaw): (f64, f64, f64), leg: &Value) -> bool {
        let drops: Vec<(f64, f64)> = self.local.iter().filter(|(_, r)| *r >= DROP_RADIUS_M).map(|(p, _)| *p).collect();
        let vx = leg.get("vx").and_then(Value::as_f64).unwrap_or(0.0);
        let vyaw = leg.get("vyaw").and_then(Value::as_f64).unwrap_or(0.0);
        let walk_s = leg.get("walk_s").and_then(Value::as_f64).unwrap_or(0.0);
        if vx <= 0.0 || walk_s <= 0.0 {
            return false;
        }
        let (v, w) = (GAIT_M_PER_S * vx / 0.3, 0.65 * vyaw);
        let (mut px, mut py, mut h) = (x, y, yaw);
        let mut t = 0.0;
        while t < walk_s + DROP_PATH_EXTRA_S {
            px += v * 0.1 * h.cos();
            py += v * 0.1 * h.sin();
            h += w * 0.1;
            let (nx, ny) = (-h.sin(), h.cos());
            for side in [-FLANK_M, 0.0, FLANK_M] {
                let (cx, cy) = (px + side * nx, py + side * ny);
                if !self.trusted.has(cx, cy) || drops.iter().any(|d| dist2(*d, (cx, cy)) < NEAR_DROP_M) {
                    return false;
                }
            }
            t += 0.1;
        }
        true
    }
}
