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
/// On a journey: something in the body's lane this near, in most of the
/// last [`TOUCH_WITHIN`]'s frames, is the nose against it ...
const TOUCH_M: f64 = 0.15;
const TOUCH_WITHIN: std::time::Duration = std::time::Duration::from_millis(600);
/// ... and after this many forward steps in a row, a stall.
const TOUCHES_TURN: u32 = 2;
/// What the nose met is not booked when a map wall is this near it (the
/// map has it, give or take the pose: a point booked beside a wall pushes
/// the route away from it, toward whatever is on the other side) ...
const TOUCH_WALL_NEAR_M: f64 = 0.15;
/// ... nor this near a booked drop: the way past the hole keeps its width.
const TOUCH_DROP_NEAR_M: f64 = 0.50;
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
        // ... and, on a journey, a nose against something is a stall too,
        // however far the pose says the body went: under a low table the
        // body pushes on its edge and slides along it a few centimetres a
        // step, never the centimetre the rule above asks, while the pose
        // slides with it (apartment's coffee table, MuJoCo 2026-10-07: the
        // sensor saw the edge at 0.10 m for 40 steps; another round, the
        // pose 0.3 m off after three minutes of it, then the hole). The
        // sensor's word, two steps in a row: turn, and book what it saw.
        let touch = if self.stick_books || !self.stick_last_step { None } else { robot.cliff().and_then(|c| touching(&c, robot.now())) };
        self.stick_touches = if touch.is_some() { self.stick_touches + 1 } else { 0 };
        self.stick_last_step = false;
        let mut booked_touch = false;
        if let (Some(o), true) = (touch, self.stick_touches >= TOUCHES_TURN) {
            self.stick_touches = 0;
            self.stick_stalls = STALLS_TURN;
            booked_touch = true;
            let a = yaw + o.bearing;
            let at = (x + (o.range_m + OBSTACLE_RADIUS_M) * a.cos(), y + (o.range_m + OBSTACLE_RADIUS_M) * a.sin());
            let book = self.touch_bookable(&*robot, (x + o.range_m * a.cos(), y + o.range_m * a.sin()), at);
            if book {
                self.remember_local(at, OBSTACLE_RADIUS_M);
            }
            tracing::info!(at = ?(x, y, yaw), seen = ?at, range_m = format!("{:.2}", o.range_m), booked = book, "map explore: stick: the nose against something the map does not have; turning");
        }
        // ... and what it pushed against goes on the books, at the nose:
        // the map says free, the body says not — a low box the map does
        // not hold, or a wall where the pose's error puts it (house2's
        // office door, MuJoCo 2026-09-28: the pose 0.2 m off, the body
        // against the wall beside the doorway for the whole budget, six
        // rounds in six). The route re-planned from here goes round it.
        if self.stick_stalls >= STALLS_TURN && !booked_touch {
            let nose = (x + BUMP_AHEAD_M * yaw.cos(), y + BUMP_AHEAD_M * yaw.sin());
            self.remember_local(nose, OBSTACLE_RADIUS_M);
            tracing::info!(at = ?(x, y, yaw), booked = ?nose, "map explore: stick: bumped; what the nose met goes on the books");
        }
        // ... and, on a journey, before the bump: something the depth
        // sensor keeps seeing in the lane within `SEEN_AHEAD_M`, where the
        // map has free floor and no wall or booked drop is near, is a thing
        // the map does not have — booked now, the route planned round it
        // before the nose meets it (the quack-rl bench, 420 journeys: the
        // stick 89.8 -> 94.3 % arrived, tip-overs 35 -> 16; the pilot 90.2
        // -> 94.0 %, 10 -> 4. The MuJoCo twin's A/B decides, 2026-10-07).
        if !self.stick_books {
            self.book_seen_ahead(&*robot, (x, y, yaw));
        }
        if err.abs() > turn_first || self.stick_stalls >= STALLS_TURN {
            // A pure turn in place: yaw past the gait's dead zone, about
            // 30-58°/s, for as long as the error asks and a second at most.
            // Closed on odometry's yaw, in short chunks: the map's pose
            // comes once a second and lags a turn at 30-58°/s by up to 50°
            // (the oracle's run, MuJoCo 2026-09-28) — a turn read off it
            // overshoots, and the next step is aimed wrong.
            self.stick_stalls = 0;
            let want = if stalled || booked_touch { err.abs().max(0.35) } else { err.abs() };
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
            tracing::info!(at = ?(x, y, yaw), edge_m = format!("{:.2}", d.edge_min_m), bearing_deg = format!("{:.0}", d.bearing.to_degrees()), booked = !known, "map explore: stick: a hole ahead; turned from it");
        } else {
            let vyaw = (YAW_GAIN * err).clamp(-0.7, 0.7);
            let _ = robot.blind_move(&json!({"vx": 0.3, "vyaw": vyaw, "duration_s": STEP_S}));
            self.stick_last_step = true;
            handle.update(|s| s.legs += 1);
            let ahead = robot.cliff().and_then(|c| {
                let now = robot.now();
                let frames = c.recent.iter().filter(|f| now.saturating_duration_since(f.at) <= std::time::Duration::from_millis(600)).count();
                c.obstacle_in_lane_walking(now, 0.0, BLIND_DROP_LANE_M, 0.6, std::time::Duration::from_millis(600), 1).map(|o| (o.range_m, frames))
            });
            let odom = odom_pose(&*robot);
            tracing::info!(at = ?(x, y, yaw), look = ?look, err_deg = format!("{:.0}", err.to_degrees()), vyaw, ahead = ?ahead.map(|(r, n)| (format!("{r:.2}"), n)), odom = ?odom.map(|(a, b, _)| (format!("{a:.3}"), format!("{b:.3}"))), "map explore: stick: step");
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
            // The exploration's stands write the books, as its own legs'
            // stands did.
            if self.stick_books {
                self.record_drops(robot);
            }
        }
        None
    }
}

/// The nearest thing in the body's lane within [`TOUCH_M`], when most of
/// the frames of the last [`TOUCH_WITHIN`] (three at least) see one there.
fn touching(c: &crate::cliff::CliffStatus, now: Instant) -> Option<crate::cliff::Obstacle> {
    let frames: Vec<&crate::cliff::CliffFrame> = c.recent.iter().filter(|f| now.saturating_duration_since(f.at) <= TOUCH_WITHIN).collect();
    let hits: Vec<crate::cliff::Obstacle> = frames
        .iter()
        .filter_map(|f| {
            f.obstacles
                .iter()
                .filter(|o| {
                    let (along, across) = (o.range_m * o.bearing.cos(), o.range_m * o.bearing.sin());
                    along > 0.0 && along <= TOUCH_M && across.abs() <= BLIND_DROP_LANE_M
                })
                .min_by(|a, b| a.range_m.total_cmp(&b.range_m))
                .copied()
        })
        .collect();
    if hits.len() < 3 || 2 * hits.len() <= frames.len() {
        return None;
    }
    hits.into_iter().min_by(|a, b| a.range_m.total_cmp(&b.range_m))
}

impl Job {
    /// Whether what the nose met at `face` (booked at `at`) goes on the
    /// books: not when the map has a wall near it, nor beside a booked drop.
    fn touch_bookable(&self, robot: &dyn Body, face: (f64, f64), at: (f64, f64)) -> bool {
        if self.local.iter().any(|(q, r)| *r >= DROP_RADIUS_M && dist2(*q, at) < TOUCH_DROP_NEAR_M) {
            return false;
        }
        if self.local.iter().any(|(q, _)| dist2(*q, at) < LOCAL_DEDUP_M) {
            return false;
        }
        let Some(grid) = robot.frame().and_then(|f| f.grid().ok()) else { return false };
        !(0..=8).any(|i| {
            let (d, b) = if i == 0 { (0.0, 0.0) } else { (TOUCH_WALL_NEAR_M, f64::from(i) * std::f64::consts::FRAC_PI_4) };
            matches!(grid.at(face.0 + d * b.cos(), face.1 + d * b.sin()), Some(Cell::Wall))
        })
    }
}

/// Whether something stands in the body's lane within `reach`, in at least
/// half of the frames of the last [`SEEN_WITHIN`] and in three of them.
fn seen_in_lane(c: &crate::cliff::CliffStatus, now: Instant, reach: f64) -> bool {
    let frames: Vec<&crate::cliff::CliffFrame> = c.recent.iter().filter(|f| now.saturating_duration_since(f.at) <= SEEN_WITHIN).collect();
    if frames.len() < 3 {
        return false;
    }
    let hit = frames
        .iter()
        .filter(|f| {
            f.obstacles.iter().any(|o| {
                let (along, across) = (o.range_m * o.bearing.cos(), o.range_m * o.bearing.sin());
                along > 0.0 && along <= reach && across.abs() <= BLIND_DROP_LANE_M
            })
        })
        .count();
    hit >= 3 && 2 * hit >= frames.len()
}

const SEEN_WITHIN: std::time::Duration = std::time::Duration::from_millis(600);

/// Something seen in the lane this near is booked before the bump.
const SEEN_AHEAD_M: f64 = 0.30;

/// A map wall this near the point seen means the map has it already, give
/// or take the pose: nothing goes on the books.
const SEEN_WALL_NEAR_M: f64 = 0.15;

/// A booked drop this near the point seen: nothing goes on the books, so the
/// way past the hole stays as wide as the map has it.
const SEEN_DROP_NEAR_M: f64 = 0.50;

impl Job {
    /// Book what the sensor keeps seeing in the lane within
    /// [`SEEN_AHEAD_M`] where the map has free floor (see `stick_leg`).
    fn book_seen_ahead(&mut self, robot: &dyn Body, (x, y, yaw): (f64, f64, f64)) {
        let Some(c) = robot.cliff() else { return };
        let now = robot.now();
        if !seen_in_lane(&c, now, SEEN_AHEAD_M) {
            return;
        }
        let Some(o) = c.obstacle_in_lane_walking(now, 0.0, BLIND_DROP_LANE_M, SEEN_AHEAD_M, SEEN_WITHIN, 3) else { return };
        let Some(grid) = robot.frame().and_then(|f| f.grid().ok()) else { return };
        let a = yaw + o.bearing;
        let face = (x + o.range_m * a.cos(), y + o.range_m * a.sin());
        // The map has it already (a wall, the map's furniture), or the pose
        // is a few centimetres off a wall the map has: the route keeps off it
        // as it is. A point booked beside a mapped wall only pushes the route
        // away from it — toward whatever is on the other side.
        let mapped = (0..=8).any(|i| {
            let (d, b) = if i == 0 { (0.0, 0.0) } else { (SEEN_WALL_NEAR_M, i as f64 * std::f64::consts::FRAC_PI_4) };
            !matches!(grid.at(face.0 + d * b.cos(), face.1 + d * b.sin()), Some(Cell::Free))
        }) || [0.05, 0.1].iter().any(|d| !matches!(grid.at(face.0 + d * a.cos(), face.1 + d * a.sin()), Some(Cell::Free)));
        if mapped {
            return;
        }
        let at = (x + (o.range_m + OBSTACLE_RADIUS_M) * a.cos(), y + (o.range_m + OBSTACLE_RADIUS_M) * a.sin());
        // Beside a booked drop an obstacle on the books narrows the one way
        // past the hole and pushes the route toward the rim: the bump keeps
        // its own booking there.
        if self.local.iter().any(|(q, r)| *r >= DROP_RADIUS_M && dist2(*q, at) < SEEN_DROP_NEAR_M) {
            return;
        }
        if self.local.iter().any(|(q, _)| dist2(*q, at) < LOCAL_DEDUP_M) {
            return;
        }
        self.remember_local(at, OBSTACLE_RADIUS_M);
        tracing::info!(at = ?(x, y, yaw), booked = ?at, range_m = format!("{:.2}", o.range_m), "map explore: seen ahead before the bump: on the books");
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::cliff::{CliffFrame, CliffStatus, Obstacle};

    fn frames(now: Instant, hits: &[Option<(f64, f64)>]) -> CliffStatus {
        let recent = hits
            .iter()
            .enumerate()
            .map(|(i, h)| CliffFrame {
                seq: i as u64,
                at: now - std::time::Duration::from_millis(60 * (hits.len() - i) as u64),
                head_yaw: 0.0,
                moving: true,
                drops: vec![],
                floors: vec![],
                obstacles: h.map(|(range_m, bearing)| Obstacle { bearing, range_m }).into_iter().collect(),
                floor_beams: 0,
                judged: 0,
            })
            .collect();
        CliffStatus { recent, ..Default::default() }
    }

    #[test]
    fn the_nose_against_a_table_edge_is_a_touch() {
        let now = Instant::now();
        // apartment's coffee table: the edge at 0.10 m in 7-9 of 8 frames.
        let c = frames(now, &[Some((0.10, 0.0)), Some((0.11, 0.05)), None, Some((0.10, -0.02)), Some((0.12, 0.0)), Some((0.10, 0.0)), Some((0.13, 0.1)), Some((0.10, 0.0))]);
        let o = touching(&c, now).expect("a touch");
        assert!((o.range_m - 0.10).abs() < 1e-9);
    }

    #[test]
    fn far_off_the_lane_or_a_few_frames_is_no_touch() {
        let now = Instant::now();
        // Approaching: 0.3 m off.
        assert!(touching(&frames(now, &[Some((0.30, 0.0)); 8]), now).is_none());
        // A door jamb beside the body: 0.2 m to the side, outside its lane.
        assert!(touching(&frames(now, &[Some((0.21, 1.2)); 8]), now).is_none());
        // A stray return or two.
        assert!(touching(&frames(now, &[Some((0.10, 0.0)), None, None, Some((0.10, 0.0)), None, None, None, None]), now).is_none());
    }
}
