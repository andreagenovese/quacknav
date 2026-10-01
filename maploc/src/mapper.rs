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
//! a resumed session whose robot moved while the daemon was down. A
//! mapper resumed lost on a saved map also keeps a *shadow*: a fresh map
//! of its walk, asked every 30 s where it fits in the saved one
//! ([`crate::align::match_maps`]); agreeing answers give a soft seed
//! (the fit composed with the odometry since the shadow began) that two
//! windows confirm like any candidate (`MAPLOC_SHADOW=0` turns it off).

use crate::accumulator::{AccumulatorConfig, WindowAccumulator};
use crate::grid::OccupancyGrid;
use crate::pipeline::Slam;
use crate::pose_graph::{between, compose, wrap_pi};
use crate::relocalize::{RelocalizeConfig, relocalize_against_grid, score_pose};
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
    /// Gaussian prior during the match: on the tracked pose, its position
    /// at a stand on where the stand began (see `Mapper::absorb_window`).
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
    /// ... and a search winner turned more than this from it is an alias
    /// too. 0 = any heading.
    pub hard_lost_search_yaw_rad: f32,
    pub relocalize_agree_m: f32,
    pub relocalize_agree_rad: f32,
    /// When suspicion came from a sit, a fall or a session resume (soft —
    /// nothing has CONTRADICTED the pose) and this many windows could not
    /// be judged either way (unmapped view), give up and resume at the
    /// odometry-carried pose. Without an escape, a robot that sits facing
    /// an unmapped corner stays "searching" forever; with evidence of
    /// displacement the escape never applies.
    pub suspect_give_up_windows: u32,
    /// The pose covariance's motion noise and window weighting (see
    /// [`crate::uncertainty`]).
    pub uncertainty: crate::uncertainty::UncertaintyConfig,
    pub settle: SettleConfig,
}

/// After a pose is found on a map from an earlier run — a boot's search, or
/// the search after a fall — the windows correct the pose but ink nothing
/// until the corrections have stopped: `windows` in a row the map judges,
/// agrees with, and moves the pose by no more than a jitter. A resumed
/// session that came home 12 cm off and mapped on at once is how
/// casa_arredata's walls went from 3.8 to 17 cm off (2026-09-24). A window
/// the map cannot judge — new floor — neither settles nor unsettles; after
/// `max_held_windows` held windows inking resumes anyway, noted, so a duck
/// sent to new floor is not stopped from mapping it. Off by default
/// (`enabled`): on six resumed sessions one came out much better and the
/// rest even or a little worse, casa_arredata's among them (docs/results.md;
/// `MAPLOC_SETTLE=1`, the bench's switch, removed 2026-09-30).
#[derive(Debug, Clone, Copy)]
pub struct SettleConfig {
    pub enabled: bool,
    pub windows: u32,
    pub max_correction_m: f32,
    pub max_correction_rad: f32,
    pub max_residual_m: f32,
    pub min_beams: u32,
    pub max_held_windows: u32,
}

impl Default for SettleConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            windows: 2,
            max_correction_m: 0.02,
            max_correction_rad: 0.02,
            max_residual_m: 0.05,
            min_beams: 100,
            max_held_windows: 12,
        }
    }
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
            hard_lost_search_yaw_rad: 0.6,
            relocalize_agree_m: 0.3,
            relocalize_agree_rad: 0.35,
            suspect_give_up_windows: 10,
            uncertainty: crate::uncertainty::UncertaintyConfig::default(),
            settle: SettleConfig::default(),
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
    /// A window the watchdog would have called a contradiction, which the
    /// tracking's match put back on the map (see `watchdog_rescue`).
    WindowRescued { mean_residual_m: f32 },
    WindowQuarantined {
        mean_residual_m: f32,
        n_observed: u32,
    },
    /// The search proposed a pose; the next window must confirm it.
    RelocalizeCandidate {
        pose: Pose2,
        mean_residual_m: f32,
    },
    /// The shadow map asked where it sits in the saved one (see [`Shadow`]):
    /// the fit of its frame, the answer's numbers, and how many answers in
    /// a row now agree.
    ShadowAsked {
        fit: Pose2,
        score: f32,
        margin: f32,
        overlap: f32,
        cells: usize,
        agreed: u32,
    },
    /// Enough answers agreed: the pose they put the duck at is the soft
    /// seed, for the windows to confirm.
    ShadowSeed { pose: Pose2 },
    /// A window refuted that seed.
    ShadowSeedRefuted,
    /// A pending candidate a window did not confirm, and dropped: `verdict`
    /// "unjudgeable", "ambiguous" or "refuted"; the window's residual and
    /// beams at the pose, and the travel since nomination.
    RelocalizeCandidateJudged {
        pose: Pose2,
        verdict: &'static str,
        mean_residual_m: f32,
        n_observed: u32,
        n_beams: u32,
        chord_m: f32,
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
    /// Settling after a resume: the window corrected the pose and was not
    /// inked (see [`SettleConfig`]).
    WindowHeld {
        residual_m: f32,
        correction_m: f32,
    },
    /// Settling is over: `held` windows were held back, and `gave_up` when
    /// the map could not judge enough of them to settle and inking resumed
    /// on the cap.
    Settled {
        held: u32,
        gave_up: bool,
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
    /// The map of what the lost duck has walked since boot, asked now and
    /// then where it sits in the saved one (see [`Shadow`]).
    shadow: Option<Box<Shadow>>,
    /// The soft seed came from the shadow's fit: its uniqueness is the
    /// fit's, over the whole walk, and the one-window gates are not asked.
    seed_from_map: bool,
    /// The map as it stood when the current stand began — what the
    /// watchdog judges the stand's windows against. Judging against the
    /// LIVE map lets a kidnapped stand vouch for itself: its first window
    /// paints the kidnapper's room, and every following window then
    /// "agrees with the map" it just painted (measured: vs-map 0.005
    /// while vs-truth 0.3–0.5). Ink earned during a stand never testifies
    /// for that stand.
    stand_grid: Option<OccupancyGrid>,
    /// `continuous` only: frames since the last correction, and when that
    /// was. See [`MapperConfig::continuous_correct_s`].
    roll: WindowAccumulator,
    roll_at: f32,
    /// The last window handed to `absorb_window`, whatever became of it —
    /// a bench inspects it to score composites against ground truth. One
    /// composite clone per window; noise next to the integration itself.
    last_window: Option<(Pose2, Scan)>,
    /// How sure the pose is, map frame (see [`crate::uncertainty`]): grown by
    /// odometry, shrunk by every window the map judges, set afresh when a
    /// search confirms a pose. Meaningless while lost.
    cov: crate::uncertainty::Cov3,
    /// The odometry pose the covariance was last carried to.
    cov_odom: Option<Pose2>,
    /// Settling after a resume (see [`SettleConfig`]): settled windows in a
    /// row, and windows held so far.
    settling: Option<(u32, u32)>,
    /// The valleys refused while lost: (candidate, tracked pose then, the
    /// valley's direction), see [`Mapper::valley_blocks`].
    valleys: Vec<(Pose2, Pose2, (f32, f32))>,
    /// How far the tracking corrections of the current stand have moved
    /// the pose, map frame: the stand's windows are matched with their
    /// prior where the stand began, not on the last window's answer (see
    /// `absorb_window`).
    stand_moved: (f32, f32),
}

/// `MAPLOC_WATCHDOG_RESCUE=0`: the watchdog judges a window at the carried
/// pose alone, as before 2026-09-28.
fn watchdog_rescue() -> bool {
    std::env::var("MAPLOC_WATCHDOG_RESCUE").map_or(true, |v| v != "0")
}

/// `MAPLOC_LOCAL_AFTER_BOOT=0`: every loss on a resumed map searches the
/// whole map with every hypothesis, as before 2026-09-28.
fn local_after_boot() -> bool {
    std::env::var("MAPLOC_LOCAL_AFTER_BOOT").map_or(true, |v| v != "0")
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

/// Two valleys that cross resolve each other (see `Mapper::valley_blocks`);
/// `MAPLOC_VALLEY_CROSS=0` for the valley test alone. On since the replay
/// of 35 recorded boots (2026-09-25): 33 confirmed against 32, none wrong
/// that was not wrong before, casa_arredata's alias still refused, the
/// median boot 179 -> 155 s (one house2 boot 328 -> 117 s).
fn valley_cross() -> bool {
    std::env::var("MAPLOC_VALLEY_CROSS").map(|v| v != "0").unwrap_or(true)
}
/// Refused valleys remembered while lost.
const VALLEYS_KEPT: usize = 12;

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

/// The particle filter (`mcl.rs`) once ran here as a boot search on a
/// resumed map (`MAPLOC_MCL=1`, with `_N`, `_YAW`, `_TRAVEL`, `_RESID`):
/// measured and left off (docs/todo-map.md), the multi-hypothesis search
/// and the shadow map took its place, and it was removed 2026-09-30. `mcl.rs` stays, Pollen's code.

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
        let shadow_cfg = cfg.clone();
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
        if shadow_enabled()
            && let Some(saved) = mapper.slam.render()
        {
            mapper.shadow = Some(Box::new(Shadow {
                fresh: Mapper::new(shadow_cfg, Slam::new(crate::pipeline::SlamConfig::default())),
                saved,
                origin: None,
                chord: 0.0,
                starts: None,
                next_ask: None,
                prev: None,
                agreed: 0,
                agreed_wide: 0,
            }));
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
            stand_moved: (0.0, 0.0),
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
            shadow: None,
            seed_from_map: false,
            stand_grid: None,
            roll: WindowAccumulator::new(rolling),
            roll_at: 0.0,
            last_window: None,
            // A fresh map's origin is the pose: known exactly.
            cov: crate::uncertainty::diagonal(0.0, 0.0),
            cov_odom: None,
            settling: None,
            valleys: Vec::new(),
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
    /// The raw odometry of the last sample observed, for the benches.
    pub fn last_odom(&self) -> Option<Pose2> {
        self.odom_window.last().map(|&(_, x, y, yaw)| (x, y, yaw))
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
    /// How sure the tracked pose is — map-frame covariance over (x, y, yaw) —
    /// or `None` while lost, when there is no pose to be sure of.
    pub fn pose_covariance(&self) -> Option<crate::uncertainty::Cov3> {
        (!self.lost).then_some(self.cov)
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

    /// The pose and composite of the last closed window (see field doc).
    pub fn last_window(&self) -> Option<&(Pose2, Scan)> {
        self.last_window.as_ref()
    }

    /// One control-loop tick. `t_s` is seconds on any monotonic timebase —
    /// the host's uptime, a recording's timestamps — as long as one mapper
    /// sees only one. Notes are appended, not replaced.
    pub fn observe(&mut self, t_s: f32, sample: MapperSample, notes: &mut Vec<Note>) {
        if self.lost
            && let Some(mut shadow) = self.shadow.take()
        {
            let mut ignored = Vec::new();
            shadow.fresh.observe(t_s, sample, &mut ignored);
            let starts = *shadow.starts.get_or_insert((shadow.fresh.slam().tracked(), self.slam.tracked()));
            if let Some(found) = shadow.ask(t_s, notes) {
                // Where the duck stands on the saved map now: the fit
                // carries the shadow's frame onto it, and odometry since the
                // shadow began carries its start to here. Not the shadow's
                // own pose at the end of its walk: a map drawn down a
                // corridor stretches along it, and the apartment's duck,
                // seeded where its stretched map put it, 0.35 m off along
                // the corridor, was confirmed 0.50 m off by windows that
                // cannot see along it (x17, 2026-09-30).
                let (fresh0, main0) = starts;
                let pose = compose(found, compose(fresh0, between(main0, self.slam.tracked())));
                self.soft_seed = Some((pose, self.slam.tracked()));
                self.seed_agreed = 0;
                self.seed_from_map = true;
                notes.push(Note::ShadowSeed { pose });
            }
            self.shadow = Some(shadow);
        }
        if let Some(prev) = self.cov_odom
            && !self.lost
        {
            self.cov = crate::uncertainty::predict(&self.cov, self.slam.tracked(), between(prev, sample.odom), &self.cfg.uncertainty);
        }
        self.cov_odom = Some(sample.odom);
        self.slam.observe_odom(sample.odom);
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
            self.stand_moved = (0.0, 0.0);
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
            && let Some(shadow) = self.shadow.as_mut()
        {
            shadow.fresh.frame(t_s, scan.clone());
        }
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
                let judged = self.check_candidate(&mut grid, composite, cand, then, now);
                if std::env::var_os("RELOC_DEBUG").is_some() {
                    let v = match &judged.1 {
                        Verdict::Confirmed(_, r) => format!("confirmed {r:.3}"),
                        Verdict::Refuted => "refuted".into(),
                        Verdict::Ambiguous => "ambiguous".into(),
                        Verdict::Unjudgeable => "unjudgeable".into(),
                    };
                    eprintln!("    seed ({:.2},{:.2},{:.0}°): {v}", judged.0.0, judged.0.1, judged.0.2.to_degrees());
                }
                match judged {
                    (implied, Verdict::Confirmed(pose, resid)) => {
                        self.seed_agreed += 1;
                        // At boot the seed is the pose the session ended
                        // at, and a duck switched on in another room that
                        // looks the same agrees with it in one window: the
                        // agreement has to be unique before it is believed.
                        let unique = self.seed_from_map
                            || !self.resumed_from_session
                            || self.unique_at(&mut grid, composite, pose) == Some(true);
                        // The valley test is not asked of the shadow's seed
                        // either: kept, it refused a right seed 3 cm off in
                        // casa_arredata and did not stop the apartment's
                        // wrong confirmation of x17, which the windows' own
                        // search made too (2026-09-30).
                        let unique = unique && (self.seed_from_map || !(self.resumed_from_session || self.after_fall) || {
                            let probe = composite.decimated(self.cfg.relocalize_max_beams);
                            !self.valley_blocks(&mut grid, &probe, pose, now, notes)
                        });
                        if self.seed_agreed >= 2 && unique {
                            self.resume_at(&mut grid, pose, composite, t_s);
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
                        if std::mem::take(&mut self.seed_from_map) {
                            notes.push(Note::ShadowSeedRefuted);
                        }
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
                        // Never at a boot on a saved map: the seed there
                        // is the pose the session was saved at, and one
                        // saved with the duck down read (-65, -286) —
                        // nothing on the map could judge it, and giving
                        // up "resumed" it (the twin's house2, 2026-09-24).
                        if self.unjudged >= self.cfg.suspect_give_up_windows && !self.after_fall && !self.resumed_from_session {
                            self.resume_at(&mut grid, implied, composite, t_s);
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
            // A candidate this window does not confirm is dropped, and why
            // is noted.
            let judged = self.pending_reloc.take().map(|(cand, then)| (cand, then, self.check_candidate(&mut grid, composite, cand, then, now)));
            if let Some((_, then, (implied, v @ (Verdict::Ambiguous | Verdict::Unjudgeable | Verdict::Refuted)))) = &judged {
                let verdict = match v {
                    Verdict::Ambiguous => "ambiguous",
                    Verdict::Unjudgeable => "unjudgeable",
                    _ => "refuted",
                };
                let wd = self.cfg.watchdog;
                let a = self.judge_untrusted(&mut grid, composite, *implied, wd.clamp_m, wd.wall_threshold_fp, wd.observed_fp);
                notes.push(Note::RelocalizeCandidateJudged {
                    pose: *implied,
                    verdict,
                    mean_residual_m: a.mean_residual_m,
                    n_observed: a.n_observed,
                    n_beams: a.n_beams,
                    chord_m: (now.0 - then.0).hypot(now.1 - then.1),
                });
            }
            if let Some((cand, then, (_, Verdict::Confirmed(pose, resid)))) = judged
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
                if (self.resumed_from_session || self.after_fall) && self.valley_blocks(&mut grid, &probe, pose, now, notes) {
                    // Agreement along a valley is agreement with every
                    // pose on it: dropped, and the search goes on until a
                    // window sees what pins the pose down.
                } else if self.booting && chord < confirm_travel_m() {
                    self.pending_reloc = Some((cand, then));
                    notes.push(Note::RelocalizeCandidate {
                        pose,
                        mean_residual_m: resid,
                    });
                } else {
                    self.resume_at(&mut grid, pose, composite, t_s);
                    notes.push(Note::Relocalized {
                        pose,
                        mean_residual_m: resid,
                    });
                    return;
                }
            }

            // Hard-lost for too long: odometry has carried the pose all
            // along; resume there rather than keep the map cold.
            if self.hard_lost && self.cfg.lost_give_up_windows > 0 && !self.after_fall {
                self.lost_windows += 1;
                if self.lost_windows > self.cfg.lost_give_up_windows {
                    let here = self.slam.tracked();
                    self.resume_at(&mut grid, here, composite, t_s);
                    notes.push(Note::ResumedUnverified { pose: here });
                    return;
                }
            }
            // No confirmation: search this window for a fresh candidate.
            let probe = composite.decimated(self.cfg.relocalize_max_beams);
            // ... and turned no more than `hard_lost_search_yaw_rad` from
            // it: a corridor's alias turned half a turn sits a metre off
            // and matches as well as the pose (house2, MuJoCo, 2026-09-27:
            // confirmed at residual 0.002, 180° off, the duck lost for
            // fourteen minutes with odometry knowing it had not turned).
            let near_enough = |r: &crate::relocalize::RelocalizeResult| {
                !self.hard_lost
                    || self.cfg.hard_lost_search_radius_m <= 0.0
                    || ((r.pose.0 - now.0).hypot(r.pose.1 - now.1) <= self.cfg.hard_lost_search_radius_m
                        && (self.cfg.hard_lost_search_yaw_rad <= 0.0
                            || wrap_pi(r.pose.2 - now.2).abs() <= self.cfg.hard_lost_search_yaw_rad))
            };
            // Boot on a saved map: keep every plausible basin and let the
            // viewpoints decide. `multi` off falls back to the single-best
            // agreement below, which is what a kidnap in place wants.
            // The boot's own search: a resumed map, no pose anyone vouches
            // for. Past the boot a "lost" is local, and the search below
            // keeps near the odometry-carried pose — this branch, gated on
            // the session having been resumed, ran for every later loss
            // too and confirmed an alias a metre off, half a turn round.
            if multi_hypothesis() && self.hard_lost && self.resumed_from_session && (self.booting || !local_after_boot()) {
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
            // Judged where the pose was carried, not where the map puts it:
            // a pose 0.2-0.3 m off contradicts at every window, no window
            // corrects it, and odometry carries it farther — house2 beside
            // the stairwell, MuJoCo 2026-09-28: quarantine after quarantine,
            // the pose 0.44 m off. So first the tracking's own match, as it
            // would run below (its 0.30 m and 0.20 rad, its 0.02 m residual
            // after): where it lands on the map, and the window agrees
            // there, this is drift to correct, not a contradiction.
            let rescue_pose: Option<Pose2> = if watchdog_rescue() && self.cfg.tracking.enabled && a.mean_residual_m > wd.max_mean_residual_m {
                rescue_match(grid, composite, pose).filter(|p| {
                    let b = score_pose(grid, composite, *p, wd.clamp_m, wd.wall_threshold_fp, wd.observed_fp);
                    b.mean_residual_m <= wd.max_mean_residual_m && b.n_observed >= wd.min_observed_beams
                })
            } else {
                None
            };
            let rescued = rescue_pose.is_some();
            if rescued {
                notes.push(Note::WindowRescued { mean_residual_m: a.mean_residual_m });
            }
            if !rescued
                && a.n_observed >= wd.min_observed_beams
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
        // The match's prior is where the stand began (odometry's carry
        // since, the windows' corrections not), in position; the heading's
        // stays the pose's own, for odometry's yaw drifts at a stand and the
        // windows are what correct it. A stand's windows are the same scene
        // seen again, not new evidence: with the prior on the last window's
        // answer, each correction became the next one's starting point, and
        // along a direction the scene barely pins they added up. The twin's
        // duck, standing fourteen minutes before one long wall, had its pose
        // walked 1.56 m along it by 132 corrections of about a centimetre,
        // each improving its window's residual (casa_grande, 2026-10-01; the
        // normal matrix there said 0.3-0.5 of conditioning, the ink's
        // roughness, so the projection below did not catch it). Anchored,
        // the same recording's pose stays within 9 cm of where it stopped.
        let prior = (pose.0 - self.stand_moved.0, pose.1 - self.stand_moved.1, pose.2);
        let mut pose = pose;
        let mut correction: Option<Pose2> = None;
        // A rescued window is not corrected by the rescue's match: applied,
        // one of them — a local alias at residual 0.05 — took a replay from
        // 0.06 m to 0.41 m. It is only not called a contradiction; the
        // tracking's own correction, below, stays the judge of the pose.
        if self.cfg.tracking.enabled
            && let Some(grid) = self.stand_grid.as_mut()
            && let Some((delta, before, after, n_used)) =
                tracking_correction(grid, composite, pose, prior, &self.cfg.tracking)
        {
            pose = compose(pose, delta);
            correction = Some(delta);
            let tracked = self.slam.tracked();
            self.slam.set_tracked(compose(tracked, delta));
            let moved = compose(tracked, delta);
            self.stand_moved.0 += moved.0 - tracked.0;
            self.stand_moved.1 += moved.1 - tracked.1;
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
        let judged = self.stand_grid.as_mut().map(|grid| window_at_pose(grid, composite, pose, &self.cfg.uncertainty));
        let skip = self.cfg.uncertainty.skip_recent_submaps;
        let independent = if skip == 0 {
            judged.as_ref().and_then(|j| j.info)
        } else {
            self.slam
                .render_older(skip)
                .and_then(|mut older| window_at_pose(&mut older, composite, pose, &self.cfg.uncertainty).info)
        };
        if let Some(info) = independent {
            self.cov = crate::uncertainty::fuse(&self.cov, &info);
        }
        // Settling after a resume: the window corrects the pose, it does not
        // ink, until the corrections have stopped (see `SettleConfig`).
        if let Some((run, held)) = self.settling {
            let st = self.cfg.settle;
            let seen = judged.as_ref().is_some_and(|j| j.n_used >= st.min_beams);
            let small = correction.is_none_or(|d| d.0.hypot(d.1) <= st.max_correction_m && d.2.abs() <= st.max_correction_rad);
            let agrees = judged.as_ref().is_some_and(|j| j.residual_m <= st.max_residual_m);
            let run = if seen && small && agrees {
                run + 1
            } else if seen {
                0
            } else {
                run
            };
            let held = held + 1;
            if run >= st.windows || held > st.max_held_windows {
                self.settling = None;
                notes.push(Note::Settled { held, gave_up: run < st.windows });
            } else {
                self.settling = Some((run, held));
                notes.push(Note::WindowHeld {
                    residual_m: judged.as_ref().map_or(f32::INFINITY, |j| j.residual_m),
                    correction_m: correction.map_or(0.0, |d| d.0.hypot(d.1)),
                });
                return;
            }
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
            tracking_correction(grid, &composite, pose, pose, &self.cfg.tracking)
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
    fn resume_at(&mut self, grid: &mut OccupancyGrid, pose: Pose2, composite: &Scan, t_s: f32) {
        // What the confirming window alone says of the pose: the search's
        // odometry since boot vouches for nothing. What it cannot see is
        // capped at the width of the search's own agreement.
        let cap = crate::uncertainty::diagonal(f64::from(self.cfg.relocalize_agree_m), f64::from(self.cfg.relocalize_agree_rad));
        self.cov = match window_information_at(grid, composite, pose, &self.cfg.uncertainty) {
            Some(info) => crate::uncertainty::from_information(&info, &cap),
            None => cap,
        };
        let settle = self.cfg.settle.enabled && (self.resumed_from_session || self.after_fall);
        self.slam.set_tracked(pose);
        self.stand_moved = (0.0, 0.0);
        // Let the submap manager see the jump BEFORE inking: after a
        // cross-room carry the current submap's grid is still anchored at
        // the pre-carry pose, and a composite integrated there is silently
        // clipped to nothing — the travel rule opens (or re-anchors to) a
        // submap that actually covers where the robot now stands.
        if !self.frozen() {
            self.slam.tick(t_s);
        }
        // Settling: the confirming window inks nothing either.
        if settle {
            self.settling = Some((0, 0));
        } else {
            self.ink(pose, composite);
        }
        self.lost = false;
        self.suspect = 0;
        self.unjudged = 0;
        self.seed_agreed = 0;
        self.soft_seed = None;
        self.shadow = None;
        self.seed_from_map = false;
        self.pending_reloc = None;
        self.last_search = None;
        self.lost_windows = 0;
        self.hard_lost = false;
        self.after_fall = false;
        self.valleys.clear();
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

    /// Does the valley test refuse `pose` on this window? A refusal is
    /// noted. With `MAPLOC_VALLEY_CROSS=1`, a valley is not a refusal when an
    /// earlier window refused the same place — carried here by odometry —
    /// along a valley crossing this one by 45° or more: each window alone
    /// lets the pose slide along its own wall, the two together do not.
    /// Replayed (2026-09-25), the valley test refused 84 right poses in 91,
    /// most within 10 cm, one window at a time.
    fn valley_blocks(&mut self, grid: &mut OccupancyGrid, probe: &Scan, pose: Pose2, now: Pose2, notes: &mut Vec<Note>) -> bool {
        let Some(along) = crate::relocalize::valley_at(grid, probe, pose, &self.cfg.relocalize) else {
            return false;
        };
        notes.push(Note::RelocalizeAmbiguous { pose, along });
        if !valley_cross() {
            return true;
        }
        let (agree_m, agree_rad) = (self.cfg.relocalize_agree_m, self.cfg.relocalize_agree_rad);
        let crossed = self.valleys.iter().any(|&(p, then, a)| {
            let implied = compose(p, between(then, now));
            let same = (implied.0 - pose.0).hypot(implied.1 - pose.1) <= agree_m && wrap_pi(implied.2 - pose.2).abs() <= agree_rad;
            // Both directions are the map's (the slide is in map
            // coordinates): comparable as they are.
            same && (a.0 * along.1 - a.1 * along.0).abs() >= std::f32::consts::FRAC_1_SQRT_2
        });
        self.valleys.push((pose, now, along));
        if self.valleys.len() > VALLEYS_KEPT {
            self.valleys.remove(0);
        }
        !crossed
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
        // A candidate is not a trusted pose (see `judge_untrusted`).
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

    /// The judge for a pose that is NOT trusted: by endpoints. Along the
    /// ray (`relocalize::score_pose_rays`, `MAPLOC_RAY_JUDGE=1`, removed
    /// 2026-09-30) was measured off (2026-09-15): on the two boot
    /// recordings against run 71's map it turned one wrong fix right
    /// (1788872069) and one right fix wrong (1788929139, the kitchen alias
    /// at 57 s) — a true pose in a map with doubled walls has beams that
    /// "cross" a phantom wall, and the judge refuses the truth until an
    /// alias that crosses nothing comes along. The test is right in
    /// principle and needs a tolerance for the map's own noise before it
    /// can be the default.
    fn judge_untrusted(
        &self,
        grid: &mut OccupancyGrid,
        scan: &Scan,
        pose: Pose2,
        clamp_m: f32,
        wall_threshold_fp: i16,
        observed_fp: i16,
    ) -> crate::relocalize::PoseAgreement {
        score_pose(grid, scan, pose, clamp_m, wall_threshold_fp, observed_fp)
    }

    /// Localize only, and localised: the map is frozen once the pose is
    /// tracked on it — while lost or searching the mapper works as in
    /// mapping, so the boot search confirms the way it always did (a
    /// frozen search aliased 110° on explmap1, 2026-09-16).
    /// Freeze the map, or thaw it, at run time: frozen, nothing inks and
    /// the pose is corrected against the map as it is (see `frozen`).
    pub fn set_frozen(&mut self, on: bool) {
        self.cfg.frozen = on;
    }
    /// Whether the map is set frozen (whatever the pose).
    pub fn frozen_set(&self) -> bool {
        self.cfg.frozen
    }
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

/// Match `composite` (body frame, taken at `pose`) against `grid` from
/// `pose`, its Gaussian prior on `prior`; return the BODY-FRAME delta to
/// apply, the residual before and after, and the beams used — or `None`
/// when the match is not to be trusted. See [`TrackingConfig`].
/// The watchdog's rescue match (see `watchdog_rescue`): a scan match from
/// the carried pose with a prior wide enough for the drift the tracking's
/// own could not take (0.30 m, 0.20 rad, 0.02 m residual after — built for
/// the small corrections of every window), its result kept only within
/// [`RESCUE_MAX_M`] and [`RESCUE_MAX_RAD`]: a correction, not a search.
fn rescue_match(grid: &mut OccupancyGrid, composite: &Scan, pose: Pose2) -> Option<Pose2> {
    let probe = composite.decimated(512);
    let sm = ScanMatchConfig { prior_sigma_xy: 0.30, prior_sigma_yaw: 0.25, occ_threshold_fp: 150, ..ScanMatchConfig::default() };
    let r = match_scan(grid, &probe, pose, Some(pose), &sm);
    if !r.residual_m.is_finite() {
        return None;
    }
    let (dx, dy, dyaw) = (r.pose.0 - pose.0, r.pose.1 - pose.1, wrap_pi(r.pose.2 - pose.2));
    ((dx * dx + dy * dy).sqrt() <= RESCUE_MAX_M && dyaw.abs() <= RESCUE_MAX_RAD).then_some(r.pose)
}

/// The rescue's largest correction.
const RESCUE_MAX_M: f32 = 0.40;
const RESCUE_MAX_RAD: f32 = 0.30;

fn tracking_correction(
    grid: &mut OccupancyGrid,
    composite: &Scan,
    pose: Pose2,
    prior: Pose2,
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
    let r = match_scan(grid, &probe, pose, Some(prior), &sm);
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

/// A window judged at `pose` against `grid`: the information it carries
/// about the pose (see [`crate::uncertainty::window_information`]), its
/// per-beam residual there, and how many beams the map could judge.
/// Evaluated, not optimized: the pose is whatever the tracking made of it.
struct WindowAtPose {
    info: Option<crate::uncertainty::Cov3>,
    residual_m: f32,
    n_used: u32,
}

fn window_at_pose(grid: &mut OccupancyGrid, composite: &Scan, pose: Pose2, cfg: &crate::uncertainty::UncertaintyConfig) -> WindowAtPose {
    let probe = composite.decimated(512);
    let at = match_scan(
        grid,
        &probe,
        pose,
        None,
        // The watchdog's wall definition: the distance-field cache is shared.
        &ScanMatchConfig { max_iters: 0, occ_threshold_fp: 150, ..ScanMatchConfig::default() },
    );
    WindowAtPose {
        info: crate::uncertainty::window_information(&at.hessian, at.residual_m, at.n_beams_used, cfg),
        residual_m: at.residual_m,
        n_used: at.n_beams_used,
    }
}

fn window_information_at(
    grid: &mut OccupancyGrid,
    composite: &Scan,
    pose: Pose2,
    cfg: &crate::uncertainty::UncertaintyConfig,
) -> Option<crate::uncertainty::Cov3> {
    window_at_pose(grid, composite, pose, cfg).info
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

    /// A head's fan of beams from `pose` onto one long wall (y = 1.2, x in
    /// [-6, 6]) with a jamb against it (x = 0.5, y in [0.6, 1.2]), and nothing
    /// else within range. Along the wall the jamb alone says where the
    /// robot is: a few beams of the fan.
    fn long_wall_scan(pose: Pose2) -> Scan {
        let (mut angles, mut ranges) = (Vec::new(), Vec::new());
        for k in 0..96 {
            let a = -0.6 + k as f32 * (1.2 / 95.0);
            let (dx, dy) = ((pose.2 + a).cos(), (pose.2 + a).sin());
            let mut r = f32::INFINITY;
            if dy > 1e-3 {
                let t = (1.2 - pose.1) / dy;
                if (-6.0..=6.0).contains(&(pose.0 + t * dx)) {
                    r = t;
                }
            }
            if dx.abs() > 1e-3 {
                let t = (0.5 - pose.0) / dx;
                if t > 0.0 && (0.6..=1.2).contains(&(pose.1 + t * dy)) {
                    r = r.min(t);
                }
            }
            if r < 3.5 {
                angles.push(a);
                ranges.push(r);
            }
        }
        Scan::from_polar(&angles, &ranges, (0.0, 0.0), 1e-3)
    }

    /// A long stand in front of a long wall must not walk the pose along
    /// it. The windows of one stand are the same scene seen again, not new
    /// evidence: here each, matched alone, fits a little further along the
    /// wall than the last (the scan's view drifts 1 mm a second while
    /// odometry stands), the way the twin's duck, standing fourteen minutes
    /// before a single wall, had its pose walked 1.56 m along it by 132
    /// corrections of a centimetre each (casa_grande, 2026-10-01). Matched
    /// against the stand's own start, they cannot add up.
    #[test]
    fn a_long_stand_does_not_walk_the_pose_along_a_wall() {
        let mut mapper = Mapper::new(MapperConfig::default(), Slam::new(SlamConfig::default()));
        let mut notes = Vec::new();
        let up = std::f32::consts::FRAC_PI_2;
        // Map the wall from five stands, half a metre apart.
        let mut t = 0.0;
        for (i, x) in [-1.0_f32, -0.5, 0.0, 0.5, 1.0].into_iter().enumerate() {
            if i > 0 {
                t = walk_in(&mut mapper, t, (x - 0.5, 0.0, up), (x, 0.0, up), 2.0, &mut notes, long_wall_scan);
            }
            t = drive_in(&mut mapper, t, (x, 0.0, up), 8.0, &mut notes, long_wall_scan);
        }
        t = walk_in(&mut mapper, t, (1.0, 0.0, up), (0.0, 0.0, up), 2.0, &mut notes, long_wall_scan);
        mapper.set_frozen(true);
        // Stand five minutes at the origin.
        let at = mapper.slam().tracked();
        let (t0, mut next) = (t, t);
        let (mut corrections, mut worst) = (0, 0.0_f32);
        while t < t0 + 300.0 {
            notes.clear();
            mapper.observe(t, MapperSample { odom: (0.0, 0.0, up), moving: false, sitting: false, fallen: false }, &mut notes);
            if t >= next {
                mapper.frame(t, long_wall_scan((0.001 * (t - t0), 0.0, up)));
                next += 1.0 / 15.0;
            }
            corrections += notes.iter().filter(|n| matches!(n, Note::TrackingCorrected { .. })).count();
            let p = mapper.slam().tracked();
            worst = worst.max((p.0 - at.0).hypot(p.1 - at.1));
            t += 0.02;
        }
        assert!(mapper.tracking());
        assert!(corrections > 0, "the windows must have asked for something");
        assert!(worst < 0.10, "the stand walked the pose {worst:.3} m");
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

/// `MAPLOC_SHADOW=0`: no shadow map at boot.
fn shadow_enabled() -> bool {
    std::env::var("MAPLOC_SHADOW").map_or(true, |v| v != "0")
}

/// How often the shadow asks, how many answers in a row must agree, and
/// what an answer must be (the homecoming's adoption rule, see quack-nav's
/// `HomecomingConfig`, measured on 27 replayed wakes: 626 of 655 right
/// answers pass, none wrong). The wake bench's 24 wakes and x14's failed
/// one, replayed (2026-09-29): without the shadow 13 confirmed, median
/// 123 s; asking every 60 s for 3 answers, 14 (157 s); every 30 s for 3,
/// 21 (96 s); every 30 s for 2, 23 (67 s) — none wrong, the confirmed
/// poses within 0.18 m: the windows still confirm the seed.
/// `MAPLOC_SHADOW_EVERY_S` and `MAPLOC_SHADOW_ASKS` override them.
fn shadow_ask_every_s() -> f32 {
    std::env::var("MAPLOC_SHADOW_EVERY_S").ok().and_then(|v| v.parse().ok()).unwrap_or(30.0)
}
const SHADOW_MAX_SCORE: f32 = 0.16;
const SHADOW_MAX_MARGIN: f32 = 0.5;
/// A wider margin, when the answers keep saying the same: apartment's
/// duck, woken east of the stairwell, had its walk placed right sixteen
/// times in a row at margins of 0.50–0.84 and was never let in (w3,
/// 2026-09-30). On the 27 replayed wakes, runs of three or more agreeing
/// answers at 0.8 adopted 75 of 82 right sequences and nothing wrong;
/// runs of two let four fits of the other house in. Four in a row here,
/// the asks coming twice as often.
const SHADOW_WIDE_MARGIN: f32 = 0.8;
const SHADOW_WIDE_ASKS: u32 = 4;
const SHADOW_MIN_OVERLAP: f32 = 0.5;
const SHADOW_AGREE_M: f32 = 0.30;
fn shadow_asks() -> u32 {
    std::env::var("MAPLOC_SHADOW_ASKS").ok().and_then(|v| v.parse().ok()).unwrap_or(2)
}

/// The lost duck's own map of what it has walked since boot, kept beside
/// the search and asked, every 30 s (`MAPLOC_SHADOW_EVERY_S`), where it
/// sits in the saved map. Two agreeing answers at margin ≤ 0.5 (or four at
/// margin ≤ 0.8, the wide rule), once the duck has walked 0.5 m, make a
/// soft seed that two windows must still confirm. A
/// window of 200 beams in a corridor fits a dozen places; the walk's map
/// fits one. casa_arredata's duck, woken in its corridor, found nothing
/// in 240 s of windows, while its fresh map, once the search gave up,
/// was placed within 8 cm at the first ask (x14, 2026-09-29).
struct Shadow {
    fresh: Mapper,
    saved: OccupancyGrid,
    /// The shadow's pose at its first ask, and the farthest it has been
    /// from there: its answers count once the duck has walked
    /// `confirm_travel_m()` away, as the windows' do — a panorama on one
    /// spot is a map too, and sixteen kicks round a 15 cm circle had it
    /// confirm without leaving the spot (`a_wake_up_walks_before_it_believes`).
    origin: Option<Pose2>,
    chord: f32,
    /// The shadow's pose and the lost mapper's (odometry's) when the
    /// shadow saw its first sample: the seed is carried from there by
    /// odometry, not by the shadow's own tracking.
    starts: Option<(Pose2, Pose2)>,
    next_ask: Option<f32>,
    /// The last passing answer: its fit and the shadow's wall cells then.
    prev: Option<(Pose2, usize)>,
    agreed: u32,
    /// The run of agreeing answers at the wide margin.
    agreed_wide: u32,
}

impl Shadow {
    /// Ask when it is time; the fit of the shadow's frame on the saved map
    /// once `shadow_asks()` passing answers in a row agree.
    fn ask(&mut self, t_s: f32, notes: &mut Vec<Note>) -> Option<Pose2> {
        let here = self.fresh.slam().tracked();
        let origin = *self.origin.get_or_insert(here);
        self.chord = self.chord.max((here.0 - origin.0).hypot(here.1 - origin.1));
        let due = *self.next_ask.get_or_insert(t_s + shadow_ask_every_s());
        if t_s < due {
            return None;
        }
        self.next_ask = Some(t_s + shadow_ask_every_s());
        let live = self.fresh.slam().render()?;
        let cfg = crate::align::AlignConfig::default();
        let cells = crate::align::wall_cells(&live, cfg.certain_log);
        let found = crate::align::match_maps(&live, &mut self.saved, &cfg);
        let Some(best) = found.first() else {
            self.prev = None;
            self.agreed = 0;
            return None;
        };
        let margin = found.get(1).map_or(1.0, |n| best.score / n.score.max(1e-6));
        let fits = best.score <= SHADOW_MAX_SCORE && best.overlap >= SHADOW_MIN_OVERLAP;
        let wide = fits && margin <= SHADOW_WIDE_MARGIN;
        let tight = fits && margin <= SHADOW_MAX_MARGIN;
        let same = self.prev.is_some_and(|(p, c)| (p.0 - best.pose.0).hypot(p.1 - best.pose.1) <= SHADOW_AGREE_M && cells >= c);
        self.agreed = if tight && same { self.agreed + 1 } else if tight { 1 } else { 0 };
        self.agreed_wide = if wide && same { self.agreed_wide + 1 } else if wide { 1 } else { 0 };
        self.prev = wide.then_some((best.pose, cells));
        notes.push(Note::ShadowAsked { fit: best.pose, score: best.score, margin, overlap: best.overlap, cells, agreed: self.agreed.max(self.agreed_wide) });
        if (self.agreed >= shadow_asks() || self.agreed_wide >= SHADOW_WIDE_ASKS) && self.chord >= confirm_travel_m() {
            self.agreed = 0;
            self.agreed_wide = 0;
            self.prev = None;
            return Some(best.pose);
        }
        None
    }
}
