//! The modes, and what each one is allowed (the user's rule, 2026-09-18:
//! the modes apart, a core that calls them, shared only what is
//! identical). A job is one of three: MAPPING a house (guards on,
//! stands, frontiers), a JOURNEY BLIND on a frozen map with its books
//! (the planner in charge, no guard on the legs), a JOURNEY GUARDED
//! (a goal on a map without books, or the guards asked for). The
//! passage-law experiments of 2026-09-17/18 — the wall as the guide,
//! the alignment by a kick, the held leg, the seal, the planner 0.25
//! from a rim point — belong to the guarded journey alone; they leaked
//! into the blind one through shared knobs and cost it 200 s a tour
//! (house6tour 693 s against the baseline's 472–499). Mapping and the
//! blind journey keep the baseline's behaviour (`baseline-twin.md`).
//! The wall as the guide, the held leg and the seal went with the
//! explorer's old legs (2026-09-30).

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    Mapping,
    JourneyBlind,
    JourneyGuarded,
}

/// What the mode allows of the passage law and the planner. Resolved
/// once per mode; the environment knobs override a field when set, so
/// an experiment can be run in any mode on purpose.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Policy {
    pub mode: Mode,
    /// The alignment by a guarded kick then yaw (else the walking pulse).
    pub align_kick: bool,
    /// What a rim point is worth to the planner, without the inflation.
    pub drop_plan_radius_m: f64,
    /// The walking kick before a spin in tight quarters: the guarded
    /// journey's 0.5 s (6 cm, judged by the guard), else the second the
    /// gait needs to be stepping before the yaw — at 0.5 s the yaw turns
    /// less and the blind tour spun 53 times instead of 38 (house7tour,
    /// 2026-09-18: the kitchen leg 192 s against 103).
    pub tight_kick_s: f64,
    /// Legs over trusted floor walk blind (see `trusted.rs`).
    pub trusted_floor: bool,
    /// The alignment tolerance for a turn to the left. On the MuJoCo
    /// twin a left turn lands short more often than a right one (point
    /// 8, 2026-09-19: 5 of 9 left alignments 8–11° short); a tighter
    /// tolerance there buys one more yaw. The paper twin has no such
    /// bias and a tighter tolerance cost it arrivals (27/30 against
    /// 29/30 blind, 13/30 against 15/30 guarded, 2026-09-20) — so the
    /// common tolerance unless the knob says otherwise.
    pub align_tol_left_rad: f64,
}

impl Policy {
    pub(super) fn for_mode(mode: Mode) -> Policy {
        let guarded = mode == Mode::JourneyGuarded;
        Policy {
            mode,
            align_kick: env_switch("ALIGN_KICK").unwrap_or(guarded),
            drop_plan_radius_m: std::env::var("QK_DROP_PLAN_RADIUS_M")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(if guarded { DROP_PLAN_RADIUS_GUARDED_M } else { DROP_PLAN_RADIUS_M }),
            tight_kick_s: if guarded { TIGHT_KICK_S } else { PANO_KICK_S },
            trusted_floor: env_switch("TRUSTED_FLOOR").unwrap_or(mode != Mode::JourneyBlind),
            align_tol_left_rad: std::env::var("QK_ALIGN_TOL_LEFT_RAD")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(ALIGN_TOL_RAD),
        }
    }
}

/// `QK_<name>`: `1` on, `0` off, unset the mode's own.
fn env_switch(name: &str) -> Option<bool> {
    match quack_duck::env::qk(name).as_deref() {
        Some("1") => Some(true),
        Some("0") => Some(false),
        _ => None,
    }
}

impl Job {
    /// The mode of this job now: a goal on a frozen map without the
    /// guards asked for is a blind journey; a goal otherwise a guarded
    /// one; no goal is mapping.
    pub(super) fn mode_now(&self) -> Mode {
        if self.goal.is_none() {
            Mode::Mapping
        } else if self.blind() {
            Mode::JourneyBlind
        } else {
            Mode::JourneyGuarded
        }
    }

    /// Keep the policy in step with the mode (read each turn: the frozen
    /// state and the knobs can change it).
    pub(super) fn resolve_policy(&mut self) {
        let mode = self.mode_now();
        if self.policy.mode != mode {
            self.policy = Policy::for_mode(mode);
            tracing::info!(mode = ?mode, policy = ?self.policy, "map explore: mode");
        }
    }
}
