//! The traverse (ADR 0009): a passage beside a drop crossed by what the
//! sensor sees, not by where the map puts the body.
//!
//! Why: beside house2's stairwell the pose sits 0.1 m off the truth on
//! average, ±0.15 m against the booked rim four times in five and 0.28 m
//! at worst (MuJoCo, 2026-09-27) — the bath's walls, 12 cm off on the map,
//! pull the scan match east. The passage is 0.45–0.6 m wide. On the paper
//! twin a map bias of 0.18 m across it sank every mode of the passage law
//! (go_to 0–4/30): the map's route put the body against the wall, and the
//! law's sides came too late to help.
//!
//! How: stand, let the head sweep, and read the wall's points and the
//! rim's from the frames of the stand. They are kept, in the map's frame,
//! for the whole traverse — the pose's error is a bias that moves slowly,
//! so over a few legs it cancels, and a rim seen ahead two stands ago is
//! still beside the body now, where the ±68° of the sweep cannot see it.
//! The axis is the wall's line when the wall shows one; the step is a
//! short arc toward the middle, rolled out through the gait's model and
//! walked only if the body's centre keeps [`RIM_CLEAR_M`] off every rim
//! point along it. Then stand and read again, until the rim is behind.
//! `QK_TRAVERSE=1`; off until MuJoCo says so.

use super::*;
use crate::passage::BODY_HALF_M;

/// `QK_TRAVERSE=1`: a passage beside a drop is crossed by the traverse.
pub(super) fn traverse_on() -> bool {
    switch("QK_TRAVERSE").unwrap_or(false)
}

/// The stand before each step: the sweep's width of frames. The head's
/// sweep starts from the centre at every stop and goes left first — a
/// triangle of 6 s, left at 1.5 s, right at 4.5 s (`mapd/sweep.rs`) — so a
/// rim on the right needs the longer stand: at 2 s the first MuJoCo rounds
/// (house2, 2026-09-27) saw no rim beside the east passage at all, and
/// every traverse there ended before it began.
const STAND_LEFT_S: f64 = 2.0;
const STAND_RIGHT_S: f64 = 5.0;
/// Frames of the stand that count: this recent, and not walking.
const SEEN_WITHIN: Duration = Duration::from_millis(5500);
/// The step: this long, or the short one when the long does not fit.
const LEG_S: f64 = 1.0;
const SHORT_LEG_S: f64 = 0.6;
/// Steps at most in one traverse (2.4 m of passage at the long step).
const MAX_STEPS: u32 = 20;
/// Nearest the body's centre may come to a rim point along the step, and
/// its nose: the body's half-width (0.095) and the legs' drift, and a
/// little.
const RIM_CLEAR_M: f64 = 0.13;
const NOSE_RIM_CLEAR_M: f64 = 0.08;
/// Nearest the centre may come to a wall point (a bump, not a fall).
const WALL_CLEAR_M: f64 = 0.10;
/// Narrower than this between the wall and the rim, as sensed, is no way.
const MIN_WIDTH_M: f64 = 0.28;
/// Without a wall seen, the rim is kept this far.
const RIM_KEEP_M: f64 = 0.25;
/// The stretch of the passage that counts: from a little behind the body
/// to this far ahead, along the axis.
const LOOK_BACK_M: f64 = -0.25;
const LOOK_AHEAD_M: f64 = 0.6;
/// A point counts toward the passage's width only beside the body: within
/// this far along the axis, or this far off it.
const BESIDE_ALONG_M: f64 = 0.3;
const BESIDE_LAT_M: f64 = 0.12;
/// Obstacles nearer the body than this are its own legs, seen with the
/// head turned ("a wall 0.06 m away", MuJoCo, 2026-09-27).
const OWN_BODY_M: f64 = 0.15;
/// ... and in the box ahead of the trunk its front feet stand in: on
/// MuJoCo an "obstacle" at body (0.14–0.15, ±0.05) at every stand,
/// wherever the body was, blocked every arc (house2, 2026-09-27).
const OWN_AHEAD_M: f64 = 0.20;
const OWN_SIDE_M: f64 = 0.10;
/// The heading held off the axis, at most.
const HEADING_MAX_RAD: f64 = 0.35;
/// Frames a stand must give before a rim point needs another's vote.
const VOTE_FRAMES: usize = 6;
/// Two frames' rim points this near are the same rim.
const RIM_VOTE_M: f64 = 0.06;
/// A step that moved the body less than this did not walk.
const STALL_M: f64 = 0.03;
/// A traverse that ended without getting through is not tried again
/// within this of where it began.
pub(super) const TRAVERSE_AGAIN_M: f64 = 0.4;
/// Off the axis by more than this, the first look is taken again aligned.
const ON_AXIS_RAD: f64 = 0.3;
/// A turn larger than this is made in place first.
const TURN_FIRST_RAD: f64 = 0.8;

/// What the traverse keeps: points in the map's frame.
#[derive(Debug, Default, Clone)]
pub(super) struct Seen {
    /// Rim points, with the frame that saw each.
    pub rim: Vec<((f64, f64), u64)>,
    pub wall: Vec<(f64, f64)>,
    /// Frames the last stand gave.
    pub stand_frames: usize,
}

impl Seen {
    /// The rim points another frame confirms within [`RIM_VOTE_M`]: one
    /// frame's drop is not a hole (the books ask two as well). One frame's
    /// "rim" 0.3 m off the true one, 0.2 m from the body, made the east
    /// passage read 0.27 m (MuJoCo, 2026-09-27).
    /// With fewer than [`VOTE_FRAMES`] frames there is nothing to vote with
    /// and every point stands (the paper twin's stand is three frames; the
    /// duck's 5 s stand, some seventy).
    pub fn voted_rim(&self) -> Vec<(f64, f64)> {
        if self.stand_frames < VOTE_FRAMES {
            return self.rim.iter().map(|(p, _)| *p).collect();
        }
        self.rim
            .iter()
            .filter(|(p, seq)| self.rim.iter().any(|(q, s2)| s2 != seq && dist2(*p, *q) < RIM_VOTE_M))
            .map(|(p, _)| *p)
            .collect()
    }
}

/// One step's decision, in the body's frame (x ahead, y left).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Step {
    /// Walk this arc. `axis` is the passage's axis off the nose.
    Walk { vyaw: f64, walk_s: f64, rim_m: f64, wall_m: f64, axis: f64 },
    /// Turn in place to this heading off the nose first.
    Turn { to: f64 },
    /// No rim beside or ahead: the passage is behind (or was never here).
    Clear,
    /// Wall and rim closer than [`MIN_WIDTH_M`].
    Narrow { width_m: f64, rim_m: f64, wall_m: f64 },
    /// No arc keeps the rim's clearance.
    Blocked,
}

/// Points of `seen` in the body's frame at `pose`.
fn to_body(points: &[(f64, f64)], (x, y, yaw): (f64, f64, f64)) -> Vec<(f64, f64)> {
    let (c, s) = (yaw.cos(), yaw.sin());
    points.iter().map(|(px, py)| ((px - x) * c + (py - y) * s, -(px - x) * s + (py - y) * c)).collect()
}

/// The wall's direction off the nose, fitted to the wall points on the
/// side away from the drop near `axis`; `None` if they make no line.
fn wall_axis(wall: &[(f64, f64)], axis: f64, drop_side: f64) -> Option<f64> {
    let (ca, sa) = (axis.cos(), axis.sin());
    let face: Vec<(f64, f64)> = wall
        .iter()
        .copied()
        .filter(|(px, py)| {
            let (u, v) = (px * ca + py * sa, -px * sa + py * ca);
            (-0.3..=0.9).contains(&u) && v * drop_side < -0.05 && v.abs() <= 0.8
        })
        .collect();
    if face.len() < 4 {
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
    let tr = sxx + syy;
    let disc = ((sxx - syy) * (sxx - syy) + 4.0 * sxy * sxy).sqrt();
    let (l1, l2) = ((tr + disc) / 2.0, (tr - disc) / 2.0);
    if l1 < 0.01 || l2 > l1 * 0.2 {
        return None;
    }
    let t = 0.5 * (2.0 * sxy).atan2(sxx - syy);
    let t = if wrap(t - axis).abs() <= std::f64::consts::FRAC_PI_2 { wrap(t) } else { wrap(t + std::f64::consts::PI) };
    (wrap(t - axis).abs() <= 0.4).then_some(t)
}

/// One step from what is seen, in the body's frame. Pure.
pub(super) fn plan_step(rim: &[(f64, f64)], wall: &[(f64, f64)], axis: f64, drop_side: f64) -> Step {
    let axis = wall_axis(wall, axis, drop_side).unwrap_or(axis);
    let (ca, sa) = (axis.cos(), axis.sin());
    let uv = |(px, py): &(f64, f64)| (px * ca + py * sa, -px * sa + py * ca);
    let in_stretch = |u: f64| (LOOK_BACK_M..=LOOK_AHEAD_M).contains(&u);
    // A side is beside the body: a rim point ahead on the line is where
    // the passage bends at the hole's corner, not its width (MuJoCo,
    // 2026-09-27: "wall to rim 0.20 + 0.00"). It still bounds the arcs.
    let beside = |u: f64, v: f64| u <= BESIDE_ALONG_M || v.abs() >= BESIDE_LAT_M;
    let on_side: Vec<(f64, f64)> = rim.iter().map(uv).filter(|(u, v)| in_stretch(*u) && v * drop_side > 0.0).collect();
    if on_side.is_empty() {
        return Step::Clear;
    }
    // The width is read beside the body: a rim point ahead on the line is
    // where the passage bends at the hole's corner (MuJoCo, 2026-09-27:
    // "wall to rim 0.20 + 0.00"). It still says the passage goes on — read
    // as none, the traverse ended early and the paper twin at a 0.18 m
    // bias fell from 23 to 3 of 30 — steers, and bounds the arcs.
    let rim_beside = on_side.iter().filter(|(u, v)| beside(*u, *v)).map(|(_, v)| v.abs()).fold(f64::INFINITY, f64::min);
    let rim_m = if rim_beside.is_finite() { rim_beside } else { on_side.iter().map(|(_, v)| v.abs()).fold(f64::INFINITY, f64::min) };
    let wall_m = wall
        .iter()
        .map(uv)
        .filter(|(u, v)| in_stretch(*u) && v * drop_side < -0.05 && beside(*u, *v))
        .map(|(_, v)| v.abs())
        .fold(f64::INFINITY, f64::min);
    if rim_beside.is_finite() && wall_m.is_finite() && rim_beside + wall_m < MIN_WIDTH_M {
        return Step::Narrow { width_m: rim_beside + wall_m, rim_m: rim_beside, wall_m };
    }
    // How far toward the rim's side to move: to the middle, never nearer
    // the rim than its clearance and a margin; without a wall, to the
    // rim's keeping distance.
    let toward_rim = if wall_m.is_finite() { (rim_m - wall_m) / 2.0 } else { rim_m - RIM_KEEP_M };
    let toward_rim = toward_rim.min(rim_m - RIM_CLEAR_M - 0.05).clamp(-0.15, 0.15);
    let heading = (drop_side * toward_rim).atan2(0.3).clamp(-HEADING_MAX_RAD, HEADING_MAX_RAD);
    // The nose is at `-axis` in the axis frame; the heading wanted is
    // `heading` there, so the turn is their difference.
    let turn = wrap(heading + axis);
    if turn.abs() > TURN_FIRST_RAD {
        return Step::Turn { to: turn };
    }
    let fits = |vyaw: f64, secs: f64| {
        let (v, w) = (GAIT_M_PER_S, 0.65 * vyaw);
        let (mut px, mut py, mut h) = (0.0f64, 0.0f64, 0.0f64);
        let mut t = 0.0;
        while t < secs - 1e-9 {
            px += v * 0.1 * h.cos();
            py += v * 0.1 * h.sin();
            h += w * 0.1;
            t += 0.1;
            let nose = (px + BODY_HALF_M * h.cos(), py + BODY_HALF_M * h.sin());
            if rim.iter().any(|p| dist2(*p, (px, py)) < RIM_CLEAR_M || dist2(*p, nose) < NOSE_RIM_CLEAR_M) {
                return false;
            }
            if wall.iter().any(|p| dist2(*p, (px, py)) < WALL_CLEAR_M) {
                return false;
            }
        }
        true
    };
    let vyaw = (turn / (0.65 * LEG_S)).clamp(-0.7, 0.7);
    let away = -drop_side * 0.35;
    for (vy, secs) in [(vyaw, LEG_S), (vyaw, SHORT_LEG_S), (away, SHORT_LEG_S)] {
        if fits(vy, secs) {
            return Step::Walk { vyaw: vy, walk_s: secs, rim_m, wall_m, axis };
        }
    }
    Step::Blocked
}

/// How a traverse ended.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum TraverseEnd {
    /// The rim is behind: the planner takes over.
    Through { steps: u32 },
    /// No rim seen at all at the first stand: not a passage after all.
    NotHere,
    /// As sensed, too narrow, or no arc fits: plan around it.
    NoWay { why: String },
    /// A step was refused (the sensor's guard, the gait): plan again.
    Refused { why: String },
    /// Out of steps with the rim still beside.
    Unfinished,
}

impl Job {
    /// Rim and wall points of the stand's frames, in the map's frame from
    /// the pose of now — added to `seen`.
    fn look(&self, robot: &dyn Body, (x, y, yaw): (f64, f64, f64), seen: &mut Seen) {
        let Some(cliff) = robot.cliff() else { return };
        let now = robot.now();
        let world = |b: f64, r: f64| (x + r * (yaw + b).cos(), y + r * (yaw + b).sin());
        seen.stand_frames = cliff.recent.iter().filter(|f| now.duration_since(f.at) <= SEEN_WITHIN && !f.moving).count();
        for f in cliff.recent.iter().filter(|f| now.duration_since(f.at) <= SEEN_WITHIN && !f.moving) {
            for d in &f.drops {
                // A wall's foot — an obstacle at the drop's bearing and
                // range — is no hole (see `CliffStatus::nearest_hole_m`).
                if f.obstacles.iter().any(|o| wrap(o.bearing - d.bearing).abs() < 0.2 && (o.range_m - d.range_m).abs() < 0.25) {
                    continue;
                }
                // The rim lies between the last floor row and the first
                // that missed it: the near one, to be safe — and with the
                // bottom row itself over the hole (`edge_min_m` zero), a
                // beam's spacing short of it. Read as 0.10 m, those put the
                // rim at the beak: the east passage, 0.6 m, read 0.13 m
                // wide (MuJoCo, 2026-09-27).
                let r = if d.edge_min_m > 0.0 { d.edge_min_m } else { (d.range_m - crate::cliff::EDGE_UNKNOWN_M).max(0.10) };
                if r <= 1.0 {
                    seen.rim.push((world(d.bearing, r), f.seq));
                }
            }
            for o in &f.obstacles {
                let (bx, by) = (o.range_m * o.bearing.cos(), o.range_m * o.bearing.sin());
                let own = o.range_m < OWN_BODY_M || (bx < OWN_AHEAD_M && by.abs() < OWN_SIDE_M);
                if o.range_m <= 1.0 && !own {
                    seen.wall.push(world(o.bearing, o.range_m));
                }
            }
        }
    }

    /// Cross the passage beside a drop by the sensor: `axis` its axis in
    /// the map's frame, `drop_side` +1 when the drop is to its left.
    pub(super) fn traverse(&mut self, robot: &mut dyn Body, axis: f64, drop_side: f64, handle: &ExploreHandle) -> TraverseEnd {
        let mut seen = Seen::default();
        let mut axis = axis;
        let mut walked = 0u32;
        let mut refused = 0u32;
        for step in 0..MAX_STEPS {
            let _ = stand(robot, if drop_side > 0.0 { STAND_LEFT_S } else { STAND_RIGHT_S });
            let Some(pose) = robot.frame().map(|f| f.pose()) else {
                return TraverseEnd::Refused { why: "no pose".into() };
            };
            self.look(&*robot, pose, &mut seen);
            let voted = seen.voted_rim();
            let (rim, wall) = (to_body(&voted, pose), to_body(&seen.wall, pose));
            let plan = plan_step(&rim, &wall, wrap(axis - pose.2), drop_side);
            if matches!(plan, Step::Narrow { .. } | Step::Blocked) {
                // What made it narrow: the nearest points of each kind, in
                // the body's frame and the map's.
                let near3 = |body: &[(f64, f64)], map: &[(f64, f64)]| {
                    let mut v: Vec<((f64, f64), (f64, f64))> = body.iter().copied().zip(map.iter().copied()).collect();
                    v.sort_by(|a, b| a.0.0.hypot(a.0.1).total_cmp(&b.0.0.hypot(b.0.1)));
                    v.truncate(3);
                    v.iter().map(|(b, m)| format!("body({:.2},{:.2}) map({:.2},{:.2})", b.0, b.1, m.0, m.1)).collect::<Vec<_>>().join(" ")
                };
                tracing::info!(walls = %near3(&wall, &seen.wall), rims = %near3(&rim, &voted), drop_side, "map explore: traverse: what is near");
            }
            tracing::info!(step, at = ?pose, rim_points = rim.len(), wall_points = wall.len(), plan = ?plan, "map explore: traverse");
            match plan {
                // Not seen from here, the body off the axis: the rim may be
                // beside it, where the sweep's ±68° does not reach (paper
                // twin, 2026-09-27: 26 traverses given up, the body at 30–60°
                // to the passage). Onto the axis, and look again.
                Step::Clear if walked == 0 && step == 0 && wrap(axis - pose.2).abs() > ON_AXIS_RAD => {
                    let ok = self.align(robot, axis);
                    tracing::info!(ok, axis, "map explore: traverse: onto the axis to see the rim");
                    if !ok {
                        return TraverseEnd::NotHere;
                    }
                }
                Step::Clear if walked == 0 && step <= 1 => return TraverseEnd::NotHere,
                Step::Clear => return TraverseEnd::Through { steps: walked },
                Step::Narrow { width_m, rim_m, wall_m } => {
                    return TraverseEnd::NoWay { why: format!("the passage as sensed is {width_m:.2} m, wall to rim ({wall_m:.2} + {rim_m:.2})") };
                }
                // No arc now is not a narrow passage: plan again from here.
                // As a narrow one it widened the rim for the planner and the
                // route went (paper twin, 2026-09-27: 18 of them, 6 goals).
                Step::Blocked => return TraverseEnd::Refused { why: "no step keeps the rim's clearance".into() },
                Step::Turn { to } => {
                    let ok = self.align(robot, wrap(pose.2 + to));
                    if !ok {
                        refused += 1;
                        if refused >= 2 {
                            return TraverseEnd::Refused { why: "the turn onto the passage was refused".into() };
                        }
                    }
                }
                Step::Walk { vyaw, walk_s, axis: a, .. } => {
                    axis = wrap(pose.2 + a);
                    // The passage's own lane (the body's width) for the guard: its
                    // ±0.22 m cliff lane meets the rim beside a 0.45 m passage
                    // on every step (paper twin, 2026-09-27: 60 refusals).
                    let leg = json!({"vx": 0.3, "vyaw": vyaw, "walk_s": walk_s, "stop_s": 0.0, "judged": true, "phase": "traverse", "gap": true, "steer": false, "passage": true, "cliff_margin_m": passage_cliff_margin_m()});
                    match self.guarded_step(robot, pose, &leg) {
                        Ok(_) => {
                            let after = robot.frame().map(|f| f.pose());
                            // A step that did not move the body is a bump —
                            // the nose against something low the sensor
                            // reads as a rim (paper twin, 2026-09-27: 14
                            // steps on one spot against a box).
                            if after.is_none_or(|(ax, ay, _)| dist2((ax, ay), (pose.0, pose.1)) < STALL_M) {
                                refused += 1;
                                if refused >= 2 {
                                    return TraverseEnd::Refused { why: "the steps did not move the body".into() };
                                }
                                continue;
                            }
                            walked += 1;
                            refused = 0;
                            if let Some((ax, ay, _)) = after {
                                self.walked((pose.0, pose.1), (ax, ay));
                            }
                            handle.update(|s| s.legs += 1);
                        }
                        Err(e) => {
                            handle.update(|s| s.refusals += 1);
                            refused += 1;
                            if refused >= 2 {
                                return TraverseEnd::Refused { why: e };
                            }
                        }
                    }
                }
            }
        }
        TraverseEnd::Unfinished
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A straight passage along +x: the wall 0.30 m to the right, the rim
    /// 0.25 m to the left, points every 5 cm from 0.1 to 0.8 m ahead.
    fn passage(wall_y: f64, rim_y: f64) -> (Vec<(f64, f64)>, Vec<(f64, f64)>) {
        let xs: Vec<f64> = (2..=16).map(|i| 0.05 * f64::from(i)).collect();
        (xs.iter().map(|x| (*x, rim_y)).collect(), xs.iter().map(|x| (*x, wall_y)).collect())
    }

    #[test]
    fn centred_and_aligned_walks_straight_on() {
        let (rim, wall) = passage(-0.28, 0.28);
        match plan_step(&rim, &wall, 0.0, 1.0) {
            Step::Walk { vyaw, walk_s, .. } => {
                assert!(vyaw.abs() < 0.1, "{vyaw}");
                assert_eq!(walk_s, LEG_S);
            }
            s => panic!("{s:?}"),
        }
    }

    /// Nearer the rim than the wall: the arc bends away from the rim.
    #[test]
    fn near_the_rim_it_bends_toward_the_wall() {
        let (rim, wall) = passage(-0.40, 0.18);
        match plan_step(&rim, &wall, 0.0, 1.0) {
            Step::Walk { vyaw, .. } => assert!(vyaw < 0.0, "{vyaw}"),
            s => panic!("{s:?}"),
        }
    }

    /// Wall and rim 0.24 m apart: no way.
    #[test]
    fn too_narrow_as_sensed() {
        let (rim, wall) = passage(-0.12, 0.12);
        assert!(matches!(plan_step(&rim, &wall, 0.0, 1.0), Step::Narrow { .. }));
    }

    /// The map's axis 20° off the wall's line: the wall's line wins.
    #[test]
    fn the_wall_gives_the_axis() {
        let (rim, wall) = passage(-0.28, 0.28);
        match plan_step(&rim, &wall, 0.35, 1.0) {
            Step::Walk { axis, .. } => assert!(axis.abs() < 0.05, "{axis}"),
            s => panic!("{s:?}"),
        }
    }

    /// No rim in the stretch: the passage is behind.
    #[test]
    fn no_rim_is_clear() {
        let (_, wall) = passage(-0.28, 0.28);
        let rim = vec![(-0.6, 0.28)];
        assert_eq!(plan_step(&rim, &wall, 0.0, 1.0), Step::Clear);
    }

    /// Facing the rim across the axis: turn onto the passage first.
    #[test]
    fn across_the_axis_it_turns_first() {
        let (rim, wall) = passage(-0.28, 0.28);
        let rot = |p: &(f64, f64)| (p.0 * 1.2f64.cos() - p.1 * 1.2f64.sin(), p.0 * 1.2f64.sin() + p.1 * 1.2f64.cos());
        let (rim2, wall2): (Vec<_>, Vec<_>) = (rim.iter().map(rot).collect(), wall.iter().map(rot).collect());
        assert!(matches!(plan_step(&rim2, &wall2, 1.2, 1.0), Step::Turn { .. }));
    }
}
