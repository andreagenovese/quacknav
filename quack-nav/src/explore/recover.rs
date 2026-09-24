//! Split out of `explore.rs` on 2026-09-18 (the user's: one core, the modes apart, shared only what is identical). Behaviour unchanged by construction.

use super::*;



impl Job {
    /// Seal a rim point for the planner: the route goes round. The way
    /// round is the long way (the kitchen door and the living room's:
    /// 10 m at 0.03 m/s), and the time at the mouth is spent: the
    /// budget grows once, by what a go-round needs (rim14, retreat8,
    /// study1 all ran out 1.5–2 m short, 2026-09-18).
    pub(super) fn seal_rim(&mut self, at: (f64, f64)) {
        tracing::info!(at = ?at, "map explore: the rim is sealed for the planner; the route goes round");
        if self.widened.last() != Some(&at) {
            self.widened.push(at);
        }
        self.sealed.push(at);
        if !self.budget_extended && self.goal.is_some() {
            self.budget_extended = true;
            self.max_s += GO_ROUND_EXTRA_S;
            tracing::info!(max_s = self.max_s, "map explore: the budget grows for the way round");
        }
    }

    /// A refused leg: what the map did not know goes into the local
    /// obstacle list and the route is replanned; a hopeless frontier is
    /// left alone; a lost robot ends the job.
    pub(super) fn refusal(
        &mut self,
        robot: &mut dyn Body,
        grid: &Grid,
        pose: (f64, f64, f64),
        e: &str,
    ) -> Option<(State, String)> {
        let (x, y, yaw) = pose;
        // Whatever comes next, it is not the aim that was just refused.
        self.aim = None;
        self.going = None;
        if e.contains("robot lost") || e.contains("unreachable") {
            return Some((State::Failed, e.to_owned()));
        }
        let seen = robot.cliff();
        let now = robot.now();
        if e.contains("turn in place") {
            // The passage law's refusal: the leg as steered does not fit,
            // and the message names the turn that would make it fit — a
            // kick's worth toward the middle or along the axis. Backing
            // off instead (the default below) was twenty steps back in a
            // row where one turn would do (grow5, 2026-09-15).
            let sign = if e.contains("turn in place left") { 1.0 } else { -1.0 };
            self.spin(robot, sign, PASSAGE_TURN_RAD);
            self.since_leg += 1;
            return None;
        }
        if e.starts_with("a passage") {
            // Too narrow for the body: what is ahead is an obstacle for the
            // planner, then back out as for a wall.
            self.remember_local((x + 0.25 * yaw.cos(), y + 0.25 * yaw.sin()), OBSTACLE_RADIUS_M);
            if self.boxed_in(robot, grid, pose) {
                let _ = self.back_off(robot, grid, None);
            } else {
                // Not boxed in: the same leg again is the same refusal
                // again (explmap3, 2026-09-19: "a passage 0.10 m wide"
                // two hundred times at one spot in two minutes, the
                // sides read from an eight-second-old frame). Turn on
                // the spot toward the freer side and let the next stand
                // read the sides afresh.
                self.passage_refusals += 1;
                if self.passage_refusals >= 2 {
                    let sign = self.turn_toward(grid, pose);
                    tracing::info!(sign, "map explore: a passage refused again on the spot; turning toward the freer side");
                    self.spin(robot, sign, PASSAGE_TURN_RAD * 2.0);
                    self.passage_refusals = 0;
                }
            }
            if let Some((t, _)) = self.target.take() {
                self.refused.push((t, BLOCK_REFUSED_M));
            }
            self.since_leg += 1;
            return None;
        }
        if e.starts_with("a drop") && e.contains("on the books at") {
            // Our own books refused the leg, not the sensor: nothing new
            // was seen, so nothing goes on the books — a phantom 0.4 m
            // ahead was booked here at every such refusal, and the duck
            // walled itself in (full7, 2026-09-15). The planner keeps
            // wider of that drop from now on, and the body moves — except
            // in a passage: there the way is the axis, and widening the
            // rim by 0.10 m sealed the 0.55 m passage's 0.18 m of
            // plannable floor at the first hesitation (frozen2,
            // 2026-09-16); the passage law re-centres instead.
            if self.passage_axis.is_none()
                && let Some(p) = self.books_refusal.take()
            {
                self.widened.push(p);
            }
            self.books_refusal.take();
            if let Some(axis) = self.passage_axis {
                // In a passage, off the axis toward the rim, every move
                // had a rule against it — the leg by the books, the step
                // back by a drop behind, the turn in place by the drop
                // under the beak (its kick advances) — and the same leg
                // was refused 47 times (frozen6, 2026-09-16). The one
                // move nothing forbids: yaw only, no kick, toward the
                // axis; slow (~17°/s) but the body stays where it is.
                // The passage law may then align again.
                let e = wrap(axis - yaw);
                if e.abs() > 0.1 {
                    tracing::info!(err_deg = format!("{:.0}", e.to_degrees()), "map explore: passage: turning to the axis (tight quarters, no forward kick)");
                    let ok = self.align(robot, axis);
                    tracing::info!(ok, "map explore: passage: turned to the axis");
                    let _ = stand(robot, self.turn_stand_s());
                }
                self.spins_since_leg = 0;
                return None;
            }
            self.last_back = None;
            if self.boxed_in(robot, grid, pose) {
                let _ = self.back_off(robot, grid, None);
            }
        } else if e.starts_with("a drop") {
            // The drop that refused the step is the one in the lane ahead;
            // any other on the books is a fallback. Recording the nearest
            // drop of any bearing put a stand's stale sighting at the
            // current pose, 17 cm outside the stairwell, sealing a passage.
            let edge = seen
                .as_ref()
                .and_then(|s| s.drop_in_lane(now, 0.0, 0.3).or_else(|| s.nearest(now)))
                .map(|d| (d.bearing, d.range_m.max(0.2)))
                .unwrap_or((0.0, 0.4));
            let b = yaw + edge.0;
            let seen_at = (x + edge.1 * b.cos(), y + edge.1 * b.sin());
            self.remember_refused_drop(seen_at);
            // The planner plans for a body the guard does not have: 0.17 m
            // from a rim point for the route, 0.25 from the edge for the
            // leg — so the route enters a passage the guard will not walk
            // and the duck insists there (rim2/rim3, 2026-09-17: legs of
            // 421 and 447 s, the goal 2.4 m away), while the house has a
            // way round through the kitchen and the living room that the
            // explorer found by itself. So the guard's refusal widens
            // the drop for the planner — at once outside a passage, in a
            // passage only once the passage law has had its chances
            // (see `DROP_REFUSALS_SEAL`) — and the route goes round.
            // Refusals on the spot are one refusal: three in a second
            // without a step between sealed the rim for a leg one
            // second too long (mouthD3, 2026-09-20). Counted again only
            // once the body has moved, or after DROP_REFUSAL_AGAIN_S.
            let again = !self.policy.seal || self.last_drop_refusal.is_none_or(|(at, from)| {
                (now - at).as_secs_f64() >= DROP_REFUSAL_AGAIN_S || dist2(from, (x, y)) >= DROP_REFUSAL_MOVED_M
            });
            if again {
                self.drop_refusals_in_row += 1;
                self.last_drop_refusal = Some((now, (x, y)));
            }
            if self.passage_axis.is_none() || self.drop_refusals_in_row >= DROP_REFUSALS_SEAL {
                tracing::info!(at = ?seen_at, in_a_row = self.drop_refusals_in_row, "map explore: the guard refused a drop on the route; the planner keeps wider of it");
                self.widened.push(seen_at);
                // Refused over and over at the same rim: a passage the
                // guard will not walk today, whatever the route says —
                // sealed for the planner, the route goes round (the house
                // has a way: the paper twin with the measured pulse
                // passes the stairwell's side one time in three,
                // 2026-09-18).
                if self.policy.seal && self.drop_refusals_in_row >= DROP_REFUSALS_SEAL {
                    self.seal_rim(seen_at);
                }
            }
            // Tail away from the drop only when boxed in; else the next
            // plan turns in place toward the aim and walks on.
            if self.boxed_in(robot, grid, pose) {
                let _ = self.back_off(robot, grid, Some(if edge.0 > 0.0 { 1.0 } else { -1.0 }));
            }
        } else if e.contains("sensor sees something") || e.starts_with("a wall") {
            let hit = self.note_obstacle_ahead(robot, grid, pose);
            // Nose against it by either account: the map's wall may be
            // nearer than what the sensor sees (pose error, low objects).
            let mapped = grid.clearance(x, y, yaw, 3.0);
            let nearest = if mapped.by == Blocked::Wall {
                hit.1.min(mapped.free_m)
            } else {
                hit.1
            };
            // Nose against a wall after an arc: the mirror of that arc
            // retraces the way in (the user's rule, 2026-09-08).
            let mirror = if self.last_leg_vyaw.abs() > 0.3 { Some(-self.last_leg_vyaw.signum()) } else { None };
            if nearest < NOSE_STUCK_M && self.boxed_in(robot, grid, pose) && self.back_off(robot, grid, mirror) {
                // Having backed away and turned, do not steer straight
                // back toward the same frontier: leave it for later.
                if let Some((t, _)) = self.target.take() {
                    self.refused.push((t, BLOCK_REFUSED_M));
                }
            }
        } else if e.contains("did not move") {
            // Whatever stopped the leg is at the beak or the flank, unseen:
            // put it on the books right ahead and step back and turn — a
            // repeat of the same leg is what a stall becomes otherwise.
            self.remember_local((x + 0.15 * yaw.cos(), y + 0.15 * yaw.sin()), OBSTACLE_RADIUS_M);
            self.last_back = None;
            if self.boxed_in(robot, grid, pose) && self.back_off(robot, grid, None) {
                if let Some((t, _)) = self.target.take() {
                    self.refused.push((t, BLOCK_REFUSED_M));
                }
            }
        } else if e.contains("has not looked") || e.contains("unmapped space") {
            // The sweep at a stand maps what is ahead.
            let _ = stand(robot, FRONTIER_STOP_S);
        } else if e.contains("seated") || e.contains("not sure") {
            let _ = stand(robot, self.turn_stand_s());
        } else {
            robot.sleep(WAIT);
        }
        if let Some((t, n)) = &mut self.target {
            *n += 1;
            if *n >= REFUSALS_PER_TARGET {
                self.refused.push((*t, BLOCK_REFUSED_M));
                self.target = None;
            }
        }
        // Refusal after refusal with no leg in between: the same spot, the
        // same answer. Stop recording it and get out of there.
        self.since_leg += 1;
        if self.since_leg >= REFUSAL_STREAK_MAX {
            self.unseal(robot, grid, (x, y), "refused over and over on the spot");
        }
        None
    }

    /// Nothing works from here: leave the frontier that led here alone
    /// (or the plan after backing out is the same plan), forget the local
    /// obstacles around — the map has had its stands to ink them — back
    /// out, and let a stand map the new spot. Counted in `stuck`, reset by
    /// the next leg that walks.
    pub(super) fn unseal(&mut self, robot: &mut dyn Body, grid: &Grid, at: (f64, f64), why: &str) {
        // Three refusals in thirty seconds from the same spot are one
        // finding, not three: the map has not had a stand in between to
        // change its mind. Counted three times they ended a run in the
        // stairwell passage after five minutes (MuJoCo run 76).
        let now = robot.now();
        let counts = self.last_unseal.is_none_or(|(t, p)| {
            (now - t).as_secs_f64() >= STUCK_GAP_S || dist2(p, at) >= STUCK_MOVE_M
        });
        if counts {
            self.stuck += 1;
            self.last_unseal = Some((now, at));
        }
        // The same spot, over and over: forgetting what was booked there
        // only brings the same route back (casa_arredata, 2026-09-24: 21
        // times in half an hour beside the stairwell). After a few, the spot
        // itself is where the planner does not go — not a drop, not an
        // obstacle the next unseal forgets — and the job picks elsewhere.
        let here = self.unseals_here.filter(|(p, _)| dist2(*p, at) < NO_GO_SAME_M).map_or(1, |(_, n)| n + 1);
        self.unseals_here = Some((at, here));
        if here >= NO_GO_AFTER && !self.no_go.iter().any(|p| dist2(*p, at) < NO_GO_SAME_M) {
            tracing::info!(at = ?at, times = here, "map explore: stuck on this spot again and again; the planner keeps off it from now on");
            self.no_go.push(at);
            self.kept_route = None;
        }
        self.since_leg = 0;
        if let Some((t, _)) = self.target.take() {
            self.refused.push((t, BLOCK_REFUSED_M));
        }
        let before = self.local.len();
        // Wider each time: the obstacle sealing a doorway may be further
        // than the one under the beak.
        let reach = LOCAL_FORGET_M * f64::from(self.stuck);
        // Obstacles may be forgotten (the map has had its stands to ink
        // them); a drop never — the map cannot show it, and forgetting
        // the stairwell to get unstuck is how a twin fell into it.
        self.local.retain(|(p, r)| *r >= DROP_RADIUS_M || dist2(at, *p) > reach);
        tracing::info!(
            forgotten = before - self.local.len(),
            attempt = self.stuck,
            "map explore: {why}, backing out"
        );
        self.last_back = None;
        if !self.back_off(robot, grid, None) {
            let _ = stand(robot, FRONTIER_STOP_S);
        }
    }

    /// The route without the obstacles the duck wrote itself, when that
    /// route is so much shorter that the detour is our own doing — the
    /// decision half of [`Job::doubt_the_detour`], kept pure so it can be
    /// tested against a grid without a body. Drops are left in place: the
    /// map cannot show a stairwell, so a route that only exists by
    /// forgetting one is no route at all.
    pub(super) fn ours_to_doubt(
        grid: &Grid,
        local: &[((f64, f64), f64)],
        at: (f64, f64),
        goal: (f64, f64),
        route_m: f64,
        inflate: f64,
        lanes: &[(f64, f64)],
    ) -> Option<Vec<(f64, f64)>> {
        let drops: Vec<ExtraWall> = local
            .iter()
            .filter(|(_, r)| *r >= DROP_RADIUS_M)
            .map(|(p, _)| (*p, DROP_PLAN_RADIUS_M))
            .collect();
        if drops.len() == local.len() {
            return None; // nothing on the books but drops: not ours to doubt
        }
        let bare = path_to(grid, at.0, at.1, goal, &drops, inflate, lanes)?;
        let bare_m = bare.len() as f64 * grid.cell_m;
        // The long way round is the map's doing, not ours.
        (bare_m * DETOUR_SUSPECT < route_m).then_some(bare)
    }

    /// Is this detour our own doing? See [`DETOUR_SUSPECT`]. `true` when
    /// the guesses in the way were dropped and the route should be planned
    /// again from the top.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn doubt_the_detour(
        &mut self,
        robot: &mut dyn Body,
        grid: &Grid,
        at: (f64, f64),
        goal: (f64, f64),
        f: &Frontier,
        straight_m: f64,
        inflate: f64,
    ) -> bool {
        if self.doubts >= DETOUR_DOUBTS_MAX
            || straight_m > DETOUR_NEAR_M
            || f.distance_m < DETOUR_SUSPECT * straight_m
        {
            return false;
        }
        // The trail is borrowed from `self`, which is about to be written.
        let lanes: Vec<(f64, f64)> = self.lanes().to_vec();
        let Some(bare) = Self::ours_to_doubt(grid, &self.local, at, goal, f.distance_m, inflate, &lanes)
        else {
            return false;
        };
        let bare_m = bare.len() as f64 * grid.cell_m;
        self.doubts += 1;
        // The stand first: the mapper's window is what inks a real wall, so
        // whatever is truly there survives the forgetting that follows.
        let _ = stand(robot, FRONTIER_STOP_S);
        let before = self.local.len();
        self.local.retain(|(p, r)| {
            *r >= DROP_RADIUS_M || !bare.iter().any(|w| dist2(*w, *p) < LOCAL_FORGET_M)
        });
        tracing::info!(
            at = ?at,
            route_m = format!("{:.2}", f.distance_m),
            without_our_guesses_m = format!("{:.2}", bare_m),
            forgotten = before - self.local.len(),
            doubt = self.doubts,
            "map explore: this detour looks like our own doing; standing to let the map say"
        );
        true
    }
}
