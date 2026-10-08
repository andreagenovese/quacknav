//! Split out of `explore.rs` on 2026-09-18 (the user's: one core, the modes apart, shared only what is identical). Behaviour unchanged by construction.

use super::*;

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
/// Fine alignment ([`Job::align`], the turn before a step away from a
/// drop; once the passage entry's, until 2026-09-30): iterations, tolerance, the pulse that
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

impl Job {
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
    pub(super) fn blind_drop_ahead(&self, cliff: &crate::cliff::CliffStatus, now: Instant, reach: f64) -> Option<crate::cliff::Drop> {
        let within = Duration::from_millis(1200);
        let frames = cliff.recent.iter().filter(|f| now.duration_since(f.at) <= within).count();
        if frames == 0 {
            return None;
        }
        cliff.hole_in_lane_walking(now, 0.0, BLIND_DROP_LANE_M, reach, within, 2.min(frames))
    }

    /// Whether the leg, walked as the gait walks it, crosses a cell the
    /// map has not seen as floor — along its line and a body's half-width
    /// to either side.
    fn leg_into_unknown(&self, robot: &dyn Body, (x, y, yaw): (f64, f64, f64), leg: &Value) -> bool {
        let Some(grid) = robot.frame().and_then(|f| f.grid().ok()) else { return false };
        let vx = leg.get("vx").and_then(Value::as_f64).unwrap_or(0.0);
        let vyaw = leg.get("vyaw").and_then(Value::as_f64).unwrap_or(0.0);
        let walk_s = leg.get("walk_s").and_then(Value::as_f64).unwrap_or(1.0);
        if vx <= 0.0 {
            return false;
        }
        let (v, w) = (gait_m_per_s() * vx / 0.3, quack_duck::gait::numbers().yaw_rate_per_unit * vyaw);
        let (mut px, mut py, mut h) = (x, y, yaw);
        let mut t = 0.0;
        while t < walk_s {
            px += v * 0.1 * h.cos();
            py += v * 0.1 * h.sin();
            h += w * 0.1;
            t += 0.1;
            // The body's own cell is often unknown — the sensor looks
            // ahead, not down — so from a stride out.
            if v * t < 0.15 {
                continue;
            }
            for side in [-0.08, 0.0, 0.08] {
                let (qx, qy) = (px - side * h.sin(), py + side * h.cos());
                if !matches!(grid.at(qx, qy), Some(Cell::Free) | Some(Cell::Wall)) {
                    return true;
                }
            }
        }
        false
    }

    pub(super) fn guarded_step(&self, robot: &mut dyn Body, pose: (f64, f64, f64), leg: &Value) -> Result<Value, String> {
        // Trusted floor (see `trusted.rs`): a leg over floor the duck
        // knows walks blind, in mapping and on a guarded journey alike.
        // ... and the books always have their say: a bearing between two
        // of the sensor's columns grazes a rim and "sees floor" 7 cm past
        // it (rim18, 2026-09-18), the booked rim does not.
        let trusted = self.policy.trusted_floor
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
        // The hybrid journey (the user's, 2026-09-24): blind on the floor
        // the map knows, the guard's on floor it does not — a leg that
        // runs onto an unknown cell is judged as a guarded one.
        let into_unknown = self.blind() && !trusted && self.leg_into_unknown(&*robot, pose, leg);
        if into_unknown {
            tracing::info!(at = ?(pose.0, pose.1), "map explore: the leg runs onto floor the map does not know; guarded");
        }
        if (self.blind() || trusted) && !into_unknown && !hole_in_view && !thing_ahead && robot.pose_trusted() {
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
                && let Some(o) = cliff.obstacle_in_lane_walking(robot.now(), 0.0, BLIND_OBSTACLE_LANE_M, gait_m_per_s() * walk_s + BLIND_OBSTACLE_REACH_M, Duration::from_millis(1200), 2)
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
                && let Some(d) = self.blind_drop_ahead(&cliff, robot.now(), gait_m_per_s() * walk_s + BLIND_DROP_MARGIN_M)
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
                let fits = ((d.edge_min_m - BLIND_DROP_MARGIN_M) / gait_m_per_s()).min(walk_s);
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
            && let Some(cliff) = robot.cliff()
        {
            // The step's own default (`QK_CLIFF_MARGIN_M`), so the cut and
            // the step's guard judge the edge with the same margin.
            let margin = crate::tools::cliff_margin_m();
            if let Some(d) = self.blind_drop_ahead(&cliff, robot.now(), gait_m_per_s() * walk_s + 0.15 + margin) {
                let fits = ((d.edge_min_m - 0.15 - margin - 0.02) / gait_m_per_s()).min(walk_s);
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
}
