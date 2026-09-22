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
#[serde(default, deny_unknown_fields)]
pub struct GaitConfig {
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
        Self {
            yaw_trim: 0.0,
            yaw_gain_left: 1.0,
            yaw_gain_right: 1.0,
            yaw_max: YAW_MAX,
        }
    }
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