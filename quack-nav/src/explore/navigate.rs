//! The journey, on its own loop (2026-09-28, the user's: navigation,
//! homecoming and exploration apart, so a fix to one does not break
//! another).
//!
//! A journey used to run the explorer's loop (`Job::run`) and branch off
//! into the stick only at its leg, after some thirty of the explorer's own
//! rules — the pose watchdogs, the map-against-sensor panorama, the route
//! jump, the detour doubted. This loop is the journey and nothing else: the
//! budget and the stop; the pose, waited for when maploc does not vouch for
//! it; the arrival, judged after a stand; the route, planned from the pose
//! on the planner's walls (the map, the books) and kept while it holds; the
//! stick's leg (see `stick.rs`). What it shares with the rest: the planner
//! (`frontier.rs`), maploc's pose, the books, the `Body`.
//!
//! The exploration travels to its frontiers on the same loop ([`Job::travel`],
//! the user's, 2026-09-29): the explorer picks where to go and maps at the
//! frontier; the way there is the navigation's, which crosses the passages
//! beside the stairwells that the explorer's guarded legs refused (both
//! houses' bathrooms unreached after two sessions).

use super::*;

/// A frontier's stand this near a drop on the books is the drop's own edge
/// (see `Job::travel`); the passages beside the stairwells, 0.49-0.54 m,
/// keep their middles 0.25 m from the rim.
pub(super) const FRONTIER_OFF_DROP_M: f64 = 0.20;
/// A hole the stick's guard sees within this of the frontier's stand ends
/// the way there (see `Job::travel`).
pub(super) const FRONTIER_HOLE_SEEN_M: f64 = 0.6;

/// How long the way to a frontier `route_m` away may take: a minute, and
/// half a minute a metre (the stick makes 0.1 m/s with its stands), five
/// minutes at most.
pub(super) fn travel_budget(route_m: f64) -> Duration {
    Duration::from_secs_f64((60.0 + 30.0 * route_m).min(300.0))
}

impl Job {
    /// The exploration's way to a frontier's stand, on the navigation's loop:
    /// arrived, the explorer's next pass arrives there (`arrive`); failed,
    /// the frontier is refused as a refused leg refuses it. `Some` only to
    /// end the job (stopped).
    pub(super) fn travel_to_frontier(&mut self, handle: &ExploreHandle, robot: &mut dyn Body, stand: (f64, f64), route_m: f64) -> Option<(State, String)> {
        let deadline = robot.now() + travel_budget(route_m);
        match self.travel(handle, robot, stand, deadline, false) {
            (State::Stopped, why) => Some((State::Stopped, why)),
            (State::Done, _) => None,
            (_, why) => {
                tracing::info!(why, "map explore: the way to the frontier failed; it is refused");
                handle.update(|s| s.refusals += 1);
                if let Some((t, _)) = self.target.take() {
                    self.refused.push((t, BLOCK_REFUSED_M));
                }
                None
            }
        }
    }

    /// Run a journey to `self.goal` (see the module).
    pub(super) fn run_journey(&mut self, handle: &ExploreHandle, robot: &mut dyn Body) -> (State, String) {
        let Some(goal) = self.goal else {
            return (State::Failed, "a journey with no goal".into());
        };
        let deadline = self.started + Duration::from_secs_f64(self.max_s);
        self.travel(handle, robot, goal, deadline, !self.exploration_travel)
    }

    /// Walk to `goal` by `deadline` (see the module): `Done` "arrived at",
    /// `Failed` with why, `Stopped`. A `journey` is the whole job: its
    /// budget and battery end it, it waits for a lost pose and confirms its
    /// arrival after a stand, and the map's state sets its policy. For the
    /// exploration (`journey` false) the explorer's own loop does all that:
    /// a lost pose hands back at once, and the stick stands as the mapper
    /// needs ([`LEG_STOP_S`]).
    pub(super) fn travel(&mut self, handle: &ExploreHandle, robot: &mut dyn Body, goal: (f64, f64), deadline: Instant, journey: bool) -> (State, String) {
        self.stick_stand_s = if journey { STICK_STAND_S } else { LEG_STOP_S };
        self.stick_careful = !journey && switch("QK_STICK_CAREFUL").unwrap_or(true);
        self.stick_books = !journey;
        self.kept_route = None;
        let mut stuck = 0u32;
        loop {
            if handle.stop.load(Ordering::Relaxed) {
                return (State::Stopped, "stopped on request".into());
            }
            if robot.now() > deadline {
                return if journey {
                    (State::Done, format!("time budget of {:.0} s spent", self.max_s))
                } else {
                    (State::Failed, format!("the way to ({:.2}, {:.2}) took too long", goal.0, goal.1))
                };
            }
            if journey
                && let Some(min) = self.battery_min_pct
                && self.battery_checked.is_none_or(|t| (robot.now() - t).as_secs_f64() >= BATTERY_EVERY_S)
            {
                self.battery_checked = Some(robot.now());
                if let Some(pct) = robot.battery_percent()
                    && pct < min
                {
                    return (State::Done, format!("battery at {pct:.0} %: the journey ends here"));
                }
            }
            let Some(frame) = robot.frame() else {
                robot.sleep(WAIT);
                continue;
            };
            let was_down = self.fell.is_some();
            self.note_fall(robot, &frame);
            if frame.seated
                && !was_down
                && let Some(t) = crate::rlnav::trace::tracer()
            {
                t.fall(&*robot);
            }
            if journey {
                self.journey_policy(robot, handle);
            }
            // The pose: planned on only when maploc vouches for it; while
            // it does not, stand — standing still is what relocalization
            // needs — and give up after `LOST_PATIENCE`. The exploration's
            // own loop has its ways of finding itself again: handed back.
            if !robot.pose_trusted() {
                if !journey {
                    return (State::Failed, "the pose is not trusted on the way".into());
                }
                let now = robot.now();
                let since = *self.lost_since.get_or_insert(now);
                if now - since > LOST_PATIENCE {
                    return (
                        State::Failed,
                        if frame.seated { "the duck is seated or fallen and did not get up".into() } else { "the duck could not find its position again".into() },
                    );
                }
                let _ = stand(robot, FRONTIER_STOP_S);
                continue;
            }
            self.lost_since = None;
            let pose = frame.pose();
            let (x, y, _) = pose;
            let Ok(grid) = frame.grid() else {
                robot.sleep(WAIT);
                continue;
            };
            // The exploration plans across the unknown as the journey does.
            // Planning off it (every unknown cell walled but near the goal,
            // `QK_TRAVEL_OFF_UNKNOWN=1`, removed 2026-09-30) kept
            // casa_arredata's duck out of the bathroom (9 % of it in a
            // session, 84-96 % without), and the user's rule is that the
            // duck gets through and a fall is fixed by its own cause
            // (2026-09-29).
            // A frontier on a hole's rim is the hole: the map never knows
            // a hole's floor, so its edge stays a frontier for ever. The
            // explorer's guarded legs refused to walk there; the stick goes
            // where it is sent, and casa_arredata's duck, sent to a stand
            // 5 cm from the stairwell's rim, fell in (MuJoCo, 2026-09-29).
            if !journey
                && let Some(d) = self.local.iter().filter(|(_, r)| *r >= DROP_RADIUS_M).map(|(p, _)| dist2(*p, goal)).min_by(f64::total_cmp)
                && d < FRONTIER_OFF_DROP_M
            {
                return (State::Failed, format!("the frontier at ({:.2}, {:.2}) is {d:.2} m from a drop on the books", goal.0, goal.1));
            }
            // Arrived: judged on the pose after a stand, once — a stand's
            // correction can move the pose off the goal, and then the
            // journey goes on from there.
            if dist2((x, y), goal) < GOAL_ARRIVE_M {
                let _ = stand(robot, FRONTIER_STOP_S);
                if journey && !self.goal_confirmed {
                    self.goal_confirmed = true;
                    if let Some(f) = robot.frame()
                        && dist2((f.x, f.y), goal) >= GOAL_ARRIVE_M
                    {
                        tracing::info!("map explore: at the goal's coordinates, but the stand moved the pose off it; going on");
                        continue;
                    }
                }
                return (State::Done, format!("arrived at ({:.2}, {:.2})", goal.0, goal.1));
            }
            // A stand to look, now and then, and when something stands in
            // the lane: the stick's legs are judged on the frames the
            // sensor gives, and a look before walking on is what keeps them
            // fresh. Without it, on the paper twin with the map 0.18 m off
            // across the stairwell's passage, 7 journeys in 30 fell where
            // the explorer's loop, with these stands, lost none.
            let now = robot.now();
            let due = self.last_pose_stand.is_none_or(|(t, p)| (now - t).as_secs_f64() >= FAST_POSE_EVERY_S || dist2(p, (x, y)) >= FAST_POSE_EVERY_M);
            let recent = self.last_pose_stand.is_some_and(|(t, _)| (now - t).as_secs_f64() < FAST_AHEAD_EVERY_S);
            let ahead = !recent
                && robot
                    .cliff()
                    .and_then(|c| c.obstacle_in_lane(robot.now(), 0.0, lane_half_m()))
                    .is_some_and(|o| o.range_m < FAST_STOP_AHEAD_M);
            if due || ahead {
                let _ = stand(robot, FAST_POSE_STAND_S);
                self.last_pose_stand = Some((robot.now(), (x, y)));
            }
            // The route, from the pose, on the map and the books.
            self.walked((x, y), (x, y));
            let walls = self.planner_walls();
            let lanes = self.lanes();
            // The route kept from the last plan, trimmed to the body, while
            // it is fresh, the books unchanged, the body on it and its first
            // metre passable; else planned anew. Re-planned at every stand,
            // a pose jumping about in the stairwell's passage (paper twin,
            // the map 0.18 m off) swung the route onto the rim, and 7
            // journeys in 30 fell (the explorer's loop, its kept route off,
            // fell as often).
            let books = self.local.len();
            let kept = self.kept_route.take().and_then(|(raw, pulled, at, b)| {
                let (i, off) = pulled.iter().enumerate().map(|(i, p)| (i, dist2(*p, (x, y)))).min_by(|a, b| a.1.total_cmp(&b.1))?;
                let trimmed = pulled[i..].to_vec();
                let ok = (robot.now() - at).as_secs_f64() < KEEP_ROUTE_S
                    && b == books
                    && off <= KEEP_ROUTE_OFF_M
                    && trimmed.len() >= 2
                    && route_passable(&grid, &trimmed, 1.0, &walls, inflate_m(), &lanes);
                ok.then_some((raw, trimmed, at))
            });
            let (raw, path, at) = match kept {
                Some(k) => k,
                None => {
                    // No way with the usual margins: once more at the tightest
                    // the body allows (drops at their own radius, walls at
                    // the body's half-width) before anything is forgotten. A
                    // passage the duck fits through is walked — the user's
                    // rule: if it can pass, it must — with the hole guard on
                    // every step. casa_ingombra's 0.49 m passage beside the
                    // stairwell, its rim booked, closed at the 0.20 m drop
                    // margin of a live map (MuJoCo twin, 2026-10-06).
                    let planned = path_to_both(&grid, x, y, goal, &walls, inflate_m(), &lanes).or_else(|| {
                        let tight = self.planner_walls_tight();
                        let found = path_to_both(&grid, x, y, goal, &tight, SQUEEZE_INFLATE_M, &lanes);
                        if found.is_some() {
                            tracing::info!(at = ?(x, y), "map explore: no way with the usual margins; through at the tightest the body allows");
                        }
                        found
                    });
                    let Some((raw, path)) = planned else {
                        // No way from here: what the body booked itself
                        // around it is forgotten — a bump booked in a
                        // doorway can close it — wider each time; a drop
                        // never. Then stand, and plan again; `STUCK_MAX`
                        // times, and the journey fails.
                        stuck += 1;
                        if stuck > STUCK_MAX {
                            return (State::Failed, format!("no way to ({:.2}, {:.2}) on the map", goal.0, goal.1));
                        }
                        let reach = LOCAL_FORGET_M * f64::from(stuck);
                        let before = self.local.len();
                        self.local.retain(|(p, r)| *r >= DROP_RADIUS_M || dist2((x, y), *p) > reach);
                        tracing::info!(forgotten = before - self.local.len(), attempt = stuck, "map explore: journey: no way to the goal from here; standing");
                        let _ = stand(robot, FRONTIER_STOP_S);
                        continue;
                    };
                    (raw, path, robot.now())
                }
            };
            self.kept_route = Some((raw.clone(), path.clone(), at, books));
            stuck = 0;
            let f = Frontier {
                cells: 0,
                centroid: goal,
                target: goal,
                stand: goal,
                distance_m: path.len() as f64 * grid.cell_m,
                cost: 0,
                score: 0.0,
                path,
            };
            let straight = dist2((x, y), goal);
            tracing::info!(at = ?(x, y), route_m = format!("{:.2}", f.distance_m), straight_m = format!("{straight:.2}"), journey, "map explore: route to the goal");
            let (local, trail, route) = (self.local.clone(), self.trail.clone(), f.path.clone());
            handle.update(|s| {
                s.trail = trail;
                if journey {
                    s.frontiers_left = 1;
                }
                s.target = Some(goal);
                s.target_distance_m = Some(straight);
                s.local_obstacles = local.len();
                s.local = local;
                s.route = route;
                s.route_raw = raw;
                s.goal = Some(goal);
            });
            self.stick_hole_at = None;
            if let Some(verdict) = self.stick_leg(handle, robot, pose, &f) {
                return verdict;
            }
            // A hole the stick's guard saw near the frontier is the
            // frontier: a hole's floor is never mapped, so its unknown is
            // a frontier from the first look. Early in house2's exploration,
            // the book still empty, the duck was sent into the stairwell
            // itself, turned from it three times and fell in at the fourth
            // step (MuJoCo, 2026-09-29). Seen further off, it is the rim of
            // a passage on the way, and the way goes on.
            if !journey
                && let Some(h) = self.stick_hole_at.take()
                && dist2(h, goal) < FRONTIER_HOLE_SEEN_M
            {
                return (State::Failed, format!("a hole seen {:.2} m from the frontier at ({:.2}, {:.2})", dist2(h, goal), goal.0, goal.1));
            }
        }
    }

    /// The journey's policy, from the map's state: on a frozen map a blind
    /// journey, and the planner's radius for the drops (`planner_walls`
    /// reads it).
    fn journey_policy(&mut self, robot: &dyn Body, handle: &ExploreHandle) {
        let frozen = robot.frozen_map();
        if frozen != self.frozen {
            tracing::info!(frozen, "map explore: the live map is {}", if frozen { "frozen: a journey trusts the planner" } else { "live: guards and stands" });
            self.frozen = frozen;
        }
        if self.blind() {
            handle.update(|s| s.blind = true);
        }
        self.resolve_policy();
    }
}
