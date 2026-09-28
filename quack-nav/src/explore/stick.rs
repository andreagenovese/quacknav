//! The stick (the user's "prova folle", 2026-09-28): every rule off, the
//! body held to the Dijkstra route and nothing else.
//!
//! What it asks: how much of what the explorer does on a journey is worth
//! it. With `QK_STICK=1` a journey's leg is only this — a point of the
//! route [`LOOK_M`] ahead, a turn in place to it when it is more than
//! [`TURN_FIRST_RAD`] off the nose, else a short step curving onto it —
//! a bump (steps that do not move the body) booked at the nose and turned from —
//! the sensor's own guard against a true hole in the step's lane kept,
//! with a stand every [`STAND_EVERY`] steps for the mapper's still window,
//! the pose corrected only there. No passage law, no rim rules, no steps
//! back, no guard of ours: the route is re-planned from the pose at every
//! leg, as it always is, and followed as closely as the gait allows.

use super::*;

/// `QK_STICK=1`: a journey follows the route and nothing else.
pub(super) fn stick_on() -> bool {
    switch("QK_STICK").unwrap_or(false)
}

/// The route's point aimed at: this far from the body.
const LOOK_M: f64 = 0.2;
/// Off the nose by more than this, turn in place first.
const TURN_FIRST_RAD: f64 = 0.6;
/// The step: short, so the route re-planned from the next pose keeps the
/// body on it.
const STEP_S: f64 = 0.6;
/// A step that moved the body less than this did not walk; this many in a
/// row, and the body turns in place to the route instead.
const STALL_M: f64 = 0.01;
const STALLS_TURN: u32 = 3;
/// The sensor's hole guard: a true hole within the step's reach and this.
const DROP_GUARD_MARGIN_M: f64 = 0.15;
/// Where a bump is booked: this far ahead of the body's centre.
const BUMP_AHEAD_M: f64 = 0.15;
/// A stand every this many steps, for the mapper.
const STAND_EVERY: u32 = 6;
const STAND_S: f64 = 2.0;
/// The yaw asked per radian of heading error, as the gait turns 0.65 of
/// it a second: the error closed over about the step.
const YAW_GAIN: f64 = 1.0 / (0.65 * STEP_S);

impl Job {
    /// One leg of the stick (see the module): `None`, the job goes on.
    pub(super) fn stick_leg(&mut self, handle: &ExploreHandle, robot: &mut dyn Body, (x, y, yaw): (f64, f64, f64), f: &Frontier) -> Option<(State, String)> {
        let look = f.path.iter().copied().find(|p| dist2(*p, (x, y)) >= LOOK_M).unwrap_or(f.stand);
        let err = wrap((look.1 - y).atan2(look.0 - x) - yaw);
        // The one rule it keeps: steps that do not move the body are a
        // wall under the beak, and a curving step against it pushes for
        // ever (the paper twin, 2026-09-28: 878 steps on one spot, 8 cm
        // from the west wall). Three of them: turn in place instead.
        let stalled = self.stick_last.is_some_and(|p| dist2(p, (x, y)) < STALL_M);
        self.stick_stalls = if stalled { self.stick_stalls + 1 } else { 0 };
        self.stick_last = Some((x, y));
        // ... and what it pushed against goes on the books, at the nose:
        // the map says free, the body says not — a low box the map does
        // not hold, or a wall where the pose's error puts it (house2's
        // office door, MuJoCo 2026-09-28: the pose 0.2 m off, the body
        // against the wall beside the doorway for the whole budget, six
        // rounds in six). The route re-planned from here goes round it.
        if self.stick_stalls >= STALLS_TURN {
            let nose = (x + BUMP_AHEAD_M * yaw.cos(), y + BUMP_AHEAD_M * yaw.sin());
            self.remember_local(nose, OBSTACLE_RADIUS_M);
            tracing::info!(at = ?(x, y, yaw), booked = ?nose, "map explore: stick: bumped; what the nose met goes on the books");
        }
        if err.abs() > TURN_FIRST_RAD || self.stick_stalls >= STALLS_TURN {
            // A pure turn in place: yaw past the gait's dead zone, about
            // 30-58°/s, for as long as the error asks and a second at most.
            let vyaw = quack_duck::body::TURN_IN_PLACE_RAD_S * err.signum();
            let secs = (err.abs() / 0.6).clamp(0.3, 1.0);
            self.stick_stalls = 0;
            let _ = robot.blind_move(&json!({"vx": 0.0, "vyaw": vyaw, "duration_s": secs}));
            tracing::info!(at = ?(x, y, yaw), look = ?look, err_deg = format!("{:.0}", err.to_degrees()), "map explore: stick: turn");
        } else if let Some(d) = robot.cliff().and_then(|c| self.blind_drop_ahead(&c, robot.now(), GAIT_M_PER_S * STEP_S + DROP_GUARD_MARGIN_M)) {
            // The one guard it keeps: a true hole the sensor sees in the
            // step's own lane. Without it, the paper twin's journeys with a
            // map bias of 0.18-0.25 m across the stairwell's passage walked
            // into it (4 and 30 of 30). What it sees goes on the books, the
            // route re-planned keeps off it; the body turns from it.
            self.record_drops(robot);
            let vyaw = -quack_duck::body::TURN_IN_PLACE_RAD_S * d.bearing.signum();
            let _ = robot.blind_move(&json!({"vx": 0.0, "vyaw": vyaw, "duration_s": 0.5}));
            tracing::info!(at = ?(x, y, yaw), edge_m = format!("{:.2}", d.edge_min_m), bearing_deg = format!("{:.0}", d.bearing.to_degrees()), "map explore: stick: a hole ahead; booked, turned from it");
        } else {
            let vyaw = (YAW_GAIN * err).clamp(-0.7, 0.7);
            let _ = robot.blind_move(&json!({"vx": 0.3, "vyaw": vyaw, "duration_s": STEP_S}));
            handle.update(|s| s.legs += 1);
            tracing::info!(at = ?(x, y, yaw), look = ?look, err_deg = format!("{:.0}", err.to_degrees()), vyaw, "map explore: stick: step");
        }
        self.stick_steps += 1;
        if self.stick_steps % STAND_EVERY == 0 {
            let _ = stand(robot, STAND_S);
        }
        None
    }
}
