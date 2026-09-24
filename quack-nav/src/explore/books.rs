//! Split out of `explore.rs` on 2026-09-18 (the user's: one core, the modes apart, shared only what is identical). Behaviour unchanged by construction.

use super::*;

/// `QUACKSAT_TRAIL_LEG=0`: no doorway margins on the trail, for measuring.
pub(super) fn trail_leg_enabled() -> bool {
    trail_enabled() && std::env::var("QUACKSAT_TRAIL_LEG").map(|v| v != "0").unwrap_or(true)
}
/// A leg "on the trail": trail points within [`TRAIL_NEAR_M`] of the
/// heading line for the first `TRAIL_LEG_M` ahead, sampled every 5 cm,
/// each sample covered. The shortest such leg is [`TRAIL_LEG_MIN_S`].
pub(super) const TRAIL_LEG_M: f64 = 0.30;
pub(super) const TRAIL_NEAR_M: f64 = 0.10;
pub(super) const TRAIL_LEG_MIN_S: f64 = 0.6;
pub(super) fn drop_reach_m() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| knob("QK_DROP_REACH_M", DROP_REACH_M))
}
pub(super) fn trail_enabled() -> bool {
    std::env::var("QUACKSAT_TRAIL").map(|v| v != "0").unwrap_or(true)
}
/// Spacing of the trail points, and the most kept (the oldest go first).
pub(super) const TRAIL_STEP_M: f64 = 0.05;
pub(super) const TRAIL_MAX: usize = 20_000;
/// Radius given to a sensor-seen obstacle and to a drop edge in the local
/// obstacle list, on top of the planner's own inflation. A sensor hit is a
/// point on the obstacle's surface, so it needs little more than a wall
/// cell does: 0.10 + 0.15 sealed a 0.4 m corridor beside a console.
pub(super) const OBSTACLE_RADIUS_M: f64 = 0.05;
/// A drop edge on the books gets this radius (plus the planner's inflation):
/// 0.45 made a 0.6 m disc that sealed the 0.54 m passage beside the stairwell.
pub(super) const DROP_RADIUS_M: f64 = 0.10;
/// After a fall, how long the pose must stay trusted before a drop goes
/// on the books again. A fall throws the pose (0.3 m off at the fall of
/// 2026-09-23 on the twin, and the window that "confirmed" it after was
/// no better), and every rim the duck saw next went on the books where
/// that pose had it: drops in the free corridor beside the stairwell,
/// and a route of 9.85 m for a goal 4.46 m away.
pub(super) const BOOKS_AFTER_FALL_S: f64 = 20.0;
/// How far past a drop edge the hole is assumed to continue, away from the
/// duck. A sighting is one point on the rim; the void behind it is not
/// sensed at all, and the map cannot hold it either — a stairwell reads as
/// floor in five of six maps we have (measured 2026-09-13, after a fall).
/// So the rim is remembered together with the floor it stands for, in steps
/// of [`DROP_RADIUS_M`], which is what stops the planner routing through
/// the middle of a hole it has already refused to step into at the edge.
/// Kept short on purpose: 0.45 m of radius once sealed the 0.54 m passage
/// beside this same stairwell. `QK_DROP_REACH_M=0` goes back to points.
pub(super) const DROP_REACH_M: f64 = 0.30;
/// A drop ray is head-on when no neighbouring drop ray within this
/// bearing is nearer by more than this: the rim's nearest point.
pub(super) const REACH_NEIGHBOUR_RAD: f64 = 0.25;
pub(super) const REACH_SLOPE_M: f64 = 0.03;
/// `QUACKSAT_EDGE_DISCRIMINATE=0`: every sensed drop goes on the books as
/// a hole, as before. A drop with an obstacle within this bearing and this
/// range of it is the edge of that obstacle, not a hole (see `is_true_hole`).
pub(super) fn edge_discriminate() -> bool {
    std::env::var("QUACKSAT_EDGE_DISCRIMINATE").map(|v| v != "0").unwrap_or(true)
}
pub(super) const EDGE_BEARING_RAD: f64 = 0.12;
pub(super) const EDGE_RANGE_M: f64 = 0.35;
/// Sealed in by its own local obstacles: forget those within this
/// distance and back out, at most this many times in a row.
pub(super) const LOCAL_FORGET_M: f64 = 0.6;
/// What a drop is worth to the planner: with the costmap's own inflation
/// (0.05) on top, 0.25 from a rim point — the guard's own edge margin —
/// so the route runs on the wall's side of a passage beside a hole
/// (the wall as the guide, 2026-09-17; 0.17 put the route 20–25 cm from
/// the rim, where the guard refused every other leg).
pub(super) const DROP_PLAN_RADIUS_M: f64 = 0.12;
/// ... and for a guarded journey, the guard's own 0.25 with the inflation
/// (the wall as the guide, 2026-09-17): the route on the wall's side.
pub(super) const DROP_PLAN_RADIUS_GUARDED_M: f64 = 0.20;
/// A leg's path, played through the gait model, must keep the body this
/// far from every drop on the books: half the body plus a margin for the
/// gait's start latency and its yaw overshoot. The cliff guard only sees
/// where the head has looked; an arc ends on a heading the sensor never
/// swept, and a twin fell into the stairwell on exactly such an arc.
/// 0.05 (0.15 with the rim's radius) since 2026-09-16 — under the
/// planner's 0.17, never over it: at 0.20 a route the planner allowed
/// past a rim point was refused by the leg 60 times (frozen8).
pub(super) const DROP_PATH_MARGIN_M: f64 = 0.05;
pub(super) const RECORD_FRESH_S: f64 = 2.0;
/// Frames that must agree before a hole goes on the books.
pub(super) const MIN_DROP_FRAMES: usize = 2;
/// Trail points this near a kept drop are a passage's lane, kept in the
/// ground book with the drops (see [`ExploreHandle::keep_ground`]).
pub(super) const LANE_KEEP_M: f64 = 0.6;
/// A booked drop this near the body's centre — standing there, or on
/// the trail — is struck off: the body stood ON it, so it is floor.
/// Not wider: at 0.20 a stand 28 cm north of the stairwell struck 21
/// points at once — rim points booked 12 cm north of the true rim — and
/// the duck's next kick went into the hole (full6, 2026-09-15); at 0.15
/// a guided drive 20–30 cm from the rim struck eight true rim points,
/// booked 5–10 cm conservative, and left the west rim bare (stairwell1,
/// 2026-09-21 — the user's rule: "only where it walked with its body").
/// At 0.06 the same drive struck the two points it had really walked
/// over, 17 and 26 cm into the floor. The trail strike in
/// [`ExploreHandle::keep_ground`] is what clears a passage's books.
pub(super) const STRIKE_M: f64 = 0.06;
/// A sensor obstacle within this distance of one already recorded is the
/// same obstacle.
pub(super) const LOCAL_DEDUP_M: f64 = 0.10;
/// A frontier the duck could not reach is left alone within this radius.
pub(super) const BLOCK_REFUSED_M: f64 = 0.3;

impl Job {
    /// The local obstacles as the planner should see them: drops widened
    /// to [`DROP_PLAN_RADIUS_M`].
    pub(super) fn planner_walls(&self) -> Vec<ExtraWall> {
        self.local
            .iter()
            .map(|(p, r)| {
                if *r >= DROP_RADIUS_M {
                    // The whole rim within reach, not the one point: the
                    // refusals come at the mouth, the passage is beside
                    // the points further on.
                    let sealed = self.sealed.iter().any(|w| dist2(*w, *p) < SEAL_MATCH_M);
                    let widened = self.widened.iter().any(|w| dist2(*w, *p) < WIDEN_MATCH_M);
                    let r = self.policy.drop_plan_radius_m;
                    (*p, if sealed { r + DROP_SEAL_M } else if widened { r + DROP_WIDEN_M } else { r })
                } else {
                    (*p, *r)
                }
            })
            .chain(self.no_go.iter().map(|p| (*p, NO_GO_RADIUS_M)))
            .collect()
    }

    /// The first drop on the books that the body would cross on `leg`,
    /// played from `pose` through the gait model (0.12 m/s at vx 0.3,
    /// 0.65 rad/s per unit of yaw), if any.
    pub(super) fn drop_on_path(&self, pose: (f64, f64, f64), leg: &Value) -> Option<(f64, f64)> {
        let vx = leg.get("vx").and_then(Value::as_f64).unwrap_or(0.0);
        let vyaw = leg.get("vyaw").and_then(Value::as_f64).unwrap_or(0.0);
        let walk_s = leg.get("walk_s").and_then(Value::as_f64).unwrap_or(0.0);
        if walk_s <= 0.0 || vx <= 0.0 {
            return None;
        }
        // A passage leg holds the axis between a wall and a rim and is
        // judged by the sensor with the body's own lane; the books' margin
        // shrinks to the flank plus a little, or the 0.44 m passage's
        // axis (0.22 from its rim points) is refused by the books alone.
        let margin = if leg.get("passage").is_some() || leg.get("gap").and_then(Value::as_bool).unwrap_or(false) {
            PASSAGE_DROP_PATH_MARGIN_M
        } else {
            DROP_PATH_MARGIN_M
        };
        self.drop_on_motion_margin(pose, vx, vyaw, walk_s, margin)
    }

    /// Every drop the sensor has on record, put on the books at the pose
    /// of now — called right after a stand, when the frames and the pose
    /// agree. Recording only the drops that refused a step left the
    /// stairwell's edge covered in patches, and a blind step back went
    /// through a gap in them.
    /// A hole in the floor or the edge of a low box? The depth sensor
    /// reports both as a drop — the floor is missing where it should be —
    /// but a box edge has the box itself standing at the same bearing and
    /// about the same distance, while past a true hole there is nothing
    /// until the far wall. Told apart, the strict rules (no blind step
    /// back, the passage primitive, the wide planner radius) apply beside
    /// real holes only; a box edge goes on the books as the obstacle it
    /// is. Everything the depth guard refuses is unchanged: safety does
    /// not depend on this judgement.
    pub(super) fn is_true_hole(frame: &crate::cliff::CliffFrame, d: &crate::cliff::Drop) -> bool {
        !frame.obstacles.iter().any(|o| {
            wrap(o.bearing - d.bearing).abs() < EDGE_BEARING_RAD
                && (o.range_m - d.range_m).abs() < EDGE_RANGE_M
        })
    }

    /// Keep `drops_bookable` in step: a fall (the duck seated or down
    /// while the job runs) holds the drop books shut until the pose has
    /// been trusted for [`BOOKS_AFTER_FALL_S`] on end.
    pub(super) fn note_fall(&mut self, robot: &dyn Body, frame: &MapFrame) {
        let trusted = robot.pose_trusted() && !frame.seated;
        if frame.seated {
            if self.fell.is_none() {
                tracing::info!("map explore: a fall; no drop goes on the books until the pose is trusted again");
            }
            self.fell = Some(None);
        } else if let Some(since) = self.fell {
            let now = robot.now();
            match (trusted, since) {
                (false, _) => self.fell = Some(None),
                (true, None) => self.fell = Some(Some(now)),
                (true, Some(t)) if (now - t).as_secs_f64() >= BOOKS_AFTER_FALL_S => {
                    tracing::info!("map explore: the pose trusted again since the fall; drops go on the books again");
                    self.fell = None;
                    self.relocate_steps = 0;
                }
                _ => {}
            }
        }
        self.drops_bookable = trusted && self.fell.is_none();
    }

    pub(super) fn record_drops(&mut self, robot: &dyn Body) {
        let (Some(frame), Some(cliff)) = (robot.frame(), robot.cliff()) else { return };
        let (x, y, yaw) = frame.pose();
        let now = robot.now();
        // Every drop point each fresh frame proposes, tagged with its frame.
        let proposals: Vec<(usize, ((f64, f64), f64))> = cliff
            .recent
            .iter()
            .filter(|f| now.duration_since(f.at).as_secs_f64() <= RECORD_FRESH_S && !f.moving)
            .enumerate()
            .flat_map(|(k, f)| f.drops.iter().map(move |d| (k, f, d)))
            .flat_map(|(k, f, d)| {
                let b = yaw + d.bearing;
                let r = d.range_m.max(0.2);
                let hole = !edge_discriminate() || Self::is_true_hole(f, d);
                let radius = if hole { DROP_RADIUS_M } else { OBSTACLE_RADIUS_M };
                // The rim, and — for a true hole — the floor that is not
                // there behind it (see `DROP_REACH_M`): only where the ray
                // meets the rim head-on, which is where its range is the
                // least among its neighbours in the frame. Along an
                // oblique ray "behind the rim" is the floor beside the
                // hole — thirty phantom drops up to 36 cm into the
                // corridor from one stand north of the stairwell (rim1,
                // 2026-09-16).
                let head_on = !f.drops.iter().any(|o| {
                    o.bearing != d.bearing
                        && wrap(o.bearing - d.bearing).abs() <= REACH_NEIGHBOUR_RAD
                        && o.range_m < d.range_m - REACH_SLOPE_M
                });
                let reach = if hole && head_on { drop_reach_m() } else { 0.0 };
                let steps = (reach / DROP_RADIUS_M).round().max(0.0) as usize;
                (0..=steps)
                    .map(|i| {
                        let along = r + i as f64 * DROP_RADIUS_M;
                        (k, ((x + along * b.cos(), y + along * b.sin()), radius))
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        // A hole goes on the books only when more than one frame saw it:
        // a real hole is missing from every frame that looks at it, a
        // zone the sensor failed to read once ("Missing") is a hole for
        // one frame and floor the next — and one such frame put a drop
        // half a metre north of the stairwell that sealed the corridor
        // for a whole run (grow3, 2026-09-15). Obstacle edges are cheap
        // and stay single-frame.
        let points = Self::vote_drops(&proposals);
        let edges = points.iter().filter(|(_, r)| *r < DROP_RADIUS_M).count();
        if edges > 0 {
            tracing::debug!(edges, holes = points.len() - edges, "map explore: drops on the books");
        }
        let mut on_walked = 0usize;
        for (p, radius) in points {
            // A hole is not where the duck has stood: a drop point on
            // floor the body walked is the pose's error, not a rim
            // (rim1, 2026-09-16: thirty phantoms up to 36 cm into the
            // corridor's floor, the pose 17 cm off standing).
            if radius >= DROP_RADIUS_M && self.trusted.walked_at(p.0, p.1) {
                on_walked += 1;
                continue;
            }
            self.remember_local(p, radius);
        }
        if on_walked > 0 {
            tracing::info!(on_walked, "map explore: drop points on floor the body walked; not booked");
        }
    }

    /// The points that go on the books out of what the fresh frames
    /// proposed: obstacle edges as they are, holes only where at least
    /// [`MIN_DROP_FRAMES`] distinct frames put one within [`LOCAL_DEDUP_M`].
    pub(super) fn vote_drops(proposals: &[(usize, ((f64, f64), f64))]) -> Vec<((f64, f64), f64)> {
        let mut points: Vec<((f64, f64), f64)> = Vec::new();
        for (k, (p, radius)) in proposals {
            if *radius < DROP_RADIUS_M {
                points.push((*p, *radius));
                continue;
            }
            let mut frames: std::collections::HashSet<usize> = std::collections::HashSet::new();
            frames.insert(*k);
            for (k2, (p2, r2)) in proposals {
                if *r2 >= DROP_RADIUS_M && dist2(*p2, *p) < LOCAL_DEDUP_M {
                    frames.insert(*k2);
                }
            }
            if frames.len() >= MIN_DROP_FRAMES {
                points.push((*p, *radius));
            }
        }
        points
    }

    /// A drop on the books where the duck now stands was not a drop: the
    /// body is on that floor. Strike that point and only that point: a rim
    /// booked 10 cm short of the true rim is wrong by 10 cm, and what was
    /// booked deeper behind it (the reach) is the more likely to be the
    /// hole, not the less — struck with the rim, the whole corner of the
    /// stairwell left the books and the duck fell (full6, 2026-09-15).
    pub(super) fn strike_drops_under(&mut self, here: (f64, f64)) {
        // Nothing looked: nothing is proven (see `ExploreStatus::blind`).
        if self.blind() {
            return;
        }
        let before = self.local.len();
        self.local.retain(|(p, r)| *r < DROP_RADIUS_M || dist2(*p, here) >= STRIKE_M);
        if self.local.len() == before {
            return;
        }
        tracing::info!(struck = before - self.local.len(), at = ?here, "map explore: drops struck off the books — the duck is standing there");
    }

    /// The body went from `from` to `to`: the straight line between them
    /// joins the trail, a point every [`TRAIL_STEP_M`]. A leg is short
    /// (0.3–0.5 m) and its arc shallow, so the chord is the path within a
    /// cell or two. Off with `QUACKSAT_TRAIL=0`, for measuring.
    pub(super) fn walked(&mut self, from: (f64, f64), to: (f64, f64)) {
        self.trusted.walked(from, to);
        let d = dist2(from, to);
        let n = (d / TRAIL_STEP_M).ceil().max(1.0) as usize;
        for k in 0..=n {
            let t = k as f64 / n as f64;
            let p = (from.0 + t * (to.0 - from.0), from.1 + t * (to.1 - from.1));
            if self.trail.last().is_none_or(|l| dist2(*l, p) >= TRAIL_STEP_M * 0.5) {
                self.trail.push(p);
            }
        }
        if self.trail.len() > TRAIL_MAX {
            let cut = self.trail.len() - TRAIL_MAX;
            self.trail.drain(..cut);
        }
    }

    /// Whether the first `ahead_m` of the heading line from `pose` runs
    /// over the trail: every 5 cm sample has a trail point within
    /// [`TRAIL_NEAR_M`].
    pub(super) fn on_trail(&self, (x, y, yaw): (f64, f64, f64), ahead_m: f64) -> bool {
        if self.trail.is_empty() {
            return false;
        }
        let n = (ahead_m / 0.05).ceil() as usize;
        (1..=n).all(|k| {
            let d = k as f64 * 0.05;
            let p = (x + d * yaw.cos(), y + d * yaw.sin());
            self.trail.iter().rev().take(4000).any(|t| dist2(*t, p) < TRAIL_NEAR_M)
        })
    }

    /// Cells the planner may use whatever the margins say: where this
    /// body has walked, and the book's lanes.
    pub(super) fn lanes(&self) -> Vec<(f64, f64)> {
        if !trail_enabled() {
            return Vec::new();
        }
        // A sealed rim (see `DROP_SEAL_M`) seals its lanes too: the lanes
        // are what let the route through whatever the margins say, and
        // a route through a passage the guard refuses three times over
        // is the one thing the seal is for.
        self.trail
            .iter()
            .chain(self.lanes.iter())
            .copied()
            .filter(|l| !self.sealed.iter().any(|w| dist2(*w, *l) < SEAL_MATCH_M))
            .collect()
    }

    /// Is a drop on the books within `near` of the body?
    pub(super) fn drop_within(&self, near: f64) -> bool {
        let Some((here, _)) = self.last_pose else {
            return false;
        };
        self.local
            .iter()
            .any(|((dx, dy), r)| *r >= DROP_RADIUS_M && dist2((*dx, *dy), here) < near)
    }

    /// Record what the sensor met, once per spot.
    pub(super) fn remember_local(&mut self, point: (f64, f64), radius: f64) {
        // A drop where an untrusted pose puts it is a phantom, and the
        // planner keeps wide of it for the rest of the run.
        if radius >= DROP_RADIUS_M && !self.drops_bookable {
            return;
        }
        if self
            .local
            .iter()
            .any(|(p, _)| dist2(*p, point) < LOCAL_DEDUP_M)
        {
            return;
        }
        self.local.push((point, radius));
    }

    /// A drop booked from the guard's REFUSAL — the edge estimated from
    /// the refusal's range and bearing at the pose of the moment — as
    /// against one voted by a stand's frames (`record_drops`). In the
    /// GUARDED journey on a frozen map with its ground book the refusal
    /// books nothing: the rim is on the books already, and the refusal's
    /// point is the rim as a pose 10–17 cm off has it — house2's book
    /// crept from 39 to 66 in an afternoon of guarded runs at the
    /// stairwell's mouth (172 refusals in one run), the rim walking into
    /// the passage until no lane was left (rimG3, 2026-09-20). The blind
    /// journey keeps booking it, as the eleven tours that never fell did:
    /// with it off, the refused rim was never on the books, the route
    /// stayed, and the passage leg walked into the hole (house22tour,
    /// 2026-09-21; house20tour the same with every booking off).
    pub(super) fn remember_refused_drop(&mut self, point: (f64, f64)) {
        if self.frozen && self.ground_drops > 0 && self.policy.mode == Mode::JourneyGuarded {
            if !self.frozen_drops_noted {
                self.frozen_drops_noted = true;
                tracing::info!(at = ?point, "map explore: a drop refused on a frozen map with its ground book; the refusal books none");
            }
            return;
        }
        self.remember_local(point, DROP_RADIUS_M);
    }

    /// Nose against something: back away, then straight on. Always with [`BACK_VYAW`]: the
    /// gait backs up only with a yaw component, and (measured on the twin,
    /// twice) only with a positive one — `vyaw` -0.5 or -0.7 leaves the
    /// body where it is. So the swing goes the same way whichever side the
    /// obstacle is on; the next leg's heading correction sorts it out.
    /// Blind, so no more often than [`BACK_EVERY`].
    /// A drop on the books within [`BACK_DROP_NEAR_M`] of the body that is
    /// not ahead of it (|bearing| > 60°): stepping back beside a drop is
    /// what two falls into the twin's stairwell were; stepping back from
    /// a drop ahead moves away from it.
    pub(super) fn drop_beside(&self, (x, y, yaw): (f64, f64, f64)) -> bool {
        self.local.iter().any(|((dx, dy), r)| {
            *r >= DROP_RADIUS_M && dist2((*dx, *dy), (x, y)) < BACK_DROP_NEAR_M && wrap((dy - y).atan2(dx - x) - yaw).abs() > 1.05
        })
    }
}
