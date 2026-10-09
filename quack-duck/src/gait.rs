//! The gait's limits: what a walking command becomes once the body's own
//! veer and turning asymmetry are corrected for.

use serde::Deserialize;

/// Corrections to every walking command the satellite sends (`robot.move`,
/// mapping steps, the explorer's legs): a yaw trim for a gait that veers
/// when told to go straight, and a gain per turning side for a gait that
/// turns better one way. Defaults are "off" (0, 1, 1); on the MuJoCo twin
/// a straight 3 s leg veers about 20° right and `yaw_trim = 0.2` cancels
/// it. Applied only while walking forward.
#[derive(Debug, Clone, Deserialize)]
#[serde(from = "RawGaitConfig")]
pub struct GaitConfig {
    /// The walk robotd runs (see [`Profile`]): it sets every number below
    /// that the file leaves out, and the explorer's gait model.
    pub profile: Profile,
    /// Added to the yaw command (rad/s, + = left) whenever vx > 0.
    pub yaw_trim: f64,
    /// Multiplies a left (positive) yaw command.
    pub yaw_gain_left: f64,
    /// Multiplies a right (negative) yaw command.
    pub yaw_gain_right: f64,
    /// The most yaw worth sending (rad/s), after trim and gain. A gain
    /// above 1 can push a legitimate request past what the gait can turn,
    /// so the correction is clamped here rather than at each call site.
    /// Measured on the twin: alpha's raw curve went flat at 0.9 (0.63
    /// achieved, 2026-09-13), but with its gains on it still climbs past
    /// that (asked 0.9 → 0.81/1.00 achieved, 2026-09-14), and velstand's
    /// climbs to 0.9 achieved at 1.63 sent. Asking for more slows the walk
    /// (0.12 m/s straight, 0.08 at the top of the curve).
    pub yaw_max: f64,
}

impl Default for GaitConfig {
    fn default() -> Self {
        Self::from(RawGaitConfig::default())
    }
}

/// `[gait]` as written: what is left out comes from the profile.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct RawGaitConfig {
    profile: Profile,
    yaw_trim: Option<f64>,
    yaw_gain_left: Option<f64>,
    yaw_gain_right: Option<f64>,
    yaw_max: Option<f64>,
}

impl From<RawGaitConfig> for GaitConfig {
    fn from(raw: RawGaitConfig) -> Self {
        let n = raw.profile.numbers();
        Self {
            profile: raw.profile,
            yaw_trim: raw.yaw_trim.unwrap_or(n.yaw_trim),
            yaw_gain_left: raw.yaw_gain_left.unwrap_or(n.yaw_gain_left),
            yaw_gain_right: raw.yaw_gain_right.unwrap_or(n.yaw_gain_right),
            yaw_max: raw.yaw_max.unwrap_or(n.yaw_max),
        }
    }
}

/// The walking policy robotd runs, and what the navigation assumes of it.
/// `alpha` is the pair (`alpha_walking` + `alpha_stand`) every number before
/// 2026-10-08 was measured on, and its numbers here are exactly the
/// constants the code had then: a profile never changes the other one.
/// `velstand` is the duck's default walk since Pollen's policy set v5 (one
/// network that walks on a twist and stands at zero), measured on the
/// MuJoCo twin with set v7 (see [`VELSTAND`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    Alpha,
    /// The default since 2026-10-08: the walk Pollen's ducks ship with.
    #[default]
    Velstand,
}

impl Profile {
    /// As `[gait] profile` and a pilot's `meta.gait` spell it.
    pub fn name(self) -> &'static str {
        match self {
            Profile::Alpha => "alpha",
            Profile::Velstand => "velstand",
        }
    }

    pub fn from_name(name: &str) -> Option<Profile> {
        match name {
            "alpha" => Some(Profile::Alpha),
            "velstand" => Some(Profile::Velstand),
            _ => None,
        }
    }
}

/// A walk's numbers (see [`Profile`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Numbers {
    /// Forward speed at vx 0.3 (m/s).
    pub m_per_s: f64,
    /// Yaw rate per unit of yaw command while walking (rad/s per unit).
    pub yaw_rate_per_unit: f64,
    /// What a turn in place asks for, and the most a standstill may ask.
    pub turn_in_place_rad_s: f64,
    pub max_turn_in_place_rad_s: f64,
    /// A journey's turn-in-place coast before it has measured one (rad).
    pub coast_prior_rad: f64,
    /// How long the body goes on once no twist is applied, and so what a
    /// stand adds before it is still for the mapper (s).
    pub settle_s: f64,
    /// `[gait]`'s defaults under this walk.
    pub yaw_trim: f64,
    pub yaw_gain_left: f64,
    pub yaw_gain_right: f64,
    pub yaw_max: f64,
}

impl Profile {
    pub const fn numbers(self) -> Numbers {
        match self {
            Profile::Alpha => Numbers {
                m_per_s: 0.12,
                yaw_rate_per_unit: 0.65,
                turn_in_place_rad_s: 1.5,
                max_turn_in_place_rad_s: 1.6,
                coast_prior_rad: 0.3,
                settle_s: 0.0,
                yaw_trim: 0.0,
                yaw_gain_left: 1.0,
                yaw_gain_right: 1.0,
                yaw_max: YAW_MAX,
            },
            Profile::Velstand => VELSTAND,
        }
    }
}

/// velstand, Hub policy set v7, on the MuJoCo twin at daemon-v0.16.1
/// (`scripts/twin/gaitprobe.py`, the arena, three trials each, 2026-10-08):
/// 0.115 m/s at vx 0.3 (alpha 0.119); 6° of veer in 10 s straight (alpha
/// 25°: v7 trained its yaw bias out, so set v5's trim 0.16 and gains
/// 1.63 / 1.58 would now steer it); 0.62 rad/s per unit of yaw while
/// walking, about even both ways; a turn in place at ±1.5 of 44-51°/s, the
/// dead zone at ±1.0 as alpha's; 9° of coast after a spin.
const VELSTAND: Numbers = Numbers {
    m_per_s: 0.115,
    yaw_rate_per_unit: 0.62,
    turn_in_place_rad_s: 1.5,
    max_turn_in_place_rad_s: 1.6,
    coast_prior_rad: 0.3,
    settle_s: 0.6,
    yaw_trim: 0.02,
    yaw_gain_left: 1.0,
    yaw_gain_right: 1.0,
    yaw_max: YAW_MAX,
};

/// Whether a body robotd labels `policy` is walking. alpha has a standing
/// network, so its label says it ("walk" or not). velstand has none: it
/// stands under the walking network, labelled "walk" standing still too
/// (`docs/study/upstream-asks.md` §6a) — the mapper's still windows never
/// opened, the head sweep never ran and robotd's idle glancing took the
/// head (MuJoCo, daemon 0.16.1, 2026-10-08). Under velstand a "walk" body
/// with no twist applied (`robot.state.move.applied`) stands still.
pub fn walking(policy: &str, applied: [f64; 3]) -> bool {
    match numbers_profile() {
        Profile::Velstand if policy == "walk" => applied.iter().any(|v| v.abs() > STILL_TWIST),
        _ => policy == "walk",
    }
}

/// An applied twist under this is none (robotd's smoothing ends at 1e-300).
const STILL_TWIST: f64 = 1e-3;

/// How long velstand's body goes on once the twist is gone: 0.1-0.55 s on
/// the twin (`scripts/twin/settleprobe.py`, set v7, 2026-10-08; alpha 0-0.3).
/// The mapper's still windows began in that, smeared, and agreed with the
/// map too little to correct the pose: 9 corrections a round against
/// alpha's 30, and the pose 0.3-0.5 m off in long journeys.
pub const VELSTAND_SETTLE_NS: u64 = (VELSTAND.settle_s * 1e9) as u64;

/// [`walking`] with velstand's settle: a stream's own memory of the last
/// tick that had a twist (alpha: the label, as [`walking`]).
#[derive(Debug, Default, Clone)]
pub struct Stillness {
    last_twist_ns: Option<u64>,
}

impl Stillness {
    pub fn moving(&mut self, policy: &str, applied: [f64; 3], t_ns: u64) -> bool {
        let walking_now = walking(policy, applied);
        if numbers_profile() != Profile::Velstand || policy != "walk" {
            return walking_now;
        }
        if walking_now {
            self.last_twist_ns = Some(t_ns);
            return true;
        }
        self.last_twist_ns.is_some_and(|t| t_ns.saturating_sub(t) < VELSTAND_SETTLE_NS)
    }
}

fn numbers_profile() -> Profile {
    active()
}

/// The walk this process drives: `[gait] profile` once quack-navd has read
/// it (velstand when the file leaves it out); alpha in a process that reads
/// no config — the tests, the paper twin and the quack-rl simulator, all
/// built on alpha — unless it sets one.
pub fn active() -> Profile {
    ACTIVE.get().copied().unwrap_or(Profile::Alpha)
}

static ACTIVE: std::sync::OnceLock<Profile> = std::sync::OnceLock::new();

/// Set the walk this process drives, once (quack-navd at start, from
/// `[gait] profile`; the quack-rl simulator from `--gait`).
pub fn set_active(profile: Profile) {
    let _ = ACTIVE.set(profile);
}

/// The numbers of the walk this process drives.
pub fn numbers() -> Numbers {
    active().numbers()
}

/// The clamp every gait measured so far was run under.
const YAW_MAX: f64 = 0.9;

impl GaitConfig {
    /// The yaw actually sent for a wanted yaw while walking at `vx`.
    pub fn yaw(&self, vx: f64, vyaw: f64) -> f64 {
        if vx <= 0.0 {
            return vyaw;
        }
        let gain = if vyaw > 0.0 { self.yaw_gain_left } else { self.yaw_gain_right };
        (vyaw * gain + self.yaw_trim).clamp(-self.yaw_max, self.yaw_max)
    }
}
#[cfg(test)]
mod profile_tests {
    use super::*;

    #[test]
    fn alpha_is_the_code_before_profiles() {
        // The constants every number before 2026-10-08 came from.
        let n = Profile::Alpha.numbers();
        assert_eq!(n.m_per_s, 0.12);
        assert_eq!(n.yaw_rate_per_unit, 0.65);
        assert_eq!(n.turn_in_place_rad_s, crate::body::TURN_IN_PLACE_RAD_S);
        assert_eq!(n.max_turn_in_place_rad_s, crate::body::MAX_TURN_IN_PLACE_RAD_S);
        assert_eq!(n.yaw_rate_per_unit, crate::body::YAW_RATE_PER_UNIT);
        assert_eq!(n.coast_prior_rad, 0.3);
        assert_eq!(n.settle_s, 0.0);
        let g = GaitConfig::from(RawGaitConfig { profile: Profile::Alpha, ..Default::default() });
        assert_eq!((g.profile, g.yaw_trim, g.yaw_gain_left, g.yaw_gain_right, g.yaw_max), (Profile::Alpha, 0.0, 1.0, 1.0, 0.9));
        // Left out, the walk is velstand: Pollen's ducks ship with it.
        assert_eq!(GaitConfig::default().profile, Profile::Velstand);
    }

    #[test]
    fn a_profile_fills_what_the_file_leaves_out() {
        let g: GaitConfig = serde_json::from_str(r#"{"profile": "velstand", "yaw_trim": 0.05}"#).unwrap();
        assert_eq!(g.profile, Profile::Velstand);
        assert_eq!(g.yaw_trim, 0.05);
        assert_eq!(g.yaw_gain_left, VELSTAND.yaw_gain_left);
        assert_eq!(g.yaw_max, VELSTAND.yaw_max);
        let a: GaitConfig = serde_json::from_str(r#"{"profile": "alpha", "yaw_trim": 0.08}"#).unwrap();
        assert_eq!((a.profile, a.yaw_trim, a.yaw_gain_left), (Profile::Alpha, 0.08, 1.0));
        assert!(serde_json::from_str::<GaitConfig>(r#"{"profile": "roller"}"#).is_err());
    }

    #[test]
    fn alpha_still_is_the_label() {
        // No profile set in the tests: alpha, the label alone, no memory.
        let mut s = Stillness::default();
        assert!(s.moving("walk", [0.0; 3], 0));
        assert!(!s.moving("stand", [0.3, 0.0, 0.0], 1));
    }
}
