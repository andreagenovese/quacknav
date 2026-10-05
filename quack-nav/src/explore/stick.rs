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
/// The stand before a hole the guard saw goes on the books: long enough for
/// the two still frames the vote asks for.
const BOOK_STAND_S: f64 = 1.5;
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
    pub(super) fn stick_turn(&mut self, robot: &mut dyn Body, sign: f64, want: f64) -> f64 {
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
        // Careful only beside a hole the sensor sees and the books do not
        // hold yet: the paper twin's duck walked 7 cm from the stairwell's
        // unbooked rim for minutes, then fell. A booked rim is on the
        // planner's walls and the route keeps off it: there the stick trusts
        // the route, as on a journey (the user's, 2026-09-29: the Dijkstra
        // route, when the holes are on the books).
        let unbooked_seen = robot
            .cliff()
            .and_then(|c| c.nearest(robot.now()))
            .and_then(|d| {
                let r = if d.edge_min_m > 0.0 { d.edge_min_m } else { (d.range_m - crate::cliff::EDGE_UNKNOWN_M).max(0.10) };
                (r < CAREFUL_NEAR_M).then(|| (x + r * (yaw + d.bearing).cos(), y + r * (yaw + d.bearing).sin()))
            })
            .filter(|p| !self.local.iter().any(|(q, rr)| *rr >= DROP_RADIUS_M && dist2(*q, *p) < KNOWN_RIM_M));
        let careful = self.stick_careful && unbooked_seen.is_some();
        // ... and never beside it: off the rim first (see `off_the_rim`).
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
        // The pilot keeps counting past three (it reads the count): booked
        // once, at the third.
        if self.stick_stalls >= STALLS_TURN && (self.pilot.is_none() || self.stick_stalls == STALLS_TURN) {
            let nose = (x + BUMP_AHEAD_M * yaw.cos(), y + BUMP_AHEAD_M * yaw.sin());
            self.remember_local(nose, OBSTACLE_RADIUS_M);
            tracing::info!(at = ?(x, y, yaw), booked = ?nose, "map explore: stick: bumped; what the nose met goes on the books");
        }
        let before = crate::rlnav::trace::tracer().map(|_| crate::rlnav::trace::Snapshot::take(&*robot));
        let what;
        if let Some(pilot) = self.pilot.clone() {
            what = self.pilot_move(handle, robot, (x, y, yaw), f, &*pilot, guard_margin);
        } else if err.abs() > turn_first || self.stick_stalls >= STALLS_TURN {
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
            what = json!({"src": "stick", "act": if err > 0.0 { "turn_left" } else { "turn_right" }, "vyaw": quack_duck::body::TURN_IN_PLACE_RAD_S * err.signum(), "want": want, "turned": turned});
            // ... and a stand after it: the mapper corrects the pose at the
            // stands only, and a turn is where odometry drifts most.
            if want >= STAND_AFTER_TURN_RAD {
                self.trace_leg(&*robot, before, &what);
                self.traced_stand(robot, self.stick_stand_s);
                self.stick_since_stand = 0.0;
                self.stick_steps += 1;
                return self.stick_tail(robot, stand_every);
            }
        } else if let Some(d) = robot.cliff().and_then(|c| self.blind_drop_ahead(&c, robot.now(), GAIT_M_PER_S * STEP_S + guard_margin)) {
            what = self.turn_from_hole(robot, (x, y, yaw), d, "stick");
        } else {
            let vyaw = (YAW_GAIN * err).clamp(-0.7, 0.7);
            let _ = robot.blind_move(&json!({"vx": 0.3, "vyaw": vyaw, "duration_s": STEP_S}));
            handle.update(|s| s.legs += 1);
            tracing::info!(at = ?(x, y, yaw), look = ?look, err_deg = format!("{:.0}", err.to_degrees()), vyaw, "map explore: stick: step");
            what = json!({"src": "stick", "act": "step", "vx": 0.3, "vyaw": vyaw, "secs": STEP_S});
        }
        self.trace_leg(&*robot, before, &what);
        self.stick_steps += 1;
        self.stick_tail(robot, stand_every)
    }

    /// The leg's end, whoever chose its move: a stand every
    /// [`STAND_EVERY_M`] walked.
    fn stick_tail(&mut self, robot: &mut dyn Body, stand_every: f64) -> Option<(State, String)> {
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
            self.traced_stand(robot, self.stick_stand_s);
            self.stick_since_stand = 0.0;
            // The exploration's stands write the books, as its own legs'
            // stands did.
            if self.stick_books {
                self.record_drops(robot);
            }
        }
        None
    }

    /// The hole guard's answer (the stick's, and the shield over the
    /// pilot): a true hole the sensor sees in the step's own lane. Without
    /// it, the paper twin's journeys with a map bias of 0.18-0.25 m across
    /// the stairwell's passage walked into it (4 and 30 of 30). What it
    /// sees goes on the books, the route re-planned keeps off it; the body
    /// turns from it. Booked only when it is a rim the books do not hold
    /// yet: a hole seen near a booked rim is that rim, seen from a pose a
    /// little off, and booked again it moves the rim into the passage —
    /// five such points, 0.2-0.3 m east of the stairwell's rim, closed
    /// house2's east passage and g4 for the rest of the tour (MuJoCo,
    /// 2026-09-28).
    fn turn_from_hole(&mut self, robot: &mut dyn Body, (x, y, yaw): (f64, f64, f64), d: crate::cliff::Drop, src: &str) -> Value {
        let seen_at = {
            let r = if d.edge_min_m > 0.0 { d.edge_min_m } else { (d.range_m - crate::cliff::EDGE_UNKNOWN_M).max(0.10) };
            (x + r * (yaw + d.bearing).cos(), y + r * (yaw + d.bearing).sin())
        };
        self.stick_hole_at = Some(seen_at);
        let known = self.local.iter().any(|(p, r)| *r >= DROP_RADIUS_M && dist2(*p, seen_at) < KNOWN_RIM_M);
        if !known {
            // The books take a drop only from frames seen standing
            // (`record_drops`), and the step just walked leaves none:
            // on the exploration's travel, where the books are still
            // being written, a stand first. Without it house2's
            // stairwell rim was seen ten times and booked none, and the
            // route ran 4 cm from it (MuJoCo, 2026-09-29).
            if self.stick_books {
                let _ = stand(robot, BOOK_STAND_S);
            }
            self.record_drops(robot);
        }
        let vyaw = -quack_duck::body::TURN_IN_PLACE_RAD_S * d.bearing.signum();
        let _ = robot.blind_move(&json!({"vx": 0.0, "vyaw": vyaw, "duration_s": 0.5}));
        tracing::info!(at = ?(x, y, yaw), edge_m = format!("{:.2}", d.edge_min_m), bearing_deg = format!("{:.0}", d.bearing.to_degrees()), booked = !known, src, "map explore: stick: a hole ahead; turned from it");
        json!({"src": src, "act": "turn_from_hole", "vx": 0.0, "vyaw": vyaw, "secs": 0.5, "edge_m": d.edge_min_m, "bearing": d.bearing})
    }

    /// The pilot's leg (see `crate::rlnav`): it reads the route, the
    /// sensor, the map and the books around the body, and picks the move;
    /// a forward step into a true hole in the lane is the hole guard's.
    fn pilot_move(&mut self, handle: &ExploreHandle, robot: &mut dyn Body, pose: (f64, f64, f64), f: &Frontier, pilot: &dyn crate::rlnav::Brain, guard_margin: f64) -> Value {
        use crate::rlnav::{Action, ObsInput, TURN_RAD, WAIT_S, observe};
        let Some(grid) = robot.frame().and_then(|fr| fr.grid().ok()) else {
            robot.sleep(WAIT);
            return json!({"src": "pilot", "act": "no_map"});
        };
        let walls = self.planner_walls();
        let cliff = robot.cliff();
        let now = robot.now();
        let obs = observe(&ObsInput {
            pose,
            grid: &grid,
            walls: &walls,
            route: &f.path,
            goal: f.stand,
            cliff: cliff.as_ref(),
            now,
            last: self.pilot_last,
            stalls: self.stick_stalls,
            moved_m: self.pilot_moved_m,
        });
        let action = pilot.act(&obs);
        let odom0 = odom_pose(&*robot).map(|p| (p.0, p.1)).unwrap_or((pose.0, pose.1));
        let guarded = action.forward().then(|| cliff.as_ref().and_then(|c| self.blind_drop_ahead(c, now, GAIT_M_PER_S * STEP_S + guard_margin))).flatten();
        // Backing is blind (the sensor looks ahead): only onto floor the map
        // knows, off the books' drops. The bench's first pilot backed into
        // an unbooked stairwell, turning and backing by turns beside it
        // (quack-rl, mixed 100020, 2026-10-05).
        let back_refused = action == Action::Back && !self.back_is_safe(&grid, pose);
        // ... and a step off the books' drops, and off the map's unknown
        // beside them (a hole is never mapped as floor): the sensor's guard
        // sees the lane ahead, not a rim beside the body that the gait's
        // veer takes it over — a pilot that only ever stepped straight fell
        // 8 times in 40 beside the bench's stairwells with the sensor's
        // guard alone (quack-rl, 2026-10-05). Unknown far from any drop is
        // floor nothing looked at (a dark rug, under a chair): refused
        // there, a patch across a corridor held the pilot for good.
        // ... and no pushing on: two legs that did not move the body, and
        // something ahead (the sensor's lane, or the map's wall at the
        // nose), a step only scuffs along it — on the bench, sideways into
        // an unbooked hole against the wall, which the wall hid from the
        // sensor (quack-rl, stairwell 100035, 2026-10-05).
        let pushing = action.forward()
            && self.stick_stalls >= PUSH_STALLS
            && (cliff
                .as_ref()
                .and_then(|c| c.obstacle_in_lane_walking(now, 0.0, BLIND_DROP_LANE_M, PUSH_AHEAD_M, std::time::Duration::from_millis(1200), 1))
                .is_some()
                || [-0.09, 0.0, 0.09].iter().any(|side| {
                    let (c, s) = (pose.2.cos(), pose.2.sin());
                    let a = 0.11 + PUSH_AHEAD_M / 2.0;
                    matches!(grid.at(pose.0 + a * c - side * s, pose.1 + a * s + side * c), Some(Cell::Wall))
                }));
        let step_refused = guarded.is_none()
            && action.forward()
            && (pushing || action.timed().is_some_and(|(vx, vyaw, secs)| {
                let leg = json!({"vx": vx, "vyaw": vyaw, "walk_s": secs});
                let drops: Vec<(f64, f64)> = self.local.iter().filter(|(_, r)| *r >= DROP_RADIUS_M).map(|(p, _)| *p).collect();
                self.drop_on_path(pose, &leg).is_some() || step_into_unknown(&grid, pose, vx, vyaw, secs, &drops)
            }));
        let what = if back_refused || step_refused {
            let _ = stand(robot, WAIT_S);
            self.pilot_last = Some(Action::Wait);
            let why = if back_refused {
                "no known floor behind"
            } else if pushing {
                "pushing against something ahead"
            } else {
                "the step's way crosses a drop or the unknown beside one"
            };
            json!({"src": "shield", "act": "wait", "proposed": action.name(), "why": why})
        } else if let Some(d) = guarded {
            let mut w = self.turn_from_hole(robot, pose, d, "shield");
            w["proposed"] = json!(action.name());
            self.pilot_last = Some(if d.bearing > 0.0 { Action::TurnRight } else { Action::TurnLeft });
            w
        } else {
            match action {
                Action::TurnLeft | Action::TurnRight => {
                    let sign = if action == Action::TurnLeft { 1.0 } else { -1.0 };
                    let turned = self.stick_turn(robot, sign, TURN_RAD);
                    json!({"src": "pilot", "act": action.name(), "vyaw": quack_duck::body::TURN_IN_PLACE_RAD_S * sign, "want": TURN_RAD, "turned": turned})
                }
                Action::Wait => {
                    let _ = stand(robot, WAIT_S);
                    json!({"src": "pilot", "act": action.name(), "secs": WAIT_S})
                }
                Action::Step(_) | Action::Back => {
                    let (vx, vyaw, secs) = action.timed().unwrap_or((0.0, 0.0, 0.0));
                    let _ = robot.blind_move(&json!({"vx": vx, "vyaw": vyaw, "duration_s": secs}));
                    if action.forward() {
                        handle.update(|s| s.legs += 1);
                    }
                    json!({"src": "pilot", "act": action.name(), "vx": vx, "vyaw": vyaw, "secs": secs})
                }
            }
        };
        if guarded.is_none() && !back_refused && !step_refused {
            self.pilot_last = Some(action);
        }
        let odom1 = odom_pose(&*robot).map(|p| (p.0, p.1)).or_else(|| robot.frame().map(|fr| (fr.x, fr.y))).unwrap_or(odom0);
        self.pilot_moved_m = dist2(odom0, odom1);
        tracing::info!(at = ?pose, act = action.name(), shield = guarded.is_some() || back_refused || step_refused, moved_m = format!("{:.3}", self.pilot_moved_m), "map explore: pilot");
        what
    }

    /// Whether the floor behind the body is the map's known floor (or a
    /// wall: a bump, not a fall), off the drops on the books: from the
    /// body's rear edge back as far as a back-off and a margin reach,
    /// across its width.
    fn back_is_safe(&self, grid: &crate::map::Grid, (x, y, yaw): (f64, f64, f64)) -> bool {
        const REAR_M: f64 = 0.11;
        const REACH_M: f64 = 0.18;
        let (c, s) = (yaw.cos(), yaw.sin());
        let mut d = REAR_M;
        while d <= REAR_M + REACH_M + 1e-9 {
            for side in [-0.1, 0.0, 0.1] {
                let p = (x - d * c - side * s, y - d * s + side * c);
                if !matches!(grid.at(p.0, p.1), Some(Cell::Free) | Some(Cell::Wall)) {
                    return false;
                }
                if self.local.iter().any(|(q, r)| *r >= DROP_RADIUS_M && dist2(*q, p) < r + 0.05) {
                    return false;
                }
            }
            d += 0.03;
        }
        true
    }

    /// A leg on the trace (`QK_RL_TRACE`), when one is recorded.
    fn trace_leg(&self, robot: &dyn Body, before: Option<crate::rlnav::trace::Snapshot>, what: &Value) {
        if let (Some(t), Some(b)) = (crate::rlnav::trace::tracer(), before) {
            t.event(robot, "leg", b, what.clone());
        }
    }

    /// The stick's stand, on the trace too.
    fn traced_stand(&self, robot: &mut dyn Body, secs: f64) {
        let tracer = crate::rlnav::trace::tracer();
        let before = tracer.map(|_| crate::rlnav::trace::Snapshot::take(&*robot));
        let _ = stand(robot, secs);
        if let (Some(t), Some(b)) = (tracer, before) {
            t.event(&*robot, "stand", b, json!({"secs": secs}));
        }
    }
}

/// The pilot's legs that did not move the body before a step against
/// something ahead within [`PUSH_AHEAD_M`] is refused.
const PUSH_STALLS: u32 = 2;
const PUSH_AHEAD_M: f64 = 0.25;

/// Unknown this near a drop on the books is the hole's (see
/// [`step_into_unknown`]).
const UNKNOWN_NEAR_DROP_M: f64 = 0.35;

/// Whether a step `(vx, vyaw, secs)` from `pose`, played through the gait
/// (0.12 m/s at vx 0.3, 0.65 rad/s per unit of yaw — a short curving
/// pulse may turn twice that), takes the body's front over a cell the map
/// does not know within [`UNKNOWN_NEAR_DROP_M`] of a drop on the books:
/// ahead of the body's front edge by up to a step and a margin, across its
/// width.
fn step_into_unknown(grid: &crate::map::Grid, (x, y, yaw): (f64, f64, f64), vx: f64, vyaw: f64, secs: f64, drops: &[(f64, f64)]) -> bool {
    if drops.is_empty() {
        return false;
    }
    const FRONT_M: f64 = 0.11;
    const MARGIN_M: f64 = 0.08;
    let v = GAIT_M_PER_S * vx / 0.3;
    for gain in [0.65, 1.3] {
        let w = gain * vyaw;
        let (mut px, mut py, mut h) = (x, y, yaw);
        let mut t = 0.0;
        while t <= secs + 1e-9 {
            for ahead in [FRONT_M, FRONT_M + MARGIN_M / 2.0, FRONT_M + MARGIN_M] {
                for side in [-0.09, 0.0, 0.09] {
                    let (qx, qy) = (px + ahead * h.cos() - side * h.sin(), py + ahead * h.sin() + side * h.cos());
                    if !matches!(grid.at(qx, qy), Some(Cell::Free) | Some(Cell::Wall)) && drops.iter().any(|d| dist2(*d, (qx, qy)) < UNKNOWN_NEAR_DROP_M) {
                        return true;
                    }
                }
            }
            px += v * 0.1 * h.cos();
            py += v * 0.1 * h.sin();
            h += w * 0.1;
            t += 0.1;
        }
    }
    false
}
