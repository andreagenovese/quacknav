//! The gait ledger: what this duck learns of its own walk, kept between
//! runs.
//!
//! The walk's numbers the navigation plans with — the forward speed, the
//! yaw a unit of command turns, the trim that cancels a straight step's
//! veer, the turn in place's rate and its coast past the stop — are coded
//! priors, one set per daemon line and profile, measured on the MuJoCo
//! twin (`quack_duck::gait`, `quack_duck::daemon`). A real duck, and every
//! walking policy Pollen ships after the measurement, walks its own way.
//! Every leg and turn the explorer makes is a measurement of that way:
//! the command, the map's pose and odometry before and after (what the
//! calibration reads off the traces, `quack-rl`'s `rl_calib`, number by
//! number). This ledger takes the same measurements as they happen, keeps
//! the last [`KEEP`] of each, and lays their median over the prior once
//! [`MIN_SAMPLES`] are in — bounded about the prior, so one bad session
//! cannot take the duck far — through `quack_duck::gait::learn`, which
//! every reader of the numbers takes without knowing. The samples are
//! written beside the map (`ledger_path`, `/var/lib/quack-nav/gait-ledger.json`),
//! per daemon line and profile, so the next run starts where this one
//! got, and a daemon of another line starts from its own priors.
//!
//! What it learns and what takes it:
//!
//! | number | from | taken by |
//! |---|---|---|
//! | `m_per_s` | a free forward step's distance by the map's pose over its time | the planner's strides, the stands' spacing, the guards' reach |
//! | `yaw_trim` | a straight step's yaw rate by odometry, as the trim that would cancel it | every walking command sent (`GaitConfig::yaw`) |
//! | `yaw_rate_per_unit` | a curving leg of [`LEG_MIN_S`] or more: its yaw rate per unit of command | the stick's steering gain, the guards' arcs |
//! | `coast_rad` | the yaw past a turn in place's close, read off the map at the next leg | how far short of the goal a turn in place stops |
//! | `turn_rate_rad_s` | a turn in place's rate over its own time | the time a turn in place gets before it is given up ([`turn_budget_s`]); the turns are closed on odometry |
//! | `settle_s` | after the applied twist is gone, the time until odometry stands still ([`SettleWatch`], on the body's state stream) | how long a stand is held before the mapper's still window, and when a stop counts as still (`quack_duck::gait::Stillness`) |
//! | `ypu_pulse_left`, `ypu_pulse_right` | a curving pulse of [`PULSE_MIN_S`] or more at \|vyaw\| 0.3–0.6: its yaw per unit of steering sent, by side (the trim cancels the veer: neither counted) | the walk's left and right yaw gains, as the mean of the two sides over each ([`Ledger::gains`]): a side that turns more per unit is sent less |
//!
//! The yaw per unit is not learned from the stick's 0.6 s pulses, which
//! are nearly all the curving legs there are, on purpose: a pulse's yaw
//! per unit is not one number on the duck's walk. It falls with the size
//! of the command — on the MuJoCo twin (AB21, 2026-10-28, 447 pulses of
//! frozen main's stick) 0.99 rad/s per unit at |vyaw| 0.3, 0.82 at 0.4,
//! 0.76 at 0.7 — and the stick's steering gain is 1 / (yaw per unit ×
//! pulse): learned as one number and laid under that gain it closes a
//! loop (a lower number, a higher gain, bigger commands, a lower number
//! again) that ran the value to its bound (0.82 then 0.63) and doubled
//! the stick's curving pulses (912 against 447 in casa_ingombra, 17 s a
//! journey slower). A gain curve by command size, or a stick that closes
//! its heading on odometry rather than on a gain, is the way; until then
//! the number stays at its prior, which the twin's calibration fitted
//! on legs of a second or more. The pulses do give the *ratio* of the
//! two sides: at the same commands the walk turns more to one side than
//! the other (the twin, AB22's frozen main: 0.72 rad/s per unit to the
//! left against 0.78 to the right in casa_ingombra, 0.71 against 0.91
//! in casa_arredata; the calibration's pulse gains 0.75 and 0.84), and
//! the two sides' medians over pulses of the same sizes (|vyaw| 0.3–0.6,
//! the clamp left out) give each side's gain about their mean, bounded
//! 0.75–1.25. The gains move the commands by a few percent, so the
//! samples slide a little along the curve: a loop too, but one that
//! shrinks (a 10 % command moves the number 4 %, the gain 4 % of that).
//!
//! A step counts only on a pose the map vouches for at both ends, nothing
//! within 0.35 m ahead, and the body moved; its yaw only when no turn in
//! place ended within 2 s before it (the turn's coast would read as the
//! step's veer); a turn only when it turned.
//! `QK_GAIT_LEDGER=0` leaves the priors alone: nothing learned, nothing
//! written. `robot.gait` reports it and forgets it on request
//! (`crate::tools`); `robot.map_status` carries the report as `gait`.
//! The bench and the paper twin, which never init it, run on the priors
//! as before.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::rlnav::trace::Snapshot;

/// Samples kept per number: the median of the last ones is the value. A
/// walk that changed (another policy on the same line) shows through in
/// about half of them.
pub const KEEP: usize = 32;
/// Samples before a learned value is laid over the prior.
pub const MIN_SAMPLES: usize = 5;
/// ... and for the trim, whose samples are the fewest (a free straight
/// step not on the heels of a turn: 14 in a 15-minute exploration of the
/// twin's casa_arredata) and the noisiest (a 0.6 s step's yaw by
/// odometry).
pub const MIN_SAMPLES_TRIM: usize = 10;
/// The file is rewritten no more often than this while samples come.
const WRITE_EVERY: Duration = Duration::from_secs(30);
/// Nothing ahead nearer than this for a step to count as the gait's.
const FREE_AHEAD_M: f64 = 0.35;
/// A step that starts within this of a turn in place's end gives no yaw
/// sample: the turn's coast (13–17° on the twin) falls into the step and
/// reads as a veer (the twin, 2026-10-27: the trim at its bound after
/// six such steps).
const AFTER_TURN_S: f64 = 2.0;
/// A curving leg this long or longer gives a yaw per unit sample: the
/// stick's 0.6 s pulses never (the module: their yaw per unit is not one
/// number).
pub const LEG_MIN_S: f64 = 0.8;
/// A curving pulse this long or longer, at |vyaw| up to [`PULSE_MAX_VYAW`]
/// (the stick's clamp of 0.7 left out: the flat end of the curve), gives
/// its side's yaw per unit sent (the module: the sides' ratio).
pub const PULSE_MIN_S: f64 = 0.5;
pub const PULSE_MAX_VYAW: f64 = 0.6;
/// A side's gain about the two sides' mean lies within this.
const GAIN_BOUNDS: (f64, f64) = (0.75, 1.25);
/// A trim in force past this (rad/s) is a veer the sides cannot be read
/// apart from: no side samples. The twin's is 0.02–0.04.
const SIDE_MAX_TRIM: f64 = 0.06;
/// Samples before a side's number is laid in: a pulse's yaw is the
/// noisiest sample there is (the calibration's pulse gain spread, 0.5 of
/// the gain), and the sides differ by a tenth.
pub const MIN_SAMPLES_SIDE: usize = 16;
/// Odometry under these rates, from one state to the next, stands still.
const SETTLE_STILL_M_S: f64 = 0.02;
const SETTLE_STILL_RAD_S: f64 = 0.05;
/// A body not still this long after its twist is gone is moved by
/// something else: no settle sample.
const SETTLE_MAX_S: f64 = 2.0;
/// The slowest turn in place measured on any side before the ledger
/// (30°/s): the rate a turn's time budget assumes until one is learned.
const TURN_RATE_FLOOR_RAD_S: f64 = 0.5;

/// The numbers the ledger keeps, by name in the file, with their bounds
/// about the prior: `(low, high)` as factors of the prior, or absolute
/// where the prior may be 0 (the trim) or is a floor (the coast).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Number {
    MPerS,
    YawRatePerUnit,
    YawTrim,
    CoastRad,
    TurnRateRadS,
    SettleS,
    YpuPulseLeft,
    YpuPulseRight,
}

impl Number {
    pub const ALL: [Number; 8] = [
        Number::MPerS,
        Number::YawRatePerUnit,
        Number::YawTrim,
        Number::CoastRad,
        Number::TurnRateRadS,
        Number::SettleS,
        Number::YpuPulseLeft,
        Number::YpuPulseRight,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Number::MPerS => "m_per_s",
            Number::YawRatePerUnit => "yaw_rate_per_unit",
            Number::YawTrim => "yaw_trim",
            Number::CoastRad => "coast_rad",
            Number::TurnRateRadS => "turn_rate_rad_s",
            Number::SettleS => "settle_s",
            Number::YpuPulseLeft => "ypu_pulse_left",
            Number::YpuPulseRight => "ypu_pulse_right",
        }
    }

    fn unit(self) -> &'static str {
        match self {
            Number::MPerS => "m/s at vx 0.3",
            Number::YawRatePerUnit => "rad/s per unit",
            Number::YawTrim => "rad/s",
            Number::CoastRad => "rad",
            Number::TurnRateRadS => "rad/s",
            Number::SettleS => "s",
            Number::YpuPulseLeft | Number::YpuPulseRight => "rad/s per unit of steering, over a pulse",
        }
    }

    /// The coded prior of this walk, and the trim the file set.
    fn prior(self, file_trim: f64) -> f64 {
        let n = quack_duck::gait::priors();
        match self {
            Number::MPerS => n.m_per_s,
            Number::YawRatePerUnit => n.yaw_rate_per_unit,
            Number::YawTrim => file_trim,
            Number::CoastRad => n.coast_prior_rad,
            Number::TurnRateRadS => n.turn_in_place_rad_s,
            Number::SettleS => n.settle_s,
            // A pulse turns about 0.85 of the steady rate's yaw (the
            // calibration's pulse gain prior).
            Number::YpuPulseLeft | Number::YpuPulseRight => 0.85 * n.yaw_rate_per_unit,
        }
    }

    /// Samples before this number's value is laid over the prior.
    fn min_samples(self) -> usize {
        match self {
            Number::YawTrim => MIN_SAMPLES_TRIM,
            Number::YpuPulseLeft | Number::YpuPulseRight => MIN_SAMPLES_SIDE,
            _ => MIN_SAMPLES,
        }
    }

    /// Where a learned value may lie, about the prior.
    fn bounds(self, prior: f64) -> (f64, f64) {
        match self {
            Number::MPerS => (0.5 * prior, 1.6 * prior),
            Number::YawRatePerUnit => (0.5 * prior, 1.8 * prior),
            Number::YawTrim => (-0.25, 0.25),
            Number::CoastRad => (0.0, 0.7),
            Number::TurnRateRadS => (0.4 * prior, 2.0 * prior),
            Number::SettleS => (0.0, 1.5),
            Number::YpuPulseLeft | Number::YpuPulseRight => (0.3 * prior, 1.8 * prior),
        }
    }
}

/// The settle, watched on the body's state stream: from the state whose
/// applied twist is gone to the first whose odometry stands still
/// ([`SETTLE_STILL_M_S`], [`SETTLE_STILL_RAD_S`] from the state before).
/// A twist back before that, or [`SETTLE_MAX_S`] gone by, and there is no
/// sample. Pure: the cliff guard's body lane feeds it.
#[derive(Debug, Default)]
pub struct SettleWatch {
    was_walking: bool,
    stopped_at_ns: Option<u64>,
    last: Option<(u64, (f64, f64, f64))>,
}

impl SettleWatch {
    /// One state: its clock, whether the applied twist is on, its odometry
    /// (x, y, yaw) when the daemon gives one. The settle in seconds when
    /// this state closes one.
    pub fn tick(&mut self, t_ns: u64, walking: bool, odom: Option<(f64, f64, f64)>) -> Option<f64> {
        let mut settled = None;
        if walking {
            self.stopped_at_ns = None;
        } else if self.was_walking {
            self.stopped_at_ns = Some(t_ns);
        } else if let (Some(t0), Some((t1, o1)), Some(o2)) = (self.stopped_at_ns, self.last, odom) {
            let dt = t_ns.saturating_sub(t1) as f64 * 1e-9;
            if dt > 0.0 {
                let v = (o2.0 - o1.0).hypot(o2.1 - o1.1) / dt;
                let w = wrap(o2.2 - o1.2).abs() / dt;
                let since = t_ns.saturating_sub(t0) as f64 * 1e-9;
                if v < SETTLE_STILL_M_S && w < SETTLE_STILL_RAD_S {
                    settled = Some(since);
                    self.stopped_at_ns = None;
                } else if since > SETTLE_MAX_S {
                    self.stopped_at_ns = None;
                }
            }
        }
        self.was_walking = walking;
        if let Some(o) = odom {
            self.last = Some((t_ns, o));
        }
        settled
    }
}

/// One number's samples and what they say.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Entry {
    prior: f64,
    unit: String,
    /// The last [`KEEP`] samples, oldest first.
    samples: Vec<f64>,
    /// Samples ever taken for this walk.
    seen: u64,
    /// The median of the samples, bounded — `None` under [`MIN_SAMPLES`].
    value: Option<f64>,
}

impl Entry {
    fn push(&mut self, number: Number, sample: f64) {
        self.samples.push(sample);
        if self.samples.len() > KEEP {
            self.samples.remove(0);
        }
        self.seen += 1;
        self.value = (self.samples.len() >= number.min_samples()).then(|| {
            let (lo, hi) = number.bounds(self.prior);
            median(&self.samples).clamp(lo, hi)
        });
    }
}

/// One walk's ledger: a daemon line and a profile.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Walk {
    numbers: BTreeMap<String, Entry>,
    /// Legs and turns that gave a sample.
    legs: u64,
    updated_unix: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileBody {
    version: u32,
    walks: BTreeMap<String, Walk>,
}

impl Default for FileBody {
    fn default() -> Self {
        Self { version: 1, walks: BTreeMap::new() }
    }
}

/// The ledger of this process's walk (see the module).
#[derive(Debug)]
pub struct Ledger {
    path: Option<PathBuf>,
    /// `<daemon line>/<profile>`.
    key: String,
    /// `[gait] yaw_trim` as the file set it: the trim the straight steps
    /// were walked with until one is learned, and the trim's prior.
    file_trim: f64,
    /// `[gait] yaw_gain_left`, `yaw_gain_right` as the file set them: the
    /// gains the curving pulses were sent with until a pair is learned.
    file_gains: (f64, f64),
    body: FileBody,
    dirty: bool,
    written: Option<Instant>,
    /// When the last turn in place ended (the body's clock).
    turned_at: Option<Instant>,
}

fn median(v: &[f64]) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = s.len();
    if n == 0 {
        return 0.0;
    }
    if n % 2 == 1 { s[n / 2] } else { (s[n / 2 - 1] + s[n / 2]) / 2.0 }
}

fn wrap(a: f64) -> f64 {
    (a + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI
}

fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl Ledger {
    /// A ledger for this process's walk, loaded from `path` when it holds
    /// one (a file that does not read is a fresh ledger, said in the log).
    pub fn open(path: Option<PathBuf>, file_trim: f64, file_gains: (f64, f64)) -> Self {
        let key = format!("{}/{}", quack_duck::daemon::active().name(), quack_duck::gait::active().name());
        let body = match &path {
            Some(p) => match std::fs::read_to_string(p) {
                Ok(text) => match serde_json::from_str::<FileBody>(&text) {
                    Ok(b) => b,
                    Err(e) => {
                        tracing::warn!(path = %p.display(), error = %e, "gait ledger: the file does not read; starting afresh");
                        FileBody::default()
                    }
                },
                Err(_) => FileBody::default(),
            },
            None => FileBody::default(),
        };
        let mut l = Self { path, key, file_trim, file_gains, body, dirty: false, written: None, turned_at: None };
        l.refresh_priors();
        l
    }

    /// A ledger kept in memory only (the tests, a check on the bench).
    pub fn in_memory(file_trim: f64) -> Self {
        Self::open(None, file_trim, (1.0, 1.0))
    }

    /// The priors are the code's, not the file's: a build with other
    /// priors re-bounds what was learned.
    fn refresh_priors(&mut self) {
        let w = self.body.walks.entry(self.key.clone()).or_default();
        for n in Number::ALL {
            let e = w.numbers.entry(n.name().into()).or_default();
            e.prior = n.prior(self.file_trim);
            e.unit = n.unit().into();
            e.value = (e.samples.len() >= n.min_samples()).then(|| {
                let (lo, hi) = n.bounds(e.prior);
                median(&e.samples).clamp(lo, hi)
            });
        }
    }

    fn walk(&self) -> &Walk {
        self.body.walks.get(&self.key).expect("the walk's entry is made at open")
    }

    fn walk_mut(&mut self) -> &mut Walk {
        self.body.walks.get_mut(&self.key).expect("the walk's entry is made at open")
    }

    /// The learned value of a number, when there is one.
    pub fn value(&self, n: Number) -> Option<f64> {
        self.walk().numbers.get(n.name()).and_then(|e| e.value)
    }

    /// What is laid over the coded numbers.
    pub fn learned(&self) -> quack_duck::gait::Learned {
        let gains = self.gains();
        quack_duck::gait::Learned {
            m_per_s: self.value(Number::MPerS),
            yaw_rate_per_unit: self.value(Number::YawRatePerUnit),
            yaw_trim: self.value(Number::YawTrim),
            coast_rad: self.value(Number::CoastRad),
            settle_s: self.value(Number::SettleS),
            yaw_gain_left: gains.map(|g| g.0),
            yaw_gain_right: gains.map(|g| g.1),
        }
    }

    /// The walk's left and right yaw gains, once both sides' yaw per unit
    /// over a pulse are learned: each side's gain is the two sides' mean
    /// over its own, within [`GAIN_BOUNDS`] — the side that turns more per
    /// unit is sent less, and the two together turn as the one number
    /// says.
    pub fn gains(&self) -> Option<(f64, f64)> {
        let (l, r) = (self.value(Number::YpuPulseLeft)?, self.value(Number::YpuPulseRight)?);
        let m = 0.5 * (l + r);
        let (lo, hi) = GAIN_BOUNDS;
        Some(((m / l).clamp(lo, hi), (m / r).clamp(lo, hi)))
    }

    /// The trim the straight steps are walked with now.
    fn trim_in_force(&self) -> f64 {
        self.value(Number::YawTrim).unwrap_or(self.file_trim)
    }

    /// The gain a yaw of this sign is sent with now.
    fn gain_in_force(&self, vyaw: f64) -> f64 {
        let (l, r) = self.gains().unwrap_or(self.file_gains);
        if vyaw > 0.0 { l } else { r }
    }

    fn take(&mut self, n: Number, sample: f64) {
        if !sample.is_finite() {
            return;
        }
        let w = self.walk_mut();
        w.legs += 1;
        w.updated_unix = unix_now();
        let e = w.numbers.entry(n.name().into()).or_default();
        e.push(n, sample);
        let (value, seen) = (e.value, e.seen);
        self.dirty = true;
        tracing::debug!(number = n.name(), sample = format!("{sample:.3}"), value = ?value.map(|v| format!("{v:.3}")), seen, "gait ledger: a sample");
    }

    /// A forward step the body walked freely: `vx` and `vyaw` as the
    /// explorer asked (the trim not in them), `secs` walked, `near_ahead_m`
    /// the nearest thing the sensor saw ahead meanwhile (infinite for
    /// none). Speed by the map's pose; yaw by odometry where it is.
    /// Returns which numbers took a sample.
    pub fn observe_step(&mut self, before: &Snapshot, after: &Snapshot, vx: f64, vyaw: f64, secs: f64, near_ahead_m: f64) -> Vec<Number> {
        let mut took = Vec::new();
        if vx <= 0.05 || secs < 0.2 || near_ahead_m <= FREE_AHEAD_M || !before.trusted || !after.trusted {
            return took;
        }
        let (Some(p0), Some(p1)) = (before.pose, after.pose) else { return took };
        let walked = (p1.0 - p0.0).hypot(p1.1 - p0.1);
        let speed = walked / secs * 0.3 / vx;
        if speed > 0.02 && speed < 0.6 {
            self.take(Number::MPerS, speed);
            took.push(Number::MPerS);
        } else {
            // The body did not move, or the pose jumped: not the gait.
            return took;
        }
        // The yaw, not on the heels of a turn in place: its coast would
        // read as the step's.
        if self.turned_at.is_some_and(|t| before.at.saturating_duration_since(t).as_secs_f64() < AFTER_TURN_S) {
            return took;
        }
        let dyaw = match (before.odom, after.odom) {
            (Some(a), Some(b)) => wrap(b.2 - a.2),
            _ => wrap(p1.2 - p0.2),
        };
        if vyaw.abs() < 0.1 {
            // The veer, as the trim that would cancel it: the yaw the
            // body turned on its own at the trim it was sent with.
            let ypu = quack_duck::gait::numbers().yaw_rate_per_unit;
            let trim = self.trim_in_force() - dyaw / secs / ypu;
            self.take(Number::YawTrim, trim);
            took.push(Number::YawTrim);
        } else if vyaw.abs() > 0.3 {
            if secs >= PULSE_MIN_S && vyaw.abs() <= PULSE_MAX_VYAW {
                // This side's yaw per unit of steering sent over a pulse:
                // the two sides' ratio is the gains (the module). The trim
                // is not steering — it cancels the veer, and the veer is
                // in the yaw — so neither is counted: a trim in the
                // denominator would read the veer as a difference between
                // the sides (the bench's perturbed walk, trim −0.13: 0.99
                // against 0.55 where the walk's sides are 0.75 and 0.84).
                // What the trim does not cancel inside a pulse (the gait
                // turning on scales the trim, the veer is whole) is read
                // as a side difference too, by about a fifth of the veer
                // over the steering: a walk that veers more than
                // [`SIDE_MAX_TRIM`] gives no side sample — its sides
                // cannot be read apart from its veer.
                let steer = vyaw * self.gain_in_force(vyaw);
                let ypu = dyaw / secs / steer;
                if steer.abs() > 0.2 && ypu > 0.0 && self.trim_in_force().abs() <= SIDE_MAX_TRIM {
                    let side = if vyaw > 0.0 { Number::YpuPulseLeft } else { Number::YpuPulseRight };
                    self.take(side, ypu);
                    took.push(side);
                }
            }
            if secs > LEG_MIN_S {
                // A leg long enough for the gait to be turning for most of
                // it; never the stick's 0.6 s pulses (the module).
                let sent = vyaw + self.trim_in_force();
                if sent.abs() > 0.2 {
                    let ypu = dyaw / secs / sent;
                    if ypu > 0.0 {
                        self.take(Number::YawRatePerUnit, ypu);
                        took.push(Number::YawRatePerUnit);
                    }
                }
            }
        }
        took
    }

    /// A settle the body's state stream closed ([`SettleWatch`]).
    pub fn observe_settle(&mut self, secs: f64) {
        if (0.0..=SETTLE_MAX_S).contains(&secs) {
            self.take(Number::SettleS, secs);
        }
    }

    /// A turn in place that ended at `at` (the body's clock): how far it
    /// turned toward where it was asked, by odometry, over the time it
    /// was commanded. The steps right after it give no yaw sample.
    pub fn observe_turn(&mut self, at: Instant, turned_rad: f64, secs: f64) -> bool {
        self.turned_at = Some(at);
        if turned_rad < 0.25 || secs < 0.1 {
            return false;
        }
        self.take(Number::TurnRateRadS, turned_rad / secs);
        true
    }

    /// What the body turned past a turn in place's close, read off the
    /// map at the next leg (`explore::stick`'s probe, already filtered).
    /// Returns the coast in force after it.
    pub fn observe_coast(&mut self, extra_rad: f64) -> f64 {
        self.take(Number::CoastRad, extra_rad.max(0.0));
        self.coast_rad()
    }

    /// The coast the turns stop short by: learned, else the prior.
    pub fn coast_rad(&self) -> f64 {
        self.value(Number::CoastRad).unwrap_or_else(|| Number::CoastRad.prior(self.file_trim))
    }

    /// The seconds a turn in place of `want` radians gets before it is
    /// given up as not turning: twice its time at the rate this duck's
    /// turns were measured at, and a second — at the slowest rate ever
    /// measured on any side ([`TURN_RATE_FLOOR_RAD_S`]) until one is
    /// learned, as before the ledger.
    pub fn turn_budget_s(&self, want: f64) -> f64 {
        let rate = self.value(Number::TurnRateRadS).unwrap_or(TURN_RATE_FLOOR_RAD_S);
        2.0 * want / rate + 1.0
    }

    /// Forget this walk's samples: the priors stand again.
    pub fn reset(&mut self) {
        let key = self.key.clone();
        self.body.walks.remove(&key);
        self.refresh_priors();
        self.dirty = true;
        tracing::info!(walk = %key, "gait ledger: forgotten; the priors stand");
    }

    /// The ledger as `robot.map_status` and `robot.gait` tell it.
    pub fn report(&self) -> Value {
        let w = self.walk();
        let numbers: serde_json::Map<String, Value> = Number::ALL
            .iter()
            .map(|n| {
                let e = w.numbers.get(n.name()).cloned().unwrap_or_default();
                (
                    n.name().to_string(),
                    json!({
                        "prior": round4(e.prior),
                        "learned": e.value.map(round4),
                        "samples": e.samples.len(),
                        "seen": e.seen,
                        "unit": e.unit,
                    }),
                )
            })
            .collect();
        json!({
            "on": true,
            "walk": self.key,
            "path": self.path.as_ref().map(|p| p.display().to_string()),
            "legs": w.legs,
            "updated_unix": w.updated_unix,
            "min_samples": {"default": MIN_SAMPLES, "yaw_trim": MIN_SAMPLES_TRIM, "sides": MIN_SAMPLES_SIDE},
            "numbers": numbers,
        })
    }

    /// Write the file when something changed: at once with `force`, else
    /// no more often than [`WRITE_EVERY`].
    pub fn write(&mut self, force: bool) {
        let Some(path) = &self.path else { return };
        if !self.dirty || (!force && self.written.is_some_and(|w| w.elapsed() < WRITE_EVERY)) {
            return;
        }
        let text = match serde_json::to_string_pretty(&self.body) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(error = %e, "gait ledger: cannot serialize");
                return;
            }
        };
        let tmp = path.with_extension("json.tmp");
        let done = std::fs::write(&tmp, text).and_then(|_| std::fs::rename(&tmp, path));
        match done {
            Ok(()) => {
                self.dirty = false;
                self.written = Some(Instant::now());
            }
            Err(e) => tracing::warn!(path = %path.display(), error = %e, "gait ledger: cannot write"),
        }
    }
}

fn round4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

// --- the process's ledger -------------------------------------------------

static LEDGER: OnceLock<Option<Mutex<Ledger>>> = OnceLock::new();

/// `QK_GAIT_LEDGER=0`: the walk's numbers stay the coded priors — nothing
/// learned from the duck's own legs, nothing written beside the map. On by
/// default (the ledger, see `quack_nav::ledger`).
fn ledger_on() -> bool {
    quack_duck::env::qk("GAIT_LEDGER").is_none_or(|v| v != "0")
}

/// Open the process's ledger, once: quack-navd at start, after the
/// daemon line and the profile are set. `path` is the file beside the map
/// (`None` keeps it in memory); `file_trim` and `file_gains` are
/// `[gait] yaw_trim`, `yaw_gain_left` and `yaw_gain_right` as the config
/// set them. What the file holds is laid over the numbers at once.
pub fn init(path: Option<PathBuf>, file_trim: f64, file_gains: (f64, f64)) {
    let _ = LEDGER.get_or_init(|| {
        if !ledger_on() {
            tracing::info!("gait ledger: off (QK_GAIT_LEDGER=0); the coded numbers stand");
            return None;
        }
        let l = Ledger::open(path, file_trim, file_gains);
        let learned = l.learned();
        quack_duck::gait::learn(learned);
        tracing::info!(walk = %l.key, path = ?l.path.as_ref().map(|p| p.display().to_string()), learned = ?learned, "gait ledger: open");
        Some(Mutex::new(l))
    });
}

fn with<R>(f: impl FnOnce(&mut Ledger) -> R) -> Option<R> {
    let l = LEDGER.get()?.as_ref()?;
    let mut g = l.lock().unwrap_or_else(|e| e.into_inner());
    let r = f(&mut g);
    quack_duck::gait::learn(g.learned());
    g.write(false);
    Some(r)
}

/// Whether the process's ledger is open and learning.
pub fn on() -> bool {
    LEDGER.get().is_some_and(Option::is_some)
}

/// The nearest thing the sensor saw ahead (|bearing| < 0.3 rad) in the
/// frames judged between two snapshots: infinite for none.
fn near_ahead(robot: &dyn crate::explore::Body, before: &Snapshot, after: &Snapshot) -> f64 {
    robot
        .cliff()
        .map(|c| {
            c.recent
                .iter()
                .filter(|f| f.at >= before.at && f.at <= after.at)
                .flat_map(|f| f.obstacles.iter().filter(|o| o.bearing.abs() < 0.3).map(|o| o.range_m))
                .fold(f64::INFINITY, f64::min)
        })
        .unwrap_or(f64::INFINITY)
}

/// A forward step just walked: `before` taken before it, the body's
/// state now its end. `vx`, `vyaw` as asked, `secs` walked.
pub fn step(robot: &dyn crate::explore::Body, before: &Snapshot, vx: f64, vyaw: f64, secs: f64) {
    if !on() {
        return;
    }
    let after = Snapshot::take(robot);
    let near = near_ahead(robot, before, &after);
    let took = with(|l| l.observe_step(before, &after, vx, vyaw, secs, near)).unwrap_or_default();
    if !took.is_empty() {
        tracing::debug!(took = ?took.iter().map(|n| n.name()).collect::<Vec<_>>(), "gait ledger: a step");
    }
}

/// A turn in place just closed at `at` (the body's clock): how far it
/// turned, over how long it was commanded.
pub fn turn(at: Instant, turned_rad: f64, secs: f64) {
    let _ = with(|l| l.observe_turn(at, turned_rad, secs));
}

/// What a turn coasted past its close: the coast in force after it,
/// `None` when the ledger is off (the caller keeps its own).
pub fn coast(extra_rad: f64) -> Option<f64> {
    with(|l| l.observe_coast(extra_rad))
}

/// A settle the body's state stream closed: the seconds from the applied
/// twist going to the odometry standing still ([`SettleWatch`]).
pub fn settle(secs: f64) {
    let _ = with(|l| l.observe_settle(secs));
}

/// The time a turn in place of `want` radians gets ([`Ledger::turn_budget_s`]):
/// on the learned rate once there is one, on the slowest measured side
/// until then — and with the ledger off.
pub fn turn_budget_s(want: f64) -> f64 {
    LEDGER
        .get()
        .and_then(Option::as_ref)
        .map(|l| l.lock().unwrap_or_else(|e| e.into_inner()).turn_budget_s(want))
        .unwrap_or(2.0 * want / TURN_RATE_FLOOR_RAD_S + 1.0)
}

/// The report, `{"on": false}` when the ledger is off.
pub fn report() -> Value {
    with(|l| l.report()).unwrap_or_else(|| json!({"on": false}))
}

/// Forget this walk's samples; the priors stand. `false` when off.
pub fn reset() -> bool {
    with(|l| {
        l.reset();
        l.write(true);
    })
    .is_some()
}

/// Write what changed, now (a job's end, the daemon's stop).
pub fn flush() {
    let _ = with(|l| l.write(true));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn snap(at: Instant, pose: (f64, f64, f64), odom: (f64, f64, f64), trusted: bool) -> Snapshot {
        Snapshot { at, pose: Some(pose), odom: Some(odom), trusted }
    }

    fn walk_straight(l: &mut Ledger, n: usize, speed_at_03: f64, veer_rad_s: f64) {
        let t0 = Instant::now();
        for i in 0..n {
            let secs = 0.6;
            let d = speed_at_03 * secs;
            let yaw = veer_rad_s * secs;
            let before = snap(t0, (i as f64, 0.0, 0.0), (i as f64, 0.0, 0.0), true);
            let after = snap(t0 + Duration::from_millis(600), (i as f64 + d, 0.0, yaw), (i as f64 + d, 0.0, yaw), true);
            l.observe_step(&before, &after, 0.3, 0.0, secs, f64::INFINITY);
        }
    }

    #[test]
    fn a_walk_is_learned_from_its_free_steps_after_enough_of_them() {
        let mut l = Ledger::in_memory(0.0);
        let prior = quack_duck::gait::priors().m_per_s;
        walk_straight(&mut l, MIN_SAMPLES - 1, prior * 1.2, 0.0);
        assert_eq!(l.value(Number::MPerS), None, "under the minimum the prior stands");
        walk_straight(&mut l, 1, prior * 1.2, 0.0);
        let v = l.value(Number::MPerS).unwrap();
        assert!((v - prior * 1.2).abs() < 1e-6, "{v} vs {}", prior * 1.2);
        assert_eq!(l.learned().m_per_s, Some(v));
    }

    #[test]
    fn the_veer_becomes_the_trim_that_cancels_it() {
        let mut l = Ledger::in_memory(0.02);
        let ypu = quack_duck::gait::priors().yaw_rate_per_unit;
        // The body drifts left 0.1 rad/s on a straight step sent with trim 0.02.
        walk_straight(&mut l, MIN_SAMPLES_TRIM - 1, 0.1, 0.1);
        assert_eq!(l.value(Number::YawTrim), None, "the trim waits for more samples than the rest");
        walk_straight(&mut l, 1, 0.1, 0.1);
        let trim = l.value(Number::YawTrim).unwrap();
        assert!((trim - (0.02 - 0.1 / ypu)).abs() < 1e-6, "{trim}");
    }

    #[test]
    fn a_bad_step_is_not_the_gait() {
        let mut l = Ledger::in_memory(0.0);
        let t0 = Instant::now();
        let before = snap(t0, (0.0, 0.0, 0.0), (0.0, 0.0, 0.0), true);
        let after = snap(t0 + Duration::from_millis(600), (0.06, 0.0, 0.0), (0.06, 0.0, 0.0), true);
        // Something 0.2 m ahead: a bump, not the gait.
        assert!(l.observe_step(&before, &after, 0.3, 0.0, 0.6, 0.2).is_empty());
        // The pose not trusted at one end.
        let lost = snap(t0, (0.0, 0.0, 0.0), (0.0, 0.0, 0.0), false);
        assert!(l.observe_step(&lost, &after, 0.3, 0.0, 0.6, f64::INFINITY).is_empty());
        // The body did not move (0.5 cm in 0.6 s).
        let still = snap(t0 + Duration::from_millis(600), (0.005, 0.0, 0.0), (0.005, 0.0, 0.0), true);
        assert!(l.observe_step(&before, &still, 0.3, 0.0, 0.6, f64::INFINITY).is_empty());
        // A turn, not a step.
        assert!(l.observe_step(&before, &after, 0.0, 1.0, 0.6, f64::INFINITY).is_empty());
        assert_eq!(l.walk().legs, 0);
    }

    #[test]
    fn the_median_is_bounded_about_the_prior_and_the_newest_samples_count() {
        let mut l = Ledger::in_memory(0.0);
        let prior = quack_duck::gait::priors().m_per_s;
        // Absurd speeds: bounded to 1.6 × the prior.
        walk_straight(&mut l, MIN_SAMPLES, 0.5, 0.0);
        assert!((l.value(Number::MPerS).unwrap() - 1.6 * prior).abs() < 1e-9);
        // KEEP newer samples at the prior: the old ones are gone.
        walk_straight(&mut l, KEEP, prior, 0.0);
        assert!((l.value(Number::MPerS).unwrap() - prior).abs() < 1e-9);
        let e = &l.walk().numbers["m_per_s"];
        assert_eq!(e.samples.len(), KEEP);
        assert_eq!(e.seen as usize, KEEP + MIN_SAMPLES);
    }

    #[test]
    fn a_step_on_the_heels_of_a_turn_gives_no_yaw_sample() {
        let mut l = Ledger::in_memory(0.0);
        let t0 = Instant::now();
        l.observe_turn(t0, 1.0, 1.0);
        // A straight step 1 s after the turn: the speed counts, the yaw not.
        let before = snap(t0 + Duration::from_secs(1), (0.0, 0.0, 0.0), (0.0, 0.0, 0.0), true);
        let after = snap(t0 + Duration::from_millis(1600), (0.06, 0.0, 0.2), (0.06, 0.0, 0.2), true);
        assert_eq!(l.observe_step(&before, &after, 0.3, 0.0, 0.6, f64::INFINITY), vec![Number::MPerS]);
        // Three seconds after: both.
        let before = snap(t0 + Duration::from_secs(3), (0.0, 0.0, 0.0), (0.0, 0.0, 0.0), true);
        let after = snap(t0 + Duration::from_millis(3600), (0.06, 0.0, 0.2), (0.06, 0.0, 0.2), true);
        assert_eq!(l.observe_step(&before, &after, 0.3, 0.0, 0.6, f64::INFINITY), vec![Number::MPerS, Number::YawTrim]);
    }

    #[test]
    fn the_sticks_pulses_give_no_yaw_per_unit_a_long_curving_leg_does() {
        let mut l = Ledger::in_memory(0.0);
        let prior = quack_duck::gait::priors().yaw_rate_per_unit;
        let t0 = Instant::now();
        let vyaw = 0.5;
        // The stick's 0.6 s pulse: the speed and its side's yaw per unit
        // sent count, the yaw per unit not (the module: a pulse's is not
        // one number; the sides' ratio is).
        let yaw = 0.8 * prior * vyaw * 0.6;
        let before = snap(t0, (0.0, 0.0, 0.0), (0.0, 0.0, 0.0), true);
        let after = snap(t0 + Duration::from_millis(600), (0.06, 0.0, yaw), (0.06, 0.0, yaw), true);
        assert_eq!(l.observe_step(&before, &after, 0.3, vyaw, 0.6, f64::INFINITY), vec![Number::MPerS, Number::YpuPulseLeft]);
        assert_eq!(l.value(Number::YawRatePerUnit), None);
        // A curving leg of a second: the gait turning for most of it.
        let yaw = prior * vyaw * 1.0;
        for i in 0..MIN_SAMPLES {
            let before = snap(t0, (i as f64, 0.0, 0.0), (i as f64, 0.0, 0.0), true);
            let after = snap(t0 + Duration::from_secs(1), (i as f64 + 0.1, 0.0, yaw), (i as f64 + 0.1, 0.0, yaw), true);
            assert_eq!(l.observe_step(&before, &after, 0.3, vyaw, 1.0, f64::INFINITY), vec![Number::MPerS, Number::YpuPulseLeft, Number::YawRatePerUnit]);
        }
        let v = l.value(Number::YawRatePerUnit).unwrap();
        assert!((v - prior).abs() < 1e-9, "{v} vs {prior}");
        assert_eq!(l.learned().yaw_rate_per_unit, Some(v));
    }

    #[test]
    fn a_turns_budget_follows_the_rate_it_learned() {
        let mut l = Ledger::in_memory(0.0);
        let t0 = Instant::now();
        // Before a rate is learned: the slowest side ever measured.
        assert!((l.turn_budget_s(1.0) - (2.0 / TURN_RATE_FLOOR_RAD_S + 1.0)).abs() < 1e-9);
        for _ in 0..MIN_SAMPLES {
            l.observe_turn(t0, 0.8, 1.0);
        }
        assert!((l.turn_budget_s(1.0) - (2.0 / 0.8 + 1.0)).abs() < 1e-9);
        // The ledger of a process that never opened one: the floor.
        assert!((turn_budget_s(1.0) - (2.0 / TURN_RATE_FLOOR_RAD_S + 1.0)).abs() < 1e-9);
    }

    #[test]
    fn a_turn_and_a_coast() {
        let mut l = Ledger::in_memory(0.0);
        let t0 = Instant::now();
        assert!(!l.observe_turn(t0, 0.1, 0.5), "a turn that did not turn");
        for _ in 0..MIN_SAMPLES {
            assert!(l.observe_turn(t0, 1.0, 1.0));
        }
        assert!((l.value(Number::TurnRateRadS).unwrap() - 1.0).abs() < 1e-9);
        let prior = quack_duck::gait::priors().coast_prior_rad;
        assert_eq!(l.coast_rad(), prior);
        for _ in 0..MIN_SAMPLES - 1 {
            assert_eq!(l.observe_coast(0.1), prior, "the prior until the minimum");
        }
        assert!((l.observe_coast(0.1) - 0.1).abs() < 1e-9);
        assert_eq!(l.learned().coast_rad, Some(0.1));
    }

    /// A pulse of the stick's: `vyaw` asked, the body turning `ypu` per
    /// unit of what was sent on that side.
    fn pulse(l: &mut Ledger, vyaw: f64, ypu: f64) -> Vec<Number> {
        let t0 = Instant::now();
        // The trim cancels the veer: neither shows in the yaw.
        let yaw = ypu * vyaw * l.gain_in_force(vyaw) * 0.6;
        let before = snap(t0, (0.0, 0.0, 0.0), (0.0, 0.0, 0.0), true);
        let after = snap(t0 + Duration::from_millis(600), (0.06, 0.0, yaw), (0.06, 0.0, yaw), true);
        l.observe_step(&before, &after, 0.3, vyaw, 0.6, f64::INFINITY)
    }

    #[test]
    fn the_sides_ratio_over_the_pulses_is_the_gains() {
        let mut l = Ledger::in_memory(0.0);
        // The right turns a fifth more per unit than the left.
        let (left, right) = (0.8, 0.96);
        for _ in 0..MIN_SAMPLES_SIDE {
            assert_eq!(pulse(&mut l, 0.5, left), vec![Number::MPerS, Number::YpuPulseLeft]);
        }
        assert_eq!(l.gains(), None, "one side alone is no gain");
        assert_eq!(l.learned().yaw_gain_left, None);
        for _ in 0..MIN_SAMPLES_SIDE - 1 {
            assert_eq!(pulse(&mut l, -0.5, right), vec![Number::MPerS, Number::YpuPulseRight]);
        }
        assert_eq!(l.gains(), None, "a side's number waits for more samples than the rest");
        pulse(&mut l, -0.5, right);
        let (gl, gr) = l.gains().unwrap();
        let m = 0.5 * (left + right);
        assert!((gl - m / left).abs() < 1e-9 && (gr - m / right).abs() < 1e-9, "{gl} {gr}");
        assert!(gl > 1.0 && gr < 1.0, "the side that turns more is sent less");
        assert_eq!((l.learned().yaw_gain_left, l.learned().yaw_gain_right), (Some(gl), Some(gr)));
        // With the gains in force the body turns the same either way:
        // the samples still say what each side does per unit sent.
        assert_eq!(pulse(&mut l, 0.5, left), vec![Number::MPerS, Number::YpuPulseLeft]);
        assert!((l.value(Number::YpuPulseLeft).unwrap() - left).abs() < 1e-9);
        // The clamp's pulses and the short ones give no side sample.
        assert_eq!(pulse(&mut l, 0.7, left), vec![Number::MPerS]);
        let before = snap(Instant::now(), (0.0, 0.0, 0.0), (0.0, 0.0, 0.0), true);
        let after = snap(Instant::now() + Duration::from_millis(400), (0.04, 0.0, 0.1), (0.04, 0.0, 0.1), true);
        assert_eq!(l.observe_step(&before, &after, 0.3, 0.5, 0.4, f64::INFINITY), vec![Number::MPerS]);
        // Absurd sides: the gains stay within their bounds.
        let mut l = Ledger::in_memory(0.0);
        for _ in 0..MIN_SAMPLES_SIDE {
            pulse(&mut l, 0.5, 0.3);
            pulse(&mut l, -0.5, 1.5);
        }
        assert_eq!(l.gains(), Some((GAIN_BOUNDS.1, GAIN_BOUNDS.0)));
        // A walk that veers past what the sides can be read apart from:
        // no side sample, the file's gains stand.
        let mut l = Ledger::in_memory(0.1);
        assert_eq!(pulse(&mut l, 0.5, left), vec![Number::MPerS]);
        assert_eq!(l.gain_in_force(0.5), 1.0);
    }

    #[test]
    fn the_settle_is_watched_from_the_twist_to_the_still_odometry() {
        let mut w = SettleWatch::default();
        let ns = |s: f64| (s * 1e9) as u64;
        // Walking at 0.1 m/s, 20 states a second.
        for i in 0..10 {
            assert_eq!(w.tick(ns(i as f64 * 0.05), true, Some((i as f64 * 0.005, 0.0, 0.0))), None);
        }
        // The twist is gone at 0.5 s; the body goes on, slowing, still at 0.85 s.
        let mut x = 0.045;
        let mut out = None;
        for i in 10..30 {
            let t = i as f64 * 0.05;
            let v = if t < 0.85 { 0.1 } else { 0.0 };
            x += v * 0.05;
            if let Some(s) = w.tick(ns(t), false, Some((x, 0.0, 0.0))) {
                out = Some((t, s));
                break;
            }
        }
        let (t, s) = out.expect("a settle");
        assert!((t - 0.85).abs() < 1e-9 && (s - 0.35).abs() < 1e-9, "{t} {s}");
        // Still after: nothing more until another twist.
        assert_eq!(w.tick(ns(0.9), false, Some((x, 0.0, 0.0))), None);
        // A twist back before the body is still: no sample.
        assert_eq!(w.tick(ns(1.0), true, Some((x, 0.0, 0.0))), None);
        assert_eq!(w.tick(ns(1.05), false, Some((x + 0.005, 0.0, 0.0))), None);
        assert_eq!(w.tick(ns(1.1), true, Some((x + 0.01, 0.0, 0.0))), None);
        // A body that never stands still: no sample past the cap.
        for i in 0..50 {
            let t = 1.15 + i as f64 * 0.05;
            assert_eq!(w.tick(ns(t), false, Some((x + 0.01 + 0.1 * (t - 1.1), 0.0, 0.0))), None);
        }
        // The ledger takes it, bounded, and lays it over the walk's settle.
        let mut l = Ledger::in_memory(0.0);
        for _ in 0..MIN_SAMPLES {
            l.observe_settle(0.35);
        }
        assert_eq!(l.learned().settle_s, Some(0.35));
        l.observe_settle(5.0);
        assert_eq!(l.walk().numbers["settle_s"].seen, MIN_SAMPLES as u64, "past the cap is not a settle");
    }

    #[test]
    fn the_file_keeps_the_walk_between_runs_and_a_reset_forgets_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gait-ledger.json");
        let prior = quack_duck::gait::priors().m_per_s;
        {
            let mut l = Ledger::open(Some(path.clone()), 0.0, (1.0, 1.0));
            walk_straight(&mut l, MIN_SAMPLES, prior * 1.1, 0.0);
            l.write(true);
        }
        let mut again = Ledger::open(Some(path.clone()), 0.0, (1.0, 1.0));
        assert!((again.value(Number::MPerS).unwrap() - prior * 1.1).abs() < 1e-9, "read back");
        let r = again.report();
        assert_eq!(r["on"], true);
        assert_eq!(r["numbers"]["m_per_s"]["samples"], MIN_SAMPLES);
        assert_eq!(r["numbers"]["turn_rate_rad_s"]["learned"], Value::Null);
        again.reset();
        assert_eq!(again.value(Number::MPerS), None);
        again.write(true);
        let third = Ledger::open(Some(path), 0.0, (1.0, 1.0));
        assert_eq!(third.value(Number::MPerS), None, "the reset was written");
        // A file that does not read is a fresh ledger.
        let bad = dir.path().join("bad.json");
        std::fs::write(&bad, "not json").unwrap();
        assert_eq!(Ledger::open(Some(bad), 0.0, (1.0, 1.0)).value(Number::MPerS), None);
    }

    #[test]
    fn the_walk_is_keyed_by_line_and_profile() {
        let l = Ledger::in_memory(0.0);
        assert_eq!(l.key, format!("{}/{}", quack_duck::daemon::active().name(), quack_duck::gait::active().name()));
        assert_eq!(l.report()["walk"], l.key);
    }
}
