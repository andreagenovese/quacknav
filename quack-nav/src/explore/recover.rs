//! Split out of `explore.rs` on 2026-09-18 (the user's: one core, the modes apart, shared only what is identical). Behaviour unchanged by construction.

use super::*;

impl Job {
    /// Nothing works from here: leave the frontier that led here alone
    /// (or the plan after backing out is the same plan), forget the local
    /// obstacles around — the map has had its stands to ink them — back
    /// out, and let a stand map the new spot. Counted in `stuck`, for the
    /// whole job (the explorer's old legs, which reset it, went 2026-09-30).
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
        if !self.back_off(robot, grid) {
            let _ = stand(robot, FRONTIER_STOP_S);
        }
    }

}
