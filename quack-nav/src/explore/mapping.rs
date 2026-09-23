//! Split out of `explore.rs` on 2026-09-18 (the user's: one core, the modes apart, shared only what is identical). Behaviour unchanged by construction.

use super::*;

pub(super) const FRONTIER_STOP_S: f64 = 6.0;
/// How long a mapping leg stands. See [`Job::stop_s`].
pub(super) fn map_stand_s() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| knob("QK_MAP_STAND_S", LEG_STOP_S))
}
/// Give up when the pose stays untrusted this long.
pub(super) const LOST_PATIENCE: Duration = Duration::from_secs(180);
/// The panorama: the sensor sees the front hemisphere at a stand, so on
/// arriving somewhere mostly unknown the duck turns in place in steps of
/// this angle (closed on the map's own yaw, the gait's turning rate being
/// noisy), standing at each, this many times — a full circle, seen. Taken
/// at the start and where more than this share of the floor ahead within
/// this radius is unknown, never twice within this distance.
pub(super) const PANO_STEP_RAD: f64 = 0.785;
pub(super) const PANO_STEPS: u32 = 8;
/// The stand at each step: the body takes a couple of seconds to settle
/// after the spin and the mapper's head sweep needs a few more; six
/// seconds left sectors half-swept (measured by sector after a panorama).
// (Six was tried again on 2026-09-08 for the two-minute panorama the user
// saw on MuJoCo; the sector measurement above stands, so eight stays.)
pub(super) const PANO_STAND_S: f64 = 8.0;
pub(super) const PANO_TURN_MAX_S: f64 = 6.0;
/// The walking kick that gets the gait stepping before the spin, and the
/// spin itself.
pub(super) const PANO_KICK_S: f64 = 1.0;
pub(super) const PANO_SPIN: f64 = 0.7;
/// Command chunk while spinning, and how far before the step angle the
/// command stops: the body turns on by about this much afterwards.
pub(super) const PANO_CHUNK_S: f64 = 0.25;
pub(super) const PANO_LEAD_RAD: f64 = 0.35;
pub(super) const PANO_UNKNOWN_SHARE: f64 = 0.5;
pub(super) const PANO_RADIUS_M: f64 = 1.5;
pub(super) const PANO_MIN_GAP_M: f64 = 2.5;
/// How often to look at the map while waiting for it.
pub(super) const WAIT: Duration = Duration::from_secs(1);
/// From the pose the map claims, how many of the sensor's obstacles in
/// the newest frame sit where the map has a wall (agree), and how many
/// lie *beyond* a wall the map has in that direction (disagree): seeing
/// through a mapped wall is the one thing a true pose cannot do. Floor
/// the map shows beyond an obstacle is not counted — a map under
/// construction misses every low piece of furniture the sensor sees.
pub(super) fn map_sensor_agreement(
    grid: &Grid,
    cliff: &crate::cliff::CliffStatus,
    (x, y, yaw): (f64, f64, f64),
) -> (usize, usize) {
    let (mut agree, mut disagree) = (0, 0);
    let Some(frame) = cliff.recent.iter().filter(|f| !f.moving).last() else {
        return (0, 0);
    };
    for o in &frame.obstacles {
        if o.range_m > AGREE_RANGE_M {
            continue;
        }
        let c = grid.clearance(x, y, yaw + o.bearing, 3.0);
        if c.by != Blocked::Wall {
            continue;
        }
        if (c.free_m - o.range_m).abs() <= AGREE_TOL_M {
            agree += 1;
        } else if o.range_m > c.free_m + AGREE_TOL_M {
            disagree += 1;
        }
    }
    (agree, disagree)
}

impl Job {
    /// The frontier to pursue: the one already chosen while it is still
    /// there, else the cheapest.
    pub(super) fn pick(&mut self, fs: &[Frontier]) -> Option<Frontier> {
        if let Some((t, _)) = self.target
            && let Some(same) = fs.iter().find(|f| dist2(f.target, t) < 0.3)
        {
            return Some(same.clone());
        }
        // Finish the room before leaving it: a frontier beyond LOCAL_M costs
        // more the farther it is, so a sliver at hand beats a wide opening
        // two rooms away, and the duck stops criss-crossing the flat.
        let f = fs
            .iter()
            .min_by(|a, b| Self::local_score(a).total_cmp(&Self::local_score(b)))?
            .clone();
        self.target = Some((f.target, 0));
        Some(f)
    }

    pub(super) fn local_score(f: &Frontier) -> f64 {
        // Gentle, and never in favour of a sliver: the slivers around the
        // start are reborn at every map settle, and a strong locality
        // kept the duck finishing them for half an hour (run 52).
        let sliver = if f.cells < LOCAL_MIN_CELLS { SLIVER_PENALTY } else { 1.0 };
        f.score * sliver * (1.0 + LOCAL_SLOPE * (f.distance_m - LOCAL_M).max(0.0) / LOCAL_M)
    }

    /// Somewhere already mapped and worth standing in again: the nearest
    /// point of the trail that lies at least [`ANCHOR_BACK`] points back,
    /// within [`ANCHOR_MAX_M`]. `None` when the duck has not walked far
    /// enough yet, or when everything old is too far to be worth it.
    pub(super) fn anchor_target(&self, from: (f64, f64)) -> Option<(f64, f64)> {
        if self.trail.len() <= ANCHOR_BACK {
            return None;
        }
        self.trail[..self.trail.len() - ANCHOR_BACK]
            .iter()
            .copied()
            .map(|p| (p, dist2(from, p)))
            .filter(|(_, d)| *d <= ANCHOR_MAX_M && *d > ANCHOR_ARRIVE_M)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(p, _)| p)
    }

    /// Whether the floor around `at` is mostly unknown and no panorama
    /// was taken near here.
    pub(super) fn wants_panorama(&self, grid: &Grid, (x, y, yaw): (f64, f64, f64)) -> bool {
        // Never on the way to a goal. A panorama is how a mapping job
        // learns a room it has not seen, and it costs eighty to a hundred
        // and thirteen seconds; a journey across floor already mapped has
        // nothing to learn from one. Measured on the twin: two three-metre
        // journeys took 161 s and 282 s, and one or two panoramas in the
        // middle of them were 123 s and 168 s of that — three quarters and
        // three fifths of the trip, against ordinary legs of 4.4 s. When a
        // journey does meet unknown floor, the guards refuse the step and
        // the planner routes round it, which is the right answer anyway.
        if self.goal.is_some() {
            return false;
        }
        grid.unknown_ahead(x, y, yaw, PANO_RADIUS_M) > PANO_UNKNOWN_SHARE
            && self.panoramas.iter().all(|p| dist2(*p, (x, y)) > PANO_MIN_GAP_M)
    }

    /// Turn in place a step at a time, standing at each, until the whole
    /// circle has been seen. Each step is closed on the map's yaw.
    pub(super) fn panorama(&mut self, handle: &ExploreHandle, robot: &mut dyn Body, at: (f64, f64)) {
        self.panoramas.push(at);
        tracing::info!(at = ?at, "map explore: panorama");
        // Close each step on the fastest heading there is: odometry from
        // the state stream (tens of Hz) when the cliff guard is on, the
        // map's pose (1 Hz) otherwise — a second of latency at 30°/s made
        // 45° steps into 75° and eight of them nearly two turns.
        let yaw_now = |robot: &dyn Body| {
            robot
                .cliff()
                .and_then(|c| c.odom_yaw)
                .or_else(|| robot.frame().map(|f| f.yaw))
        };
        let mut stuck = 0;
        for _ in 0..PANO_STEPS {
            // A panorama is a minute and a half of standing and turning,
            // and it used to see a stop request only when it was over: a
            // `robot.map_explore {stop}` sat unanswered for a hundred
            // seconds while the duck finished looking around.
            if handle.stop.load(Ordering::Relaxed) {
                tracing::info!("map explore: panorama cut short by a stop request");
                return;
            }
            let Some(yaw0) = yaw_now(robot) else { break };
            // A pure turn in place first, past the gait's dead zone: no
            // kick to be refused, the body within a few centimetres. The
            // kick below is what is left for a gait that does not turn so.
            if let Some(turned) = self.turn_in_place(robot, PANO_SPIN.signum(), PANO_STEP_RAD)
                && turned >= PANO_STEP_RAD - PANO_LEAD_RAD
            {
                stuck = 0;
                let _ = stand(robot, PANO_STAND_S);
                continue;
            }
            // The gait does not turn in place from a standstill at all
            // (measured: 1–2° in 6 s, any yaw); it does once stepping, so
            // a one-second walking kick first — through map_step, so it is
            // refused if something is right ahead — then yaw only, which
            // spins about 30°/s for 15 cm of drift. On the twin the spin
            // goes left whatever the sign; the sign is kept for a duck
            // that honours it.
            if let Err(why) = robot.step(&json!({"vx": 0.3, "vyaw": PANO_SPIN, "walk_s": PANO_KICK_S, "stop_s": 0.0})) {
                // A refused kick used to fall through to the yaw alone,
                // which does not turn this gait: the beak stayed on the
                // drop that refused it, and every step after was refused
                // the same way — four steps of 6.9 s turning 4° in all at
                // the start of a run on the twin (2026-09-23, a drop
                // 0.65 m ahead of the spawn; the night before the same
                // steps had happened to face elsewhere). The careful turn
                // knows the ways round a refused kick; two steps in a row
                // that still do not turn end the panorama here.
                tracing::info!(why, "map explore: panorama kick refused; the careful turn instead");
                self.spin(robot, PANO_SPIN.signum(), PANO_STEP_RAD);
                let turned = yaw_now(robot).is_some_and(|y| wrap(y - yaw0).abs() >= PANO_STEP_RAD - PANO_LEAD_RAD);
                stuck = if turned { 0 } else { stuck + 1 };
                if stuck >= 2 {
                    tracing::info!("map explore: the panorama cannot turn here; ending it");
                    return;
                }
                let _ = stand(robot, PANO_STAND_S);
                continue;
            }
            stuck = 0;
            let started = robot.now();
            // Stop early by what the gait keeps turning after the command
            // ends (about 20° at this rate, measured: 45° steps came out
            // as 67°), in short chunks so the check is frequent.
            while (robot.now() - started).as_secs_f64() < PANO_TURN_MAX_S {
                let _ = robot.blind_move(&json!({"vx": 0.0, "vyaw": PANO_SPIN, "duration_s": PANO_CHUNK_S}));
                if yaw_now(robot).is_some_and(|y| wrap(y - yaw0).abs() >= PANO_STEP_RAD - PANO_LEAD_RAD) {
                    break;
                }
            }
            let _ = stand(robot, PANO_STAND_S);
        }
    }
}
