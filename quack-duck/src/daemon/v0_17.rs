//! Pollen's daemons from daemon-v0.16.2 on: policy set v8 (Pollen's #393, a
//! velstand that "tracks yaw commands"; the set's floor moved to v8 with
//! that release). Validated on daemon-v0.17.6.

use crate::gait::{Numbers, YAW_MAX};

/// velstand, Hub policy set v8, on the MuJoCo twin at daemon-v0.17.6
/// (`scripts/twin/gaitprobe.py`, the arena, three trials each, beside set v7
/// on daemon-v0.16.1 under the same load, 2026-10-19): 0.095 m/s at vx 0.3
/// (v7 0.115); 11-24° of veer to the left in 10 s straight (v7 a few degrees
/// either way), so a trim of -0.03; 1.1 rad/s per unit of yaw while walking
/// (v7 0.62); a turn in place at ±1.0 of 63-68°/s, where v7 barely moved, and
/// at ±1.5 of 89-108°/s — so a turn asks 1.0, about the rate v7 turned at
/// 1.5 and every scan and window was tuned on, and the most Pollen's own pad
/// asks since #393; 12-23° of coast after a spin at 1.0, the prior's 0.3 rad.
/// Still within 0.4 s of the twist's end, as v7.
///
/// Its turns go on further than v7's: on the journeys a turn closed on
/// odometry at 30° ended 77° round by the map (v7 64°, D17 2026-10-19). The
/// exploration, which closed its turns with no coast, overshot past its
/// own threshold and turned back and forth (256 turns to v7's 112 in a first
/// session, the apartment 58 % mapped to 83 %), so it learns the coast too.
pub(super) const VELSTAND: Numbers = Numbers {
    m_per_s: 0.095,
    yaw_rate_per_unit: 1.1,
    turn_in_place_rad_s: 1.0,
    max_turn_in_place_rad_s: 1.1,
    coast_prior_rad: 0.3,
    coast_while_exploring: true,
    settle_s: 0.6,
    yaw_trim: -0.03,
    yaw_gain_left: 1.0,
    yaw_gain_right: 1.0,
    yaw_max: YAW_MAX,
};
