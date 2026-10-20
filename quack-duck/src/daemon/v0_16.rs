//! Pollen's daemons up to daemon-v0.16.1: policy set v7 and before. The
//! numbers every result before 2026-10-19 was measured with.

use crate::gait::{Numbers, YAW_MAX};

/// velstand, Hub policy set v7, on the MuJoCo twin at daemon-v0.16.1
/// (`scripts/twin/gaitprobe.py`, the arena, three trials each, 2026-10-08):
/// 0.115 m/s at vx 0.3 (alpha 0.119); 6° of veer in 10 s straight (alpha
/// 25°: v7 trained its yaw bias out, so set v5's trim 0.16 and gains
/// 1.63 / 1.58 would now steer it); 0.62 rad/s per unit of yaw while
/// walking, about even both ways; a turn in place at ±1.5 of 44-51°/s, the
/// dead zone at ±1.0 as alpha's; 9° of coast after a spin.
pub(super) const VELSTAND: Numbers = Numbers {
    m_per_s: 0.115,
    yaw_rate_per_unit: 0.62,
    turn_in_place_rad_s: 1.5,
    max_turn_in_place_rad_s: 1.6,
    coast_prior_rad: 0.3,
    coast_while_exploring: false,
    settle_s: 0.6,
    yaw_trim: 0.02,
    yaw_gain_left: 1.0,
    yaw_gain_right: 1.0,
    yaw_max: YAW_MAX,
};
