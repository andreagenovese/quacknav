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
/// A journey's route is kept between plans (the user, 2026-09-16: too
/// many re-plans make a walking duck erratic; a re-plan only for an
/// obstacle) unless the books changed, a leg was refused, the body is
/// more than [`KEEP_ROUTE_OFF_M`] off it, its first metre is no longer
/// passable, or it is older than this (see `navigate.rs`).
pub(super) const KEEP_ROUTE_S: f64 = 30.0;
pub(super) const KEEP_ROUTE_OFF_M: f64 = 0.40;
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
/// [`smooth_path`]): 0.6 m. At two (the straight look) the aim cut the
/// grid path's corners by up to a body's width and was held there for
/// metres; the planned route is the one with the margins in it (the
/// user's, 2026-09-23: "the duck should stay truer to the green line" —
/// a metre, then 0.6 on 2026-09-24).
pub(super) const STRING_PULL_M: f64 = 0.6;
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
/// A journey's stands for the pose (see `navigate.rs`): one of
/// [`FAST_POSE_STAND_S`] every [`FAST_POSE_EVERY_S`] or
/// [`FAST_POSE_EVERY_M`], one when the sensor sees something in the lane
/// within [`FAST_STOP_AHEAD_M`]. The map does not need the stands
/// (nothing inks); a hole is on the books, an obstacle ahead is an
/// obstacle even seen from a walking body.
pub(super) const FAST_POSE_STAND_S: f64 = 2.0;
pub(super) const FAST_POSE_EVERY_S: f64 = 20.0;
pub(super) const FAST_POSE_EVERY_M: f64 = 1.5;
pub(super) const FAST_STOP_AHEAD_M: f64 = 0.5;
/// A stand for something ahead no more often than this.
pub(super) const FAST_AHEAD_EVERY_S: f64 = 10.0;
/// A held passage leg beside a drop lasts at most this long
/// (`QK_DROP_LEG_S` to measure).
pub(super) const DROP_LEG_S: f64 = 2.0;
pub(super) fn drop_leg_s() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| knob("QK_DROP_LEG_S", DROP_LEG_S))
}
pub(super) fn commit() -> bool {
    std::env::var("QK_COMMIT").map(|v| v == "1").unwrap_or(false)
}
pub(super) fn go_exit_rad() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| knob("QK_GO_EXIT_RAD", GO_EXIT_RAD))
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

    /// The stand after a turn in place or an alignment.
    pub(super) fn turn_stand_s(&self) -> f64 {
        LEG_STOP_S
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

    /// `QK_FOLLOW_ROUTE`: the planned route walked as faithfully as the
    /// gait allows — the aim 0.3 m along it, no straight-to-the-goal
    /// shortcut, legs of 1.5 s at most, a turn in place beyond 20°.
    pub(super) fn follow(&self) -> bool {
        switch("QK_FOLLOW_ROUTE").unwrap_or(false)
    }

    /// How long to stand after a leg: the stand is how a stop-and-scan
    /// mapper sees at all, three seconds by default — but in `continuous`
    /// the mapper inks while walking and the stand buys only the head
    /// sweep and a fresh frame (`QK_MAP_STAND_S=0` to measure it).
    pub(super) fn stop_s(&self) -> f64 {
        map_stand_s()
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
