//! The mapping host loop — stillness, windows, the tracking watchdog and
//! kidnap recovery — shared by robotd's worker and the offline bench.
//!
//! [`crate::pipeline::Slam`] exists so the graph wiring is written once;
//! this module exists for the same reason one level up. The still
//! detector, the window flush rules, the quality gates and the
//! lost/relocalize state machine used to live in robotd's worker, with the
//! replay bench keeping a hand-mirrored copy — the exact arrangement whose
//! drift the pipeline module was created to prevent. A ground-truth
//! recording is only worth its disk space if the bench replays it through
//! the *same* decisions the robot made, so the decisions moved here and
//! both hosts drive this.
//!
//! The state machine, in one paragraph: scans integrate only through
//! vetted still windows (see [`crate::accumulator`]); before a window
//! inks, it is scored against the map at the tracked pose
//! ([`crate::relocalize::score_pose`]) and a window the map can judge but
//! flatly contradicts flips the mapper to *lost* — a kidnapped robot's
//! scans land in territory the map knows and disagree everywhere, while a
//! robot exploring a new room lands in territory the map cannot judge and
//! keeps mapping. While lost, nothing inks and every window becomes a
//! brute-force relocalize attempt ([`crate::relocalize`]); an accepted
//! pose snaps tracking there and mapping resumes. The same watchdog heals
//! a resumed session whose robot moved while the daemon was down.

use crate::accumulator::{AccumulatorConfig, WindowAccumulator};
use crate::grid::OccupancyGrid;
use crate::pipeline::Slam;
use crate::pose_graph::{between, compose, wrap_pi};
use crate::relocalize::{RelocalizeConfig, relocalize_against_grid, score_pose, score_pose_rays};
use crate::scan_matcher::{ScanMatchConfig, match_scan};
use crate::submap::{Pose2, Scan};

/// Scan-to-map tracking correction at every vetted window.
///
/// Between loop closures the tracked pose is dead reckoning: odometry
/// deltas only. Every window the map can judge is also a measurement of
/// where the robot stands, and leaving it unused lets small drift grow
/// until the watchdog calls it a kidnap. This step matches the composite
/// against the map as it stood when the stand began (never the stand's
/// own ink), seeded and regularized at the tracked pose, and moves the
/// tracked pose by the result — bounded, and only along the directions the
/// scene constrains: a composite facing one straight wall pins the
/// distance to that wall and nothing else, and a "correction" along the
/// wall would be sliding.
#[derive(Debug, Clone, Copy)]
pub struct TrackingConfig {
    pub enabled: bool,
    /// The largest correction one window may apply; a larger one is not a
    /// drift correction but a disagreement, left to the watchdog.
    pub max_correction_m: f32,
    pub max_correction_rad: f32,
    /// Gaussian prior on the tracked pose during the match.
    pub prior_sigma_xy: f32,
    pub prior_sigma_yaw: f32,
    /// The map must judge at least this many beams, and this fraction of
    /// the composite, for the match to mean anything.
    pub min_observed_beams: u32,
    pub min_observed_fraction: f32,
    /// A translation eigen-direction whose normal-matrix eigenvalue is
    /// below this fraction of the larger one is unconstrained: the
    /// correction's component along it is dropped.
    pub min_conditioning: f32,
    /// Yaw is corrected only when its normal-matrix entry, scaled to the
    /// translation ones, clears this floor.
    pub min_yaw_stiffness: f32,
    /// A correction must improve the residual by this factor over the
    /// pose it started from, or the match found nothing better than noise.
    pub min_improvement: f32,
    /// A correction larger than this cuts the current submap.
    ///
    /// What is already drawn in it was drawn at the pose before the
    /// correction, and a submap is rigid: no later optimisation can
    /// separate the old ink from the new. Measured on the twin, the live
    /// pose oscillates between 1 and 35 cm and comes back — so the map's
    /// error is not the pose wandering but ink laid while it wandered and
    /// never revised (2026-09-11). Cutting at the correction bounds each
    /// submap to one pose's worth of error.
    ///
    /// Zero disables it.
    pub cut_on_correction_m: f32,
    /// The map's own noise floor: a window whose residual at the tracked
    /// pose is already below this has nothing to correct, and moving the
    /// pose by map noise would be worse than leaving odometry alone.
    pub min_residual_before_m: f32,
    /// And a correction must *end* below this, in metres, not merely
    /// improve on where it started.
    ///
    /// The correction pulls the pose onto the map, which repairs drift
    /// while the map is right and reinforces it once the map is wrong:
    /// measured against the twin's truth, the corrections that drove one
    /// run's pose to over a metre each improved their window's residual
    /// and each ended around 0.055 m, while the corrections that helped
    /// ended at 0.009–0.026 (2026-09-11). Where the map and the sensor
    /// still disagree after the move, the move was toward a lie.
    ///
    /// Zero means no absolute bar, which is how it behaved before.
    ///
    /// **Two centimetres, and it is the strongest thing measured on this
    /// twin.** Seven recorded sessions of one house, scored against the
    /// house's own walls: as it was, 4.7 % of mapped wall more than 10 cm
    /// out at the median, 7.8 % at the mean, 21.6 % at the worst; with the
    /// correction disabled entirely, 3.4 / 4.9 / 14.6; with this bar at
    /// 0.02, **1.2 / 2.5 / 10.9**, better on six of the seven recordings
    /// and better on all three statistics at once, which nothing else
    /// tried managed. At 0.03 it is 2.1 / 4.8 / 16.0, so the value
    /// matters.
    pub max_residual_after_m: f32,
}

impl Default for TrackingConfig {
    fn default() -> Self {
        Self {
            // On: measured against MuJoCo ground truth on an hour-long
            // exploration (run 67, 2026-09-06) the pose error fell from
            // 0.38 m median / 0.73 m late-run to 0.16 / 0.15 with it; only
            // a gentle drive with near-perfect odometry loses a little.
            enabled: true,
            max_correction_m: 0.30,
            max_correction_rad: 0.20,
            prior_sigma_xy: 0.15,
            prior_sigma_yaw: 0.10,
            min_observed_beams: 100,
            min_observed_fraction: 0.30,
            min_conditioning: 0.10,
            min_yaw_stiffness: 0.05,
            min_improvement: 0.8,
            max_residual_after_m: 0.02,
            // Off until measured.
            cut_on_correction_m: 0.0,
            min_residual_before_m: 0.0,
        }
    }
}

/// Stillness from odometry itself, not just the host's moving flag: a
/// robot pushed by hand is moving whatever the control loop asked for.
#[derive(Debug, Clone, Copy)]
pub struct StillConfig {
    /// Displacement window length.
    pub window_s: f32,
    /// Max translation across the window to count as still.
    pub max_dxy_m: f32,
    /// Max |yaw| across the window to count as still.
    pub max_dyaw_rad: f32,
}

impl Default for StillConfig {
    fn default() -> Self {
        Self {
            window_s: 0.5,
            max_dxy_m: 0.01,
            max_dyaw_rad: 0.05,
        }
    }
}

/// The tracking watchdog's thresholds. All three must hold to declare
/// tracking lost — the bar is deliberately high, because a false "lost"
/// stops the map cold until a relocalize succeeds.
#[derive(Debug, Clone, Copy)]
pub struct WatchdogConfig {
    /// The map must be able to judge at least this many beams.
    pub min_observed_beams: u32,
    /// ... and at least this fraction of the window's beams. A floor, not
    /// a majority: a kidnapped robot mostly paints new territory (12 % of
    /// beams judged, measured), and the verdict lives in the judged beams
    /// — an explorer's judged beams agree with the map, a kidnapped
    /// robot's contradict it.
    pub min_observed_fraction: f32,
    /// Mean residual over the judged beams above which the window is a
    /// contradiction, not noise. Map noise floor is ~0.05–0.09 m; honest
    /// inter-stop drift stays well under 0.2 m.
    pub max_mean_residual_m: f32,
    /// Per-beam residual clamp for the score.
    pub clamp_m: f32,
    /// A cell is a wall for the distance field past this. 150 matches the
    /// wire frame's wall definition: one double-inked window (2 × 85)
    /// qualifies, so a thinly-mapped revisit is not scored against a
    /// field that pretends its own walls are not there.
    pub wall_threshold_fp: i16,
    /// A cell is *observed* (judgeable) past this |log-odds|.
    pub observed_fp: i16,
    /// Consecutive contradicting windows before tracking is declared
    /// lost. The first contradiction is quarantined (not inked) — one
    /// window can be a lean, a passer-by, or fresh phantom ink; a kidnap
    /// contradicts on every window.
    pub lost_after_windows: u32,
}

/// Long beams the watchdog needs before it lets them overrule a
/// contradiction (see `LONG_BEAM_M`).
const LOW_THING_MIN_LONG: u32 = 30;

impl Default for WatchdogConfig {
    fn default() -> Self {
        Self {
            min_observed_beams: 100,
            min_observed_fraction: 0.05,
            max_mean_residual_m: 0.25,
            clamp_m: 0.5,
            wall_threshold_fp: 150,
            observed_fp: 50,
            lost_after_windows: 2,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MapperConfig {
    /// `false` = stop-and-scan (windows, votes, gates); `true` = ink every
    /// frame directly (more coverage, blurrier walls, no watchdog).
    pub continuous: bool,
    /// In `continuous`, how often to match what the sensor sees against the
    /// map and move the tracked pose onto it — the same correction a
    /// stop-and-scan window earns, on a rolling window instead of a still
    /// one. Zero is the old behaviour.
    ///
    /// Continuous had no such correction at all: `frame` inks and returns,
    /// and the correction lives inside the still-window path, so the pose
    /// was odometry plus loop closures for as long as the robot kept
    /// walking. Measured on the twin (2026-09-13): continuous drifts 17.7 cm
    /// at the median against 9 for stop-and-scan and 32 cm at the peak, and
    /// its worst map is 16.6 % of wall misplaced against 4; with the stands
    /// taken away as well — which in this mode ink nothing — the robot's own
    /// watchdog declared the position lost after two minutes.
    ///
    /// **Measured, and it makes the map worse: off by default.** With the
    /// correction every second (rolling composites of 150–550 beams, no
    /// vote, the same bars as a still window), 97 of 771 attempts moved the
    /// pose in one run — and the maps came out at 12.0 % and 23.2 % of wall
    /// misplaced, the worst of the day, against 1.1–6.9 % for continuous
    /// with no correction at all. Continuous inks every frame at once, so a
    /// correction toward a wrong patch of map is inked before anything can
    /// refute it: the positive feedback the absolute bar was added to stop
    /// (2026-09-12), with no still window left to stop it. A correction
    /// that could work here would have to hold the ink until the pose is
    /// confirmed, which is a different mapper.
    pub continuous_correct_s: f32,
    pub accumulator: AccumulatorConfig,
    pub still: StillConfig,
    /// A still window flushes after this long even if the stand continues,
    /// so the map builds while you watch it.
    pub window_flush_after_s: f32,
    /// A vetted window with fewer beams is discarded, not inked — a seated
    /// robot's floor-clutter windows measured 2–27 beams; a real stop
    /// measures in the hundreds.
    pub min_window_beams: usize,
    /// How many times a vetted window inks. One pass writes log-odds 85
    /// per wall cell and a wall starts at 150, so a lap that stops once
    /// per spot would paint itself invisibly; a window has survived
    /// per-cell frame voting and is worth more than one raw frame.
    pub window_ink_passes: usize,
    pub watchdog: WatchdogConfig,
    pub tracking: TrackingConfig,
    /// Localize only: the map is never inked and no submap is opened;
    /// the still windows serve the tracking correction and the watchdog
    /// alone. A house mapped once, driven many times, does not drift
    /// with the driving (quacksat, 2026-09-16).
    pub frozen: bool,
    pub relocalize: RelocalizeConfig,
    /// A relocalize probe is the composite decimated to at most this many
    /// beams: a window composite carries thousands, and the brute-force
    /// search is O(cells × yaws × beams) — full composites would cost
    /// seconds per attempt on the robot for no accuracy the search needs.
    pub relocalize_max_beams: usize,
    /// A relocalize candidate never snaps tracking by itself: the NEXT
    /// window, moved to the candidate-implied pose via odometry, must
    /// also agree with the map this well. A wrong basin in a young map
    /// scored 0.022 on the search's own probe (measured, field test
    /// four) and poisoned everything after; a second, independent window
    /// from a slightly different moment is what a coincidence fails.
    pub relocalize_confirm_max_residual_m: f32,
    /// ... and the map must be able to judge at least this fraction of
    /// the confirming window. Through the ToF's keyhole, a wall wedge
    /// aliases onto any other wall at the same range — the measured
    /// kidnap landed 204 of 1680 beams on old walls at residual 0.005
    /// while 0.3 m off the truth. A window that sees the scene it claims
    /// to stand in gets judged on half its beams, not an eighth.
    pub relocalize_confirm_min_fraction: f32,
    /// A search candidate is nominated for confirmation only after this
    /// many consecutive searches agreed on it (within
    /// `relocalize_agree_m` / `_rad`). Measured on the twin: in a
    /// symmetric flat the search's winner jumped between two basins from
    /// one window to the next and the confirmation, judged from the same
    /// stand, accepted whichever came last — 3 m off, twice in a day.
    /// Candidates that disagree are ambiguity, and ambiguity is not a
    /// pose.
    pub relocalize_agree_windows: u32,
    /// Hard-lost give-up: after this many windows without a unique,
    /// agreed and confirmed candidate, tracking resumes at the
    /// odometry-carried pose, unverified. A watchdog "lost" is often the
    /// MAP being locally wrong (the window agreed with the true walls at
    /// 0.04 m while contradicting the map at 0.28, measured) — and a
    /// search through an ambiguous map moved a correct pose 3 m away.
    /// Odometry over a minute is the better bet; 0 disables.
    pub lost_give_up_windows: u32,
    /// Hard-lost search radius: a watchdog "lost" with tracking corrected
    /// against the map every window means the map is locally wrong far
    /// more often than the robot moved, so a search winner farther than
    /// this from the odometry-carried pose is an alias, not a pose
    /// (measured: winners at residual 0.000 3.6 m off, twice). A sit, a
    /// fall or a session resume keep the global search. 0 = global.
    pub hard_lost_search_radius_m: f32,
    pub relocalize_agree_m: f32,
    pub relocalize_agree_rad: f32,
    /// When suspicion came from a sit, a fall or a session resume (soft —
    /// nothing has CONTRADICTED the pose) and this many windows could not
    /// be judged either way (unmapped view), give up and resume at the
    /// odometry-carried pose. Without an escape, a robot that sits facing
    /// an unmapped corner stays "searching" forever; with evidence of
    /// displacement the escape never applies.
    pub suspect_give_up_windows: u32,
}

impl Default for MapperConfig {
    fn default() -> Self {
        Self {
            continuous: false,
            continuous_correct_s: 0.0,
            accumulator: AccumulatorConfig::default(),
            still: StillConfig::default(),
            window_flush_after_s: 3.0,
            min_window_beams: 60,
            window_ink_passes: 2,
            watchdog: WatchdogConfig::default(),
            tracking: TrackingConfig::default(),
            frozen: false,
            relocalize: RelocalizeConfig {
                // Align the search's idea of a wall with the watchdog's
                // (and the 2×-ink reality) — the stock 200 was tuned on
                // prototype captures inked far more than twice.
                wall_threshold_fp: 150,
                // A composite worth relocalizing on carries ≥ 60 beams
                // (min_window_beams); demanding that many *in the map*
                // kills the measured failure mode where a 44-beam wedge
                // "accepted" a pose across the room.
                min_beams_used: 60,
                ..RelocalizeConfig::default()
            },
            relocalize_max_beams: 256,
            relocalize_confirm_max_residual_m: 0.10,
            relocalize_confirm_min_fraction: 0.3,
            relocalize_agree_windows: 2,
            lost_give_up_windows: 8,
            hard_lost_search_radius_m: 1.0,
            relocalize_agree_m: 0.3,
            relocalize_agree_rad: 0.35,
            suspect_give_up_windows: 10,
        }
    }
}

/// One control-loop tick's worth of the robot, as the mapper needs it.
/// (Gravity, trunk height and head joints feed the *reprojection*, which
/// stays in the host — this crate never links the kinematics.)
#[derive(Debug, Clone, Copy)]
pub struct MapperSample {
    pub odom: Pose2,
    /// The host's "the robot is doing something" verdict.
    pub moving: bool,
    /// Seated: never map from sitting height — the ToF sees knees and
    /// floor clutter, and the ground-truth protocol uses the sit as its
    /// kidnap marker.
    pub sitting: bool,
    /// Fallen over: a fall can displace and rotate the robot, and the
    /// scans from the floor are garbage anyway.
    pub fallen: bool,
}

/// What one call did — the host turns these into log lines; the bench
/// turns them into metrics. Data, not strings, so both can.
#[derive(Debug, Clone, Copy)]
pub enum Note {
    WindowIntegrated {
        beams: usize,
        windows: u32,
        /// The watchdog's agreement score for this window (residual over
        /// the beams the map could judge, that count, and the window's
        /// total) — diagnostics the bench plots to tune the thresholds.
        mean_residual_m: f32,
        n_observed: u32,
        n_beams: u32,
    },
    WindowDiscarded {
        beams: usize,
    },
    /// A first contradicting window: not inked, not yet lost.
    WindowQuarantined {
        mean_residual_m: f32,
        n_observed: u32,
    },
    /// The search proposed a pose; the next window must confirm it.
    RelocalizeCandidate {
        pose: Pose2,
        mean_residual_m: f32,
    },
    /// The robot sat: it may have been carried, and neither odometry nor
    /// a keyhole ToF view can prove it was not (a kidnapped wall wedge
    /// aliases onto any wall, measured). The pose is suspect until a
    /// window confirms it — the current pose is pre-seeded as the
    /// relocalize candidate, so an unmoved robot confirms in one window.
    SuspectAfterSit,
    /// The robot fell: same treatment as a sit — a fall can drag and spin.
    SuspectAfterFall,
    /// Soft suspicion (sit/fall/resume) expired: nothing could judge the
    /// pose either way for `suspect_give_up_windows` windows, so tracking
    /// resumed at the odometry-carried pose, unverified.
    ResumedUnverified {
        pose: Pose2,
    },
    /// The map could judge this window and flatly contradicts it.
    LostTracking {
        mean_residual_m: f32,
        n_observed: u32,
    },
    Relocalized {
        pose: Pose2,
        mean_residual_m: f32,
    },
    RelocalizeRejected {
        best_pose: Pose2,
        mean_residual_m: f32,
    },
    /// A candidate that agreed, not believed: the scan leaves a valley
    /// through it along `along` (see [`crate::relocalize::valley_at`]).
    RelocalizeAmbiguous {
        pose: Pose2,
        along: (f32, f32),
    },
    LoopClosed {
        n_loops: usize,
        dx: f32,
        dy: f32,
        dyaw: f32,
    },
    /// A vetted window matched the pre-stand map and moved the tracked
    /// pose by (dx, dy, dyaw) in the map frame.
    TrackingCorrected {
        dx: f32,
        dy: f32,
        dyaw: f32,
        residual_before_m: f32,
        residual_after_m: f32,
        n_beams_used: u32,
    },
}

/// One confirmation attempt's outcome: a candidate pose checked against a
/// fresh window. Confirmed = the window agrees at the implied pose;
/// refuted = the map judged it and said no (real evidence of
/// displacement); ambiguous = the map judged some beams and the verdict
/// fell between agreement and contradiction — keep searching, but this is
/// NOT a window the map had no opinion about; unjudgeable = the view
/// lands where the map truly has no opinion, the only kind of window the
/// give-up escape may consume.
enum Verdict {
    Confirmed(Pose2, f32),
    Refuted,
    Ambiguous,
    Unjudgeable,
}

pub struct Mapper {
    cfg: MapperConfig,
    slam: Slam,
    acc: WindowAccumulator,
    /// (t_s, x, y, yaw) over the last `still.window_s`.
    odom_window: Vec<(f32, f32, f32, f32)>,
    was_still: bool,
    /// The lost settings to restore once the boot's search has confirmed
    /// a pose (see `resumed_lost`).
    after_boot: Option<(f32, u32)>,
    window_opened: Option<f32>,
    windows: u32,
    lost: bool,
    /// Consecutive contradicting windows so far (reset by any agreeing one).
    suspect: u32,
    /// A relocalize candidate awaiting confirmation: (candidate pose, the
    /// tracked pose when it was proposed — odometry deltas since then move
    /// the candidate along with the robot).
    pending_reloc: Option<(Pose2, Pose2)>,
    /// The last search winner, carried to the pose of now, and how many
    /// consecutive searches agreed with it.
    last_search: Option<(Pose2, Pose2, u32)>,
    /// While lost on a map it already has, every basin the search still
    /// thinks plausible: the pose, when it was last seen, and how many
    /// windows have agreed with it, and how far the body has walked while
    /// it survived. One still window of an 8×8 ToF cannot tell a kitchen
    /// from another rectangle across the flat, and two windows a second
    /// apart are the same window twice — but an alias is contradicted by a
    /// viewpoint a few metres on, and the true place is not. Carried
    /// forward by odometry between windows.
    hypotheses: Vec<Hypothesis>,
    /// This mapper was resumed on a map from an earlier run and has never
    /// known where it is. Only then are several hypotheses carried: a
    /// kidnap has a prior worth using, and its recovery is measured.
    resumed_from_session: bool,
    /// A fall since the pose was last confirmed: the pose before it is no
    /// prior worth trusting (a fall drags and spins the body, and on the
    /// twin the duck got up elsewhere and relocalized 0.64 m off), so the
    /// valley test applies as at a boot, and nothing resumes unverified.
    after_fall: bool,
    /// Booted on a saved map and not yet confirmed anywhere on it: the
    /// confirmation of a candidate needs travel, not just a second look.
    booting: bool,
    /// Windows spent hard-lost (the watchdog's verdict, not a sit/fall).
    lost_windows: u32,
    /// Lost by the watchdog's verdict — a contradiction with the map —
    /// as opposed to a sit, a fall or a resume. Only this kind may give
    /// up and resume on odometry: a carried robot's seed, once refuted,
    /// must never (see the kidnap tests).
    hard_lost: bool,
    /// The "I was not moved" hypothesis, when suspicion is soft (sit,
    /// fall, session resume — no scan has contradicted the pose). Same
    /// (pose, tracked-then) carrying as `pending_reloc`. Cleared when a
    /// judgeable window REFUTES it: that is evidence of displacement, and
    /// the give-up escape must never fire after evidence.
    soft_seed: Option<(Pose2, Pose2)>,
    /// Windows since suspicion that could not be judged either way.
    unjudged: u32,
    /// Consecutive windows that AGREED with the soft seed. Two are needed
    /// before tracking resumes on it — see the seed-confirmation comment.
    seed_agreed: u32,
    /// The map as it stood when the current stand began — what the
    /// watchdog judges the stand's windows against. Judging against the
    /// LIVE map lets a kidnapped stand vouch for itself: its first window
    /// paints the kidnapper's room, and every following window then
    /// "agrees with the map" it just painted (measured: vs-map 0.005
    /// while vs-truth 0.3–0.5). Ink earned during a stand never testifies
    /// for that stand.
    stand_grid: Option<OccupancyGrid>,
    /// The particle filter at boot, see [`BootSearch`]; `None` otherwise.
    boot: Option<BootSearch>,
    /// `continuous` only: frames since the last correction, and when that
    /// was. See [`MapperConfig::continuous_correct_s`].
    roll: WindowAccumulator,
    roll_at: f32,
    /// The last window handed to `absorb_window`, whatever became of it —
    /// a bench inspects it to score composites against ground truth. One
    /// composite clone per window; noise next to the integration itself.
    last_window: Option<(Pose2, Scan)>,
}

/// `MAPLOC_MULTI_HYP=0` turns the multi-hypothesis boot search off, for
/// measuring against the single-best agreement it replaces.
fn multi_hypothesis() -> bool {
    std::env::var("MAPLOC_MULTI_HYP")
        .map(|v| v != "0")
        .unwrap_or(true)
}

/// How far the body must have got from where it first saw a hypothesis
/// before the hypothesis can be believed — the chord, not the path. The
/// path lied: a duck turning on the spot drifts a decimetre a kick, and
/// twelve kicks "walked" 1.5 m without leaving the square metre it woke
/// on, which is how four wake-ups in five confirmed the mirror image
/// (2026-09-14). `MAPLOC_HYP_TRAVEL` overrides it.
fn hypothesis_travel_m() -> f32 {
    std::env::var("MAPLOC_HYP_TRAVEL")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1.0)
}

/// How many hits the leading hypothesis must have over the runner-up before
/// it is nominated. One was not a lead: in a corridor the 180° twin agrees
/// with every window the truth agrees with until a doorway comes into
/// view, and a lead of one is noise (a wake-up came home 123° wrong that
/// way, 2026-09-14). `MAPLOC_HYP_LEAD` overrides it.
fn hypothesis_lead() -> u32 {
    std::env::var("MAPLOC_HYP_LEAD").ok().and_then(|v| v.parse().ok()).unwrap_or(3)
}

/// `MAPLOC_RAY_JUDGE=1` judges candidates along the ray
/// (`relocalize::score_pose_rays`) instead of by endpoints alone. OFF by
/// default, measured off (2026-09-15): on the two boot recordings against
/// run 71's map it turned one wrong fix right (1788872069) and one right
/// fix wrong (1788929139, the kitchen alias at 57 s) — a true pose in a
/// map with doubled walls has beams that "cross" a phantom wall, and the
/// judge refuses the truth until an alias that crosses nothing comes
/// along. The test is right in principle and needs a tolerance for the
/// map's own noise before it can be the default.
fn ray_judge() -> bool {
    std::env::var("MAPLOC_RAY_JUDGE").map(|v| v == "1").unwrap_or(false)
}

/// At boot, how far the body must have moved between the window that
/// nominated a candidate and the one that confirms it. Two windows from
/// the same spot are the same window twice, and an alias agrees with
/// itself; the wall that tells the alias from the truth is the one that
/// comes into view after a leg. `MAPLOC_CONFIRM_TRAVEL` overrides it.
fn confirm_travel_m() -> f32 {
    std::env::var("MAPLOC_CONFIRM_TRAVEL")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.5)
}

/// `MAPLOC_MCL=1` runs the particle filter (`mcl.rs`, wired to nothing
/// before this) as a boot search on a resumed map: it proposes, the still
/// windows judge, exactly as the brute-force search's candidates are judged.
fn boot_mcl() -> bool {
    std::env::var("MAPLOC_MCL")
        .map(|v| v == "1")
        .unwrap_or(false)
}
fn boot_mcl_particles() -> usize {
    std::env::var("MAPLOC_MCL_N")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(800)
}
/// A lock is not a candidate until the body has swept this much yaw and
/// moved this far since the seed: the filter has no motion gate of its own
/// (`mcl.rs`), and a stationary 45° wedge locks on a mirror image as
/// happily as on the truth.
fn boot_mcl_yaw_rad() -> f32 {
    std::env::var("MAPLOC_MCL_YAW")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.8)
}
fn boot_mcl_travel_m() -> f32 {
    std::env::var("MAPLOC_MCL_TRAVEL")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.10)
}
fn boot_mcl_lock_residual_m() -> f32 {
    std::env::var("MAPLOC_MCL_RESID")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.08)
}

/// The particle filter's boot search: alive only while a resumed mapper is
/// lost. Fed every frame — walking or standing, which the still-window
/// search cannot use — and every odometry tick; when it locks and the body
/// has moved enough for the lock to mean something, its pose goes into
/// `pending_reloc` like any other candidate, to be confirmed or refuted by
/// the next still window.
struct BootSearch {
    mcl: crate::mcl::Localizer,
    grid: OccupancyGrid,
    last_odom: Option<Pose2>,
    origin_odom: Option<Pose2>,
    /// Body-frame motion since the last frame, folded into one `predict`.
    pending: Pose2,
    yaw_swept: f32,
    posture_ok: bool,
    frames: u32,
    proposed: Option<Pose2>,
    /// Locks put to the uniqueness test, how many it turned away, and how
    /// many it could not judge (thin window, no basin).
    judged: u32,
    refused: u32,
    unjudged: u32,
    /// The window the last judgement used: one judgement per window.
    judged_window: Option<Pose2>,
}

/// A place the search thinks the duck might be, while it is lost on a map
/// it already has.
#[derive(Clone, Copy)]
struct Hypothesis {
    pose: Pose2,
    /// The raw odometry pose when it was last carried forward. While the
    /// mapper is lost the tracked pose is frozen on purpose — the tick
    /// that advances it is skipped, so a guess is never laundered into the
    /// graph — and odometry is the only thing that still moves.
    seen: Pose2,
    /// Windows that have agreed with it.
    hits: u32,
    /// The raw odometry pose where it was first seen.
    origin: Pose2,
    /// The farthest the body has been from `origin` while it survived —
    /// a chord, so turning on the spot counts for nothing.
    travel: f32,
}

impl Mapper {
    /// A mapper resumed on a saved map, which does not know where it is:
    /// the pose is lost from the first window and the search decides,
    /// under the uniqueness and agreement gates, instead of the saved
    /// `tracked` pose being taken on trust. This is what a robot switched
    /// on in a house it has mapped before needs — the saved pose is only
    /// right if it was switched on where it was switched off.
    pub fn resumed_lost(cfg: MapperConfig, slam: Slam) -> Self {
        let mut cfg = cfg;
        // The two settings that make a kidnap recoverable are wrong at
        // boot. `hard_lost_search_radius_m` keeps the search near where the
        // duck thinks it is — but on a resumed map that is the pose the
        // previous run ended at, which is exactly what must not be trusted;
        // search the whole map instead. And giving up means falling back
        // to that same pose, so do not: keep searching and let the client
        // decide what to do while the pose is unconfirmed.
        let after_boot = (cfg.hard_lost_search_radius_m, cfg.lost_give_up_windows);
        cfg.hard_lost_search_radius_m = 0.0;
        cfg.lost_give_up_windows = 0;
        let mut mapper = Self::new(cfg, slam);
        mapper.after_boot = Some(after_boot);
        mapper.lost = true;
        mapper.hard_lost = true;
        mapper.resumed_from_session = true;
        mapper.booting = true;
        if boot_mcl()
            && let Some(grid) = mapper.slam.render()
        {
            let mcfg = crate::mcl::MclConfig {
                n_particles: boot_mcl_particles(),
                // The mapper's own wall definition, so the grid's one-slot
                // distance-field cache is shared with the watchdog.
                wall_threshold_fp: 150,
                see_through_fp: 300,
                unknown_residual_m: Some(0.20),
                locked_max_residual_m: boot_mcl_lock_residual_m(),
                ..crate::mcl::MclConfig::default()
            };
            let mut mcl = crate::mcl::Localizer::new(mcfg, 0xC0FF_EE);
            // A fifth of the cloud around where the session ended: a duck
            // switched on where it was switched off locks in a stand.
            mcl.seed_mixed(&[mapper.slam.tracked()], 0.2, &grid, 0.3, 0.3);
            mapper.boot = Some(BootSearch {
                mcl,
                grid,
                last_odom: None,
                origin_odom: None,
                pending: (0.0, 0.0, 0.0),
                yaw_swept: 0.0,
                posture_ok: true,
                frames: 0,
                proposed: None,
                judged: 0,
                refused: 0,
                unjudged: 0,
                judged_window: None,
            });
        }
        mapper
    }

    pub fn new(cfg: MapperConfig, slam: Slam) -> Self {
        let accumulator = cfg.accumulator;
        // The rolling window of `continuous` must not vote. The vote asks
        // several frames to agree on the same world cell, which is what
        // makes a *still* window sharp — and what empties a moving one: the
        // endpoints walk with the body, nothing agrees, and the composite
        // comes out with no beams at all (measured 2026-09-13, the first
        // attempt corrected the pose exactly zero times in seven minutes).
        let rolling = AccumulatorConfig {
            min_window_frames: usize::MAX,
            ..accumulator
        };
        let mut mapper = Self {
            acc: WindowAccumulator::new(accumulator),
            cfg,
            slam,
            odom_window: Vec::new(),
            was_still: false,
            after_boot: None,
            window_opened: None,
            windows: 0,
            lost: false,
            suspect: 0,
            pending_reloc: None,
            last_search: None,
            hypotheses: Vec::new(),
            after_fall: false,
            resumed_from_session: false,
            booting: false,
            lost_windows: 0,
            hard_lost: false,
            soft_seed: None,
            unjudged: 0,
            seed_agreed: 0,
            stand_grid: None,
            boot: None,
            roll: WindowAccumulator::new(rolling),
            roll_at: 0.0,
            last_window: None,
        };
        // A resumed session cannot vouch for its pose: the robot may have
        // been moved, or even booted in another room, while the daemon was
        // down. Suspect until a window confirms — an unmoved robot
        // confirms in one. (A fresh mapper has nothing to confirm against
        // and starts trusting, as it must.)
        if mapper.slam.n_submaps() > 0 {
            mapper.arm_suspicion();
        }
        mapper
    }

    /// Soft suspicion: keep tracking odometry, ink nothing, and let the
    /// windows either confirm the carried pose, refute it (→ search), or
    /// exhaust the give-up budget.
    fn arm_suspicion(&mut self) {
        self.lost = true;
        self.suspect = 0;
        self.unjudged = 0;
        self.seed_agreed = 0;
        let here = self.slam.tracked();
        self.soft_seed = Some((here, here));
        self.pending_reloc = None;
        self.last_search = None;
        self.hard_lost = false;
    }

    pub fn slam(&self) -> &Slam {
        &self.slam
    }
    pub fn slam_mut(&mut self) -> &mut Slam {
        &mut self.slam
    }
    pub fn windows(&self) -> u32 {
        self.windows
    }
    pub fn still(&self) -> bool {
        self.was_still
    }
    /// False while lost (kidnapped, or a resumed session the scans refute).
    pub fn tracking(&self) -> bool {
        !self.lost
    }
    /// Frames sitting in the open still window.
    pub fn window_frames(&self) -> usize {
        self.acc.len()
    }
    /// Is `pose` the only place this composite fits? The brute-force
    /// search is run on the composite; `own` is the residual of the basin
    /// at `pose`, the rival that of the best basin elsewhere (further than
    /// 0.40 m or 35°), and the answer is own ≤ `uniqueness_ratio` · rival —
    /// the test the search applies to itself, with the same metric on both
    /// sides. `None` when the composite cannot be judged (too few observed
    /// beams, no search result, no basin at `pose`).
    ///
    /// Gates every confirmation at boot as well as the particle filter's
    /// lock. Twelve wake-ups on the twin (2026-09-14) confirmed six poses
    /// in nine that were wrong — the flat's mirror image after a minute of
    /// turning, or the saved pose itself six seconds after being switched
    /// on across the flat — because a candidate needs only a second window
    /// to *agree* with it, and in a house of repeated rectangles an alias
    /// agrees with itself as readily as the truth does.
    fn unique_at(&self, grid: &mut OccupancyGrid, composite: &Scan, pose: Pose2) -> Option<bool> {
        let wd = self.cfg.watchdog;
        let judged = self
            .judge_untrusted(grid, composite, pose, wd.clamp_m, wd.wall_threshold_fp, wd.observed_fp)
            .n_observed
            >= wd.min_observed_beams;
        if !judged {
            return None;
        }
        let probe = composite.decimated(self.cfg.relocalize_max_beams);
        let r = relocalize_against_grid(grid, &probe, &self.cfg.relocalize)?;
        let near = |bp: &Pose2| {
            (bp.0 - pose.0).hypot(bp.1 - pose.1) <= 0.40 && wrap_pi(bp.2 - pose.2).abs() <= 0.60
        };
        let own = r
            .basins
            .iter()
            .filter(|(bp, _)| near(bp))
            .map(|(_, res)| *res)
            .fold(f32::INFINITY, f32::min);
        if !own.is_finite() {
            return None;
        }
        let rival = r
            .basins
            .iter()
            .filter(|(bp, _)| !near(bp))
            .map(|(_, res)| *res)
            .fold(f32::INFINITY, f32::min);
        Some(own <= self.cfg.relocalize.uniqueness_ratio * rival)
    }

    /// The boot search, if one is running: locked now, locks judged,
    /// refused, and left unjudged. `None` when there is no search.
    pub fn boot_search(&self) -> Option<(bool, u32, u32, u32)> {
        self.boot
            .as_ref()
            .map(|b| (b.mcl.is_locked(), b.judged, b.refused, b.unjudged))
    }

    /// The pose and composite of the last closed window (see field doc).
    pub fn last_window(&self) -> Option<&(Pose2, Scan)> {
        self.last_window.as_ref()
    }

    /// One control-loop tick. `t_s` is seconds on any monotonic timebase —
    /// the host's uptime, a recording's timestamps — as long as one mapper
    /// sees only one. Notes are appended, not replaced.
    pub fn observe(&mut self, t_s: f32, sample: MapperSample, notes: &mut Vec<Note>) {
        self.slam.observe_odom(sample.odom);
        if let Some(b) = self.boot.as_mut() {
            if let Some(prev) = b.last_odom {
                let d = between(prev, sample.odom);
                b.pending = compose(b.pending, d);
                b.yaw_swept += d.2.abs();
            } else {
                b.origin_odom = Some(sample.odom);
            }
            b.last_odom = Some(sample.odom);
            b.posture_ok = !sample.sitting && !sample.fallen;
        }
        self.correct_while_walking(t_s, notes);
        self.odom_window
            .push((t_s, sample.odom.0, sample.odom.1, sample.odom.2));
        let horizon = self.cfg.still.window_s;
        self.odom_window.retain(|&(at, ..)| t_s - at <= horizon);

        let still = !sample.moving
            && !sample.sitting
            && !sample.fallen
            && self.odom_window.first().is_some_and(|&(_, fx, fy, fyaw)| {
                let dx = sample.odom.0 - fx;
                let dy = sample.odom.1 - fy;
                let dyaw = wrap_pi(sample.odom.2 - fyaw);
                (dx * dx + dy * dy).sqrt() < self.cfg.still.max_dxy_m
                    && dyaw.abs() < self.cfg.still.max_dyaw_rad
            });

        // A window flushes when the stand ends — or after
        // `window_flush_after_s` while it continues, so the map builds
        // while you watch instead of waiting for the next step. The window
        // closes here whatever comes of it: leaving it armed after a
        // fruitless finish would flush every subsequent frame alone.
        let stand_ended = self.was_still && !still;
        let ripe = self
            .window_opened
            .is_some_and(|t0| t_s - t0 >= self.cfg.window_flush_after_s);
        if (stand_ended || ripe) && !self.acc.is_empty() {
            self.window_opened = None;
            if let Some((pose, composite)) = self.acc.finish() {
                self.absorb_window(pose, &composite, t_s, notes);
            }
        }
        if stand_ended {
            self.window_opened = None;
        }
        if still && !self.was_still {
            // A stand begins: freeze the map the watchdog will judge this
            // stand's windows against.
            self.stand_grid = self.slam.render();
        }
        self.was_still = still;

        // A sit or a fall invalidates the pose: the robot cannot feel a
        // carry, and a fall can drag and spin it. Arm the lost machinery
        // with "I was not moved" as the seed — cheap to confirm when
        // true, refused when false. AFTER the window flush above, on
        // purpose: the window that closes at the sit describes the world
        // BEFORE the carry, and letting it count as the seed's first
        // agreement handed a real kidnap half its confirmation for free
        // (measured — field test five's second carry).
        if sample.fallen {
            self.after_fall = true;
        }
        if (sample.sitting || sample.fallen) && !self.lost {
            self.arm_suspicion();
            notes.push(if sample.fallen {
                Note::SuspectAfterFall
            } else {
                Note::SuspectAfterSit
            });
        }

        // While lost the tracked pose is a guess; freezing submaps or
        // running closures on it would launder the guess into the graph.
        if !self.lost {
            let loops_before = self.slam.n_loops();
            let before = self.slam.tracked();
            if !self.frozen() {
                self.slam.tick(t_s);
            }
            if self.slam.n_loops() > loops_before {
                let after = self.slam.tracked();
                notes.push(Note::LoopClosed {
                    n_loops: self.slam.n_loops(),
                    dx: after.0 - before.0,
                    dy: after.1 - before.1,
                    dyaw: wrap_pi(after.2 - before.2),
                });
                // The closure moved every anchor; a snapshot in the old
                // frame would mis-judge the stand's remaining windows by
                // exactly the correction — and the frames already pushed
                // carry PRE-correction poses: a composite mixing both
                // frames would ink smeared and displaced. Drop them; the
                // stand refills the window in a couple of seconds.
                if self.was_still {
                    self.stand_grid = self.slam.render();
                }
                if !self.acc.is_empty() {
                    self.acc = WindowAccumulator::new(self.cfg.accumulator);
                    self.window_opened = None;
                }
            }
        }
    }

    /// One reprojected depth frame, already in the body frame. Returns
    /// true when the frame was kept (accumulated or inked).
    pub fn frame(&mut self, t_s: f32, scan: Scan) -> bool {
        if self.lost
            && let Some(b) = self.boot.as_mut()
            && b.posture_ok
        {
            let d = std::mem::take(&mut b.pending);
            b.mcl.predict(d.0, d.1, d.2);
            b.mcl.update(&mut b.grid, &scan);
            b.frames += 1;
            let travelled = match (b.origin_odom, b.last_odom) {
                (Some(o), Some(l)) => (l.0 - o.0).hypot(l.1 - o.1),
                _ => 0.0,
            };
            if b.mcl.is_locked()
                && b.yaw_swept >= boot_mcl_yaw_rad()
                && travelled >= boot_mcl_travel_m()
                && b.proposed.is_none()
                && self.pending_reloc.is_none()
                && let Some((win_pose, composite)) = self.last_window.as_ref()
                && b.judged_window != Some(*win_pose)
            {
                // The lock is the filter's word; the map's own matcher gets
                // the last one. On the replay bench (2026-09-14) the filter
                // locked, with its yaw 58° off, on a pose the still window
                // then confirmed (residual 0.036) — an alias the cloud
                // cannot see because it only ever scores where it is.
                //
                // So the lock is judged the way the brute force judges
                // itself, and with the same numbers: the search runs on the
                // last composite, the basin at the lock is `own`, the best
                // basin elsewhere is the rival, and the lock is proposed
                // only if own beats rival by `uniqueness_ratio`. Same
                // metric on both sides — a first cut scored the lock with
                // `score_pose` (observed endpoints only, lenient) against
                // basins scored by `score_offsets` (every beam, strict),
                // which is not the brute force's test at all.
                //
                // The composite was measured at the window's pose, and the
                // body has walked since: the lock is carried back to the
                // window before it is judged, and proposed as a candidate
                // *at* that window so `check_candidate` carries it forward
                // exactly as it does the brute force's own.
                b.judged_window = Some(*win_pose);
                let now = self.slam.tracked();
                let at_window = compose(b.mcl.dominant_cluster_mean(), between(now, *win_pose));
                // The search is taken out of `self` while `self` judges its
                // grid, and put back the same way.
                let mut b = self
                    .boot
                    .take()
                    .expect("the boot search was here a moment ago");
                let verdict = self.unique_at(&mut b.grid, composite, at_window);
                self.boot = Some(b);
                let b = self.boot.as_mut().expect("just put back");
                let Some(unique) = verdict else {
                    b.unjudged += 1;
                    return self.frame_rest(t_s, scan);
                };
                // The rivals, for the re-seed on refusal.
                let probe = composite.decimated(self.cfg.relocalize_max_beams);
                let far: Vec<Pose2> =
                    relocalize_against_grid(&mut b.grid, &probe, &self.cfg.relocalize)
                        .map(|r| {
                            r.basins
                                .iter()
                                .filter(|(bp, _)| {
                                    (bp.0 - at_window.0).hypot(bp.1 - at_window.1) > 0.40
                                        || wrap_pi(bp.2 - at_window.2).abs() > 0.60
                                })
                                .map(|(bp, _)| *bp)
                                .collect()
                        })
                        .unwrap_or_default();
                b.judged += 1;
                if unique {
                    // Proposed, not believed: the next still window judges
                    // it as it judges any candidate, and `resume_at` is the
                    // only way in.
                    b.proposed = Some(at_window);
                    self.pending_reloc = Some((at_window, *win_pose));
                } else {
                    b.refused += 1;
                    // Not unique — or no basin at the lock at all. The cloud
                    // is sent where the search found the rivals, not back
                    // to the pose just refused (half the cloud re-seeded on
                    // the alias re-locked there in 25 frames, and the gate
                    // saw the same window again), and the motion gates start
                    // over so a new proposal needs new evidence.
                    if far.is_empty() {
                        b.mcl.seed_uniform(&b.grid);
                    } else {
                        b.mcl.seed_mixed(&far, 0.5, &b.grid, 0.3, 0.3);
                    }
                    b.origin_odom = b.last_odom;
                    b.yaw_swept = 0.0;
                }
            }
        }
        self.frame_rest(t_s, scan)
    }

    /// The part of [`Mapper::frame`] after the boot search: ink or window.
    fn frame_rest(&mut self, t_s: f32, scan: Scan) -> bool {
        if self.cfg.continuous && !self.lost {
            if self.cfg.continuous_correct_s > 0.0 {
                self.roll.push(self.slam.tracked(), scan.clone());
            }
            if !self.frozen() {
                self.slam.integrate(self.slam.tracked(), &scan);
            }
            return true;
        }
        // Continuous mode falls through here while LOST: recovery is the
        // still-window machinery in both modes, or a continuous mapper
        // that sat once would sweep its head forever with no path back.
        if !self.was_still {
            return false;
        }
        if self.acc.is_empty() {
            self.window_opened = Some(t_s);
        }
        self.acc.push(self.slam.tracked(), scan);
        true
    }

    fn absorb_window(&mut self, pose: Pose2, composite: &Scan, t_s: f32, notes: &mut Vec<Note>) {
        self.last_window = Some((pose, composite.clone()));
        let beams = composite.n_valid();
        if beams < self.cfg.min_window_beams {
            notes.push(Note::WindowDiscarded { beams });
            return;
        }

        if self.lost {
            let Some(mut grid) = self.slam.render() else {
                return;
            };
            let now = self.slam.tracked();

            // The soft seed first: "I was not moved" outranks any search
            // candidate while it stands unrefuted. It must agree with TWO
            // windows before tracking resumes on it: a single static wedge
            // falsely confirmed a real kidnap in the field (residual 0.010
            // at the old pose — the new spot's wall matched the old spot's
            // wall), and the head sweep only decorrelates the second
            // window from the first if we wait for it.
            if let Some((cand, then)) = self.soft_seed.take() {
                match self.check_candidate(&mut grid, composite, cand, then, now) {
                    (implied, Verdict::Confirmed(pose, resid)) => {
                        self.seed_agreed += 1;
                        // At boot the seed is the pose the session ended
                        // at, and a duck switched on in another room that
                        // looks the same agrees with it in one window: the
                        // agreement has to be unique before it is believed.
                        let unique = !self.resumed_from_session
                            || self.unique_at(&mut grid, composite, pose) == Some(true);
                        let unique = unique && (!(self.resumed_from_session || self.after_fall) || {
                            let probe = composite.decimated(self.cfg.relocalize_max_beams);
                            match crate::relocalize::valley_at(&mut grid, &probe, pose, &self.cfg.relocalize) {
                                Some(along) => {
                                    notes.push(Note::RelocalizeAmbiguous { pose, along });
                                    false
                                }
                                None => true,
                            }
                        });
                        if self.seed_agreed >= 2 && unique {
                            self.resume_at(pose, composite, t_s);
                            notes.push(Note::Relocalized {
                                pose,
                                mean_residual_m: resid,
                            });
                            return;
                        }
                        self.soft_seed = Some((implied, now));
                        notes.push(Note::RelocalizeCandidate {
                            pose,
                            mean_residual_m: resid,
                        });
                        return;
                    }
                    (_, Verdict::Refuted) => {
                        // Evidence of displacement: suspicion hardens, the
                        // give-up escape is off the table.
                        self.seed_agreed = 0;
                    }
                    (implied, Verdict::Ambiguous) => {
                        // Keep the hypothesis alive and keep looking, but
                        // spend none of the give-up budget on a window the
                        // map DID judge.
                        self.seed_agreed = 0;
                        self.soft_seed = Some((implied, now));
                    }
                    (implied, Verdict::Unjudgeable) => {
                        self.seed_agreed = 0;
                        self.unjudged += 1;
                        if self.unjudged >= self.cfg.suspect_give_up_windows && !self.after_fall {
                            self.resume_at(implied, composite, t_s);
                            notes.push(Note::ResumedUnverified { pose: implied });
                            return;
                        }
                        self.soft_seed = Some((implied, now));
                    }
                }
            }

            // Then the search's last candidate, if one is pending. (Already
            // two independent windows: one nominated it, this one judges.)
            // (A candidate here was nominated by a search that tested its
            // own uniqueness on the window that found it, or by the boot
            // search through `unique_at`. Testing it again on the window
            // that confirms it refused the truth as often as the alias on
            // the bench — 2026-09-14, both recordings never came home or
            // came home at 434 s — so the second window's job stays what
            // it was: to agree.)
            if let Some((cand, then)) = self.pending_reloc.take()
                && let (_, Verdict::Confirmed(pose, resid)) =
                    self.check_candidate(&mut grid, composite, cand, then, now)
            {
                // At boot, agreement from the spot it was nominated on is
                // not confirmation: keep it pending — nominated where it
                // was, so the chord keeps growing — until the body has
                // moved. A refuted or ambiguous candidate is dropped as
                // before; the searches below leave a pending one alone.
                let chord = (now.0 - then.0).hypot(now.1 - then.1);
                let probe = composite.decimated(self.cfg.relocalize_max_beams);
                // Only on a map from an earlier run that has never known
                // where it is: there nothing near tells the valley's poses
                // apart. A mapping duck lost for a moment relocalizes
                // beside where it was, and refusing that left it resuming
                // unverified — casa_libera's walls 5.6 → 21 cm off on the
                // replay, casa_arredata's map torn.
                if (self.resumed_from_session || self.after_fall)
                    && let Some(along) = crate::relocalize::valley_at(&mut grid, &probe, pose, &self.cfg.relocalize)
                {
                    // Agreement along a valley is agreement with every
                    // pose on it: dropped, and the search goes on until a
                    // window sees what pins the pose down.
                    notes.push(Note::RelocalizeAmbiguous { pose, along });
                } else if self.booting && chord < confirm_travel_m() {
                    self.pending_reloc = Some((cand, then));
                    notes.push(Note::RelocalizeCandidate {
                        pose,
                        mean_residual_m: resid,
                    });
                } else {
                    if let Some(b) = self.boot.as_ref()
                        && b.proposed.is_some()
                    {
                        notes.push(Note::RelocalizeCandidate {
                            pose: cand,
                            mean_residual_m: resid,
                        });
                    }
                    self.resume_at(pose, composite, t_s);
                    notes.push(Note::Relocalized {
                        pose,
                        mean_residual_m: resid,
                    });
                    return;
                }
            }

            if let Some(b) = self.boot.as_mut() {
                // Judged — confirmed or not — the filter may propose again.
                b.proposed = None;
            }
            // Hard-lost for too long: odometry has carried the pose all
            // along; resume there rather than keep the map cold.
            if self.hard_lost && self.cfg.lost_give_up_windows > 0 && !self.after_fall {
                self.lost_windows += 1;
                if self.lost_windows > self.cfg.lost_give_up_windows {
                    let here = self.slam.tracked();
                    self.resume_at(here, composite, t_s);
                    notes.push(Note::ResumedUnverified { pose: here });
                    return;
                }
            }
            // No confirmation: search this window for a fresh candidate.
            let probe = composite.decimated(self.cfg.relocalize_max_beams);
            let near_enough = |r: &crate::relocalize::RelocalizeResult| {
                !self.hard_lost
                    || self.cfg.hard_lost_search_radius_m <= 0.0
                    || (r.pose.0 - now.0).hypot(r.pose.1 - now.1)
                        <= self.cfg.hard_lost_search_radius_m
            };
            // Boot on a saved map: keep every plausible basin and let the
            // viewpoints decide. `multi` off falls back to the single-best
            // agreement below, which is what a kidnap in place wants.
            if multi_hypothesis() && self.hard_lost && self.resumed_from_session {
                if let Some(r) = relocalize_against_grid(&mut grid, &probe, &self.cfg.relocalize) {
                    // Carry what we had to now, then match this window's
                    // basins against it.
                    let odom_now = self
                        .odom_window
                        .last()
                        .map(|&(_, x, y, yaw)| (x, y, yaw))
                        .unwrap_or(now);
                    let carried: Vec<Hypothesis> = self
                        .hypotheses
                        .iter()
                        .map(|h| {
                            let moved = between(h.seen, odom_now);
                            Hypothesis {
                                pose: compose(h.pose, moved),
                                seen: odom_now,
                                hits: h.hits,
                                origin: h.origin,
                                travel: h
                                    .travel
                                    .max((odom_now.0 - h.origin.0).hypot(odom_now.1 - h.origin.1)),
                            }
                        })
                        .collect();
                    let agrees = |a: Pose2, b: Pose2| {
                        (a.0 - b.0).hypot(a.1 - b.1) <= self.cfg.relocalize_agree_m
                            && wrap_pi(a.2 - b.2).abs() <= self.cfg.relocalize_agree_rad
                    };
                    let mut next: Vec<Hypothesis> = Vec::new();
                    for (pose, _resid) in r.basins.iter() {
                        let seen_before = carried
                            .iter()
                            .filter(|h| agrees(h.pose, *pose))
                            .max_by_key(|h| h.hits);
                        next.push(Hypothesis {
                            pose: *pose,
                            seen: odom_now,
                            hits: seen_before.map_or(1, |h| h.hits + 1),
                            origin: seen_before.map_or(odom_now, |h| h.origin),
                            travel: seen_before.map_or(0.0, |h| h.travel),
                        });
                    }
                    // A hypothesis the search did not propose again is not
                    // thereby wrong: the search returns a handful of basins
                    // out of many, and the true one is not always among
                    // them. So SCORE each carried hypothesis against this
                    // window where odometry says it would be. A place that
                    // keeps agreeing after the body has walked is a place;
                    // an alias stops agreeing as soon as the geometry moves.
                    for h in carried.iter() {
                        if next.iter().any(|n| agrees(n.pose, h.pose)) {
                            continue;
                        }
                        let a = self.judge_untrusted(
                            &mut grid,
                            &probe,
                            h.pose,
                            self.cfg.relocalize.clamp_m,
                            self.cfg.relocalize.wall_threshold_fp,
                            self.cfg.watchdog.observed_fp,
                        );
                        let judged = a.n_observed >= self.cfg.relocalize.min_beams_used;
                        if judged && a.mean_residual_m <= self.cfg.relocalize.max_mean_residual_m {
                            next.push(Hypothesis {
                                hits: h.hits + 1,
                                ..*h
                            });
                        } else if h.hits > 1 {
                            next.push(Hypothesis {
                                hits: h.hits - 1,
                                ..*h
                            });
                        }
                    }
                    next.sort_by_key(|h| std::cmp::Reverse(h.hits));
                    next.truncate(crate::relocalize::MAX_BASINS);
                    self.hypotheses = next;
                    if std::env::var_os("RELOC_DEBUG").is_some() {
                        let top: Vec<String> = self
                            .hypotheses
                            .iter()
                            .take(4)
                            .map(|h| {
                                format!(
                                    "({:.2},{:.2},{:.0}°) x{} {:.1}m",
                                    h.pose.0,
                                    h.pose.1,
                                    h.pose.2.to_degrees(),
                                    h.hits,
                                    h.travel
                                )
                            })
                            .collect();
                        eprintln!("    hyps: {}", top.join("  "));
                    }
                    // One hypothesis clearly ahead of the rest, seen often
                    // enough AND from far enough apart: two windows a
                    // second apart are the same window twice, and every
                    // basin agrees with itself. It is walking between them
                    // that kills an alias.
                    if self.pending_reloc.is_none()
                        && let Some(h) = self.hypotheses.first().copied()
                        && h.hits >= self.cfg.relocalize_agree_windows
                        && h.travel >= hypothesis_travel_m()
                        && self.hypotheses.get(1).is_none_or(|o| h.hits >= o.hits + hypothesis_lead())
                    {
                        let pose = h.pose;
                        self.pending_reloc = Some((pose, now));
                        notes.push(Note::RelocalizeCandidate {
                            pose,
                            mean_residual_m: r.mean_residual_m,
                        });
                    }
                }
                return;
            }
            match relocalize_against_grid(&mut grid, &probe, &self.cfg.relocalize) {
                Some(r) if r.accepted && near_enough(&r) => {
                    // Agreement with the previous search, carried to now.
                    let agreed = match self.last_search.take() {
                        Some((prev, then, n)) => {
                            let carried = compose(prev, between(then, now));
                            let d = (carried.0 - r.pose.0).hypot(carried.1 - r.pose.1);
                            let dyaw = wrap_pi(carried.2 - r.pose.2).abs();
                            if d <= self.cfg.relocalize_agree_m
                                && dyaw <= self.cfg.relocalize_agree_rad
                            {
                                n + 1
                            } else {
                                1
                            }
                        }
                        None => 1,
                    };
                    self.last_search = Some((r.pose, now, agreed));
                    if agreed >= self.cfg.relocalize_agree_windows && self.pending_reloc.is_none() {
                        self.pending_reloc = Some((r.pose, now));
                    }
                    notes.push(Note::RelocalizeCandidate {
                        pose: r.pose,
                        mean_residual_m: r.mean_residual_m,
                    });
                }
                Some(r) => notes.push(Note::RelocalizeRejected {
                    best_pose: r.pose,
                    mean_residual_m: r.mean_residual_m,
                }),
                None => {}
            }
            return;
        }

        // The watchdog: score the window against the map before believing
        // it. A window the map can judge but contradicts must not ink — it
        // would paint the kidnapper's room over the real one.
        let wd = self.cfg.watchdog;
        let mut agreement = (0.0_f32, 0u32, beams as u32);
        if let Some(grid) = self.stand_grid.as_mut() {
            let a = score_pose(
                grid,
                composite,
                pose,
                wd.clamp_m,
                wd.wall_threshold_fp,
                wd.observed_fp,
            );
            agreement = (a.mean_residual_m, a.n_observed, a.n_beams);
            // A contradiction carried by the short beams alone is a low
            // thing close to the body (a bed, a box), not a lost pose:
            // quarantined (not inked) but not lost. When the long beams
            // can be judged, they decide.
            let long_says_lost = a.n_long < LOW_THING_MIN_LONG || a.long_residual_m > wd.max_mean_residual_m;
            if a.n_observed >= wd.min_observed_beams
                && a.n_observed as f32 >= wd.min_observed_fraction * a.n_beams as f32
                && a.mean_residual_m > wd.max_mean_residual_m
                && long_says_lost
            {
                // Contradicting window: never ink it. One is a suspect
                // (a lean, a passer-by, fresh phantom ink); a run of them
                // is a kidnap.
                self.suspect += 1;
                if self.suspect >= wd.lost_after_windows {
                    self.lost = true;
                    self.suspect = 0;
                    self.pending_reloc = None;
                    self.last_search = None;
                    self.lost_windows = 0;
                    self.hard_lost = true;
                    notes.push(Note::LostTracking {
                        mean_residual_m: a.mean_residual_m,
                        n_observed: a.n_observed,
                    });
                } else {
                    notes.push(Note::WindowQuarantined {
                        mean_residual_m: a.mean_residual_m,
                        n_observed: a.n_observed,
                    });
                }
                return;
            }
            self.suspect = 0;
        }
        let mut pose = pose;
        if self.cfg.tracking.enabled
            && let Some(grid) = self.stand_grid.as_mut()
            && let Some((delta, before, after, n_used)) =
                tracking_correction(grid, composite, pose, &self.cfg.tracking)
        {
            pose = compose(pose, delta);
            let tracked = self.slam.tracked();
            self.slam.set_tracked(compose(tracked, delta));
            let moved = compose(tracked, delta);
            if self.cfg.tracking.cut_on_correction_m > 0.0
                && (moved.0 - tracked.0).hypot(moved.1 - tracked.1)
                    >= self.cfg.tracking.cut_on_correction_m
            {
                self.slam.request_submap_switch();
            }
            notes.push(Note::TrackingCorrected {
                dx: moved.0 - tracked.0,
                dy: moved.1 - tracked.1,
                dyaw: wrap_pi(moved.2 - tracked.2),
                residual_before_m: before,
                residual_after_m: after,
                n_beams_used: n_used,
            });
        }
        self.ink(pose, composite);
        notes.push(Note::WindowIntegrated {
            beams,
            windows: self.windows,
            mean_residual_m: agreement.0,
            n_observed: agreement.1,
            n_beams: agreement.2,
        });
    }

    /// The tracking correction for `continuous`, where there are no still
    /// windows to carry it: every [`MapperConfig::continuous_correct_s`],
    /// the frames since the last one are composed — each at the pose it was
    /// taken at, which is what makes a moving composite legible — matched
    /// against the map, and the tracked pose moved onto it if the match
    /// clears the same bars a stop-and-scan window must clear.
    ///
    /// The grid it matches against is the one rendered here; rendering is
    /// the expensive part (80–100 ms on the duck), so it happens once per
    /// correction and not per frame.
    fn correct_while_walking(&mut self, t_s: f32, notes: &mut Vec<Note>) {
        if !self.cfg.continuous || self.lost || self.cfg.continuous_correct_s <= 0.0 {
            return;
        }
        if t_s - self.roll_at < self.cfg.continuous_correct_s {
            return;
        }
        self.roll_at = t_s;
        let Some((_, composite)) = self.roll.finish() else {
            return;
        };
        let beams = composite.n_valid();
        if beams < self.cfg.min_window_beams {
            return;
        }
        self.stand_grid = self.slam.render();
        let pose = self.slam.tracked();
        if !self.cfg.tracking.enabled {
            return;
        }
        let Some(grid) = self.stand_grid.as_mut() else {
            return;
        };
        let Some((delta, before, after, n_used)) =
            tracking_correction(grid, &composite, pose, &self.cfg.tracking)
        else {
            return;
        };
        let moved = compose(pose, delta);
        self.slam.set_tracked(moved);
        notes.push(Note::TrackingCorrected {
            dx: moved.0 - pose.0,
            dy: moved.1 - pose.1,
            dyaw: wrap_pi(moved.2 - pose.2),
            residual_before_m: before,
            residual_after_m: after,
            n_beams_used: n_used,
        });
    }

    /// Tracking resumes at `pose`; the window that earned it inks there.
    fn resume_at(&mut self, pose: Pose2, composite: &Scan, t_s: f32) {
        self.slam.set_tracked(pose);
        // Let the submap manager see the jump BEFORE inking: after a
        // cross-room carry the current submap's grid is still anchored at
        // the pre-carry pose, and a composite integrated there is silently
        // clipped to nothing — the travel rule opens (or re-anchors to) a
        // submap that actually covers where the robot now stands.
        if !self.frozen() {
            self.slam.tick(t_s);
        }
        self.ink(pose, composite);
        self.lost = false;
        self.suspect = 0;
        self.unjudged = 0;
        self.seed_agreed = 0;
        self.soft_seed = None;
        self.pending_reloc = None;
        self.last_search = None;
        self.lost_windows = 0;
        self.hard_lost = false;
        self.after_fall = false;
        self.boot = None;
        // The boot's global, never-give-up search was for a pose nobody
        // could vouch for. Confirmed, the pose is a pose: a later "lost"
        // — two windows contradicting a frozen map at an unmapped corner
        // — is local, a metre at most in the minute it takes, and the
        // search that stayed global relocalised 77° off across the flat
        // (quacksat's lane1, 2026-09-16). Back to the normal settings.
        if self.booting
            && let Some((radius, give_up)) = self.after_boot.take()
        {
            self.cfg.hard_lost_search_radius_m = radius;
            self.cfg.lost_give_up_windows = give_up;
        }
        self.booting = false;
    }

    /// Check one candidate pose against a fresh window, carried to the
    /// candidate-implied pose by the odometry accumulated since it was
    /// proposed. (A plain method, not a closure — a four-parameter closure
    /// is exactly the construct two rustfmt versions disagree about.)
    fn check_candidate(
        &self,
        grid: &mut OccupancyGrid,
        composite: &Scan,
        cand: Pose2,
        tracked_then: Pose2,
        tracked_now: Pose2,
    ) -> (Pose2, Verdict) {
        let wd = self.cfg.watchdog;
        let implied = compose(cand, between(tracked_then, tracked_now));
        // A candidate is not a trusted pose: it is judged along the ray
        // too, or the mirror image confirms itself (see
        // `relocalize::score_pose_rays`).
        let a = self.judge_untrusted(
            grid,
            composite,
            implied,
            wd.clamp_m,
            wd.wall_threshold_fp,
            wd.observed_fp,
        );
        // Two coverage floors on purpose. CONFIRMING needs the strict one
        // (relocalize_confirm_min_fraction): through the ToF keyhole a thin
        // wedge aliases, so agreement on an eighth of the beams is a
        // coincidence. REFUTING only needs the watchdog's floor: the
        // measured kidnap judged 12 % of its beams and contradicted on all
        // of them — a verdict of "cannot judge" there let the give-up
        // escape resume tracking at the kidnapped pose.
        let strong = a.n_observed >= wd.min_observed_beams
            && a.n_observed as f32 >= self.cfg.relocalize_confirm_min_fraction * a.n_beams as f32;
        let weak = a.n_observed >= wd.min_observed_beams
            && a.n_observed as f32 >= wd.min_observed_fraction * a.n_beams as f32;
        let verdict = if strong && a.mean_residual_m <= self.cfg.relocalize_confirm_max_residual_m {
            Verdict::Confirmed(implied, a.mean_residual_m)
        } else if strong || (weak && a.mean_residual_m > wd.max_mean_residual_m) {
            Verdict::Refuted
        } else if weak {
            // Judged, and the judgement fell between agreement and
            // contradiction: evidence exists but is ambiguous. Keep the
            // candidate and keep searching — but this window must not
            // count toward the give-up escape, whose premise is that the
            // map could not judge the pose AT ALL.
            Verdict::Ambiguous
        } else {
            Verdict::Unjudgeable
        };
        (implied, verdict)
    }

    /// The judge for a pose that is NOT trusted: along the ray unless the
    /// bench says otherwise.
    fn judge_untrusted(
        &self,
        grid: &mut OccupancyGrid,
        scan: &Scan,
        pose: Pose2,
        clamp_m: f32,
        wall_threshold_fp: i16,
        observed_fp: i16,
    ) -> crate::relocalize::PoseAgreement {
        if ray_judge() {
            score_pose_rays(
                grid,
                scan,
                pose,
                clamp_m,
                wall_threshold_fp,
                observed_fp,
                self.cfg.relocalize.see_through_fp,
            )
        } else {
            score_pose(grid, scan, pose, clamp_m, wall_threshold_fp, observed_fp)
        }
    }

    /// Localize only, and localised: the map is frozen once the pose is
    /// tracked on it — while lost or searching the mapper works as in
    /// mapping, so the boot search confirms the way it always did (a
    /// frozen search aliased 110° on explmap1, 2026-09-16).
    fn frozen(&self) -> bool {
        self.cfg.frozen && !self.lost
    }

    fn ink(&mut self, pose: Pose2, composite: &Scan) {
        self.windows += 1;
        if self.frozen() {
            return;
        }
        self.slam
            .integrate_weighted(pose, composite, self.cfg.window_ink_passes.max(1));
    }
}

/// Match `composite` (body frame, taken at `pose`) against `grid` around
/// `pose`; return the BODY-FRAME delta to apply, the residual before and
/// after, and the beams used — or `None` when the match is not to be
/// trusted. See [`TrackingConfig`].
fn tracking_correction(
    grid: &mut OccupancyGrid,
    composite: &Scan,
    pose: Pose2,
    cfg: &TrackingConfig,
) -> Option<(Pose2, f32, f32, u32)> {
    let probe = composite.decimated(512);
    let sm = ScanMatchConfig {
        prior_sigma_xy: cfg.prior_sigma_xy,
        prior_sigma_yaw: cfg.prior_sigma_yaw,
        // The watchdog's wall definition, so the grid's single-slot
        // distance-field cache is shared instead of rebuilt per window.
        occ_threshold_fp: 150,
        ..ScanMatchConfig::default()
    };
    // Residual at the tracked pose itself: the bar the match must clear.
    let at_seed = match_scan(
        grid,
        &probe,
        pose,
        Some(pose),
        &ScanMatchConfig { max_iters: 0, ..sm },
    );
    let r = match_scan(grid, &probe, pose, Some(pose), &sm);
    if !r.residual_m.is_finite()
        || r.n_beams_observed < cfg.min_observed_beams
        || (r.n_beams_observed as f32) < cfg.min_observed_fraction * r.n_beams_valid as f32
        || r.n_beams_used < cfg.min_observed_beams
    {
        return None;
    }
    if at_seed.residual_m.is_finite() && r.residual_m > cfg.min_improvement * at_seed.residual_m {
        return None;
    }
    if !at_seed.residual_m.is_finite() || at_seed.residual_m < cfg.min_residual_before_m {
        return None;
    }
    // And it must land somewhere the map and the sensor actually agree.
    if cfg.max_residual_after_m > 0.0 && r.residual_m > cfg.max_residual_after_m {
        return None;
    }
    // Map-frame correction, then projected onto the constrained directions.
    let (mut dx, mut dy) = (r.pose.0 - pose.0, r.pose.1 - pose.1);
    let mut dyaw = wrap_pi(r.pose.2 - pose.2);
    let h = r.hessian;
    // Eigen-decomposition of the translational 2x2 block.
    let (a, b, c) = (h[0][0], h[0][1], h[1][1]);
    let tr = a + c;
    let det = a * c - b * b;
    let disc = ((tr * tr / 4.0) - det).max(0.0).sqrt();
    let (l_max, l_min) = (tr / 2.0 + disc, tr / 2.0 - disc);
    if l_max <= 0.0 {
        return None;
    }
    if l_min < cfg.min_conditioning * l_max {
        // Weak eigenvector: for a symmetric 2x2, (b, l_min - a) or (l_min - c, b).
        let (vx, vy) = if b.abs() > 1e-9 {
            (b, l_min - a)
        } else if a <= c {
            (1.0, 0.0)
        } else {
            (0.0, 1.0)
        };
        let n = (vx * vx + vy * vy).sqrt().max(1e-9);
        let (vx, vy) = (vx / n, vy / n);
        let along = dx * vx + dy * vy;
        dx -= along * vx;
        dy -= along * vy;
    }
    // Yaw stiffness relative to translation (per-beam lever arms are ~1 m,
    // so the units are comparable).
    if h[2][2] < cfg.min_yaw_stiffness * l_max {
        dyaw = 0.0;
    }
    if dx.hypot(dy) > cfg.max_correction_m || dyaw.abs() > cfg.max_correction_rad {
        return None;
    }
    if dx.hypot(dy) < 1e-4 && dyaw.abs() < 1e-4 {
        return None;
    }
    // Back to a body-frame delta at `pose`: pose ⊕ delta = (pose.xy + d, pose.yaw + dyaw).
    let (cy, sy) = (pose.2.cos(), pose.2.sin());
    let delta = (cy * dx + sy * dy, -sy * dx + cy * dy, dyaw);
    Some((delta, at_seed.residual_m, r.residual_m, r.n_beams_used))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::{Slam, SlamConfig};

    /// What the sensor sees standing at `pose` (the robot's TRUE pose) in
    /// a rectangular room (walls at x = ±1.5, y = ±1.1): body-frame
    /// angles, true ranges. Rectangular on purpose — a square room is
    /// 90°-symmetric and a relocalizer handed one is *entitled* to pick a
    /// rotated pose.
    /// Where the mapper paints them is its own business — that gap is what
    /// the kidnap test exercises.
    /// A wall segment inside the room: `x = c` for `y ∈ span`, or `y = c`
    /// for `x ∈ span`.
    enum Seg {
        AtX(f32, (f32, f32)),
        AtY(f32, (f32, f32)),
    }

    /// A 360° scan of the 3.0 × 2.2 m room from `pose`, with these
    /// segments standing in it.
    fn scan_of(pose: Pose2, segs: &[Seg]) -> Scan {
        let mut angles = Vec::new();
        let mut ranges = Vec::new();
        for k in 0..240 {
            let a = -std::f32::consts::PI + k as f32 * (2.0 * std::f32::consts::PI / 240.0);
            let (dx, dy) = ((pose.2 + a).cos(), (pose.2 + a).sin());
            let tx = if dx > 1e-6 {
                (1.5 - pose.0) / dx
            } else if dx < -1e-6 {
                (-1.5 - pose.0) / dx
            } else {
                f32::INFINITY
            };
            let ty = if dy > 1e-6 {
                (1.1 - pose.1) / dy
            } else if dy < -1e-6 {
                (-1.1 - pose.1) / dy
            } else {
                f32::INFINITY
            };
            let mut r = tx.min(ty);
            for seg in segs {
                match *seg {
                    Seg::AtX(c, (lo, hi)) if dx.abs() > 1e-6 => {
                        let td = (c - pose.0) / dx;
                        if td > 0.0 && (lo..=hi).contains(&(pose.1 + td * dy)) {
                            r = r.min(td);
                        }
                    }
                    Seg::AtY(c, (lo, hi)) if dy.abs() > 1e-6 => {
                        let td = (c - pose.1) / dy;
                        if td > 0.0 && (lo..=hi).contains(&(pose.0 + td * dx)) {
                            r = r.min(td);
                        }
                    }
                    _ => {}
                }
            }
            if r.is_finite() && r < 1.9 {
                angles.push(a);
                ranges.push(r);
            }
        }
        Scan::from_polar(&angles, &ranges, (0.0, 0.0), 1e-3)
    }

    /// The room with a half-divider at x = 0.3, y ∈ [-1.1, 0], which
    /// breaks the rectangle's remaining 180° symmetry.
    fn room_scan(pose: Pose2) -> Scan {
        scan_of(pose, &[Seg::AtX(0.3, (-1.1, 0.0))])
    }

    /// The same room with a shelf along the north wall's east half. The
    /// half-divider alone is a few beams out of 240: the mirror image
    /// of the room scores within a hair of the truth on it, and a test
    /// about telling the two apart needs a room in which walking tells
    /// them apart.
    fn lumpy_room_scan(pose: Pose2) -> Scan {
        scan_of(
            pose,
            &[Seg::AtX(0.3, (-1.1, 0.0)), Seg::AtY(0.6, (0.6, 1.5))],
        )
    }

    fn drive(
        mapper: &mut Mapper,
        t0: f32,
        pose: Pose2,
        seconds: f32,
        notes: &mut Vec<Note>,
    ) -> f32 {
        drive_in(mapper, t0, pose, seconds, notes, room_scan)
    }

    /// Walk from `from` to `to` over `seconds` — odometry interpolated,
    /// `moving` set, frames still arriving — so the still window before
    /// the walk closes cleanly instead of straddling a teleport.
    fn walk_in(
        mapper: &mut Mapper,
        t0: f32,
        from: Pose2,
        to: Pose2,
        seconds: f32,
        notes: &mut Vec<Note>,
        scan: fn(Pose2) -> Scan,
    ) -> f32 {
        let mut t = t0;
        let end = t0 + seconds;
        let mut next_frame = t0;
        while t < end {
            let f = ((t - t0) / seconds).clamp(0.0, 1.0);
            let pose = (
                from.0 + f * (to.0 - from.0),
                from.1 + f * (to.1 - from.1),
                wrap_pi(from.2 + f * wrap_pi(to.2 - from.2)),
            );
            mapper.observe(
                t,
                MapperSample {
                    odom: pose,
                    moving: true,
                    sitting: false,
                    fallen: false,
                },
                notes,
            );
            if t >= next_frame {
                mapper.frame(t, scan(pose));
                next_frame += 1.0 / 15.0;
            }
            t += 0.02;
        }
        t
    }

    fn drive_in(
        mapper: &mut Mapper,
        t0: f32,
        pose: Pose2,
        seconds: f32,
        notes: &mut Vec<Note>,
        scan: fn(Pose2) -> Scan,
    ) -> f32 {
        // 50 Hz odometry, 15 Hz frames, robot standing at `pose`.
        let mut t = t0;
        let end = t0 + seconds;
        let mut next_frame = t0;
        while t < end {
            mapper.observe(
                t,
                MapperSample {
                    odom: pose,
                    moving: false,
                    sitting: false,
                    fallen: false,
                },
                notes,
            );
            if t >= next_frame {
                mapper.frame(t, scan(pose));
                next_frame += 1.0 / 15.0;
            }
            t += 0.02;
        }
        t
    }

    /// The whole point of the machine: a kidnap (scans that contradict the
    /// map) flips tracking to lost, nothing inks, and a good window
    /// relocalizes back to the true pose.
    #[test]
    fn a_kidnapped_mapper_stops_relocates_and_resumes() {
        let mut mapper = Mapper::new(MapperConfig::default(), Slam::new(SlamConfig::default()));
        let mut notes = Vec::new();

        // Build a map from two stands — a second viewpoint fills the
        // divider's occlusion shadow, exactly like a real mapping lap
        // does; a single-viewpoint map penalizes the true post-kidnap
        // pose for beams into the territory only the kidnapper's side of
        // the room can see.
        let mut t = drive(&mut mapper, 0.0, (0.0, 0.0, 0.0), 8.0, &mut notes);
        t = drive(&mut mapper, t, (0.9, -0.6, -1.2), 8.0, &mut notes);
        assert!(mapper.windows() >= 2, "the stands must have inked windows");
        assert!(mapper.tracking());

        // The carry: the robot is SAT, carried, and stood back up — the
        // sit arms pose suspicion, which is the kidnap signal geometry
        // cannot fake through the ToF keyhole.
        for _ in 0..50 {
            mapper.observe(
                t,
                MapperSample {
                    odom: (0.0, 0.0, 0.0),
                    moving: false,
                    sitting: true,
                    fallen: false,
                },
                &mut notes,
            );
            t += 0.02;
        }
        assert!(!mapper.tracking(), "a sit must make the pose suspect");

        // Kidnap: odometry still reads the origin, but the robot now really
        // stands at (0.8, 0.5, 0.9) — its scans are the room seen from
        // there, expressed in the body frame odometry believes in.
        let truth = (0.8, 0.5, 0.9);
        let mut next_frame = t;
        let end = t + 20.0;
        let mut relocalized = None;
        while t < end {
            mapper.observe(
                t,
                MapperSample {
                    odom: (0.0, 0.0, 0.0),
                    moving: false,
                    sitting: false,
                    fallen: false,
                },
                &mut notes,
            );
            if t >= next_frame {
                mapper.frame(t, room_scan(truth));
                next_frame += 1.0 / 15.0;
            }
            for note in notes.drain(..) {
                if let Note::Relocalized { pose, .. } = note {
                    relocalized = Some(pose);
                }
            }
            if relocalized.is_some() {
                break;
            }
            t += 0.02;
        }

        let pose = relocalized.expect("the mapper must relocalize after a kidnap");
        let err = (pose.0 - truth.0).hypot(pose.1 - truth.1);
        assert!(err < 0.25, "relocalized {pose:?}, truth {truth:?}");
        assert!(wrap_pi(pose.2 - truth.2).abs() < 0.3);
        assert!(mapper.tracking());
    }

    /// A robot that sits and stands WITHOUT being moved confirms its own
    /// pose from the first window and resumes mapping.
    #[test]
    fn a_sit_in_place_recovers_in_one_window() {
        let mut mapper = Mapper::new(MapperConfig::default(), Slam::new(SlamConfig::default()));
        let mut notes = Vec::new();
        let mut t = drive(&mut mapper, 0.0, (0.0, 0.0, 0.0), 8.0, &mut notes);
        for _ in 0..100 {
            mapper.observe(
                t,
                MapperSample {
                    odom: (0.0, 0.0, 0.0),
                    moving: false,
                    sitting: true,
                    fallen: false,
                },
                &mut notes,
            );
            t += 0.02;
        }
        assert!(!mapper.tracking());
        let before = mapper.slam().tracked();
        drive(&mut mapper, t, (0.0, 0.0, 0.0), 8.0, &mut notes);
        assert!(
            mapper.tracking(),
            "an unmoved robot must confirm its pose and resume"
        );
        let confirmed = notes.iter().rev().find_map(|n| match n {
            Note::Relocalized { pose, .. } => Some(*pose),
            _ => None,
        });
        let pose = confirmed.expect("confirmation shows up as a relocalization");
        assert!((pose.0 - before.0).hypot(pose.1 - before.1) < 0.05);
    }

    /// A fall arms the same suspicion as a sit, and an unmoved robot
    /// confirms its pose and resumes.
    #[test]
    fn a_fall_makes_the_pose_suspect_and_recovers() {
        let mut mapper = Mapper::new(MapperConfig::default(), Slam::new(SlamConfig::default()));
        let mut notes = Vec::new();
        let mut t = drive(&mut mapper, 0.0, (0.0, 0.0, 0.0), 8.0, &mut notes);
        for _ in 0..50 {
            mapper.observe(
                t,
                MapperSample {
                    odom: (0.0, 0.0, 0.0),
                    moving: false,
                    sitting: false,
                    fallen: true,
                },
                &mut notes,
            );
            t += 0.02;
        }
        assert!(!mapper.tracking(), "a fall must make the pose suspect");
        drive(&mut mapper, t, (0.0, 0.0, 0.0), 8.0, &mut notes);
        assert!(
            mapper.tracking(),
            "an unmoved robot must confirm and resume"
        );
    }

    /// A resumed session boots with a suspect pose (the robot may have
    /// been moved while the daemon was off) and confirms it from the
    /// first window when it was not.
    #[test]
    fn a_resumed_session_confirms_before_inking() {
        let mut mapper = Mapper::new(MapperConfig::default(), Slam::new(SlamConfig::default()));
        let mut notes = Vec::new();
        drive(&mut mapper, 0.0, (0.0, 0.0, 0.0), 8.0, &mut notes);
        let path = std::env::temp_dir().join(format!(
            "maploc_mapper_resume_{}.session",
            std::process::id()
        ));
        mapper.slam().save(&path).expect("save");
        let restored = crate::session::SessionState::load(&path)
            .expect("load")
            .expect("present");
        std::fs::remove_file(&path).ok();

        let mut resumed = Mapper::new(
            MapperConfig::default(),
            Slam::from_session(SlamConfig::default(), restored),
        );
        assert!(
            !resumed.tracking(),
            "a resumed map must not vouch for its pose"
        );
        drive(&mut resumed, 100.0, (0.0, 0.0, 0.0), 8.0, &mut notes);
        assert!(
            resumed.tracking(),
            "booting where it saved must confirm from the first window"
        );
    }

    /// A wake-up believes nothing it has not walked for. Turning on the
    /// spot — a decimetre of drift a kick, sixteen kicks, 1.6 m of "path"
    /// inside a 30 cm circle — is what four wake-ups in five confirmed
    /// the mirror image on (2026-09-14); legs are what tell an alias from
    /// the truth. So: no confirmation from the spot it woke on, however
    /// long it turns, and a confirmation once it has walked a metre.
    #[test]
    fn a_wake_up_walks_before_it_believes() {
        let mut mapper = Mapper::new(MapperConfig::default(), Slam::new(SlamConfig::default()));
        let mut notes = Vec::new();
        let mut t = drive_in(
            &mut mapper,
            0.0,
            (0.0, 0.0, 0.0),
            8.0,
            &mut notes,
            lumpy_room_scan,
        );
        t = drive_in(
            &mut mapper,
            t,
            (0.9, -0.6, -1.2),
            8.0,
            &mut notes,
            lumpy_room_scan,
        );
        t = drive_in(
            &mut mapper,
            t,
            (-0.9, 0.4, 2.0),
            8.0,
            &mut notes,
            lumpy_room_scan,
        );
        assert!(mapper.windows() >= 3 && mapper.tracking());
        let path = std::env::temp_dir().join(format!(
            "maploc_mapper_wakeup_{}.session",
            std::process::id()
        ));
        mapper.slam().save(&path).expect("save");
        let restored = crate::session::SessionState::load(&path)
            .expect("load")
            .expect("present");
        std::fs::remove_file(&path).ok();

        // Switched on somewhere else in the room. Odometry starts in the
        // world frame here for the test's convenience; the mapper is not
        // told that, its pose is lost from the first window.
        let mut duck = Mapper::resumed_lost(
            MapperConfig::default(),
            Slam::from_session(SlamConfig::default(), restored),
        );
        assert!(!duck.tracking());
        let relocalized = |notes: &mut Vec<Note>| {
            notes.drain(..).find_map(|n| match n {
                Note::Relocalized { pose, .. } => Some(pose),
                _ => None,
            })
        };

        // Sixteen kicks round a 15 cm circle: 1.6 m of path, no chord.
        let (cx, cy) = (-0.8_f32, 0.5_f32);
        let mut at = (cx + 0.15, cy, 0.9);
        for k in 0..16 {
            let a = k as f32 * 0.5;
            let here = (cx + 0.15 * a.cos(), cy + 0.15 * a.sin(), wrap_pi(0.9 + a));
            t = walk_in(&mut duck, t, at, here, 1.5, &mut notes, lumpy_room_scan);
            at = here;
            t = drive_in(&mut duck, t, here, 7.0, &mut notes, lumpy_room_scan);
            assert!(
                relocalized(&mut notes).is_none() && !duck.tracking(),
                "kick {k}: confirmed without leaving the spot"
            );
        }

        // Then legs of 0.4 m, east along the room and south down its far
        // wall: it must come home, and at the truth.
        let mut home = None;
        let legs: [Pose2; 8] = [
            (-0.4, 0.5, 0.0),
            (0.0, 0.5, 0.0),
            (0.4, 0.5, 0.0),
            (0.8, 0.5, 0.0),
            (1.2, 0.5, 0.0),
            (1.2, 0.1, -1.5),
            (1.2, -0.3, -1.5),
            (1.2, -0.7, -1.5),
        ];
        for here in legs {
            t = walk_in(&mut duck, t, at, here, 3.0, &mut notes, lumpy_room_scan);
            at = here;
            t = drive_in(&mut duck, t, here, 7.0, &mut notes, lumpy_room_scan);
            if std::env::var_os("RELOC_DEBUG").is_some() {
                for n in notes.iter() {
                    eprintln!("    leg {here:?}: {n:?}");
                }
            }
            if let Some(pose) = relocalized(&mut notes) {
                home = Some((pose, here));
                break;
            }
        }
        let (pose, truth) = home.expect("walking across the room must bring it home");
        assert!(
            (pose.0 - truth.0).hypot(pose.1 - truth.1) < 0.25
                && wrap_pi(pose.2 - truth.2).abs() < 0.3,
            "came home at {pose:?}, truth {truth:?}"
        );
        assert!(duck.tracking());
    }

    /// Soft suspicion facing territory the map cannot judge gives up
    /// after its budget and resumes at the odometry-carried pose —
    /// without the escape, a robot that sits facing an unmapped corner
    /// would say "searching" forever.
    #[test]
    fn soft_suspicion_gives_up_when_nothing_can_judge() {
        let mut mapper = Mapper::new(MapperConfig::default(), Slam::new(SlamConfig::default()));
        let mut notes = Vec::new();
        let mut t = drive(&mut mapper, 0.0, (0.0, 0.0, 0.0), 8.0, &mut notes);
        // Sit (suspicion), then wake up somewhere the map has never seen:
        // odometry says (5, 5) — far outside the mapped room — and the
        // scans are a fixed ring nothing can compare against.
        for _ in 0..50 {
            mapper.observe(
                t,
                MapperSample {
                    odom: (0.0, 0.0, 0.0),
                    moving: false,
                    sitting: true,
                    fallen: false,
                },
                &mut notes,
            );
            t += 0.02;
        }
        assert!(!mapper.tracking());
        let ring: Vec<f32> = (0..240)
            .map(|k| -std::f32::consts::PI + k as f32 * (2.0 * std::f32::consts::PI / 240.0))
            .collect();
        let ranges = vec![1.0f32; 240];
        let scan = || Scan::from_polar(&ring, &ranges, (0.0, 0.0), 1e-3);
        let mut next_frame = t;
        let mut resumed = None;
        let end = t + 60.0;
        while t < end {
            mapper.observe(
                t,
                MapperSample {
                    odom: (5.0, 5.0, 0.0),
                    moving: false,
                    sitting: false,
                    fallen: false,
                },
                &mut notes,
            );
            if t >= next_frame {
                mapper.frame(t, scan());
                next_frame += 1.0 / 15.0;
            }
            for n in notes.drain(..) {
                if let Note::ResumedUnverified { pose } = n {
                    resumed = Some(pose);
                }
            }
            if resumed.is_some() {
                break;
            }
            t += 0.02;
        }
        let pose = resumed.expect("soft suspicion must eventually give up");
        assert!(mapper.tracking());
        // The odometry carried the pose to (5, 5) relative to the seed.
        assert!((pose.0 - 5.0).abs() < 0.1 && (pose.1 - 5.0).abs() < 0.1);
    }

    /// Continuous mode must recover from suspicion the same way
    /// stop-and-scan does: a continuous mapper that sat once used to sweep
    /// its head forever — `lost` was armed on the sit and nothing on the
    /// continuous path could ever clear it.
    #[test]
    fn continuous_mode_recovers_from_suspicion() {
        let mut mapper = Mapper::new(
            MapperConfig {
                continuous: true,
                ..MapperConfig::default()
            },
            Slam::new(SlamConfig::default()),
        );
        let mut notes = Vec::new();
        let mut t = drive(&mut mapper, 0.0, (0.0, 0.0, 0.0), 8.0, &mut notes);
        assert!(mapper.tracking());
        for _ in 0..50 {
            mapper.observe(
                t,
                MapperSample {
                    odom: (0.0, 0.0, 0.0),
                    moving: false,
                    sitting: true,
                    fallen: false,
                },
                &mut notes,
            );
            t += 0.02;
        }
        assert!(!mapper.tracking(), "a sit must suspend continuous mapping");
        drive(&mut mapper, t, (0.0, 0.0, 0.0), 10.0, &mut notes);
        assert!(
            mapper.tracking(),
            "an unmoved continuous mapper must confirm its pose and resume"
        );
    }

    /// A displaced robot whose windows the map can judge — even on a
    /// minority of beams — must never exhaust soft suspicion into
    /// ResumedUnverified: refutation lives in the judged beams, and the
    /// give-up escape is only for views the map cannot judge at all.
    #[test]
    fn a_contradicted_seed_never_resumes_unverified() {
        let mut mapper = Mapper::new(MapperConfig::default(), Slam::new(SlamConfig::default()));
        let mut notes = Vec::new();
        let mut t = drive(&mut mapper, 0.0, (0.0, 0.0, 0.0), 8.0, &mut notes);
        for _ in 0..50 {
            mapper.observe(
                t,
                MapperSample {
                    odom: (0.0, 0.0, 0.0),
                    moving: false,
                    sitting: true,
                    fallen: false,
                },
                &mut notes,
            );
            t += 0.02;
        }
        assert!(!mapper.tracking());

        // The kidnapped view: ~85 % of beams land beyond the mapped walls
        // (unknown cells — unjudgeable), ~15 % land mid-room in carved
        // free space, far from every wall — judgeable, and contradicting.
        let mut angles = Vec::new();
        let mut ranges = Vec::new();
        for k in 0..1000 {
            let a = -std::f32::consts::PI + k as f32 * (2.0 * std::f32::consts::PI / 1000.0);
            angles.push(a);
            ranges.push(if k % 7 == 0 { 0.9 } else { 1.9 });
        }
        let scan = || Scan::from_polar(&angles, &ranges, (0.0, 0.0), 1e-3);

        let mut next_frame = t;
        let mut debug_once = true;
        let end = t + 90.0; // far past 10 windows' worth of give-up budget
        while t < end {
            mapper.observe(
                t,
                MapperSample {
                    odom: (0.0, 0.0, 0.0),
                    moving: false,
                    sitting: false,
                    fallen: false,
                },
                &mut notes,
            );
            if t >= next_frame {
                mapper.frame(t, scan());
                next_frame += 1.0 / 15.0;
            }
            if let (Some(mut g), Some((p, sc))) =
                (mapper.slam().render(), mapper.last_window().cloned())
            {
                let wd = mapper.cfg.watchdog;
                let a = crate::relocalize::score_pose(
                    &mut g,
                    &sc,
                    p,
                    wd.clamp_m,
                    wd.wall_threshold_fp,
                    wd.observed_fp,
                );
                if debug_once {
                    println!(
                        "window agreement: mean {:.3} over {}/{} ({:.0}%)",
                        a.mean_residual_m,
                        a.n_observed,
                        a.n_beams,
                        100.0 * a.n_observed as f32 / a.n_beams as f32
                    );
                    debug_once = false;
                }
            }
            for n in notes.drain(..) {
                assert!(
                    !matches!(n, Note::ResumedUnverified { .. }),
                    "a judged-and-contradicted pose must never resume unverified"
                );
                if let Note::Relocalized { pose, .. } = n {
                    panic!("nothing should confirm here, got {pose:?}");
                }
            }
            t += 0.02;
        }
        assert!(!mapper.tracking(), "the mapper must still be searching");
    }

    /// Beams into unexplored territory must never read as "lost".
    #[test]
    fn exploring_new_territory_is_not_a_kidnap() {
        let mut mapper = Mapper::new(MapperConfig::default(), Slam::new(SlamConfig::default()));
        let mut notes = Vec::new();
        let t = drive(&mut mapper, 0.0, (0.0, 0.0, 0.0), 8.0, &mut notes);
        // Face the other way from a spot the map has never judged: the
        // scans land in unknown cells.
        drive(&mut mapper, t, (0.4, -0.3, 2.5), 8.0, &mut notes);
        assert!(
            mapper.tracking(),
            "new territory must be mapped, not declared a kidnap"
        );
    }
}
