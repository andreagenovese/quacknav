//! The stick (the user's "prova folle", 2026-09-28): every rule off, the
//! body held to the Dijkstra route and nothing else.
//!
//! What it asked: how much of what the explorer does on a journey is worth
//! it — little, measured. On MuJoCo, six rounds of six goals a house, no
//! falls in any: the true map and pose 36/36 on house2 and 36/36 on
//! casa_arredata; maploc's pose on the true map 36/36 and 36/36; on the
//! explored maps 26-30/36 and 36/36 — against main's ~11/18 and 17-18/18,
//! the explorer's rules 13/18 and 17/18. Frozen 2026-09-28 as every
//! journey's navigation (see `navigate.rs`). A journey's leg is only this — a point of the
//! route [`LOOK_M`] ahead, a turn in place to it when it is more than
//! [`TURN_FIRST_RAD`] off the nose, else a short step curving onto it —
//! a bump (steps that do not move the body) booked at the nose and turned from —
//! the sensor's own guard against a true hole in the step's lane kept,
//! turns closed on odometry's yaw, a stand after every turn of
//! [`STAND_AFTER_TURN_RAD`] and every [`STAND_EVERY_M`] walked for the
//! mapper's still window,
//! the pose corrected only there. No passage law, no rim rules, no steps
//! back, no guard of ours: the route is re-planned from the pose at every
//! leg, as it always is, and followed as closely as the gait allows.

use super::*;

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
/// A hole seen this near a booked rim point is that rim: not booked again.
const KNOWN_RIM_M: f64 = 0.30;
/// Where a bump is booked: this far ahead of the body's centre.
const BUMP_AHEAD_M: f64 = 0.15;
/// A stand for the mapper every this far walked, and after a turn of this.
const STAND_EVERY_M: f64 = 0.4;
const STAND_AFTER_TURN_RAD: f64 = 0.5;
/// The stand on a journey; the exploration's travel stands as long as the
/// mapper needs (see `Job::travel`).
pub(super) const STICK_STAND_S: f64 = 2.0;
/// On the exploration's travel (`Job::travel`), within this of a drop on
/// the books the stick is careful: it turns in place past
/// [`CAREFUL_TURN_RAD`] instead of curving at up to [`TURN_FIRST_RAD`],
/// stands every [`CAREFUL_STAND_EVERY_M`], and its hole guard looks
/// [`CAREFUL_GUARD_MARGIN_M`] past the step. The map is being drawn and the
/// pose can be 0.15 m off beside the rim it has not booked whole: house2's
/// duck, its pose 14 cm toward the stairwell, curved into it on a step 29°
/// off the route (MuJoCo, 2026-09-29); the paper twin at a 0.15 m bias, 1
/// exploration in 20 fell. The journey, on a finished map, is not touched.
const CAREFUL_NEAR_M: f64 = 0.5;
const CAREFUL_TURN_RAD: f64 = 0.17;
const CAREFUL_STAND_EVERY_M: f64 = 0.2;
const CAREFUL_GUARD_MARGIN_M: f64 = 0.25;
/// The yaw asked per radian of heading error, as the gait turns 0.65 of
/// it a second: the error closed over about the step.
const YAW_GAIN: f64 = 1.0 / (0.65 * STEP_S);

/// Odometry's pose, when robotd gives it.
fn odom_pose(robot: &dyn Body) -> Option<(f64, f64, f64)> {
    robot.cliff().and_then(|c| Some((c.odom_xy?.0, c.odom_xy?.1, c.odom_yaw?)))
}

impl Job {
    /// A turn in place toward `sign` by `want` radians, closed on odometry's
    /// yaw in [`TURN_CHUNK_S`] chunks (the map's yaw without odometry), a
    /// time budget for the slowest measured rate. Returns how far it turned.
    fn stick_turn(&mut self, robot: &mut dyn Body, sign: f64, want: f64) -> f64 {
        let yaw_now = |robot: &dyn Body| odom_pose(robot).map(|p| p.2).or_else(|| robot.frame().map(|f| f.yaw));
        let Some(yaw0) = yaw_now(&*robot) else { return 0.0 };
        let goal = (want - TURN_LEAD_RAD).max(0.05);
        let vyaw = quack_duck::body::TURN_IN_PLACE_RAD_S * sign;
        let started = robot.now();
        let budget = 2.0 * want / 0.5 + 1.0;
        let mut turned = 0.0f64;
        while (robot.now() - started).as_secs_f64() < budget {
            let _ = robot.blind_move(&json!({"vx": 0.0, "vyaw": vyaw, "duration_s": TURN_CHUNK_S}));
            if let Some(y) = yaw_now(&*robot) {
                turned = wrap(y - yaw0) * sign;
                if turned >= goal {
                    break;
                }
            }
        }
        turned
    }

    /// One leg of the stick (see the module): `None`, the job goes on.
    pub(super) fn stick_leg(&mut self, handle: &ExploreHandle, robot: &mut dyn Body, (x, y, yaw): (f64, f64, f64), f: &Frontier) -> Option<(State, String)> {
        let look = f.path.iter().copied().find(|p| dist2(*p, (x, y)) >= LOOK_M).unwrap_or(f.stand);
        let err = wrap((look.1 - y).atan2(look.0 - x) - yaw);
        // Beside a drop, booked or seen by the sensor: seen only, the rim
        // is not on the books yet, and the paper twin's duck walked 7 cm
        // from the stairwell's unbooked rim for minutes, then fell.
        let careful = self.stick_careful && self.drop_within_any(&*robot, CAREFUL_NEAR_M).is_some();
        // ... and never beside it: off the rim first, as the explorer's own
        // legs do (see `off_the_rim`).
        if careful && self.off_the_rim(robot, (x, y, yaw)) {
            self.stick_last = None;
            return None;
        }
        let (turn_first, stand_every, guard_margin) =
            if careful { (CAREFUL_TURN_RAD, CAREFUL_STAND_EVERY_M, CAREFUL_GUARD_MARGIN_M) } else { (TURN_FIRST_RAD, STAND_EVERY_M, DROP_GUARD_MARGIN_M) };
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
        if err.abs() > turn_first || self.stick_stalls >= STALLS_TURN {
            // A pure turn in place: yaw past the gait's dead zone, about
            // 30-58°/s, for as long as the error asks and a second at most.
            // Closed on odometry's yaw, in short chunks: the map's pose
            // comes once a second and lags a turn at 30-58°/s by up to 50°
            // (the oracle's run, MuJoCo 2026-09-28) — a turn read off it
            // overshoots, and the next step is aimed wrong.
            self.stick_stalls = 0;
            let want = if stalled { err.abs().max(0.35) } else { err.abs() };
            let turned = self.stick_turn(robot, err.signum(), want);
            tracing::info!(at = ?(x, y, yaw), look = ?look, err_deg = format!("{:.0}", err.to_degrees()), turned_deg = format!("{:.0}", turned.to_degrees()), "map explore: stick: turn");
            // ... and a stand after it: the mapper corrects the pose at the
            // stands only, and a turn is where odometry drifts most.
            if want >= STAND_AFTER_TURN_RAD {
                let _ = stand(robot, self.stick_stand_s);
                self.stick_since_stand = 0.0;
            }
        } else if let Some(d) = robot.cliff().and_then(|c| self.blind_drop_ahead(&c, robot.now(), GAIT_M_PER_S * STEP_S + guard_margin)) {
            // The one guard it keeps: a true hole the sensor sees in the
            // step's own lane. Without it, the paper twin's journeys with a
            // map bias of 0.18-0.25 m across the stairwell's passage walked
            // into it (4 and 30 of 30). What it sees goes on the books, the
            // route re-planned keeps off it; the body turns from it.
            // Booked only when it is a rim the books do not hold yet: a hole
            // seen near a booked rim is that rim, seen from a pose a little
            // off, and booked again it moves the rim into the passage — five
            // such points, 0.2-0.3 m east of the stairwell's rim, closed
            // house2's east passage and g4 for the rest of the tour (MuJoCo,
            // 2026-09-28).
            let seen_at = {
                let r = if d.edge_min_m > 0.0 { d.edge_min_m } else { (d.range_m - crate::cliff::EDGE_UNKNOWN_M).max(0.10) };
                (x + r * (yaw + d.bearing).cos(), y + r * (yaw + d.bearing).sin())
            };
            let known = self.local.iter().any(|(p, r)| *r >= DROP_RADIUS_M && dist2(*p, seen_at) < KNOWN_RIM_M);
            if !known {
                self.record_drops(robot);
            }
            let vyaw = -quack_duck::body::TURN_IN_PLACE_RAD_S * d.bearing.signum();
            let _ = robot.blind_move(&json!({"vx": 0.0, "vyaw": vyaw, "duration_s": 0.5}));
            tracing::info!(at = ?(x, y, yaw), edge_m = format!("{:.2}", d.edge_min_m), bearing_deg = format!("{:.0}", d.bearing.to_degrees()), booked = !known, "map explore: stick: a hole ahead; turned from it");
        } else {
            let vyaw = (YAW_GAIN * err).clamp(-0.7, 0.7);
            let _ = robot.blind_move(&json!({"vx": 0.3, "vyaw": vyaw, "duration_s": STEP_S}));
            handle.update(|s| s.legs += 1);
            tracing::info!(at = ?(x, y, yaw), look = ?look, err_deg = format!("{:.0}", err.to_degrees()), vyaw, "map explore: stick: step");
        }
        self.stick_steps += 1;
        // A stand every [`STAND_EVERY_M`] walked, by odometry: at every sixth
        // step the oracle's run left maploc 0.4 m off before it gave up the
        // pose — the stick walks almost without stopping.
        if let Some((ox, oy, _)) = odom_pose(&*robot) {
            if let Some(p) = self.stick_odom_at {
                self.stick_since_stand += dist2(p, (ox, oy));
            }
            self.stick_odom_at = Some((ox, oy));
        } else {
            self.stick_since_stand += GAIT_M_PER_S * STEP_S;
        }
        if self.stick_since_stand >= stand_every {
            let _ = stand(robot, self.stick_stand_s);
            self.stick_since_stand = 0.0;
        }
        None
    }
}
