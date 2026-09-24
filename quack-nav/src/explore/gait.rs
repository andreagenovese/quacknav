//! Split out of `explore.rs` on 2026-09-18 (the user's: one core, the modes apart, shared only what is identical). Behaviour unchanged by construction.

use super::*;

/// How far the straight line to the standing point is checked for walls
/// before the grid path is followed instead.
pub(super) const STRAIGHT_LOOK_M: f64 = 2.0;
/// `QUACKSAT_REFUSED_REARM`: `0` one-shot per job, `1` after every walked
/// leg, else (default) once the body has moved [`REARM_DIST_M`].
pub(super) fn refused_rearm() -> u8 {
    match std::env::var("QUACKSAT_REFUSED_REARM").as_deref() {
        Ok("0") => 0,
        Ok("1") => 1,
        _ => 2,
    }
}
/// How far the body must move from the spot of the last clearing before
/// the refused list may be cleared again: the same spot gives the same
/// answer, and re-electing a refused frontier after every leg kept the
/// duck at a doorway re-trying it (MuJoCo runs 69–71: room stays of
/// 15–32 minutes, against 5–11 before the re-arm).
pub(super) const REARM_DIST_M: f64 = 1.0;
/// `QUACKSAT_SPIN_TIGHT=1`: a heading change beyond [`SPIN_ERR_RAD`] in
/// tight quarters is a turn in place, not an arc. Off by default: on the
/// paper twin it halves the longest room stays (31 -> 18 min in doorways
/// only) but costs completes (26/30 -> 20/30) and refusals (104 -> 182),
/// and anywhere tighter than a doorway costs more (14/30). Kept for the
/// MuJoCo measurement, where a wall bump has a price the paper twin has not.
pub(super) fn spin_tight() -> bool {
    std::env::var("QUACKSAT_SPIN_TIGHT").is_ok_and(|v| v == "1")
}
pub(super) const TIGHT_WIDTH_M: f64 = 0.8;
pub(super) const TIGHT_ROOM_M: f64 = 0.6;
pub(super) const SPIN_ERR_RAD: f64 = 0.6;
/// See [`Job::boxed_in`].
pub(super) const BOXED_M: f64 = 0.30;
/// Less room than this ahead of the nose: a turn in place is yaw only,
/// no kick (see [`Job::spin`]).
pub(super) const KICKLESS_ROOM_M: f64 = 0.35;
/// The tight-quarters turn (backing with the yaw) runs at most this long
/// and needs this much floor behind by the map.
pub(super) const TIGHT_TURN_MAX_S: f64 = 3.0;
/// No blind step back with a drop on the books this near; nearer than
/// the second, not even a backing pulse.
pub(super) const BACK_NO_DROP_M: f64 = 0.8;
pub(super) const BACK_NO_DROP_NEAR_M: f64 = 0.35;
/// The walking kick before a spin in tight quarters: half the usual, 6 cm.
pub(super) const TIGHT_KICK_S: f64 = 0.5;
/// The step back that makes room for a refused kick: 1.5 s ≈ 7 cm.
pub(super) const MAKE_ROOM_S: f64 = 1.5;
pub(super) const TIGHT_TURN_BACK_ROOM_M: f64 = 0.20;
/// `QUACKSAT_TIGHT_GAP_ONLY=0`: tight also means a corridor under
/// [`TIGHT_WIDTH_M`] or less than [`TIGHT_ROOM_M`] ahead (measured worse).
pub(super) fn tight_gap_only() -> bool {
    std::env::var("QUACKSAT_TIGHT_GAP_ONLY").map(|v| v != "0").unwrap_or(true)
}
/// `QUACKSAT_SPIN_ERR` overrides [`SPIN_ERR_RAD`], for measuring.
pub(super) fn spin_err_rad() -> f64 {
    std::env::var("QUACKSAT_SPIN_ERR").ok().and_then(|v| v.parse().ok()).unwrap_or(SPIN_ERR_RAD)
}
/// `QUACKSAT_ARC_FULL=1`: the arc's room judged with the measured advance
/// (0.9 of a straight leg). Off by default: true to the gait, but on the
/// paper twin it makes the explorer too timid (full flat 55 -> 38 %,
/// 21/30 -> 10/30 complete) — a wall bump costs nothing there, so the
/// paper twin cannot price it. To be measured on MuJoCo.
pub(super) fn arc_full() -> bool {
    std::env::var("QUACKSAT_ARC_FULL").is_ok_and(|v| v == "1")
}
/// Forward speed while turning at full yaw, as a fraction of the straight
/// speed (human drive: 0.110 / 0.121).
pub(super) const ARC_ADVANCE_FRAC: f64 = 0.9;
/// `QUACKSAT_TURN_AIM=1`: the no-room turn goes toward the aim instead
/// of the configured hand. Off by default: on the paper twin it lost
/// (full flat 21/30 -> 16/30 complete, refusals 126 -> 150; walls +
/// stairwell refusals 9 -> 31), kept for the bedroom-exit test on MuJoCo.
pub(super) fn turn_to_aim() -> bool {
    std::env::var("QUACKSAT_TURN_AIM").is_ok_and(|v| v == "1")
}
pub(super) fn arc_reserve_m() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| knob("QK_ARC_RESERVE_M", ARC_RESERVE_M))
}
pub(super) fn straight_rad() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| knob("QK_STRAIGHT_RAD", STRAIGHT_RAD))
}
pub(super) fn curve_rad() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| knob("QK_CURVE_RAD", CURVE_RAD))
}
pub(super) fn deadband_rad() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| knob("QK_DEADBAND_RAD", DEADBAND_RAD))
}
pub(super) fn prop_turn() -> bool {
    std::env::var("QK_PROP_TURN").map(|v| v != "0").unwrap_or(true)
}
pub(super) const ARC_RESERVE_M: f64 = 0.30;
/// Heading error below which no correction is applied at all.
pub(super) const DEADBAND_RAD: f64 = 0.25;
/// Heading error up to which a leg is "straight" with a gentle correction.
pub(super) const STRAIGHT_RAD: f64 = 0.35;
/// Heading error up to which a leg is a gentle curve; beyond, a tight arc.
pub(super) const CURVE_RAD: f64 = 1.0;
/// The step back: this gait needs about a second to start moving at all,
/// so a shorter one moves nothing (measured); and no more often than this.
/// `QK_BACK_S`: how long the step back lasts. Three seconds swung the body
/// 40° and left it a leg away from where it was; the human driver backs a
/// hand's breadth and re-aims (house1, 2026-09-15). Above the gait's
/// start latency, no more (`QK_BACK_S=3` restores the old step).
pub(super) fn back_s() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| knob("QK_BACK_S", 1.5))
}
/// `QK_BACK_REORIENT=0`: after the step back, head for the most open floor
/// and walk a leg there, as before. On by default: turn in place back
/// toward the leg's aim and let the planner speak — a step back is a
/// correction of the line, not a change of plan.
pub(super) fn back_reorient() -> bool {
    std::env::var("QK_BACK_REORIENT").map(|v| v != "0").unwrap_or(true)
}
/// `QK_SPIN_RAD`: a leg whose aim is more than this off the nose turns in
/// place first (closed on the yaw, [`Job::align`]) instead of walking a
/// curve — a curve from a standstill drifts sideways for its first second,
/// into a wall or a hole when the aim is beside one. 0 restores the curve.
pub(super) fn spin_rad(follow: bool) -> f64 {
    knob("QK_SPIN_RAD", if follow { 0.35 } else { 0.6 })
}
/// The +yaw stretch that gets the gait stepping before a mirrored or
/// straight step back.
pub(super) const BACK_TRIGGER_S: f64 = 0.5;
/// `QUACKSAT_SPIN_WATCH=0`: turn in place without watching the sensor, as
/// before. On by default: it only ever stops a blind turn early.
pub(super) fn spin_watch() -> bool {
    std::env::var("QUACKSAT_SPIN_WATCH").map(|v| v != "0").unwrap_or(true)
}
/// An edge nearer than this, within this half-angle of the beak, ends a
/// turn in place.
pub(super) const SPIN_WATCH_M: f64 = 0.30;
pub(super) const SPIN_WATCH_FOV_RAD: f64 = 0.9;
/// `QUACKSAT_BACK_TRAIL_ONLY=1`: near a drop, a step back only over the
/// trail (see `choose_back_side`); off until measured.
pub(super) fn back_trail_only() -> bool {
    std::env::var("QUACKSAT_BACK_TRAIL_ONLY").is_ok_and(|v| v == "1")
}
pub(super) const BACK_DROP_NEAR_M: f64 = 0.7;
/// `QUACKSAT_BACK_SIDES=1`: a step back with a chosen side (off until
/// measured alone on MuJoCo, see `passage_sensor`).
pub(super) fn back_sides() -> bool {
    std::env::var("QUACKSAT_BACK_SIDES").is_ok_and(|v| v == "1")
}
/// The shortest step back that still moves the body.
pub(super) const BACK_SHORT_S: f64 = 0.8;
/// Yaw while backing: see [`Job::back_off`].
pub(super) const BACK_VYAW: f64 = 0.7;
/// A drop nearer the body than this is left before anything else (see
/// `Job::off_the_rim`): the body's half-width and two centimetres.
pub(super) const RIM_OFF_M: f64 = 0.12;
/// Tries in a row at getting off a rim before the job's own ways decide.
pub(super) const RIM_OFF_TRIES: u32 = 3;
/// The straight leg after the step back or a panorama, toward open floor.
pub(super) const BACK_ON_S: f64 = 2.0;
pub(super) const BACK_EVERY: Duration = Duration::from_secs(10);
/// A turn in place gives up after this long (a half turn at 30°/s is 6 s).
pub(super) const SPIN_MAX_S: f64 = 8.0;
/// An accepted leg that left the duck within this much of where it stood
/// (and turned less than [`STALLED_RAD`]) walked into something the
/// sensor cannot see — under its minimum range, or beside the body.
/// A leg walks 0.15–0.35 m; the map's pose is good to a few centimetres.
pub(super) const STALLED_M: f64 = 0.08;
/// Backing speed at vx -0.3 (measured on the twin).
pub(super) const BACK_M_PER_S: f64 = 0.08;
/// The retreat along the trail before a turn beside a drop: about 0.3 m.
pub(super) const RETREAT_S: f64 = 3.5;
/// A turn in place drifts the body (15 cm measured): no drop this near.
pub(super) const SPIN_DROP_M: f64 = 0.25;
/// Whether the gait spins left whatever the sign. One probe said so;
/// eighteen (2026-09-06, `spinprobe.py`) say right turns work (-12°,
/// -47°, -50°) and that the yaw a timed command yields varies threefold
/// with the phase of the step it starts in — so turns are closed on the
/// yaw in small increments, and the sign is honoured.
pub(super) const SPIN_LEFT_ONLY: bool = false;
/// A spin keeps turning 5–10° after the command ends and settles in
/// 0.5 s (measured): stop this early and let it coast.
pub(super) const SPIN_LEAD_RAD: f64 = 0.15;
/// A pure turn in place stops this short of its goal: the command's
/// slew brings the yaw under the dead zone within a tick or two, so the
/// body coasts little — the chunk's own length is most of the overshoot.
pub(super) const TURN_LEAD_RAD: f64 = 0.10;
/// Command chunk of a turn in place: at 50–60°/s, 0.15 s is 8–9°.
pub(super) const TURN_CHUNK_S: f64 = 0.15;
/// No turn in place with a drop this near, on any side: the body is
/// 0.19 m wide and its legs swing as it turns (see [`Job::turn_in_place`]).
pub(super) const TURN_CLEAR_M: f64 = crate::passage::BODY_HALF_M + crate::passage::LEG_DRIFT_M;

/// `QK_TURN_IN_PLACE=0` turns the old way (kick, then yaw) everywhere.
pub(crate) fn turn_in_place_on() -> bool {
    static V: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *V.get_or_init(|| std::env::var("QK_TURN_IN_PLACE").map(|v| v != "0").unwrap_or(true))
}
pub(super) const STALLED_RAD: f64 = 0.15;

impl Job {
    /// Has the guard a fresh frame along the nose? Wait for one up to
    /// [`LOOK_WAIT_S`] (standing; the head sweeps), true without a guard.
    pub(super) fn looked_ahead(&self, robot: &mut dyn Body) -> bool {
        if self.blind() {
            return true;
        }
        let Some(_) = robot.cliff() else { return true };
        let started = robot.now();
        loop {
            if robot.cliff().is_some_and(|c| c.looked_at(robot.now(), 0.0)) {
                return true;
            }
            if (robot.now() - started).as_secs_f64() >= LOOK_WAIT_S {
                tracing::info!("map explore: the guard has not looked ahead; no kick that way");
                return false;
            }
            let _ = robot.blind_move(&json!({"vx": 0.0, "vyaw": 0.0, "duration_s": 0.25}));
        }
    }

    /// A pure turn in place: yaw alone past the gait's dead zone
    /// ([`quack_duck::body::TURN_IN_PLACE_RAD_S`]), no kick, the body
    /// within a few centimetres — so nothing ahead, a drop least of all,
    /// can refuse it. Closed on the odometry's yaw in short chunks, with
    /// the depth sensor's watch between them as [`Job::spin`] keeps it.
    /// Returns how far it turned toward `sign` (radians), or `None` when
    /// it is switched off (`QK_TURN_IN_PLACE=0`) or there is no yaw to
    /// close on; the caller falls back on the kick when it turned short.
    /// No stand at the end: the caller's.
    pub(super) fn turn_in_place(&mut self, robot: &mut dyn Body, sign: f64, want: f64) -> Option<f64> {
        if !turn_in_place_on() {
            return None;
        }
        let yaw_now = |robot: &dyn Body| robot.cliff().and_then(|c| c.odom_yaw).or_else(|| robot.frame().map(|f| f.yaw));
        let yaw0 = yaw_now(robot)?;
        // The legs swing while the body turns: nowhere near a drop, on any
        // side. Beside the stairwell's rim an alignment turned in place
        // 9 cm from the edge and the duck went in (twin, 2026-09-23); the
        // edge was beside it, where the watch ahead does not look.
        if let Some(near) = self.drop_within_any(robot, TURN_CLEAR_M) {
            tracing::info!(near_m = format!("{near:.2}"), "map explore: a drop this near; no turn in place here");
            return Some(0.0);
        }
        let vyaw = quack_duck::body::TURN_IN_PLACE_RAD_S * sign.signum();
        let goal = (want - TURN_LEAD_RAD).max(0.05);
        // 30°/s is the slowest measured side; a turn that has not got there
        // in twice its time is not turning.
        let budget = 2.0 * want / 0.5 + 1.0;
        let started = robot.now();
        let mut turned = 0.0_f64;
        while (robot.now() - started).as_secs_f64() < budget {
            let _ = robot.blind_move(&json!({"vx": 0.0, "vyaw": vyaw, "duration_s": TURN_CHUNK_S}));
            if let Some(y) = yaw_now(robot) {
                turned = wrap(y - yaw0) * sign.signum();
                if turned >= goal {
                    break;
                }
            }
            if spin_watch() && self.drop_within_any(robot, TURN_CLEAR_M).is_some() {
                tracing::info!("map explore: an edge came near while turning in place; stopping the turn");
                break;
            }
        }
        tracing::debug!(want_deg = format!("{:.0}", want.to_degrees()), turned_deg = format!("{:.0}", turned.to_degrees()), "map explore: turned in place");
        Some(turned)
    }

    /// The nearest drop within `radius` of the body, on any side: a booked
    /// one (its point, from the map's pose) or an edge the sensor sees
    /// (any bearing, recent stand frames). `None` when there is none that
    /// near.
    pub(super) fn drop_within_any(&self, robot: &dyn Body, radius: f64) -> Option<f64> {
        let booked = robot.frame().map(|f| f.pose()).map_or(f64::INFINITY, |(x, y, _)| {
            self.local
                .iter()
                .filter(|(_, r)| *r >= DROP_RADIUS_M)
                .map(|(p, _)| dist2(*p, (x, y)))
                .fold(f64::INFINITY, f64::min)
        });
        let seen = robot.cliff().and_then(|c| c.nearest_hole_m(robot.now())).unwrap_or(f64::INFINITY);
        let near = booked.min(seen);
        (near < radius).then_some(near)
    }

    /// Turn in place toward `sign` by about `want` radians, closed on the
    /// fastest yaw there is (as [`Job::panorama`] does): first the pure
    /// turn in place ([`Job::turn_in_place`]); when that is off or turns
    /// short, the old way — a walking kick when there is room for one, a
    /// short step back otherwise, then yaw only, in short chunks — and a
    /// mapping stand where it ends.
    pub(super) fn spin(&mut self, robot: &mut dyn Body, sign: f64, want: f64) {
        if let Some(turned) = self.turn_in_place(robot, sign, want) {
            if turned >= 0.7 * (want - TURN_LEAD_RAD).max(0.05) {
                let _ = stand(robot, self.turn_stand_s());
                return;
            }
            tracing::info!(turned_deg = format!("{:.0}", turned.to_degrees()), want_deg = format!("{:.0}", want.to_degrees()), "map explore: the turn in place fell short; the kick then");
        }
        let yaw_now = |robot: &dyn Body| {
            robot
                .cliff()
                .and_then(|c| c.odom_yaw)
                .or_else(|| robot.frame().map(|f| f.yaw))
        };
        let Some(yaw0) = yaw_now(robot) else { return };
        let (sign, want) = if SPIN_LEFT_ONLY && sign < 0.0 {
            (1.0, (std::f64::consts::TAU - want).max(0.0))
        } else {
            (sign, want)
        };
        let vyaw = PANO_SPIN * sign.signum();
        let pose = robot.frame().map(|f| f.pose());
        // Tight quarters — a drop under the beak, or something within
        // KICKLESS_ROOM_M ahead by the sensor or the map: no kick at all.
        // The kick advances 12 cm, and off the west rim of the stairwell
        // that was the fall of frozen13 (2026-09-16); between two
        // obstacles it is the bump. Yaw only instead: slow (~17°/s) but
        // the body stays put (the user's rule: near obstacles, turn in
        // place and re-align, never turn while advancing).
        let drop_near = pose.is_some_and(|(x, y, _)| {
            self.local
                .iter()
                .any(|(p, r)| *r >= DROP_RADIUS_M && dist2(*p, (x, y)) < SPIN_DROP_M)
        });
        let tight = drop_near
            || match (robot.frame(), pose) {
                (Some(f), Some(p)) => f.grid().map(|g| self.room_ahead(robot, &g, p).0 < KICKLESS_ROOM_M).unwrap_or(false),
                _ => false,
            };
        // The gait does not turn from a standstill at all (1–2° in 6 s,
        // measured — and a "yaw without a kick" spun for nothing 31 times
        // on frozen15): the legs must be stepping. In tight quarters the
        // step that gets them going is the short one BACK with the yaw
        // (~6 cm, away from what is ahead), when no drop lies there; else
        // half a kick forward, judged by the sensor.
        // Tight quarters too: a short walking kick — judged by the guard,
        // so nothing at the beak or a drop under it is walked into —
        // then yaw alone, which turns in place at ~30°/s once the gait is
        // stepping. The backing turn it used here was a transient of the
        // first half-second (rim4, 2026-09-17: forty "turning by backing"
        // in the passage beside the stairwell for no turn at all; look7
        // at boot the night before), and it is kept only for a refused
        // kick, when there is room behind and no drop on the way.
        let kick_s = if tight { self.policy.tight_kick_s } else { PANO_KICK_S };
        let kick = json!({"vx": 0.3, "vyaw": vyaw, "walk_s": kick_s, "stop_s": 0.0});
        let kicked = if !self.looked_ahead(robot) {
            Err("not looked ahead".to_string())
        } else {
            match pose {
                Some(p) => self.guarded_step(robot, p, &kick),
                None => robot.step(&kick),
            }
        };
        // Nothing blind and nothing backwards with a drop in the sensor's
        // view: the guard does not look behind, and a backing turn beside
        // the stairwell backed 44 cm into it (paper twin seed 6,
        // 2026-09-17; look9 at boot the night before). The books and the
        // map are not enough to say the floor behind is there.
        // How near the nearest drop is — the books' or, seen by the sensor,
        // its edge — sets what blind move is allowed when the kick is
        // refused: under BACK_NO_DROP_NEAR_M nothing; under BACK_NO_DROP_M
        // pulses only (0.8 s of backing with the yaw: about 9° and one or
        // two centimetres, measured 2026-09-16); beyond, the step back
        // that makes room and the backing turn.
        let nearest_drop = {
            let books = pose
                .map(|(x, y, _)| {
                    self.local
                        .iter()
                        .filter(|(_, r)| *r >= DROP_RADIUS_M)
                        .map(|(p, _)| dist2(*p, (x, y)))
                        .fold(f64::INFINITY, f64::min)
                })
                .unwrap_or(f64::INFINITY);
            let seen = robot
                .cliff()
                .and_then(|c| c.nearest(robot.now()))
                .map_or(f64::INFINITY, |d| d.edge_min_m.max(0.1));
            books.min(seen)
        };
        let drop_in_view = nearest_drop < BACK_NO_DROP_M
            && !pose.is_some_and(|p| self.back_on_trusted(p, 1.0, MAKE_ROOM_S));
        // Retreat to turn (2026-09-18, after trust1/trust3): the kick
        // refused with a drop in view, and the trail behind — back along
        // it, blind but onto floor the body walked, until the drop is a
        // guard's lane away, then the kick where the guard allows it.
        // Before this the spin fell through to yaw alone from standstill
        // (2°/s: nothing), 10 s a try, 70 s an alignment (trust1).
        if kicked.is_err() && tight && !self.blind() && nearest_drop < BACK_NO_DROP_M
            && let Some(p) = pose
        {
            let behind = self.trail.iter().rev().take(4000).map(|t| {
                let (dx, dy) = (t.0 - p.0, t.1 - p.1);
                (dx * p.2.cos() + dy * p.2.sin(), dist2(*t, (p.0, p.1)))
            }).filter(|(along, d)| *along < -0.05 && *d > 0.05).map(|(_, d)| d).fold(f64::INFINITY, f64::min);
            tracing::info!(
                nearest_drop = format!("{nearest_drop:.2}"),
                trail_pts = self.trail.len(),
                nearest_behind = format!("{behind:.2}"),
                on_trusted_0_8 = self.back_on_trusted(p, 1.0, 0.8),
                on_trusted_1_5 = self.back_on_trusted(p, 1.0, MAKE_ROOM_S),
                on_trusted_3_5 = self.back_on_trusted(p, 1.0, RETREAT_S),
                "map explore: the kick was refused beside a drop — the trail behind"
            );
        }
        let kicked = if kicked.is_err() && tight && !self.blind() && nearest_drop < BACK_NO_DROP_M
            && pose.is_some_and(|p| self.back_on_trusted(p, 1.0, RETREAT_S))
        {
            tracing::info!(nearest_drop = format!("{nearest_drop:.2}"), "map explore: the kick was refused beside a drop; retreating along the trail to turn there");
            let _ = robot.blind_move(&json!({"vx": -0.3, "vyaw": BACK_VYAW, "duration_s": RETREAT_S}));
            let _ = stand(robot, 1.0);
            match robot.frame().map(|f| f.pose()) {
                Some(p) => self.guarded_step(robot, p, &kick),
                None => robot.step(&kick),
            }
        } else if kicked.is_err() && tight && !self.blind() && drop_in_view
            && (nearest_drop >= BACK_NO_DROP_NEAR_M || pose.is_some_and(|p| self.back_on_trusted(p, 1.0, 0.8)))
        {
            // Pulses: the turn is in the start of the step, and the body
            // stays within a couple of centimetres.
            let yaw_now = |robot: &dyn Body| robot.cliff().and_then(|c| c.odom_yaw).or_else(|| robot.frame().map(|f| f.yaw));
            let y0 = yaw_now(robot);
            let mut n = 0;
            while n < 3 {
                n += 1;
                let _ = robot.blind_move(&json!({"vx": -0.3, "vyaw": 0.7 * sign.signum(), "duration_s": 0.8}));
                robot.sleep(Duration::from_millis(1500));
                if let (Some(a), Some(b)) = (y0, yaw_now(robot))
                    && (wrap(b - a) * sign.signum()) >= (want - SPIN_LEAD_RAD).max(0.2)
                {
                    break;
                }
            }
            tracing::info!(pulses = n, nearest_drop = format!("{nearest_drop:.2}"), "map explore: tight quarters beside a drop; turned by backing pulses, then the kick again");
            match robot.frame().map(|f| f.pose()) {
                Some(p) => self.guarded_step(robot, p, &kick),
                None => robot.step(&kick),
            }
        } else if kicked.is_err() && tight && !self.blind() && !drop_in_view {
            // The kick refused in tight quarters (something under 0.2 m
            // at the beak): make room first — a short step back, which
            // this gait does only with +yaw (5 cm/s), when the books and
            // the map say the floor behind is there — and kick again.
            // The backing turn below was tried 51 times at the mouth of
            // the passage beside the stairwell without turning (rim6,
            // 2026-09-17): the kick is what turns, and it needs 20 cm.
            let room_behind = match robot.frame() {
                Some(f) => {
                    let p = f.pose();
                    let books = self.drop_on_motion(p, -0.3, 0.5, MAKE_ROOM_S).is_none();
                    let wall = f.grid().map(|g| {
                        let c = g.clearance(p.0, p.1, p.2 + std::f64::consts::PI, 1.0);
                        c.by != Blocked::Wall || c.free_m > TIGHT_TURN_BACK_ROOM_M + 0.05 * MAKE_ROOM_S
                    }).unwrap_or(true);
                    books && wall
                }
                None => false,
            };
            if room_behind {
                tracing::info!("map explore: tight quarters; the kick was refused — a short step back to make room, then the kick again");
                let _ = robot.blind_move(&json!({"vx": -0.3, "vyaw": 0.5, "duration_s": MAKE_ROOM_S}));
                let _ = stand(robot, 1.0);
                match robot.frame().map(|f| f.pose()) {
                    Some(p) => self.guarded_step(robot, p, &kick),
                    None => robot.step(&kick),
                }
            } else {
                kicked
            }
        } else {
            kicked
        };
        if kicked.is_err() {
            if tight && !self.blind() && !drop_in_view {
                // The kick refused in tight quarters: the old way, ONE
                // continuous "back with the yaw toward the aim", when
                // there is room behind and no drop on the way. Measured
                // (2026-09-16, backprobe2):
                // with −0.7 the body turns −7.5°/s and does not move; with
                // +0.7 it turns +12…16°/s backing ~5 cm/s. It has to be one
                // command — in 0.5 s chunks the gait restarts each time and
                // never turns (noguard1: four tries of 14 s, the error
                // growing). A yaw alone from standstill turns 2–3°/s.
                // To the right (−yaw) the body stays within centimetres; to
                // the left (+yaw) it backs 5 cm/s, so a left turn is one second
                // at most and the rest comes at the next stand.
                let rate = if sign > 0.0 { 0.21 } else { 0.13 };
                let secs = ((want - SPIN_LEAD_RAD).max(0.2) / rate).clamp(1.0, if sign > 0.0 { 1.0 } else { TIGHT_TURN_MAX_S });
                let clear = match robot.frame() {
                    Some(f) => {
                        let p = f.pose();
                        let books = self.drop_on_motion(p, -0.3, BACK_VYAW.copysign(sign), secs).is_none();
                        let wall = f.grid().map(|g| {
                            let c = g.clearance(p.0, p.1, p.2 + std::f64::consts::PI, 1.0);
                            c.by != Blocked::Wall || c.free_m > TIGHT_TURN_BACK_ROOM_M + if sign > 0.0 { 0.05 * secs } else { 0.0 }
                        }).unwrap_or(true);
                        books && wall
                    }
                    None => true,
                };
                if clear {
                    tracing::info!(drop_near, sign, want, secs, "map explore: tight quarters; turning by backing with the yaw");
                    let _ = robot.blind_move(&json!({"vx": -0.3, "vyaw": BACK_VYAW.copysign(sign), "duration_s": secs}));
                    let _ = stand(robot, self.turn_stand_s());
                    return;
                }
                tracing::info!("map explore: tight quarters; no room behind to turn by backing — the kick it is");
        
            }
            let back = match pose {
                Some(p) if !drop_in_view => self.back_secs_clear(p, 1.0, 1.0),
                Some(_) => None,
                None => Some(1.0),
            };
            if let Some(secs) = back {
                let _ = robot.blind_move(&json!({"vx": -0.3, "vyaw": BACK_VYAW, "duration_s": secs}));
            }
            // Else: no blind step back toward a drop — and no yaw alone
            // from standstill either: it turns 2°/s and costs SPIN_MAX_S
            // for nothing. The next plan says what to do.
            if back.is_none() && drop_in_view {
                self.turns_refused_at_drop += 1;
                tracing::info!(in_a_row = self.turns_refused_at_drop, "map explore: the kick was refused beside a drop and there is no way back; no turn here");
                return;
            }
        }
        let started = robot.now();
        let goal = (want - SPIN_LEAD_RAD).max(0.2);
        while (robot.now() - started).as_secs_f64() < SPIN_MAX_S {
            let _ = robot.blind_move(&json!({"vx": 0.0, "vyaw": vyaw, "duration_s": PANO_CHUNK_S}));
            // A turn in place is blind and drifts about 15 cm: beside the
            // stairwell that is the whole margin. Between chunks the depth
            // sensor still speaks — an edge this near ends the turn where
            // it stands (three falls on the twin were blind manoeuvres
            // beside the hole, not legs; 2026-09-08).
            if spin_watch()
                && robot
                    .cliff()
                    .and_then(|c| c.drop_within(robot.now(), 0.0, SPIN_WATCH_FOV_RAD))
                    .is_some_and(|d| d.edge_min_m < SPIN_WATCH_M)
            {
                tracing::info!("map explore: an edge came near while turning in place; stopping the turn");
                break;
            }
            if yaw_now(robot).is_some_and(|y| wrap(y - yaw0).abs() >= goal) {
                break;
            }
        }
        let _ = stand(robot, self.turn_stand_s());
    }

    /// The longest step back, up to `secs`, whose simulated path crosses no
    /// drop on the books: the full one, half of it, or the shortest that
    /// still moves the body. The gait swings while backing, and a full
    /// step swung the twin's path onto the stairwell's corner while a
    /// short one was clear — with neither taken the duck stood refusing
    /// the same leg to the budget (paper twin, hole beside a passage).
    /// The shortest one is judged with half the margin: a body cornered
    /// between a wall at the beak and a drop behind has that step or none,
    /// and the drops on the books sit at the sensor's near estimate of the
    /// edge, inside it as often as not.
    pub(super) fn back_secs_clear(&self, pose: (f64, f64, f64), secs: f64, side: f64) -> Option<f64> {
        // No step back at all with a drop on the books this near: the
        // books are points, a curved step back slips between them, and
        // the guard does not look behind (paper twin seed 11, 2026-09-17:
        // backed into the stairwell from beside it; look9 the night
        // before).
        // Near a booked drop the long steps back are out (the books are
        // points, a curved step slips between them); the SHORT one —
        // 0.8 s, six centimetres — stays, judged by the books' own path
        // check with a full margin: against the west wall with the
        // stairwell 30 cm to the side it was refused for six minutes
        // (explmap2, 2026-09-19: "a drop lies where a step back would
        // go; none taken", the sensor saying 0.19 m ahead all along).
        let near_drop = self.local.iter().any(|(p, r)| *r >= DROP_RADIUS_M && dist2(*p, (pose.0, pose.1)) < BACK_NO_DROP_M)
            && !self.back_on_trusted(pose, side, BACK_SHORT_S);
        if near_drop {
            return (self.drop_on_back(pose, side, BACK_SHORT_S, DROP_PATH_MARGIN_M).is_none()).then_some(BACK_SHORT_S);
        }
        [(secs, DROP_PATH_MARGIN_M), (secs / 2.0, DROP_PATH_MARGIN_M), (BACK_SHORT_S, DROP_PATH_MARGIN_M / 2.0)]
            .into_iter()
            .filter(|(s, _)| *s <= secs)
            .find(|(s, margin)| self.drop_on_back(pose, side, *s, *margin).is_none())
            .map(|(s, _)| s)
    }

    /// The phases of a step back to `side`: +1 swings the tail right and
    /// the nose left (the only way the gait backs from a standstill), -1
    /// the mirror image, 0 straight — the last two after half a second of
    /// the first, which gets the gait stepping (measured on the twin,
    /// 2026-09-08: +0.7 then -0.7 backed 0.23 m turning -87°, +0.7 then
    /// straight backed 0.23 m turning -13°; -0.7 from a standstill moved
    /// nothing). Each phase is `(vx, vyaw, seconds)`.
    pub(super) fn back_phases(side: f64, secs: f64) -> Vec<(f64, f64, f64)> {
        if side > 0.0 || secs <= BACK_TRIGGER_S {
            vec![(-0.3, BACK_VYAW, secs)]
        } else {
            let rest = if side < 0.0 { -BACK_VYAW } else { 0.0 };
            vec![(-0.3, BACK_VYAW, BACK_TRIGGER_S), (-0.3, rest, secs - BACK_TRIGGER_S)]
        }
    }

    /// Play a step back to `side` through the gait model: the first drop
    /// on the books it would cross, and where it ends.
    pub(super) fn back_path(&self, (x, y, yaw): (f64, f64, f64), side: f64, secs: f64, margin: f64) -> (Option<(f64, f64)>, Vec<(f64, f64)>) {
        let (mut px, mut py, mut h) = (x, y, yaw);
        let mut samples = Vec::new();
        for (i, (_, vyaw, dur)) in Self::back_phases(side, secs).iter().enumerate() {
            let last = i + 1 == Self::back_phases(side, secs).len();
            let (v, w) = (-BACK_M_PER_S, 0.6 * vyaw);
            let mut t = 0.0;
            while t < dur + if last { DROP_PATH_EXTRA_S } else { 0.0 } {
                px += v * 0.1 * h.cos();
                py += v * 0.1 * h.sin();
                h += w * 0.1;
                samples.push((px, py));
                for ((dx, dy), r) in &self.local {
                    if *r >= DROP_RADIUS_M && dist2((px, py), (*dx, *dy)) < r + margin {
                        return (Some((*dx, *dy)), samples);
                    }
                }
                t += 0.1;
            }
        }
        (None, samples)
    }

    pub(super) fn drop_on_back(&self, pose: (f64, f64, f64), side: f64, secs: f64, margin: f64) -> Option<(f64, f64)> {
        self.back_path(pose, side, secs, margin).0
    }

    /// Whether a step back to `side` runs over the trail: every sample of
    /// its path within [`TRAIL_NEAR_M`] of a trail point — the body came
    /// in that way, so backing out that way is the one blind manoeuvre
    /// with a known floor under it.
    pub(super) fn back_on_trail(&self, pose: (f64, f64, f64), side: f64, secs: f64) -> bool {
        if self.trail.len() < 4 {
            return false;
        }
        let (_, samples) = self.back_path(pose, side, secs, DROP_PATH_MARGIN_M);
        samples.iter().all(|p| self.trail.iter().rev().take(4000).any(|t| dist2(*t, *p) < TRAIL_NEAR_M))
    }

    /// The side to step back to: `prefer` first (the caller's reason —
    /// away from a drop, the mirror of the arc that met the wall), then
    /// the mirror of the last leg, then each way; the first whose path
    /// crosses no drop and lies on the trail, else the first that crosses
    /// no drop, else none.
    pub(super) fn choose_back_side(&self, pose: (f64, f64, f64), prefer: Option<f64>, secs: f64, allowed: &[f64]) -> Option<f64> {
        let mut order: Vec<f64> = Vec::new();
        if let Some(p) = prefer {
            order.push(p);
        }
        if self.last_leg_vyaw.abs() > 0.3 {
            order.push(-self.last_leg_vyaw.signum());
        }
        order.extend([1.0, -1.0, 0.0]);
        let mut seen: Vec<f64> = Vec::new();
        order.retain(|s| allowed.contains(s) && if seen.contains(s) { false } else { seen.push(*s); true });
        let clear: Vec<f64> = order.iter().copied().filter(|s| self.back_secs_clear(pose, secs, *s).is_some()).collect();
        let on_trail = clear.iter().copied().find(|s| self.back_on_trail(pose, *s, secs));
        // Near a drop, a blind step back is allowed only over floor the
        // body has walked: two falls into the twin's stairwell were steps
        // back judged clear on the books, off the trail, with the pose a
        // little wrong (2026-09-08). Off the trail and near a drop, no
        // step back — the passage primitive and the turn in place are
        // what is left, and they are seen, not blind.
        if back_trail_only() && on_trail.is_none() && self.drop_beside(pose) {
            return None;
        }
        on_trail.or_else(|| clear.first().copied())
    }

    /// The first drop on the books the body would cross moving at
    /// `(vx, vyaw)` for `secs` from `pose`, through the gait model —
    /// forward 0.12 m/s at vx 0.3 and 0.65 rad/s per unit of yaw; backing
    /// 0.08 m/s with 0.6 of the yaw, and only with a positive yaw at all
    /// (measured on the twin).
    /// The nearest drop to the body — booked (from the map's pose) or seen
    /// (the nearest true hole of the recent stand frames) — as its
    /// distance and its bearing off the nose.
    pub(super) fn nearest_drop_bearing(&self, robot: &dyn Body, (x, y, yaw): (f64, f64, f64)) -> Option<(f64, f64)> {
        let booked = self
            .local
            .iter()
            .filter(|(_, r)| *r >= DROP_RADIUS_M)
            .map(|((dx, dy), _)| (dist2((x, y), (*dx, *dy)), wrap((dy - y).atan2(dx - x) - yaw)))
            .min_by(|a, b| a.0.total_cmp(&b.0));
        let seen = robot.cliff().and_then(|c| {
            let now = robot.now();
            c.recent
                .iter()
                .filter(|f| now.duration_since(f.at) <= crate::cliff::MEMORY && !f.moving)
                .flat_map(|f| {
                    f.drops.iter().filter(move |d| {
                        !f.obstacles.iter().any(|o| wrap(o.bearing - d.bearing).abs() < 0.2 && (o.range_m - d.range_m).abs() < 0.25)
                    })
                })
                .map(|d| (if d.edge_min_m > 0.0 { d.edge_min_m } else { (d.range_m - 0.10).max(0.0) }, d.bearing))
                .min_by(|a, b| a.0.total_cmp(&b.0))
        });
        match (booked, seen) {
            (Some(a), Some(b)) => Some(if a.0 <= b.0 { a } else { b }),
            (a, b) => a.or(b),
        }
    }

    /// Off the rim first: a drop this near the body (see [`RIM_OFF_M`])
    /// is never stood beside. The standing gait creeps — on the twin a
    /// centimetre every twenty seconds or so — and both falls of
    /// 2026-09-23/24 were a duck standing still 3–12 cm from the rim, every
    /// turn refused that near and no way back, for one to three minutes.
    /// So before anything else the body moves away from it: backing when
    /// the drop is ahead, on when it is behind, an arc turning away when
    /// it is beside — each only if the books put no drop nearer on the way.
    /// True when it moved.
    pub(super) fn off_the_rim(&mut self, robot: &mut dyn Body, pose: (f64, f64, f64)) -> bool {
        let Some((near, bearing)) = self.nearest_drop_bearing(&*robot, pose) else { return false };
        if near >= RIM_OFF_M {
            self.rim_offs = 0;
            return false;
        }
        if self.rim_offs >= RIM_OFF_TRIES {
            return false;
        }
        self.rim_offs += 1;
        let (vx, vyaw, secs) = if bearing.abs() <= 1.05 {
            (-0.3, BACK_VYAW, 1.5)
        } else if bearing.abs() >= 2.1 {
            (0.3, 0.0, 1.0)
        } else {
            (0.3, -0.5 * bearing.signum(), 1.0)
        };
        // Along the way, no booked drop nearer than the one it leaves.
        let nearer = self.local.iter().filter(|(_, r)| *r >= DROP_RADIUS_M).any(|((dx, dy), _)| {
            let (v, w) = if vx > 0.0 { (GAIT_M_PER_S, 0.65 * vyaw) } else { (-BACK_M_PER_S, 0.6 * vyaw) };
            let (mut px, mut py, mut h) = pose;
            let mut t = 0.0;
            let mut hit = false;
            while t < secs {
                px += v * 0.1 * h.cos();
                py += v * 0.1 * h.sin();
                h += w * 0.1;
                hit |= dist2((px, py), (*dx, *dy)) < near.min(DROP_RADIUS_M) - 0.01;
                t += 0.1;
            }
            hit
        });
        if nearer {
            tracing::info!(near_m = format!("{near:.2}"), bearing_deg = format!("{:.0}", bearing.to_degrees()), "map explore: a rim this near, and no way off it the books allow");
            return false;
        }
        tracing::info!(near_m = format!("{near:.2}"), bearing_deg = format!("{:.0}", bearing.to_degrees()), vx, vyaw, secs, "map explore: a rim this near: off it first");
        let _ = if vx < 0.0 {
            robot.blind_move(&json!({"vx": vx, "vyaw": vyaw, "duration_s": secs}))
        } else {
            let leg = json!({"vx": vx, "vyaw": vyaw, "walk_s": secs, "stop_s": 0.0});
            self.guarded_step(robot, pose, &leg)
        };
        self.going = None;
        true
    }

    pub(super) fn drop_on_motion(&self, pose: (f64, f64, f64), vx: f64, vyaw: f64, secs: f64) -> Option<(f64, f64)> {
        self.drop_on_motion_margin(pose, vx, vyaw, secs, DROP_PATH_MARGIN_M)
    }

    pub(super) fn drop_on_motion_margin(
        &self,
        (x, y, yaw): (f64, f64, f64),
        vx: f64,
        vyaw: f64,
        secs: f64,
        margin: f64,
    ) -> Option<(f64, f64)> {
        let (v, w) = if vx > 0.0 {
            (GAIT_M_PER_S * vx / 0.3, 0.65 * vyaw)
        } else if vyaw > 0.3 {
            (-BACK_M_PER_S, 0.6 * vyaw)
        } else {
            (0.0, 0.0)
        };
        let walk_s = secs;
        let (mut px, mut py, mut h) = (x, y, yaw);
        let mut t = 0.0;
        while t < walk_s + DROP_PATH_EXTRA_S {
            px += v * 0.1 * h.cos();
            py += v * 0.1 * h.sin();
            h += w * 0.1;
            for ((dx, dy), r) in &self.local {
                if *r < DROP_RADIUS_M {
                    continue;
                }
                // A drop BESIDE or BEHIND the body, within the margin,
                // cannot be the reason to refuse a leg: the body is there,
                // on floor, so the point is off by at least that much —
                // the strike judges it at the stand, the sensor judges
                // the leg. Refusing on it caged the duck beside the rim
                // it had just walked along (frozen10, 2026-09-16: 39
                // refusals at one spot, the body 11 cm from the point).
                // Never a drop AHEAD: exempting those let a spin's kick
                // step off the west rim into the stairwell (frozen13).
                let bearing = wrap((dy - y).atan2(dx - x) - yaw);
                if dist2((x, y), (*dx, *dy)) < r + margin && bearing.abs() > 1.05 {
                    continue;
                }
                if dist2((px, py), (*dx, *dy)) < r + margin {
                    return Some((*dx, *dy));
                }
            }
            t += 0.1;
        }
        None
    }

    /// Turn to `target` heading, closed on the yaw: measure after a
    /// settle, a coarse spin for a large error, one short guided arc
    /// pulse (or a blind step back when there is no room ahead) for a
    /// small one, measure again — up to [`ALIGN_ITERS`] times. Timed
    /// commands are not repeatable on this gait (measured: threefold).
    pub(super) fn align(&mut self, robot: &mut dyn Body, target: f64) -> bool {
        let yaw_now = |robot: &dyn Body| {
            robot
                .cliff()
                .and_then(|c| c.odom_yaw)
                .or_else(|| robot.frame().map(|f| f.yaw))
        };
        // `target` is a heading on the map; the loop closes on the
        // odometry's yaw, which is the fastest there is but sits in its
        // own frame — the two agree only until a loop closure or an
        // adopted map turns the map's frame. So the target is carried
        // into the odometry's frame once, here, by the offset the two
        // yaws show at this stand: with the map's yaw alone (the old way)
        // an aim 86° off the nose was "aligned, ok" at 108° (2026-09-15).
        let _ = robot.blind_move(&json!({"vx": 0.0, "vyaw": 0.0, "duration_s": ALIGN_SETTLE_S}));
        let target = match (robot.cliff().and_then(|c| c.odom_yaw), robot.frame().map(|f| f.yaw)) {
            (Some(odom), Some(map)) => target + wrap(odom - map),
            _ => target,
        };
        for _ in 0..ALIGN_ITERS {
            let _ = robot.blind_move(&json!({"vx": 0.0, "vyaw": 0.0, "duration_s": ALIGN_SETTLE_S}));
            let Some(yaw) = yaw_now(robot) else { return false };
            let e = wrap(target - yaw);
            // A left turn (e > 0) may get its own tolerance (`Policy`).
            let tol = if e > 0.0 { self.policy.align_tol_left_rad } else { ALIGN_TOL_RAD };
            if e.abs() <= tol {
                return true;
            }
            if e.abs() > 0.6 {
                self.spin(robot, e.signum(), e.abs());
                continue;
            }
            // A fine correction: a pure turn in place when the gait turns
            // so — nothing to refuse it at the mouth of a passage — else
            // a short guarded kick, then yaw alone closed on the heading.
            if let Some(turned) = self.turn_in_place(robot, e.signum(), e.abs())
                && turned > 0.5 * e.abs()
            {
                continue;
            }
            // The kick: the walking pulse it replaced turned
            // by anything from −1° to +18° with the sign it was given and
            // by +13° against it, and advanced 6 cm every time (turnprobe,
            // 2026-09-18) — at the mouth of a passage that was the whole
            // margin. The kick then yaw turns the way it is told, 9–28°/s,
            // within 5 cm (same probe). No kick to be had: no correction.
            let pose = robot.frame().map(|f| f.pose());
            let walk_s = if self.policy.align_kick { TIGHT_KICK_S } else { ALIGN_PULSE_S };
            let kick = json!({"vx": 0.3, "vyaw": 0.7 * e.signum(), "walk_s": walk_s, "stop_s": 0.0});
            let kicked = if !self.looked_ahead(robot) {
                Err("not looked ahead".to_string())
            } else {
                match pose {
                    Some(p) => self.guarded_step(robot, p, &kick),
                    None => robot.step(&kick),
                }
            };
            if kicked.is_err() {
                return false;
            }
            if !self.policy.align_kick {
                continue;
            }
            let started = robot.now();
            while (robot.now() - started).as_secs_f64() < ALIGN_YAW_MAX_S {
                let _ = robot.blind_move(&json!({"vx": 0.0, "vyaw": 0.7 * e.signum(), "duration_s": PANO_CHUNK_S}));
                let Some(y) = yaw_now(robot) else { break };
                let left = wrap(target - y);
                // Done, or past it: the momentum carries the rest.
                if left.abs() <= ALIGN_LEAD_RAD || left.signum() != e.signum() {
                    break;
                }
            }
        }
        yaw_now(robot).is_some_and(|y| wrap(target - y).abs() <= ALIGN_TOL_RAD * 1.5)
    }

    /// For the log: the room the sensor and the map leave ahead, in the
    /// doorway lane, and whether the map calls this a doorway.
    pub(super) fn room_ahead(&self, robot: &dyn Body, grid: &Grid, (x, y, yaw): (f64, f64, f64)) -> (f64, bool) {
        let mut room = grid.clearance(x, y, yaw, 3.0);
        let mut room_m = if room.by == Blocked::Wall { room.free_m } else { f64::INFINITY };
        if let Some(o) = robot
            .cliff()
            .and_then(|c| c.obstacle_in_lane(robot.now(), 0.0, gap_lane_half_m()))
        {
            room_m = room_m.min(o.range_m);
        }
        // A drop's edge ahead sizes the leg as a wall does: the leg then
        // walks up to the guard's margin from the edge instead of being
        // proposed at full length and refused, backed off, re-proposed —
        // the to-and-fro in the corridor before the stairwell (frozen18,
        // 2026-09-16). 0.12 under the near estimate, so that the leg's
        // own reserve lands the body 0.40 m short of it, as the guard asks.
        if let Some(d) = robot.cliff().and_then(|c| c.drop_in_lane(robot.now(), 0.0, lane_half_m())) {
            room_m = room_m.min((d.edge_min_m - 0.12).max(0.0));
        }
        room = grid.clearance(x, y, yaw + std::f64::consts::FRAC_PI_2, 1.0);
        let l = room;
        let r = grid.clearance(x, y, yaw - std::f64::consts::FRAC_PI_2, 1.0);
        (room_m, l.by == Blocked::Wall && r.by == Blocked::Wall && l.free_m + r.free_m < gap_max_m())
    }

    /// No way out but back: less than [`BOXED_M`] of floor ahead AND to
    /// either side, by the map's walls and the sensor's obstacles. Only
    /// then is a step back the move; otherwise a turn in place toward the
    /// free side and a leg forward (the user's rule, 2026-09-16: the step
    /// back only when there is no way out ahead or sideways).
    pub(super) fn boxed_in(&self, robot: &dyn Body, grid: &Grid, (x, y, yaw): (f64, f64, f64)) -> bool {
        let map_free = |bearing: f64| {
            let c = grid.clearance(x, y, yaw + bearing, 1.0);
            if c.by == Blocked::Wall { c.free_m } else { f64::INFINITY }
        };
        let sensor_free = |bearing: f64| {
            robot
                .cliff()
                .and_then(|c| c.obstacle_within(robot.now(), bearing, 0.5))
                .map(|o| o.range_m)
                .unwrap_or(f64::INFINITY)
        };
        [0.0, std::f64::consts::FRAC_PI_2, -std::f64::consts::FRAC_PI_2]
            .iter()
            .all(|b| map_free(*b).min(sensor_free(*b)) < BOXED_M)
    }

    /// The way to turn when the way on is blocked: the configured hand —
    /// the same side every time gets around an obstacle and along a wall
    /// to the next doorway — unless the body has no room to swing there.
    pub(super) fn turn_toward(&self, grid: &Grid, (x, y, yaw): (f64, f64, f64)) -> f64 {
        let hand = grid.clearance(x, y, yaw + self.turn * std::f64::consts::FRAC_PI_2, 1.0);
        let sign = if hand.by == Blocked::Wall && hand.free_m < TURN_ROOM_M {
            -self.turn
        } else {
            self.turn
        };
        0.7 * sign
    }

    pub(super) fn back_off(&mut self, robot: &mut dyn Body, grid: &Grid, prefer: Option<f64>) -> bool {
        let now = robot.now();
        if self.last_back.is_some_and(|t| now - t < BACK_EVERY) {
            return false;
        }
        let (side, secs) = match robot.frame() {
            Some(f) => {
                let pose = f.pose();
                let allowed: &[f64] = if back_sides() { &[1.0, -1.0, 0.0] } else { &[1.0] };
                // Near a drop and off the trail, only the shortest step
                // back, with the drop ahead (see `drop_beside`).
                let near_drop = back_trail_only()
                    && self.local.iter().any(|(p, r)| *r >= DROP_RADIUS_M && dist2(*p, (pose.0, pose.1)) < BACK_DROP_NEAR_M);
                let max_s = if near_drop && !self.back_on_trail(pose, 1.0, back_s()) { BACK_SHORT_S.min(back_s()) } else { back_s() };
                let side = if back_sides() || back_trail_only() {
                    self.choose_back_side(pose, prefer, max_s, allowed)
                } else {
                    Some(1.0)
                };
                match side.and_then(|s| self.back_secs_clear(pose, max_s, s).map(|secs| (s, secs))) {
                    Some(v) => v,
                    None => {
                        tracing::info!("map explore: a drop lies where a step back would go; none taken");
                        return false;
                    }
                }
            }
            None => (1.0, back_s()),
        };
        self.last_back = Some(now);
        tracing::info!(side, secs, on_trail = robot.frame().is_some_and(|f| self.back_on_trail(f.pose(), side, secs)), "map explore: stepping back");
        for (vx, vyaw, dur) in Self::back_phases(side, secs) {
            let _ = robot.blind_move(&json!({"vx": vx, "vyaw": vyaw, "duration_s": dur}));
        }
        // ...then back onto the line: turn in place to the aim the leg
        // had, and let the planner speak from the stand. The step back's
        // own yaw is a 40° swing; walking on from there toward "the most
        // open floor" was the lurch past the obstacle the user saw on
        // house1 (2026-09-15) — a step back corrects the line, it does
        // not choose a new one.
        if back_reorient() {
            if let (Some(aim), Some(f)) = (self.last_aim, robot.frame()) {
                let (x, y, _) = f.pose();
                let ok = self.align(robot, (aim.1 - y).atan2(aim.0 - x));
                tracing::info!(ok, aim = ?aim, "map explore: stepped back, re-aimed");
            }
            return true;
        }
        // ...then toward the most open floor the map shows, not simply
        // straight on: the yaw a step back needs is already a 40°
        // correction, and where that leaves the beak is chance. The
        // planner has its say after the stand, a full mapping stop — blind
        // manoeuvres are where the odometry the mapper leans on goes
        // wrong, and a scan right after is what re-anchors it.
        self.head_for_space(robot, grid)
    }

    /// Where the map shows the most free floor from where the duck stands:
    /// the heading, sampled all around, with the longest run of known free
    /// cells before a wall or the unknown — unknown beyond is what
    /// exploring is for, a wall is not.
    pub(super) fn freest_heading(grid: &Grid, x: f64, y: f64) -> Option<f64> {
        (0..24)
            .map(|k| {
                let h = f64::from(k) * std::f64::consts::TAU / 24.0;
                (h, grid.clearance(x, y, h, 3.0).free_m)
            })
            .filter(|(_, free)| *free >= SPACE_MIN_M)
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(h, _)| h)
    }

    /// Turn toward the most open floor and take one guarded straight leg
    /// there, ending with a mapping stand. `true` when the leg walked.
    pub(super) fn head_for_space(&mut self, robot: &mut dyn Body, grid: &Grid) -> bool {
        let Some(frame) = robot.frame() else {
            return false;
        };
        let (x, y, yaw) = frame.pose();
        let Some(heading) = Self::freest_heading(grid, x, y) else {
            return false;
        };
        let err = wrap(heading - yaw);
        if err.abs() > straight_rad() {
            let _ = self.guarded_step(robot, (x, y, yaw), &json!({"vx": 0.3, "vyaw": 0.7_f64.copysign(err), "walk_s": (err.abs() / 0.5).clamp(1.5, 3.0), "stop_s": 1.0}));
        }
        let pose = robot.frame().map(|f| f.pose()).unwrap_or((x, y, yaw));
        self.guarded_step(robot, pose, &json!({"vx": 0.3, "vyaw": 0.0, "walk_s": BACK_ON_S, "stop_s": FRONTIER_STOP_S, "centre": true}))
            .is_ok()
    }
}
