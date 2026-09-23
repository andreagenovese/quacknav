//! Split out of `explore.rs` on 2026-09-18 (the user's: one core, the modes apart, shared only what is identical). Behaviour unchanged by construction.

use super::*;

/// The look-ahead point on the path that sets the heading.
pub(super) const LOOKAHEAD_M: f64 = 0.4;
/// The same, on a journey, where it is the width of the weave. Measured in
/// the empty flat (2026-09-13): an aim taken from the path sits 0.54 m away
/// at the median — a quarter of them inside 0.29 m — and a leg walks 0.18 m,
/// so the bearing to it swings by tens of degrees with every step and the
/// duck oscillates about its own line. (An aim taken from the straight line
/// to the stand sits 1.27 m out and is 23° off; one from the path is 42°.)
/// The path is planned through known floor, so aiming further along it
/// costs no safety — the leg's guards still judge the step.
///
/// **Measured, and the cure is worse than the disease: it stays at 0.4 m.**
/// Twelve journeys in the empty flat aiming a metre along the path against
/// twenty aiming 0.4: median 86 s against 80, 0.036 m/s made good against
/// 0.040, and 2.44 m walked per metre against 2.11 (one-sided p = 0.84 —
/// no evidence of a gain in either statistic). A far aim holds the heading
/// steady but points across the corners the grid path goes round, and the
/// leg that follows it is refused or has to come back. `QK_GOAL_LOOKAHEAD_M`
/// to try it again on a house with fewer corners.
pub(super) const GOAL_LOOKAHEAD_M: f64 = 0.4;
/// Walking to a goal over floor it has already mapped, the duck could
/// skip the stand between legs — a stand is there so the stop reaches the
/// map, and on known floor there is nothing to add — standing only every
/// fifth leg so the mapper can still judge the pose.
///
/// **Measured twice, and the answer changed: on by default**
/// (`QK_FAST_GOAL=0` turns it off). Measured first in September, it did
/// not pay — 221 and 192 s standing against 235 and 214 not, the duck
/// walking further for it and bumping more. But the belief was 0.70 m
/// from the truth in those runs, and the stand was paying for two things
/// at once: the stop reaching the map, *and* a corrected pose in front of
/// the next leg's plan. With maploc's correction now held to an absolute
/// bar the pose stays near 8 cm on its own, and the stand pays for one
/// thing only.
///
/// Measured again over twenty three-metre journeys, ten each way
/// (2026-09-12): speed made good 0.024 m/s standing every leg against
/// **0.032** standing every fifth, faster in 78 of the 100 pairings
/// (one-sided permutation p = 0.016), and 29 % faster counting all the
/// journeys end to end. Half a journey used to be the duck standing
/// still — 53 % of the seconds — and it is now 11 %.
///
/// The price is real and worth knowing: the duck wanders further (2.60 m
/// walked per metre made good against 2.12) and the pose drifts about
/// half again as much (median 10.8 cm across the journeys against 7.4),
/// which is why the fifth leg still stands. It arrives no less accurately
/// — 12 cm against 14.
pub(super) const FAST_STAND_EVERY_LEGS: u32 = 5;
/// A journey's route is kept between plans (the user, 2026-09-16: too
/// many re-plans make a walking duck erratic; a re-plan only for an
/// obstacle) unless the books changed, a leg was refused, the body is
/// more than [`KEEP_ROUTE_OFF_M`] off it, its first metre is no longer
/// passable, or it is older than this. `QK_KEEP_ROUTE=0` re-plans every
/// stand as before.
pub(super) const KEEP_ROUTE_S: f64 = 30.0;
pub(super) const KEEP_ROUTE_OFF_M: f64 = 0.40;
pub(super) fn keep_route() -> bool {
    std::env::var("QK_KEEP_ROUTE").map(|v| v != "0").unwrap_or(true)
}
/// A three-way switch: `1` on, `0` off, unset the caller's default.
pub(super) fn switch(name: &str) -> Option<bool> {
    match std::env::var(name).as_deref() {
        Ok("1") => Some(true),
        Ok("0") => Some(false),
        _ => None,
    }
}
pub(super) fn lookahead_m() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| knob("QK_LOOKAHEAD_M", LOOKAHEAD_M))
}
pub(super) fn goal_lookahead_m(follow: bool) -> f64 {
    knob("QK_GOAL_LOOKAHEAD_M", if follow { 0.3 } else { GOAL_LOOKAHEAD_M })
}
/// The turn in place when there is no room for a leg and no aim to turn
/// to: an eighth of a circle, then the next plan says where. A quarter
/// turn was the old way's, when every turn cost a kick and a stand; the
/// turn in place costs a second, and a quarter turn often overshot the
/// way on and came back (the user's eye on the twin, 2026-09-23).
pub(super) const NO_ROOM_TURN_RAD: f64 = std::f64::consts::FRAC_PI_4;
/// How far along the path the string may be pulled (see
/// [`smooth_path`]): a metre. At two (the straight look) the aim cut the
/// grid path's corners by up to a body's width and was held there for
/// metres; the planned route is the one with the margins in it (the
/// user's, 2026-09-23: "the duck should stay truer to the green line").
pub(super) const STRING_PULL_M: f64 = 1.0;
pub(super) fn string_pull_m() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| knob("QK_STRING_PULL_M", STRING_PULL_M))
}
/// Within this of a drop, booked or seen, no string is pulled and no aim
/// held: the aim is a step along the route.
pub(super) const STRING_NEAR_DROP_M: f64 = 0.6;
pub(super) fn straight_look_m() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| knob("QK_STRAIGHT_LOOK_M", STRAIGHT_LOOK_M))
}
/// Go back, every so often, to somewhere already mapped.
///
/// A loop closes only where the duck passes near where it has been, and
/// frontier exploration never goes back on purpose — so how many closures
/// a run gets is an accident of its shape, and that accident is the whole
/// difference between a good map and a smeared one. Measured over seven
/// recordings of one house: the two worst runs closed fifteen loops each
/// and mapped 12.6 % and 21.6 % of their walls more than 10 cm out, while
/// runs with nineteen to thirty-three closures came in between 1.2 % and
/// 4.7 % (2026-09-11). Nothing in the solver fixed it; going back does
/// not have to be left to chance.
///
/// `QK_REANCHOR=0` turns it off, for measuring what it bought.
pub(super) const ANCHOR_EVERY_S: f64 = 180.0;
/// How far back along the trail the destination must lie — in trail
/// points, each [`TRAIL_STEP_M`] apart, so this is metres walked since.
/// Far enough back that the submaps there are old ones, which is what a
/// closure needs; the nearest such point is chosen, so the detour is
/// short.
/// Ten metres of walking back. Five was tried — it is still six submaps
/// back, and easier to find near the duck — and it made things worse:
/// with more returns the closure count rose and the maps did not follow,
/// one run closing twenty-two loops and still misplacing 17.2 % of its
/// walls with 11.7 % of them doubled, which is the signature of closures
/// that are simply wrong. More chances to close is also more chances to
/// close against the wrong place.
pub(super) const ANCHOR_BACK: usize = 200;
/// And not worth crossing the house for.
pub(super) const ANCHOR_MAX_M: f64 = 4.0;
/// When nothing old is near enough, ask again soon rather than after the
/// full wait: the duck is walking, and what was out of reach a moment ago
/// may not be now.
pub(super) const ANCHOR_RETRY_S: f64 = 30.0;
/// A full stop at the anchor. Ten seconds was tried, to let the visit
/// become a submap of its own (the manager freezes one after eight
/// seconds standing), and measured worse — see `ANCHOR_BACK`.
pub(super) const ANCHOR_STAND_S: f64 = 6.0;
/// Close enough to count as arrived.
pub(super) const ANCHOR_ARRIVE_M: f64 = 0.35;
/// Aim at the farthest point of the path the body can walk to in a
/// straight line, instead of the one a fixed number of cells ahead.
///
/// A grid path is a staircase, so a point eight cells along it sits up to
/// half a quadrant off the direction of travel, and alternates: measured
/// on the twin, three metres of journey cost thirteen turns in place, each
/// asking for 64° to 101° of heading change. Pulling the string straight
/// is the classic answer and it costs one lane test per candidate.
pub(super) fn reanchor() -> bool {
    std::env::var("QK_REANCHOR").map(|v| v != "0").unwrap_or(true)
}
pub(super) fn smooth_path() -> bool {
    std::env::var("QK_SMOOTH_PATH").map(|v| v != "0").unwrap_or(true)
}
/// How long a hurried leg stands. Zero is fastest and bumps; the stand is
/// also what puts a fresh frame and a corrected pose in front of the next
/// plan. `QK_FAST_STAND_S` to measure the middle ground.
pub(super) fn fast_stand_s() -> f64 {
    knob("QK_FAST_STAND_S", 0.0)
}
/// Fast mode (see [`Job::fast`]) — the stands are for the pose alone:
/// none after a leg, one of [`FAST_POSE_STAND_S`] every
/// [`FAST_POSE_EVERY_S`] or [`FAST_POSE_EVERY_M`], one after a refusal,
/// one when the sensor sees something in the lane within
/// [`FAST_STOP_AHEAD_M`]. The map does not need the stands (nothing
/// inks); a hole is on the books, an obstacle ahead is an obstacle even
/// seen from a walking body.
pub(super) const FAST_POSE_STAND_S: f64 = 2.0;
pub(super) const FAST_POSE_EVERY_S: f64 = 20.0;
pub(super) const FAST_POSE_EVERY_M: f64 = 1.5;
pub(super) const FAST_STOP_AHEAD_M: f64 = 0.5;
/// A stand for something ahead no more often than this.
pub(super) const FAST_AHEAD_EVERY_S: f64 = 10.0;
/// The stand after a turn in place or an alignment, in fast mode.
pub(super) const FAST_TURN_STAND_S: f64 = 1.0;
/// A journey keeps its full stand this close to a drop on the books.
/// Dense stops beside the booked drops, the guarded journey's default
/// (the user's, 2026-09-21, from the guided drive: with a stop every
/// 30–40 cm the pose in the stairwell's passage was 2 cm). Within
/// [`DROP_STAND_NEAR_M`] of a booked drop the stand after a leg is
/// [`DROP_STAND_S`] and a leg at most [`DROP_LEG_S`]. Measured A/B on
/// the guarded round trip, three pairs interleaved: passages 6/6
/// against 5/6, no seal against 30, refusals a third (3/13/14 against
/// 17/28/56), the pose in the passage 5.6–6.5 cm mean against 7.0–8.9
/// (the 12–14 cm peaks unchanged), a quarter slower. `QK_DROP_STAND_S`
/// and `QK_DROP_LEG_S` to measure again; the blind journey is not
/// touched (it stands for the pose on its own terms).
pub(super) const DROP_STAND_NEAR_M: f64 = 1.0;
pub(super) const DROP_STAND_S: f64 = 6.0;
pub(super) const DROP_LEG_S: f64 = 2.0;
pub(super) fn drop_stand_s() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| knob("QK_DROP_STAND_S", DROP_STAND_S))
}
pub(super) fn drop_leg_s() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| knob("QK_DROP_LEG_S", DROP_LEG_S))
}
pub(super) fn fast_drop_near_m() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| knob("QK_FAST_DROP_NEAR_M", 1.5))
}
pub(super) fn commit() -> bool {
    std::env::var("QK_COMMIT").map(|v| v == "1").unwrap_or(false)
}
pub(super) fn commit_hold() -> bool {
    std::env::var("QK_COMMIT_HOLD").map(|v| v != "0").unwrap_or(true)
}
pub(super) fn go_exit_rad() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| knob("QK_GO_EXIT_RAD", GO_EXIT_RAD))
}
pub(super) fn hold_aim_enabled() -> bool {
    std::env::var("QK_HOLD_AIM").map(|v| v != "0").unwrap_or(true)
}
pub(super) fn fast_goal() -> bool {
    std::env::var("QK_FAST_GOAL").map(|v| v != "0").unwrap_or(true)
}
/// A `go_to` is done this close to where it was sent.
pub(super) const GOAL_ARRIVE_M: f64 = 0.25;
/// A detour the duck may have written itself. A stall — the body did not
/// move — puts an obstacle 15 cm ahead of the beak, and the cause may have
/// been the gait rather than an object; one such phantom in a doorway turns
/// a stride into a tour of the flat. Measured on the twin (2026-09-12): one
/// second after a stall the planned route went 1.75 m → 11.10 m with the
/// goal 1.57 m away, and that journey took 344 s where its twin took 110.
/// So when the route is this much longer than the straight line, the goal
/// is near, and dropping our own guesses would shorten it this much, the
/// map is asked to referee: stand — which is how the mapper gets its window
/// — and then forget the guesses along the way we wanted. A real wall comes
/// back from the map on the next pass; a phantom does not. Drops are never
/// forgotten: the map cannot show a stairwell.
/// An aim may advance along the path but never retreat. The journeys that
/// take the longest are the ones whose aim jumps between distant points of
/// their own route — measured 2026-09-12, five and six jumps beyond 0.8 m
/// in the two slowest of a series, two of them backwards to a point the
/// duck had already passed, against two jumps and none in the two quickest
/// — and a duck steering first at one and then at the other draws a loop
/// in open floor. So the aim is held until it is reached, blocked, or
/// bettered by one further along.
/// Measured over nine journeys and **not distinguishable**: median 99 s
/// and 0.031 m/s made good against 93 s and 0.032 with the aim free, and
/// two journeys past 150 s either way. One series of five looked like a
/// cure (no bad journey, 104 s at worst) and the next was the worst of the
/// day — the same run-to-run noise that hid September's ten per cent. What
/// did move is the wandering: 2.06 m walked per metre made good against
/// 2.46. So it stays on, as an invariant worth having rather than a result
/// — `QK_HOLD_AIM=0` puts it back the way it was, for the next person who
/// wants to measure it properly.
pub(super) const AIM_REACHED_M: f64 = 0.25;
/// Commit to the aim: turn, then go, with hysteresis. Upstream's unused
/// `follower.rs` enters "go" below 0.25 rad of heading error and leaves it
/// above 0.45, and its comment names the fault this follower has spent
/// three days on: *a bipedal gait oscillates yaw every step; a single
/// threshold made forward motion stutter on/off around it.* Here every leg
/// re-decided its regime from the instantaneous error against single
/// thresholds, and re-aimed as well, so the duck chased the gait's wobble
/// and the bearing swing of a near aim — 30° off at the start of a leg at
/// the median, the sign flipping on a third of legs, 2.1 m walked per
/// metre made good in an empty flat.
///
/// Adapted for a body that turns only while walking: the existing curve
/// and arc legs ARE the turning state (the kick is inside the leg), and a
/// "go" leg is the existing straight leg — up to 3 s, no correction inside
/// the deadband, else one sized to the leg. Entry is the deadband (0.25,
/// `QK_DEADBAND_RAD`), exit is [`GO_EXIT_RAD`]. While going, a fresh aim
/// replaces the held one only if it needs no turn, and an aim abeam or
/// behind counts as reached. `QK_COMMIT=1` turns it on; `QK_COMMIT_HOLD=0`
/// keeps the gate and drops the two hold rules, to attribute.
///
/// **Measured, and it is worse: off by default** (2026-09-14, empty flat,
/// four series an arm, interleaved). Seventeen journeys against thirteen:
/// median 90 s against 67, 0.034 m/s made good against 0.046, 2.51 m
/// walked per metre against 1.80, stalls 33 against 14, turns in place 18
/// against 6, and **28 of 100 pairings**. Of its own predictions it met
/// one (a "go" leg starts 8° off, by construction) and missed the rest:
/// "go" legs were 28 % of legs, not 60; the heading error over all legs
/// was 35° at the median, not 22; the sign flipped between consecutive
/// "go" legs 44 % of the time, not 15. The gate's entry — the deadband,
/// 0.25 rad — is what a body that begins every plan 30° off almost never
/// clears, so the duck lived in the turning legs, which are the timed
/// 1.5–3 s curves and arcs, and turned in place three times as often. The
/// old follower's wider straight band (0.35) and its willingness to walk
/// while still a little off were doing more than they looked.
pub(super) const GO_EXIT_RAD: f64 = 0.45;
/// A fresh aim must be at least this much further on to replace the held
/// one; anything nearer is a retreat.
pub(super) const AIM_RETREAT_M: f64 = 0.15;
/// The held aim belongs to one goal: a different stand means a different
/// journey, and the aim goes with it.
pub(super) const AIM_SAME_GOAL_M: f64 = 0.30;
/// The short way is kept before the long one is believed. A route that
/// comes out this many times longer than the last one planned on this
/// journey is first re-planned with the body's own half-width
/// ([`SQUEEZE_INFLATE_M`]) — what sealed it was a drop or a guess booked
/// at the believed pose, plus the margin, in a passage the body fits
/// through; if the squeezed route is near the old length it is walked,
/// the sensor judging every leg as ever. Up to [`ROUTE_INSIST_MAX`] times
/// in a row without a leg walked, then the long way is taken (the user, watching house1
/// 2026-09-15: "always prefer the shortest way; now it tries twice and
/// then changes completely"). `QK_ROUTE_JUMP=0` accepts the long way at
/// once.
pub(super) const ROUTE_JUMP: f64 = 1.8;
pub(super) const ROUTE_INSIST_MAX: u32 = 2;
/// The squeezed route counts as "the short way" up to this many times the
/// last route's length.
pub(super) const ROUTE_NEAR: f64 = 1.3;
pub(super) fn route_jump() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| knob("QK_ROUTE_JUMP", ROUTE_JUMP))
}
/// Is the short way to be kept? `last_m` the route planned last time,
/// `long_m` the one just planned, `short_m` the same plan with the body's
/// own width, `insisted` how often this journey has already done so.
/// On a journey's first plan there is no last route: the squeezed one is
/// the yardstick itself — a first plan 1.8× longer than the body's-width
/// plan is the long way round a booked passage (full5, 2026-09-15: six
/// legs round the house before the rule could speak).
pub(super) fn keep_the_short_way(last_m: Option<f64>, long_m: f64, short_m: Option<f64>, insisted: u32) -> bool {
    let Some(short) = short_m else { return false };
    let last = last_m.unwrap_or(short);
    route_jump() > 0.0 && insisted < ROUTE_INSIST_MAX && long_m > route_jump() * last && short <= ROUTE_NEAR * last
}

impl Job {
    /// A job that walks to one point on the map it already has, instead of
    /// mapping: `go_to`. The guards, the books and the recoveries are the
    /// mapping job's own.
    pub fn to_goal(goal: (f64, f64), max_s: f64, turn: f64, started: Instant) -> Self {
        let mut job = Self::new(Vec::new(), max_s, false, turn, started);
        job.goal = Some(goal);
        job
    }

    /// Drops on the books before the job starts — a saved map's ground
    /// book, or the paper twin's stand-in for one (the rim of every hole
    /// it knows, every 10 cm: what a human drive and the guarded
    /// sessions wrote for house2, 51 points).
    pub fn with_books(mut self, drops: Vec<(f64, f64)>) -> Self {
        self.ground_drops = drops.len();
        self.local = drops.into_iter().map(|p| (p, DROP_RADIUS_M)).collect();
        self
    }

    /// How long to stand after this leg. Three seconds while mapping, so
    /// the stop reaches the map; none while walking to a goal on floor
    /// already mapped, where a stand adds nothing and doubles the journey
    /// — except every fifth leg, which stands so the mapper can still
    /// judge the pose against the map and correct it. Walking fast is
    /// worth nothing if it means walking blind.
    /// The stand after a turn in place or an alignment.
    pub(super) fn turn_stand_s(&self) -> f64 {
        if self.fast() { FAST_TURN_STAND_S } else { LEG_STOP_S }
    }

    /// A journey on a frozen map: the three rules the user set on
    /// 2026-09-16 and measured that evening (docs/todo-map "The evening
    /// of the 16th": three doors 624 s → 311–354 s, 18 journeys of 18,
    /// no fall) are the default there — and only there. Mapping and the
    /// boot search keep the guards and the stands: a new house has no
    /// books. Each is a switch (`1` on, `0` off) for the experiments.
    pub(super) fn frozen_journey(&self) -> bool {
        self.goal.is_some() && self.frozen
    }

    /// `QK_NO_GUARDS`: every leg, kick and pulse goes through
    /// `robot.move`, blind, and the route check is off — the planner
    /// alone (the frozen map, the books, the margins) brings the duck
    /// home. Never while mapping.
    pub(super) fn blind(&self) -> bool {
        switch("QK_NO_GUARDS").unwrap_or_else(|| self.frozen_journey())
    }

    /// `QK_FAST`: the stands are for the pose alone (see
    /// [`FAST_POSE_EVERY_S`]).
    pub(super) fn fast(&self) -> bool {
        self.goal.is_some() && switch("QK_FAST").unwrap_or(self.frozen)
    }

    /// `QK_FOLLOW_ROUTE`: the planned route walked as faithfully as the
    /// gait allows — the aim 0.3 m along it, no straight-to-the-goal
    /// shortcut, legs of 1.5 s at most, a turn in place beyond 20°.
    pub(super) fn follow(&self) -> bool {
        switch("QK_FOLLOW_ROUTE").unwrap_or_else(|| self.frozen_journey())
    }

    pub(super) fn stop_s(&self, legs: u32) -> f64 {
        if self.fast() {
            return 0.0;
        }
        if self.goal.is_none() {
            // Mapping. The stand is how a stop-and-scan mapper sees at all,
            // so it is three seconds by default — but in `continuous` the
            // mapper inks while walking and the stand buys only the head
            // sweep and a fresh frame, which is worth measuring against the
            // time it costs (`QK_MAP_STAND_S=0`).
            return map_stand_s();
        }
        if !fast_goal() {
            return if self.drop_within(DROP_STAND_NEAR_M) { drop_stand_s() } else { LEG_STOP_S };
        }
        // Near a hole, stand anyway. A mapping job stands at every leg and
        // has never fallen; a journey stands at one leg in six, and both
        // falls this branch has seen were journeys (2026-09-13). The stand
        // is when the head sweeps, so a hurrying duck meets a stairwell
        // with a 45° wedge and a stale frame.
        if self.drop_within(fast_drop_near_m()) {
            return LEG_STOP_S;
        }
        if legs % FAST_STAND_EVERY_LEGS == FAST_STAND_EVERY_LEGS - 1 {
            LEG_STOP_S
        } else {
            fast_stand_s()
        }
    }

    /// The aim to steer at: the one already held, unless it has been
    /// reached, something has come into the lane to it, or the fresh one is
    /// further along the way. See [`AIM_REACHED_M`].
    pub(super) fn hold_aim(
        &mut self,
        grid: &Grid,
        at: (f64, f64),
        yaw: f64,
        fresh: (f64, f64),
        stand: (f64, f64),
    ) -> (f64, f64) {
        let held = self.aim.and_then(|(held, for_stand)| {
            (dist2(for_stand, stand) < AIM_SAME_GOAL_M).then_some(held)
        });
        let keep = held.filter(|held| {
            let reach = dist2(at, *held);
            // Committed to this aim: it is reached once abeam or behind,
            // and a fresh aim replaces it only if it needs no turn — the
            // old rule swapped the aim on nearly every leg, since a path
            // point 0.15 m further on turns up every 0.18 m walked, and a
            // held aim that is never held holds nothing.
            let going = commit() && commit_hold()
                && self.going.is_some_and(|g| dist2(g, *held) < AIM_REACHED_M);
            let along = (held.0 - at.0) * yaw.cos() + (held.1 - at.1) * yaw.sin();
            if reach < AIM_REACHED_M || (going && along < AIM_REACHED_M) {
                return false; // reached
            }
            if dist2(at, fresh) > reach + AIM_RETREAT_M {
                let turn = wrap((fresh.1 - at.1).atan2(fresh.0 - at.0) - yaw).abs();
                if !going || turn < deadband_rad() {
                    return false; // the fresh one is further on, and no turn to get there
                }
            }
            let heading = (held.1 - at.1).atan2(held.0 - at.0);
            grid.lane_clear(at.0, at.1, heading, reach, lane_half_m())
                && self.clear_of_local(at.0, at.1, heading, reach, lane_half_m())
        });
        if let Some(held) = keep
            && held != fresh
        {
            tracing::debug!(at = ?at, ?held, ?fresh, "map explore: holding the aim");
        }
        let aim = keep.unwrap_or(fresh);
        self.aim = Some((aim, stand));
        aim
    }

    /// Turn-then-go with hysteresis: `true` when this leg walks straight
    /// at `aim`. Entry below the deadband; once going at this aim, exit
    /// only above [`GO_EXIT_RAD`]. A different aim starts over.
    pub(super) fn gate(&mut self, aim: (f64, f64), err: f64) -> bool {
        let same = self.going.is_some_and(|g| dist2(g, aim) < AIM_REACHED_M);
        let going = if same { err.abs() <= go_exit_rad() } else { err.abs() < deadband_rad() };
        self.going = going.then_some(aim);
        going
    }
}
