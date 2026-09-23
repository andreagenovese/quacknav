//! Split out of `explore.rs` on 2026-09-18 (the user's: one core, the modes apart, shared only what is identical). Behaviour unchanged by construction.

use super::*;

/// Passage mode: a drop on the books this near, and the planned path
/// passing beside it through a gap narrower than [`PASSAGE_MAX_W_M`]
/// between what the map calls a wall and the drops themselves. The duck
/// then turns onto the passage's axis and takes short straight legs
/// centred in it — an arc toward a waypoint enters a 0.54 m passage at an
/// angle, and the cliff lane (±0.22 m) then meets the edge every time.
pub(super) const PASSAGE_DROP_M: f64 = 1.0;
/// Where the passage is judged when the body is not in it yet: this far
/// along the path's heading, at its mouth. Approaching a passage beside
/// a drop obliquely, the guard sees the rim ahead-left and refuses leg
/// after leg (full1's south mouth, full4's north mouth, 2026-09-15: 15–19
/// refusals and step-backs at each); the human driver stands in front of
/// the mouth, aligned with the axis, then enters straight (house1). With
/// the sides read at the mouth the passage law engages before the body
/// is between them: align to the axis, centre on it, enter.
/// `QK_MOUTH_M=0` reads the sides at the body only, as before.
pub(super) const PASSAGE_MOUTH_M: f64 = 0.6;
/// `QK_TRUSTED_KICK` (try C): a kick this short may go blind over
/// trusted floor with the hole in view.
pub(super) const TRUSTED_KICK_MAX_S: f64 = 0.6;
/// `QK_WALL_FIT` (try D): wall points within this of the nearest one are
/// the same face, and the face's line is the axis.
pub(super) const WALL_FIT_BAND_M: f64 = 0.25;
/// With `QK_WALL_FIT`: the heading held in the passage aims at the line
/// this far ahead (pure pursuit), at most this far off the axis.
pub(super) const PASSAGE_PURSUIT_M: f64 = 0.5;
pub(super) const PASSAGE_PURSUIT_MAX_RAD: f64 = 0.35;
pub(super) fn passage_mouth_m() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| knob("QK_MOUTH_M", PASSAGE_MOUTH_M))
}
/// Hugging the wall in a passage: the line held is this far from the
/// wall's face (a body's half-width, 0.11, and a little), in passages up
/// to [`HUG_MAX_W_M`] wide.
pub(super) const HUG_M: f64 = 0.16;
pub(super) const HUG_MAX_W_M: f64 = 0.75;
/// How far the wall that guides is looked for, from the body.
pub(super) const HUG_WALL_LOOK_M: f64 = 0.6;
/// Measurement switches for the passage (`=1` turns each on). All OFF by
/// default: on MuJoCo, spawned inside the passage's south mouth, the four
/// together fell into the stairwell once in three attempts (the 0.17 m
/// lane let the body reach 11 cm from the edge, then a blind step back
/// judged clear on the books went in), while the old behaviour crossed
/// once in two; measured one at a time from here (2026-09-08).
pub(super) fn passage_sensor() -> bool {
    std::env::var("QUACKSAT_PASSAGE_SENSOR").is_ok_and(|v| v == "1" || v == "2")
}
/// `QUACKSAT_PASSAGE_SENSOR=2`: the sensor's side REPLACES the map's
/// where the sensor has one (a wall or a drop's edge beside the body),
/// instead of the nearer of the two. The map's side moves with the
/// pose's error — 10–17 cm along the stairwell's passage on MuJoCo — and
/// the nearer-of-two keeps the map's when it errs toward the body,
/// steering the body away from a wall that is not there and toward the
/// hole (paper twin `--bias`, 2026-09-20: 25–26/30 at 15 cm with either,
/// 13 and 9/30 at 20 cm).
pub(super) fn passage_sensor_replaces() -> bool {
    std::env::var("QUACKSAT_PASSAGE_SENSOR").is_ok_and(|v| v == "2")
}
/// The blind leg's one guard (see `guarded_step`): the lane it judges the
/// sensor's drops in, and the margin past the leg's advance.
pub(super) const BLIND_DROP_LANE_M: f64 = 0.17;
/// The blind leg's obstacle guard while walking: the lane, and the margin
/// past the leg's advance (a cube 0.40 m ahead was outside a fixed 0.35
/// reach and walked over, dyn2, 2026-09-19).
pub(super) const BLIND_OBSTACLE_LANE_M: f64 = 0.12;
pub(super) const BLIND_OBSTACLE_REACH_M: f64 = 0.25;
/// ... and only near the nose: a jamb or a wall met at 17–27° while
/// walking is the trunk's pitch, not a thing in the way (27 refusals
/// on the blind tour, house15tour).
pub(super) const BLIND_OBSTACLE_BEARING_RAD: f64 = 0.26;
/// The cone widens at short range to the lane's own angle — at 0.25 m
/// a 12 cm lane is 26°, and a cube 0.25 m ahead at 18° inside the lane
/// was outside the 15° cone, walked over and dragged 1.9 m (sideA016r,
/// 2026-09-22). Measured on the side-obstacle ladder (a cube dropped
/// 0.45 m ahead, 10–16 cm beside a blind leg's path, the right side
/// where the gait veers): 6/6 clear with it and the pushed booking,
/// against 2 hits in 4 without. `QK_BLIND_CONE_LANE=0` for the old cone.
pub(super) fn blind_cone_lane() -> bool {
    std::env::var("QK_BLIND_CONE_LANE").map(|v| v != "0").unwrap_or(true)
}
pub(super) fn blind_cone_rad(range_m: f64) -> f64 {
    if blind_cone_lane() {
        BLIND_OBSTACLE_BEARING_RAD.max((BLIND_OBSTACLE_LANE_M / range_m.max(0.05)).atan())
    } else {
        BLIND_OBSTACLE_BEARING_RAD
    }
}
/// A thing seen ahead and not on a mapped wall is booked this much
/// beyond the range read, and a little wider — a 7 cm cube reads
/// 10–17 cm nearer than it is (lowprobe, sideA016r), and the route
/// round the booked point crossed the cube. The same ladder as above.
/// `QK_LOW_BOOK_PUSH_M` to measure.
pub(super) const LOW_BOOK_PUSH_M: f64 = 0.12;
/// ... and never as wide as a drop: the books tell a drop from an obstacle
/// by the radius alone (`r >= DROP_RADIUS_M`), and 0.05 + 0.12 / 2 = 0.11
/// made every low thing booked this way a drop — for the planner's drop
/// margin, the turn rules, and the ground book it was saved into. Nine of
/// house2's eleven drops far from the stairwell were these (2026-09-23;
/// the user's: "phantom drops on the books inhibit the navigation").
pub(super) const LOW_BOOK_RADIUS_MAX_M: f64 = DROP_RADIUS_M - 0.01;
pub(super) fn low_book_push_m() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| knob("QK_LOW_BOOK_PUSH_M", LOW_BOOK_PUSH_M))
}
/// Something low this near ahead ends the blind walk for kicks too.
pub(super) const THING_AHEAD_M: f64 = 0.35;
/// A seen point this near a mapped wall is that wall.
pub(super) const MAPPED_WALL_M: f64 = 0.15;
/// A true hole this near, this far off the nose, ends the blind walk.
pub(super) const HOLE_IN_VIEW_M: f64 = 1.0;
pub(super) const HOLE_IN_VIEW_HALF_M: f64 = 0.6;
pub(super) const BLIND_DROP_MARGIN_M: f64 = 0.25;
/// The shortest a leg is cut to before a drop the sensor sees.
pub(super) const BLIND_LEG_MIN_S: f64 = 0.8;
/// The guard's own margin from a drop's edge (`QK_CLIFF_MARGIN_M`), as
/// the explorer assumes it when a leg names none.
pub(super) const CLIFF_MARGIN_DEFAULT_M: f64 = 0.25;
/// The margin from an edge a passage leg asks of the guard. 0.15 was
/// measured on the 17th (the user's "B": 17 cm of admissible floor in
/// the 0.6 m passage instead of 7 — rim7 3/3, rim8 1/3); with the wall
/// as the guide the body is 0.44 m from the rim and the guard's own
/// 0.25 is kept whole. `QK_PASSAGE_CLIFF_MARGIN_M` to measure another.
pub(super) fn passage_cliff_margin_m() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| knob("QK_PASSAGE_CLIFF_MARGIN_M", 0.25))
}
pub(super) fn passage_lane() -> bool {
    std::env::var("QUACKSAT_PASSAGE_LANE").is_ok_and(|v| v == "1")
}
/// A passage narrower than this (between wall and drop) is not walked.
// 0.43 let the 0.44 m strip east of the twin's stairwell count as a
// passage once the body hugged the wall; two of thirty runs ended stuck in
// it (paper twin, 2026-09-08). The west passage is 0.54.
pub(super) const PASSAGE_MIN_W_M: f64 = 0.30;
/// Beside a drop the way must also leave the pose's error on each side:
/// 5–8 cm on the twin, 17 cm at the worst on the day an alignment 9 cm
/// from the stairwell's rim went in (2026-09-23). With it the passage is
/// 0.46 m at the least — the stairwell's real one is 0.54.
pub(super) const PASSAGE_POSE_MARGIN_M: f64 = 0.08;
use crate::passage::{BODY_HALF_M as BODY_HALF_M_NAV, LEG_DRIFT_M as LEG_DRIFT_M_NAV};
/// The same narrow passage refused this many times without the body moving
/// is no way at all: the frontier is dropped, the journey ends.
pub(super) const NARROW_REFUSALS_MAX: u32 = 3;
/// A rim the sensor sees this near beside the body goes on the books at
/// once, so the next plan keeps off it: the passage law measured the
/// stairwell's west rim 0.11 m to the right, the books held nothing
/// there, and the route ran along it.
pub(super) const RIM_BOOK_M: f64 = 0.35;
/// `QK_PASSAGE_MIN_W`: 0.30 since 2026-09-16 (was 0.50, then 0.42): the
/// width is measured wall-to-(rim point − its 0.10 radius), so the 0.44 m
/// strip east of the twin's stairwell reads 0.30–0.34 and the 0.55 m
/// west passage read 0.41 at its mouth. The duck is to pass both as the
/// human did — the passage legs are judged with the body's own lane
/// (0.115 a side), and the sensor sees the rim beside it.
pub(super) fn passage_min_w_m() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| knob("QK_PASSAGE_MIN_W", PASSAGE_MIN_W_M))
}
/// How much wider the planner keeps of a drop once the books refused a
/// leg along its route: the route and the leg disagreed there, and the
/// same route again is the same refusal again (82 in a row, full7,
/// 2026-09-15).
pub(super) const DROP_WIDEN_M: f64 = 0.10;
/// How much wider a rim the guard refused over and over is kept: enough
/// to close a 0.6 m passage beside it, so the route goes round.
pub(super) const DROP_SEAL_M: f64 = 0.30;
/// A booked drop this near a sealing point is sealed with it.
pub(super) const SEAL_MATCH_M: f64 = 1.0;
/// Sensor refusals for a drop in a row, in a passage, before the drop is
/// widened for the planner there too — the passage law gets that many
/// tries at the axis first.
pub(super) const DROP_REFUSALS_SEAL: u32 = 3;
/// A drop refusal counts again toward the seal only after the body moved
/// this far, or this long after the last one counted.
pub(super) const DROP_REFUSAL_MOVED_M: f64 = 0.05;
pub(super) const DROP_REFUSAL_AGAIN_S: f64 = 5.0;
/// Turns in place refused beside a drop in a row that seal the rim as
/// the refused legs do (each costs a plan and a stand, 3–5 s; goround2
/// spent ten minutes on 172 of them, 2026-09-20).
pub(super) const TURNS_REFUSED_SEAL: u32 = 12;
/// ... and so many, in any mode, that end the waiting there (see
/// `walk_leg`): the way on ahead, or the aim given up.
pub(super) const TURNS_REFUSED_ESCAPE: u32 = 4;
/// A booked drop this near a widened point is widened with it: the
/// sensor's point and the book's are the same rim seen twice.
pub(super) const WIDEN_MATCH_M: f64 = 0.25;
pub(super) const PASSAGE_MAX_W_M: f64 = 0.9;
/// Heading error to the axis above which the duck turns in place first.
pub(super) const PASSAGE_ALIGN_RAD: f64 = 0.25;
pub(super) const PASSAGE_LEG_S: f64 = 1.5;
/// The held passage leg: one long straight walk along the wall.
pub(super) const PASSAGE_HELD_LEG_S: f64 = 3.0;
/// Steering per metre of offset from the passage's centre line.
pub(super) const PASSAGE_GAIN: f64 = 1.5;
/// When the straight leg along the axis would cross a drop, the axis
/// turns this much away from the drop's side per attempt, up to
/// [`PASSAGE_BIAS_MAX_RAD`]; a leg that walks gives half of it back.
/// The path's direction bends toward the target beyond the hole, the
/// passage does not — repeating the same refused leg was the deadlock.
pub(super) const PASSAGE_BIAS_STEP_RAD: f64 = 0.175;
pub(super) const PASSAGE_BIAS_MAX_RAD: f64 = 0.52;
/// The same on a passage leg (see [`Job::drop_on_path`]).
pub(super) const PASSAGE_DROP_PATH_MARGIN_M: f64 = 0.03;
/// Fine alignment (passage entry): iterations, tolerance, the pulse that
/// turns 15–25° when there is room to kick, and the settle time.
pub(super) const ALIGN_ITERS: u32 = 6;
pub(super) const ALIGN_TOL_RAD: f64 = 0.2;
/// The walking pulse of the old alignment (`QK_ALIGN_KICK=0`).
pub(super) const ALIGN_PULSE_S: f64 = 0.6;
/// The fine correction's yaw-only stretch: at most this long, stopped
/// this far short of the heading (the body turns on by about that).
pub(super) const ALIGN_YAW_MAX_S: f64 = 1.2;
pub(super) const ALIGN_LEAD_RAD: f64 = 0.15;
pub(super) const ALIGN_SETTLE_S: f64 = 0.5;
/// Drops are put on the books only from frames this recent: a leg ends
/// with a 3 s stand, and the guard's 3 s memory still holds frames from
/// the walk, whose bearings belong to poses up to a leg behind — one of
/// those landed 12 cm inside the passage beside the stairwell and, widened
/// for the planner, sealed it.
/// How much a passage refusal's "turn in place" turns: a kick's worth.
pub(super) const PASSAGE_TURN_RAD: f64 = 0.35;
/// The middle level between the leg guard and the pose watchdog: before
/// a leg, the route's first `ROUTE_CHECK_M` are played against the
/// sensor's frames of this stand. A route through a wall the sensor sees,
/// or across a drop's edge, is a route planned on a pose that is off (or
/// a map that is short): what was seen goes on the books at once and the
/// plan is made again, without walking into the refusal first — and a
/// route contradicted at consecutive stands counts against the pose like
/// a map/sensor disagreement does (the user, 2026-09-15: "what does the
/// duck do when the route does not match reality?"). `QK_ROUTE_CHECK_M=0`
/// switches it off.
pub(super) const ROUTE_CHECK_M: f64 = 0.6;
pub(super) fn route_check_m() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| knob("QK_ROUTE_CHECK_M", ROUTE_CHECK_M))
}
/// How near a sensed obstacle the route may pass (the body's half-width
/// and the lane's slack), and how far past a sensed edge's near bound a
/// route point must lie to count as crossing it — the route beside a
/// stairwell comes close to the rim and turns, and 0.30 m short of the
/// rim was a conflict at 121 stands in a row (mid1, 2026-09-16).
/// 0.12, not the guard's lane: the route beside the stairwell runs 17–19
/// cm from the west wall (the rim's margin pushes it there), and at 0.18
/// the wall itself was "an obstacle on the route" twelve times at one
/// stand (frozen1, 2026-09-16).
pub(super) const ROUTE_OBSTACLE_M: f64 = 0.12;
pub(super) const ROUTE_EDGE_M: f64 = 0.05;
/// The same conflict from the same stand this many times: the middle
/// level has said its piece; the leg guard decides.
pub(super) const ROUTE_REPEAT_MAX: u32 = 2;

impl Job {
    /// Walk one leg toward `f` — the aim, the passage beside a drop, the
    /// turn in place, the guarded step and whatever the refusal asks for.
    /// `Some` when the job must end. Shared by the mapping loop and by
    /// `go_to`: they differ only in how `f` is chosen.
    /// The middle level (see `ROUTE_CHECK_M`): true when the route was
    /// contradicted by the sensor and the books changed — plan again.
    pub(super) fn route_contradicted(&mut self, robot: &dyn Body, pose: (f64, f64, f64), path: &[(f64, f64)]) -> bool {
        if route_check_m() <= 0.0 || self.blind() {
            return false;
        }
        let Some((at, what, seen)) = self.route_vs_sensor(robot, pose, path) else {
            self.route_conflicts = 0;
            self.route_repeat = None;
            return false;
        };
        // An obstacle the sensor sees beside a route on trusted floor is
        // the wall the map already has (the route drawn along the wall
        // by the hug, the wall 12 cm off it: thirteen re-plans at the
        // mouth of the passage, retreat5, 2026-09-18). A drop's edge
        // still counts, wherever the route runs.
        if self.policy.trusted_floor && !what.starts_with("a drop") && self.trusted.has(at.0, at.1) {
            return false;
        }
        // The same word from the same spot, again: the books already have
        // it and the plan did not change — let the leg guard judge.
        let here = (pose.0, pose.1);
        let repeats = match self.route_repeat {
            Some((stand, point, n)) if dist2(stand, here) < 0.10 && dist2(point, seen) < 0.15 => n + 1,
            _ => 1,
        };
        self.route_repeat = Some((here, seen, repeats));
        if repeats > ROUTE_REPEAT_MAX {
            return false;
        }
        self.route_conflicts += 1;
        tracing::info!(route_at = ?(format!("{:.2}", at.0), format!("{:.2}", at.1)), seen = ?(format!("{:.2}", seen.0), format!("{:.2}", seen.1)), what, streak = self.route_conflicts, "map explore: the route runs into what the sensor sees; planning again");
        if what.starts_with("a drop") {
            // One frame's edge at the pose of the moment, as a refusal's.
            self.remember_refused_drop(seen);
            self.widened.push(seen);
        } else {
            self.remember_local(seen, OBSTACLE_RADIUS_M);
        }
        // Contradicted at stand after stand: the pose is in doubt, as a
        // map/sensor disagreement would put it.
        if self.route_conflicts >= 2 {
            self.distrust += 1;
        }
        true
    }

    /// The passage beside a drop the duck stands at, if any: the axis
    /// heading (the planned path's direction over the next 0.8 m) and the
    /// offset to the passage's centre line, positive when the centre is to
    /// the left. Each side's free width is the nearer of the map's wall
    /// and the drops on the books beside or just ahead of the duck.
    pub(super) fn passage(&mut self, robot: &dyn Body, grid: &Grid, (x, y, yaw): (f64, f64, f64), path: &[(f64, f64)], stand: Option<(f64, f64)>) -> Option<(f64, f64, f64)> {
        self.passage_narrow = false;
        let drops: Vec<(f64, f64)> = self
            .local
            .iter()
            .filter(|(_, r)| *r >= DROP_RADIUS_M)
            .map(|(p, _)| *p)
            .collect();
        // A drop near AND not behind: past the stairwell the drops of its
        // south rim stayed "near" for a metre, the passage law kept
        // engaging on open floor, and an alignment there ended in a fall
        // (frozen12, 2026-09-16).
        let ahead_or_beside = |d: &(f64, f64)| {
            let (dx, dy) = (d.0 - x, d.1 - y);
            dx * yaw.cos() + dy * yaw.sin() > -0.15
        };
        if !drops.iter().any(|d| dist2(*d, (x, y)) < PASSAGE_DROP_M && ahead_or_beside(d)) {
            self.passage_axis = None;
            self.passage_bias = 0.0;
            tracing::debug!("map explore: passage: no drop near");
            return None;
        }
        let from_path = match (waypoint(path, 0.4, grid.cell_m), waypoint(path, 1.0, grid.cell_m)) {
            (Some(a), Some(b)) if dist2(a, b) >= 0.3 => Some((b.1 - a.1).atan2(b.0 - a.0)),
            _ => None,
        };
        let toward_stand = stand.map(|(sx, sy)| (sy - y).atan2(sx - x));
        let h0 = from_path
            .or(self.passage_axis)
            .or_else(|| toward_stand.filter(|h| self.drop_on_motion((x, y, *h), 0.3, 0.0, PASSAGE_LEG_S).is_none()))?;
        // The wall as the guide: the axis is the wall's own line, read
        // from the map on the side away from the nearest drop, the way
        // the route goes — not the route's direction, which bends toward
        // the living-room door right after the 0.7 m passage and put the
        // axis into the wall (paper twin, 2026-09-17: "a passage 0.09 m
        // wide", the same leg refused 57 times at the mouth, 11 runs of
        // 30). Without a wall within reach the route's direction stands.
        let h = if self.policy.hug {
            let nearest = drops
                .iter()
                .filter(|d| ahead_or_beside(d))
                .min_by(|a, b| dist2(**a, (x, y)).total_cmp(&dist2(**b, (x, y))));
            nearest
                .and_then(|d| {
                    // Away from the nearest drop: measured on the paper
                    // twin against "beside the route, opposite the drop"
                    // (22/30 arrived against 15–16/30, 2026-09-18) — and
                    // kept, though rim13 on MuJoCo found a wall north of
                    // the stairwell's north rim that way once.
                    let away = (y - d.1).atan2(x - d.0);
                    let mut best: Option<(f64, f64)> = None;
                    let mut b = away - 1.2;
                    while b <= away + 1.2 {
                        let c = grid.clearance(x, y, b, HUG_WALL_LOOK_M);
                        if c.by == Blocked::Wall {
                            if best.is_none_or(|(_, w)| c.free_m < w) {
                                best = Some((b, c.free_m));
                            }
                        }
                        b += 0.1;
                    }
                    // Try D (`QK_WALL_FIT`, point 2, 2026-09-20): the wall's
                    // line fitted to the wall CELLS of the face — the map's
                    // own cell centres within `WALL_FIT_BAND_M` of the
                    // nearest, on the side away from the drop — not the
                    // nearest ray's bearing turned by 90°. At 10–13 cm from
                    // a mapped wall jagged by its 5 cm cells one step of a
                    // cell is 25–50° of axis, and the axis read from stand
                    // to stand flapped between the corridor and the wall
                    // itself (mouthB1: −2.0 against −2.9, fifty alignments,
                    // the straight legs into the wall). Ray hits will not
                    // do: `clearance` steps half a cell, the hits scatter
                    // ±2.5 cm and a 30 cm face fits a few degrees off,
                    // differently at every stand (paper twin, 5/30 against
                    // 24/30).
                    let fitted = best.filter(|_| self.policy.wall_fit).and_then(|(bw, near)| {
                        let mut face: Vec<(f64, f64)> = Vec::new();
                        let r = HUG_WALL_LOOK_M + grid.cell_m;
                        let (c0, c1) = (((x - r - grid.x_min) / grid.cell_m).floor().max(0.0) as usize, ((x + r - grid.x_min) / grid.cell_m).ceil().max(0.0) as usize);
                        let (r0, r1) = (((y - r - grid.y_min) / grid.cell_m).floor().max(0.0) as usize, ((y + r - grid.y_min) / grid.cell_m).ceil().max(0.0) as usize);
                        for row in r0..=r1.min(grid.rows.saturating_sub(1)) {
                            for col in c0..=c1.min(grid.cols.saturating_sub(1)) {
                                if grid.cell(row, col) != Some(crate::map::Cell::Wall) {
                                    continue;
                                }
                                let (px, py) = (grid.x_min + (col as f64 + 0.5) * grid.cell_m, grid.y_min + (row as f64 + 0.5) * grid.cell_m);
                                let d = dist2((px, py), (x, y));
                                // The face: near the nearest hit, and on the
                                // wall's side (within the sweep's half-plane).
                                if d <= near + WALL_FIT_BAND_M && wrap((py - y).atan2(px - x) - bw).abs() <= 1.4 {
                                    face.push((px, py));
                                }
                            }
                        }
                        if face.len() < 3 {
                            return None;
                        }
                        let n = face.len() as f64;
                        let (mx, my) = (face.iter().map(|p| p.0).sum::<f64>() / n, face.iter().map(|p| p.1).sum::<f64>() / n);
                        let (mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0);
                        for (px, py) in &face {
                            sxx += (px - mx) * (px - mx);
                            syy += (py - my) * (py - my);
                            sxy += (px - mx) * (py - my);
                        }
                        // The face must be a line, not a blob: its long
                        // axis clearly longer than its short one.
                        let tr = sxx + syy;
                        let disc = ((sxx - syy) * (sxx - syy) + 4.0 * sxy * sxy).sqrt();
                        let (l1, l2) = ((tr + disc) / 2.0, (tr - disc) / 2.0);
                        if l1 < 0.005 || l2 > l1 * 0.35 {
                            return None;
                        }
                        Some(0.5 * (2.0 * sxy).atan2(sxx - syy))
                    });
                    fitted.or_else(|| best.map(|(bw, _)| bw + std::f64::consts::FRAC_PI_2)).map(|t| {
                        if wrap(t - h0).abs() <= std::f64::consts::FRAC_PI_2 { wrap(t) } else { wrap(t + std::f64::consts::PI) }
                    })
                })
                .unwrap_or(h0)
        } else {
            h0
        };
        let (nx, ny) = (-h.sin(), h.cos());
        let side_at = |px: f64, py: f64, bearing: f64| {
            let c = grid.clearance(px, py, bearing, 1.0);
            if c.by == Blocked::Wall { c.free_m } else { f64::INFINITY }
        };
        let (mut left, mut right) = (side_at(x, y, h + std::f64::consts::FRAC_PI_2), side_at(x, y, h - std::f64::consts::FRAC_PI_2));
        // Not between the walls yet? Read them at the mouth ahead
        // (see `PASSAGE_MOUTH_M`); the drops then count from there on.
        let mut along_from = -0.3;
        let mut at_mouth = false;
        if (!left.is_finite() || !right.is_finite()) && passage_mouth_m() > 0.0 {
            let m = passage_mouth_m();
            let (mx, my) = (x + m * h.cos(), y + m * h.sin());
            let (mut ml, mut mr) = (side_at(mx, my, h + std::f64::consts::FRAC_PI_2), side_at(mx, my, h - std::f64::consts::FRAC_PI_2));
            // A side with no wall but drops beside the mouth is bounded by
            // the nearest of them: beside a stairwell the east side of the
            // passage is the hole, and a mouth read for walls alone never
            // engaged there (frozen2, 2026-09-16).
            for (dx, dy) in drops.iter().map(|d| (d.0 - mx, d.1 - my)) {
                let along = dx * h.cos() + dy * h.sin();
                if !(-0.3..=1.0).contains(&along) {
                    continue;
                }
                let lat = dx * nx + dy * ny;
                let free = (lat.abs() - DROP_RADIUS_M).max(0.0);
                if lat > 0.0 { ml = ml.min(free) } else { mr = mr.min(free) }
            }
            // A mouth with room on both sides; a drop on the line itself
            // (a side at 0.0) is an obstacle on the route, not a passage
            // (frozen8, 2026-09-16: "at its mouth left=0.0").
            if ml.is_finite() && mr.is_finite() && ml >= 0.10 && mr >= 0.10 {
                // The mouth point lies on the body's own line along the
                // axis, so its distances to the walls are the body's:
                // the offset centring must undo before it is between them.
                left = ml;
                right = mr;
                along_from = m - 0.3;
                at_mouth = true;
            }
        }
        // Which side the drops constrain: +1 left, -1 right (the nearer).
        let (mut drop_side, mut drop_near) = (0.0, f64::INFINITY);
        for (dx, dy) in drops.iter().map(|d| (d.0 - x, d.1 - y)) {
            let along = dx * h.cos() + dy * h.sin();
            if !(along_from..=along_from + 1.3).contains(&along) {
                continue;
            }
            let lat = dx * nx + dy * ny;
            let free = (lat.abs() - DROP_RADIUS_M).max(0.0);
            if free < drop_near {
                drop_near = free;
                drop_side = lat.signum();
            }
            if lat > 0.0 {
                left = left.min(free);
            } else {
                right = right.min(free);
            }
        }
        // The sensor's own word, relative to the body: the wall it sees
        // beside itself and the drop edge it sees, projected onto the
        // axis. The map's wall and the drops on the books are in map
        // coordinates and move with the pose error (10–30 cm in run 73);
        // the passage is 0.54 m wide and the guard's lane leaves 5 cm.
        let mut sensed_drop_near = f64::INFINITY;
        if passage_sensor() && let Some(cliff) = robot.cliff() {
            let now = robot.now();
            let (mut s_left, mut s_right) = (f64::INFINITY, f64::INFINITY);
            let (mut s_drop_near, mut s_drop_side) = (f64::INFINITY, 0.0);
            let mut rims: Vec<(f64, f64)> = Vec::new();
            for f in cliff.recent.iter().filter(|f| now.duration_since(f.at) <= crate::cliff::MEMORY && !f.moving) {
                for o in &f.obstacles {
                    let a = yaw + o.bearing - h;
                    let (along, lat) = (o.range_m * a.cos(), o.range_m * a.sin());
                    if !(-0.2..=0.8).contains(&along) || lat.abs() > 0.6 {
                        continue;
                    }
                    if lat > 0.0 { s_left = s_left.min(lat) } else { s_right = s_right.min(-lat) }
                }
                for d in &f.drops {
                    let a = yaw + d.bearing - h;
                    let r = d.edge_min_m.max(0.15);
                    let (along, lat) = (r * a.cos(), r * a.sin());
                    if !(-0.2..=0.8).contains(&along) || lat.abs() > 0.6 {
                        continue;
                    }
                    let free = lat.abs();
                    if free < s_drop_near {
                        s_drop_near = free;
                        s_drop_side = lat.signum();
                    }
                    if lat > 0.0 { s_left = s_left.min(free) } else { s_right = s_right.min(free) }
                    if free < RIM_BOOK_M {
                        let b = yaw + d.bearing;
                        rims.push((x + r * b.cos(), y + r * b.sin()));
                    }
                }
            }
            // What the sensor sees of the rim this near goes on the books
            // now, not at the next stand: the route is planned from them.
            let mut booked = 0;
            for p in rims {
                if !self.local.iter().any(|(q, rr)| *rr >= DROP_RADIUS_M && dist2(*q, p) < 0.08) {
                    self.remember_local(p, DROP_RADIUS_M);
                    booked += 1;
                }
            }
            if booked > 0 {
                tracing::info!(booked, "map explore: passage: the rim the sensor sees beside the body goes on the books");
            }
            sensed_drop_near = s_drop_near;
            if passage_sensor_replaces() {
                if s_left.is_finite() {
                    left = s_left;
                }
                if s_right.is_finite() {
                    right = s_right;
                }
                if s_drop_near.is_finite() {
                    drop_side = s_drop_side;
                }
            } else {
                left = left.min(s_left);
                right = right.min(s_right);
                if s_drop_near < drop_near {
                    drop_side = s_drop_side;
                }
            }
        }
        if !left.is_finite() || !right.is_finite() || left + right > PASSAGE_MAX_W_M {
            tracing::debug!(left, right, axis = h, "map explore: passage: sides not both bounded or too wide");
            return None;
        }
        // Beside a drop the way must leave the pose's error too — a drop
        // the sensor sees right beside the body now, not one on the books
        // somewhere near (that read a kitchen corner by the stairwell as a
        // passage too narrow to leave, 806 times, on the paper twin).
        let beside_drop = sensed_drop_near < BODY_HALF_M_NAV + LEG_DRIFT_M_NAV + PASSAGE_POSE_MARGIN_M;
        let min_w = passage_min_w_m() + if beside_drop { 2.0 * PASSAGE_POSE_MARGIN_M } else { 0.0 };
        if left + right < min_w {
            if beside_drop {
                // Not a passage to walk: the planner assumed the drops kept
                // it off this line, and here they did not — the leg is
                // refused and the route planned around (the caller).
                self.passage_narrow = true;
                tracing::info!(left, right, min_w, axis = h, "map explore: passage beside a drop narrower than the body, its drift and the pose's margin; not walked");
            } else {
                tracing::debug!(left, right, axis = h, "map explore: passage: too narrow");
            }
            return None;
        }
        self.passage_axis = Some(h);
        self.passage_at_mouth = at_mouth;
        if at_mouth {
            tracing::info!(left, right, axis = h, "map explore: passage beside a drop: at its mouth");
        }
        // Hug the wall: with a drop on one side, the line to hold is a
        // body's half-width and a little from the wall on the other, not
        // the middle — brushing the wall is a bump, the middle of a 0.54 m
        // passage leaves the drop edge 5 cm outside the guard's lane (the
        // user's rule, 2026-09-08). Positive offset = move left.
        let offset = if self.policy.hug && drop_side != 0.0 && left + right <= HUG_MAX_W_M {
            if drop_side < 0.0 { left - HUG_M } else { -(right - HUG_M) }
        } else {
            (left - right) / 2.0
        };
        Some((h, offset, drop_side))
    }

    /// `robot.step`, refused first when the leg's path crosses a drop on
    /// the books — what the cliff guard cannot see, the explorer remembers.
    /// Whether a point seen at (bearing, range) from `pose` lies on a
    /// wall the map has, within [`MAPPED_WALL_M`].
    fn on_mapped_wall(&self, robot: &dyn Body, (x, y, yaw): (f64, f64, f64), bearing: f64, range: f64) -> bool {
        let Some(grid) = robot.frame().and_then(|f| f.grid().ok()) else { return false };
        let b = yaw + bearing;
        let (px, py) = (x + range * b.cos(), y + range * b.sin());
        let r = (MAPPED_WALL_M / grid.cell_m).ceil() as i64;
        let c0 = ((px - grid.x_min) / grid.cell_m).floor() as i64;
        let r0 = ((py - grid.y_min) / grid.cell_m).floor() as i64;
        for dr in -r..=r {
            for dc in -r..=r {
                let (rr, cc) = (r0 + dr, c0 + dc);
                if rr < 0 || cc < 0 || rr >= grid.rows as i64 || cc >= grid.cols as i64 {
                    continue;
                }
                if grid.cells[rr as usize * grid.cols + cc as usize] == crate::map::Cell::Wall {
                    let (wx, wy) = (grid.x_min + (cc as f64 + 0.5) * grid.cell_m, grid.y_min + (rr as f64 + 0.5) * grid.cell_m);
                    if (wx - px).hypot(wy - py) <= MAPPED_WALL_M {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// The nearest true hole's edge in the blind leg's lane within
    /// `reach`: every frame of the last second, walking ones included —
    /// the leg is walking, and a stand is seldom in fast mode (lost4,
    /// 2026-09-19: five blind legs into the stairwell with no frame
    /// judged) — two of them agreeing when there are two.
    fn blind_drop_ahead(&self, cliff: &crate::cliff::CliffStatus, now: Instant, reach: f64) -> Option<crate::cliff::Drop> {
        let within = Duration::from_millis(1200);
        let frames = cliff.recent.iter().filter(|f| now.duration_since(f.at) <= within).count();
        if frames == 0 {
            return None;
        }
        cliff.hole_in_lane_walking(now, 0.0, BLIND_DROP_LANE_M, reach, within, 2.min(frames))
    }

    pub(super) fn guarded_step(&self, robot: &mut dyn Body, pose: (f64, f64, f64), leg: &Value) -> Result<Value, String> {
        // Trusted floor (see `trusted.rs`): a leg over floor the duck
        // knows walks blind, in mapping and on a guarded journey alike.
        // ... and the books always have their say: a bearing between two
        // of the sensor's columns grazes a rim and "sees floor" 7 cm past
        // it (rim18, 2026-09-18), the booked rim does not.
        let trusted = self.policy.trusted_floor
            && leg.get("spin").is_none()
            && self.leg_on_trusted_floor(pose, leg)
            && self.drop_on_path(pose, leg).is_none();
        if trusted {
            tracing::info!(at = ?(pose.0, pose.1), cells = self.trusted.len(), "map explore: the leg lies on trusted floor; walking it blind");
        }
        // A hole in the sensor's view (walking frames included) ends the
        // blind walk for everything — legs, kicks, pulses — until it is
        // out of view: lost5 (2026-09-19) refused four blind legs at the
        // rim and then walked into it on the alignment's blind pulses.
        let hole_in_view = robot.cliff().is_some_and(|c| {
            c.hole_in_lane_walking(robot.now(), 0.0, HOLE_IN_VIEW_HALF_M, HOLE_IN_VIEW_M, Duration::from_millis(1500), 1).is_some()
        });
        // Likewise something low right ahead, not on the map: the blind
        // kick of a turn in place walked over the cube the legs had been
        // refused for (dyn7, 2026-09-19).
        let thing_ahead = robot.cliff().is_some_and(|c| {
            c.obstacle_in_lane_walking(robot.now(), 0.0, BLIND_OBSTACLE_LANE_M, THING_AHEAD_M, Duration::from_millis(1500), 2)
                .is_some_and(|o| o.bearing.abs() <= blind_cone_rad(o.range_m) && !self.on_mapped_wall(robot, pose, o.bearing, o.range_m))
        });
        // Try C (`QK_TRUSTED_KICK`): the short kick of a turn, over floor
        // the body walked and the books clear, goes blind with the hole
        // in view too — the sensor's own drop guard below still holds.
        let trusted_kick = self.policy.trusted_kick
            && trusted
            && leg.get("vx").and_then(Value::as_f64).unwrap_or(0.0) > 0.0
            && leg.get("walk_s").and_then(Value::as_f64).unwrap_or(1.0) <= TRUSTED_KICK_MAX_S;
        if trusted_kick && hole_in_view {
            tracing::info!("map explore: a hole in view, but the kick lies on trusted floor; the kick goes blind");
        }
        if (self.blind() || trusted) && (!hole_in_view || trusted_kick) && !thing_ahead && robot.pose_trusted() {
            let vx = leg.get("vx").and_then(Value::as_f64).unwrap_or(0.0);
            let vyaw = leg.get("vyaw").and_then(Value::as_f64).unwrap_or(0.0);
            let walk_s = leg.get("walk_s").and_then(Value::as_f64).unwrap_or(1.0);
            let stop_s = leg.get("stop_s").and_then(Value::as_f64).unwrap_or(0.0);
            // Something low in the lane, seen while walking (a cube of
            // 7 cm reads at 0.22 m from 0.3 m, 0.37 from 0.5 — lowprobe,
            // 2026-09-19): the blind leg stops short of it and a stand
            // lets the planner see it. Things people leave on the floor.
            // ... and not a wall the map already has: a jamb 0.36 m off
            // at 15° in a doorway is the doorway (house16tour: the
            // kitchen leg 272 s of it); a cube on the floor is not on
            // the map.
            if vx > 0.0
                && let Some(cliff) = robot.cliff()
                && let Some(o) = cliff.obstacle_in_lane_walking(robot.now(), 0.0, BLIND_OBSTACLE_LANE_M, GAIT_M_PER_S * walk_s + BLIND_OBSTACLE_REACH_M, Duration::from_millis(1200), 2)
                && o.bearing.abs() <= blind_cone_rad(o.range_m)
                && !self.on_mapped_wall(robot, pose, o.bearing, o.range_m)
            {
                return Err(format!(
                    "the depth sensor sees something {:.2} m ahead, {:.0}° {}: this blind leg would walk into it",
                    o.range_m, o.bearing.to_degrees().abs(), if o.bearing >= 0.0 { "left" } else { "right" }
                ));
            }
            // The one guard a blind leg keeps: a drop's edge the sensor
            // sees in the leg's own lane, within its reach. The books and
            // the map are only as good as the pose, and a pose 0.6 m off
            // along a corridor is invisible to the map (lost3, 2026-09-19:
            // the duck believed itself past the stairwell's north rim
            // while standing on it). The sensor sees the hole whatever the
            // pose says. Walls are not judged (a bump), nor the books'
            // margins (the passage law's business).
            // Judged as the books judge: a true hole (no obstacle at the
            // same bearing and range — a wall's foot reads as a drop,
            // house12tour: 254 refusals beside the corridor's west wall),
            // seen by two frames.
            let mut walk_s = walk_s;
            if vx > 0.0
                && let Some(cliff) = robot.cliff()
                && let Some(d) = self.blind_drop_ahead(&cliff, robot.now(), GAIT_M_PER_S * walk_s + BLIND_DROP_MARGIN_M)
            {
                // The leg cut to end the margin short of the edge the
                // sensor sees, when a leg is left of it — a 3 s leg
                // toward a rim 0.58 m ahead was refused by one centimetre,
                // three times in a second on the spot, and the seal came
                // of it (mouthD3, 2026-09-20: an eleven-minute way round
                // for a leg one second shorter).
                // The guarded journey's alone: the blind journey re-plans
                // wider of the rim on the refusal and is the faster for
                // it (paper twin: 28/30 in 196 s with the cut against
                // 29/30 in 168 s without).
                let fits = ((d.edge_min_m - BLIND_DROP_MARGIN_M) / GAIT_M_PER_S).min(walk_s);
                if fits >= BLIND_LEG_MIN_S && !self.blind() {
                    tracing::info!(edge_m = format!("{:.2}", d.edge_min_m), walk_s = format!("{fits:.1}"), "map explore: the blind leg cut short of the drop the sensor sees");
                    walk_s = fits;
                } else {
                    return Err(format!(
                        "a drop — stairs or a hole — the sensor sees {:.2}–{:.2} m ahead, {:.0}° {}: no blind leg into it",
                        d.edge_min_m, d.range_m, d.bearing.to_degrees().abs(), if d.bearing >= 0.0 { "left" } else { "right" }
                    ));
                }
            }
            let _ = robot.blind_move(&json!({"vx": vx, "vyaw": vyaw, "duration_s": walk_s}));
            if stop_s > 0.0 {
                let _ = stand(robot, stop_s);
            }
            return Ok(json!({"walked_s": walk_s, "blind": true}));
        }
        if let Some((dx, dy)) = self.drop_on_path(pose, leg) {
            self.books_refusal.set(Some((dx, dy)));
            return Err(format!(
                "a drop — stairs or a hole — on the books at ({dx:.2}, {dy:.2}) lies on this leg's path: the map cannot show it; do not walk this way"
            ));
        }
        // The guarded leg cut short of a drop the sensor sees, as the
        // blind one is: the guard wants the leg to end its margin before
        // the edge (advance + 0.15 + margin), and refuses a 3 s leg for a
        // rim 0.47 m ahead five times on the spot (rimD1, 2026-09-20)
        // where a 1.5 s one walks. The guarded journey's alone.
        let vx = leg.get("vx").and_then(Value::as_f64).unwrap_or(0.0);
        let vyaw = leg.get("vyaw").and_then(Value::as_f64).unwrap_or(0.0);
        let walk_s = leg.get("walk_s").and_then(Value::as_f64).unwrap_or(1.0);
        if vx > 0.0
            && vyaw.abs() < 0.05
            && !self.blind()
            && self.policy.mode == Mode::JourneyGuarded
            && leg.get("spin").is_none()
            && let Some(cliff) = robot.cliff()
        {
            let margin = leg.get("cliff_margin_m").and_then(Value::as_f64).unwrap_or(CLIFF_MARGIN_DEFAULT_M);
            if let Some(d) = self.blind_drop_ahead(&cliff, robot.now(), GAIT_M_PER_S * walk_s + 0.15 + margin) {
                let fits = ((d.edge_min_m - 0.15 - margin - 0.02) / GAIT_M_PER_S).min(walk_s);
                if fits >= BLIND_LEG_MIN_S && fits < walk_s {
                    tracing::info!(edge_m = format!("{:.2}", d.edge_min_m), walk_s = format!("{fits:.1}"), "map explore: the guarded leg cut short of the drop the sensor sees");
                    let mut cut = leg.clone();
                    cut["walk_s"] = json!(fits);
                    return robot.step(&cut);
                }
            }
        }
        robot.step(leg)
    }

    /// The route's first stretch against what the sensor sees from this
    /// stand: the first point of the path that runs into a sensed
    /// obstacle or past a sensed drop's edge, with what it met, in map
    /// coordinates — or `None` when the route agrees with the sensor (or
    /// the guard has no fresh frame along it).
    pub(super) fn route_vs_sensor(&self, robot: &dyn Body, (x, y, yaw): (f64, f64, f64), path: &[(f64, f64)]) -> Option<RouteConflict> {
        let cliff = robot.cliff()?;
        let now = robot.now();
        let frames: Vec<&crate::cliff::CliffFrame> = cliff
            .recent
            .iter()
            .filter(|f| now.duration_since(f.at) <= crate::cliff::MEMORY)
            .collect();
        if frames.is_empty() {
            return None;
        }
        let mut along_total = 0.0;
        let mut prev = (x, y);
        for &(px, py) in path {
            along_total += dist2(prev, (px, py));
            prev = (px, py);
            if along_total > route_check_m() {
                break;
            }
            // The path point in the body frame.
            let (dx, dy) = (px - x, py - y);
            let bx = dx * yaw.cos() + dy * yaw.sin();
            let by = -dx * yaw.sin() + dy * yaw.cos();
            if bx < 0.05 {
                continue;
            }
            let bearing = by.atan2(bx);
            let range = bx.hypot(by);
            for f in &frames {
                // Only what the head looked at: a frame's wedge is ±HALF_FOV
                // about its head yaw.
                if wrap(f.head_yaw - bearing).abs() > crate::cliff::HALF_FOV_RAD {
                    continue;
                }
                for o in &f.obstacles {
                    let (ox, oy) = (o.range_m * o.bearing.cos(), o.range_m * o.bearing.sin());
                    if (ox - bx).hypot(oy - by) < ROUTE_OBSTACLE_M {
                        let wx = x + ox * yaw.cos() - oy * yaw.sin();
                        let wy = y + ox * yaw.sin() + oy * yaw.cos();
                        return Some(((px, py), "an obstacle the sensor sees", (wx, wy)));
                    }
                }
                for d in &f.drops {
                    if wrap(d.bearing - bearing).abs() > 0.35 {
                        continue;
                    }
                    if range > d.edge_min_m.max(0.15) + ROUTE_EDGE_M {
                        let r = d.range_m.max(0.2);
                        let wx = x + r * (yaw + d.bearing).cos();
                        let wy = y + r * (yaw + d.bearing).sin();
                        return Some(((px, py), "a drop's edge the sensor sees", (wx, wy)));
                    }
                }
            }
        }
        None
    }

    /// What the sensor (or, failing that, the map) has right ahead goes
    /// on the books as a local obstacle; returns its (bearing, range).
    pub(super) fn note_obstacle_ahead(&mut self, robot: &dyn Body, grid: &Grid, (x, y, yaw): (f64, f64, f64)) -> (f64, f64) {
        let now = robot.now();
        // The standing query first; the walking frames when it has
        // nothing — a blind leg is refused on the walking frames, and
        // booking from the standing ones alone booked the map's wall two
        // metres on while the cube 0.36 m ahead stayed off the books and
        // the re-planned route crossed it (sideB013r, 2026-09-22).
        let hit = robot
            .cliff()
            .as_ref()
            .and_then(|s| {
                s.obstacle_in_lane(now, 0.0, lane_half_m())
                    .or_else(|| s.obstacle_in_lane_walking(now, 0.0, lane_half_m(), 1.0, Duration::from_millis(1500), 1))
            })
            .map(|o| (o.bearing, o.range_m))
            .unwrap_or_else(|| {
                let ahead = grid.clearance(x, y, yaw, 3.0);
                (0.0, ahead.free_m.max(0.2))
            });
        let b = yaw + hit.0;
        let push = low_book_push_m();
        let (range, radius) = if push > 0.0 && !self.on_mapped_wall(robot, (x, y, yaw), hit.0, hit.1) {
            (hit.1 + push, (OBSTACLE_RADIUS_M + push / 2.0).min(LOW_BOOK_RADIUS_MAX_M))
        } else {
            (hit.1, OBSTACLE_RADIUS_M)
        };
        self.remember_local((x + range * b.cos(), y + range * b.sin()), radius);
        hit
    }
}
