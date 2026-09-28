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

use super::*;

impl Job {
    /// Run a journey to `self.goal` (see the module).
    pub(super) fn run_journey(&mut self, handle: &ExploreHandle, robot: &mut dyn Body) -> (State, String) {
        let Some(goal) = self.goal else {
            return (State::Failed, "a journey with no goal".into());
        };
        loop {
            if handle.stop.load(Ordering::Relaxed) {
                return (State::Stopped, "stopped on request".into());
            }
            if (robot.now() - self.started).as_secs_f64() > self.max_s {
                return (State::Done, format!("time budget of {:.0} s spent", self.max_s));
            }
            if let Some(min) = self.battery_min_pct
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
            self.note_fall(robot, &frame);
            // The journey's policy, from the map's state: on a frozen map a
            // blind journey, and the planner's radius for the drops
            // (`planner_walls` reads it).
            let frozen = robot.frozen_map();
            if frozen != self.frozen {
                tracing::info!(frozen, "map explore: the live map is {}", if frozen { "frozen: a journey trusts the planner" } else { "live: guards and stands" });
                self.frozen = frozen;
            }
            if self.blind() {
                handle.update(|s| s.blind = true);
            }
            self.resolve_policy();
            // The pose: planned on only when maploc vouches for it; while
            // it does not, stand — standing still is what relocalization
            // needs — and give up after `LOST_PATIENCE`.
            if !robot.pose_trusted() {
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
            // Arrived: judged on the pose after a stand, once — a stand's
            // correction can move the pose off the goal, and then the
            // journey goes on from there.
            if dist2((x, y), goal) < GOAL_ARRIVE_M {
                let _ = stand(robot, FRONTIER_STOP_S);
                if !self.goal_confirmed {
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
            // journeys in 30 fell (the explorer's loop with QK_KEEP_ROUTE=0
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
                    let Some((raw, path)) = path_to_both(&grid, x, y, goal, &walls, inflate_m(), &lanes) else {
                        // No way from here: what the body booked itself
                        // around it is forgotten — a bump booked in a
                        // doorway can close it — wider each time; a drop
                        // never. Then stand, and plan again; `STUCK_MAX`
                        // times, and the journey fails.
                        self.stuck += 1;
                        if self.stuck > STUCK_MAX {
                            return (State::Failed, format!("no way to ({:.2}, {:.2}) on the map", goal.0, goal.1));
                        }
                        let reach = LOCAL_FORGET_M * f64::from(self.stuck);
                        let before = self.local.len();
                        self.local.retain(|(p, r)| *r >= DROP_RADIUS_M || dist2((x, y), *p) > reach);
                        tracing::info!(forgotten = before - self.local.len(), attempt = self.stuck, "map explore: journey: no way to the goal from here; standing");
                        let _ = stand(robot, FRONTIER_STOP_S);
                        continue;
                    };
                    (raw, path, robot.now())
                }
            };
            self.kept_route = Some((raw.clone(), path.clone(), at, books));
            self.stuck = 0;
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
            tracing::info!(at = ?(x, y), route_m = format!("{:.2}", f.distance_m), straight_m = format!("{straight:.2}"), "map explore: route to the goal");
            let (local, trail, route) = (self.local.clone(), self.trail.clone(), f.path.clone());
            handle.update(|s| {
                s.trail = trail;
                s.frontiers_left = 1;
                s.target = Some(goal);
                s.target_distance_m = Some(straight);
                s.local_obstacles = local.len();
                s.local = local;
                s.route = route;
                s.route_raw = raw;
                s.goal = Some(goal);
            });
            if let Some(verdict) = self.stick_leg(handle, robot, pose, &f) {
                return verdict;
            }
        }
    }
}
