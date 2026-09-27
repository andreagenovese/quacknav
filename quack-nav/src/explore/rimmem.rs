//! What the sensor saw of a rim, kept (ADR 0009, the sensor's guidance
//! beyond the traverse).
//!
//! The turn in place is refused near a drop — booked, from the map's pose,
//! or seen by the stand's frames. Beside house2's stairwell neither held:
//! the pose was 0.22 m off, so the booked rim read 0.22 m farther than it
//! was, and the rim lay beside the body, out of the sweep's reach, the
//! stand too short for the head to look that way. A turn in place 10 cm
//! from the true rim was allowed, and the duck fell (MuJoCo, 2026-09-27).
//!
//! So the rim points of every stand are kept, in odometry's frame — which
//! does not jump when the map corrects the pose — for a minute and two
//! metres of walking at most (odometry drifts too, only slower), and the
//! near-drop checks read them beside the books and the frames of now.
//! And before a turn in place with a booked drop within [`LOOK_FIRST_M`],
//! the body stands long enough for the head's sweep to look both ways.
//! `QK_RIM_MEMORY=0` switches both off.

use super::*;

/// `QK_RIM_MEMORY=0`: no kept rim, no look before turning.
pub(super) fn rim_memory_on() -> bool {
    switch("QK_RIM_MEMORY").unwrap_or(true)
}

/// Kept this long, and while odometry has not walked this far since.
const KEEP_FOR: Duration = Duration::from_secs(60);
const KEEP_WALKED_M: f64 = 2.0;
/// Two frames' points this near are the same rim; with at least this many
/// frames at a stand, a point needs another frame's to be kept.
const VOTE_M: f64 = 0.06;
const VOTE_FRAMES: usize = 6;
/// Nearest a move may bring the body's centre, and its nose, to a kept rim
/// point: the body's half-width and the legs' drift, and a little.
const ARC_CENTRE_M: f64 = 0.13;
const ARC_NOSE_M: f64 = 0.08;
/// A booked drop this near: look both ways before turning in place.
pub(super) const LOOK_FIRST_M: f64 = 0.5;
/// The stand that looks both ways: past the sweep's right extreme (a 6 s
/// triangle from the centre, left at 1.5 s, right at 4.5 s).
const LOOK_STAND_S: f64 = 5.0;
/// ... not again within this of the last, nor this soon: once a spot, 3 cm
/// wide, looked 50 times in a round beside the stairwell, 250 s of it.
const LOOK_AGAIN_M: f64 = 0.15;
const LOOK_AGAIN: Duration = Duration::from_secs(30);

#[derive(Debug, Default)]
pub(super) struct RimMemory {
    /// Points in odometry's frame, when seen, and odometry's walked
    /// distance then.
    points: Vec<((f64, f64), Instant, f64)>,
    /// The newest frame taken in.
    last_seq: u64,
    /// Odometry's distance walked, as integrated here, and where it was.
    walked: f64,
    at: Option<(f64, f64)>,
    /// Where the last look both ways was taken (odometry).
    looked_at: Option<((f64, f64), Instant)>,
}

/// Odometry's pose, if robotd gives it.
fn odom(robot: &dyn Body) -> Option<(f64, f64, f64)> {
    robot.cliff().and_then(|c| Some((c.odom_xy?.0, c.odom_xy?.1, c.odom_yaw?)))
}

impl RimMemory {
    /// Take in the stand frames not seen yet; forget what is too old.
    fn absorb(&mut self, robot: &dyn Body) {
        let Some((ox, oy, oyaw)) = odom(robot) else { return };
        if let Some(p) = self.at {
            self.walked += dist2(p, (ox, oy));
        }
        self.at = Some((ox, oy));
        let now = robot.now();
        let walked = self.walked;
        self.points.retain(|(_, t, w)| now.duration_since(*t) <= KEEP_FOR && walked - w <= KEEP_WALKED_M);
        let Some(cliff) = robot.cliff() else { return };
        let fresh: Vec<&crate::cliff::CliffFrame> = cliff.recent.iter().filter(|f| f.seq > self.last_seq && !f.moving).collect();
        if fresh.is_empty() {
            return;
        }
        self.last_seq = fresh.iter().map(|f| f.seq).max().unwrap_or(self.last_seq);
        let mut batch: Vec<((f64, f64), u64)> = Vec::new();
        for f in &fresh {
            for d in &f.drops {
                if f.obstacles.iter().any(|o| wrap(o.bearing - d.bearing).abs() < 0.2 && (o.range_m - d.range_m).abs() < 0.25) {
                    continue;
                }
                let r = if d.edge_min_m > 0.0 { d.edge_min_m } else { (d.range_m - crate::cliff::EDGE_UNKNOWN_M).max(0.10) };
                if r > 1.0 {
                    continue;
                }
                let b = oyaw + d.bearing;
                batch.push(((ox + r * b.cos(), oy + r * b.sin()), f.seq));
            }
        }
        let vote = fresh.len() >= VOTE_FRAMES;
        for (p, seq) in &batch {
            if !vote || batch.iter().any(|(q, s)| s != seq && dist2(*p, *q) < VOTE_M) {
                self.points.push((*p, now, walked));
            }
        }
        // Dense stands keep a great many: thin to one per 3 cm.
        if self.points.len() > 2000 {
            let mut kept: Vec<((f64, f64), Instant, f64)> = Vec::new();
            for p in self.points.drain(..).rev() {
                if !kept.iter().any(|q| dist2(q.0, p.0) < 0.03) {
                    kept.push(p);
                }
            }
            self.points = kept;
        }
    }

    /// The nearest kept rim point: its distance and its bearing off the
    /// nose, from odometry's pose now.
    fn nearest(&self, robot: &dyn Body) -> Option<(f64, f64)> {
        let (ox, oy, oyaw) = odom(robot)?;
        self.points
            .iter()
            .map(|((px, py), _, _)| (dist2((*px, *py), (ox, oy)), wrap((py - oy).atan2(px - ox) - oyaw)))
            .min_by(|a, b| a.0.total_cmp(&b.0))
    }
}

impl RimMemory {
    /// The first kept rim point a move at `(vx, vyaw)` for `secs` would
    /// bring the body's centre within [`ARC_CENTRE_M`] of, or its nose
    /// within [`ARC_NOSE_M`], through the gait's model from odometry's
    /// pose now.
    fn on_arc(&self, robot: &dyn Body, vx: f64, vyaw: f64, secs: f64) -> Option<(f64, f64)> {
        let (mut px, mut py, mut h) = odom(robot)?;
        let (v, w) = if vx > 0.0 { (GAIT_M_PER_S, 0.65 * vyaw) } else if vx < 0.0 { (-BACK_M_PER_S, 0.6 * vyaw) } else { (0.0, 0.0) };
        if v == 0.0 || self.points.is_empty() {
            return None;
        }
        let mut t = 0.0;
        while t < secs - 1e-9 {
            px += v * 0.1 * h.cos();
            py += v * 0.1 * h.sin();
            h += w * 0.1;
            t += 0.1;
            let nose = (px + crate::passage::BODY_HALF_M * h.cos(), py + crate::passage::BODY_HALF_M * h.sin());
            if let Some((p, _, _)) = self.points.iter().find(|(p, _, _)| dist2(*p, (px, py)) < ARC_CENTRE_M || dist2(*p, nose) < ARC_NOSE_M) {
                return Some(*p);
            }
        }
        None
    }
}

impl Job {
    /// Whether a move would cross the kept rim (see [`RimMemory::on_arc`]):
    /// the leg's check that does not ride on the map's pose. The fall of
    /// 2026-09-27 (MuJoCo, house2, 65785ca) was a turning leg with the
    /// pose 0.22 m off: the books put the rim farther, and the sensor's
    /// lane ahead did not hold it when the arc began.
    pub(super) fn kept_rim_on_arc(&self, robot: &dyn Body, vx: f64, vyaw: f64, secs: f64) -> Option<(f64, f64)> {
        if !rim_memory_on() {
            return None;
        }
        let mut m = self.rim_memory.borrow_mut();
        m.absorb(robot);
        m.on_arc(robot, vx, vyaw, secs)
    }

    /// The kept rim's nearest point (distance, bearing off the nose), the
    /// frames of now taken in first.
    pub(super) fn kept_rim(&self, robot: &dyn Body) -> Option<(f64, f64)> {
        if !rim_memory_on() {
            return None;
        }
        let mut m = self.rim_memory.borrow_mut();
        m.absorb(robot);
        m.nearest(robot)
    }

    /// Before a turn in place: with a booked drop within [`LOOK_FIRST_M`]
    /// and no look both ways from here yet, stand while the head sweeps
    /// both ways, and take in what it saw.
    pub(super) fn look_both_ways(&mut self, robot: &mut dyn Body) {
        if !rim_memory_on() {
            return;
        }
        let Some((x, y, _)) = robot.frame().map(|f| f.pose()) else { return };
        let booked = self.local.iter().filter(|(_, r)| *r >= DROP_RADIUS_M).map(|(p, _)| dist2(*p, (x, y))).fold(f64::INFINITY, f64::min);
        if booked >= LOOK_FIRST_M {
            return;
        }
        let Some((ox, oy, _)) = odom(&*robot) else { return };
        let now = robot.now();
        if self.rim_memory.borrow().looked_at.is_some_and(|(p, t)| dist2(p, (ox, oy)) < LOOK_AGAIN_M && now.duration_since(t) < LOOK_AGAIN) {
            return;
        }
        tracing::info!(booked_m = format!("{booked:.2}"), "map explore: a drop near: looking both ways before turning");
        let _ = stand(robot, LOOK_STAND_S);
        let mut m = self.rim_memory.borrow_mut();
        m.absorb(&*robot);
        m.looked_at = Some(((ox, oy), now));
    }
}
