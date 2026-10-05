//! The simulator's numbers: how the duck walks, turns and drifts, how the
//! depth sensor sees. The defaults are the paper twin's (what the MuJoCo
//! twin measured, `quack-nav/examples/paper_twin.rs`); `rl_calib` fits them
//! to the duck's own traces (`QK_RL_TRACE`), and training then varies each
//! around its value by its spread (domain randomisation), wide while
//! nothing has been measured on the duck, narrow once it has.

use serde::{Deserialize, Serialize};

use crate::Rng;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Calib {
    // --- the gait
    /// Forward speed at vx 0.3, m/s (scales with vx).
    pub speed_at_03: f64,
    /// Yaw rate per unit of vyaw while walking, rad/s.
    pub yaw_per_unit: f64,
    /// The yaw the gait drifts by walking "straight" (|vyaw| < 0.1), rad/s.
    pub straight_veer: f64,
    /// A short curving pulse (≤ 0.8 s, |vyaw| > 0.3) turns by this gain of
    /// the yaw asked, mean and spread (the MuJoCo turnprobe: 13° ± 10°).
    pub pulse_gain_mean: f64,
    pub pulse_gain_sd: f64,
    /// Step-to-step spread of the forward speed, relative.
    pub speed_sd: f64,
    /// Backing speed, m/s.
    pub back_speed: f64,
    /// Turning in place from a standstill at |vyaw| 1.5, rad/s per side;
    /// and at +1.2 (the right side's threshold is higher: nothing there).
    pub turn_left_rad_s: f64,
    pub turn_right_rad_s: f64,
    pub turn_left_slow_rad_s: f64,
    /// Turning in place once already stepping, rad/s.
    pub spin_rate: f64,
    /// Turn-to-turn spread of the in-place rate, relative.
    pub turn_sd: f64,
    // --- the pose
    /// Odometry's drift: metres of error per metre walked (a random walk),
    /// and radians per radian turned.
    pub odom_xy_per_m: f64,
    pub odom_yaw_per_rad: f64,
    /// The map's pose error kept after a stand (the mapper's correction).
    pub stand_keep: f64,
    /// The map's pose, frame to frame: noise around its error, metres.
    pub pose_jitter_m: f64,
    /// The map's pose comes this often: quack-navd's mapd sends a frame
    /// once a second and `map.pose` between them every 50 ms
    /// (`mapd::POSE_EVERY`); between two, the journey reads the last.
    pub map_period_s: f64,
    // --- the body
    pub body_r: f64,
    // --- the depth sensor
    pub tof_hz: f64,
    pub tof_range_max: f64,
    pub tof_range_sd: f64,
    pub tof_range_bias: f64,
    /// A beam that should see something and does not.
    pub tof_dropout: f64,
    /// Things lower than this are lost by the floor threshold while
    /// walking (README: "under ~9 cm the floor threshold loses it").
    pub tof_low_walk_m: f64,
    /// Low furniture's floor rows read as a drop this often (paper twin).
    pub phantom_p: f64,
    /// A drop where the floor is, per frame, anywhere (a reflection).
    pub phantom_any_p: f64,
}

impl Default for Calib {
    fn default() -> Self {
        Self {
            speed_at_03: 0.114,
            yaw_per_unit: 0.65,
            straight_veer: -0.05,
            pulse_gain_mean: 0.85,
            pulse_gain_sd: 0.65,
            speed_sd: 0.1,
            back_speed: 0.08,
            turn_left_rad_s: 51f64.to_radians(),
            turn_right_rad_s: 58f64.to_radians(),
            turn_left_slow_rad_s: 30f64.to_radians(),
            spin_rate: 0.52,
            turn_sd: 0.1,
            odom_xy_per_m: 0.05,
            odom_yaw_per_rad: 0.015,
            pose_jitter_m: 0.01,
            map_period_s: 0.05,
            stand_keep: 0.5,
            body_r: 0.11,
            tof_hz: 15.0,
            tof_range_max: 2.2,
            tof_range_sd: 0.02,
            tof_range_bias: 0.0,
            tof_dropout: 0.02,
            tof_low_walk_m: 0.09,
            phantom_p: 0.10,
            phantom_any_p: 0.002,
        }
    }
}

/// The relative spread of each number for domain randomisation: a draw is
/// `value × (1 + spread × u)`, u uniform in [-1, 1] (additive for the
/// numbers that sit near zero).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Spread {
    pub gait: f64,
    pub turn: f64,
    pub veer_abs: f64,
    pub odom: f64,
    pub tof_noise: f64,
    pub tof_bias_abs: f64,
}

impl Default for Spread {
    /// Wide: nothing measured on the duck yet.
    fn default() -> Self {
        Self { gait: 0.3, turn: 0.35, veer_abs: 0.08, odom: 0.8, tof_noise: 1.0, tof_bias_abs: 0.03 }
    }
}

impl Spread {
    pub fn none() -> Self {
        Self { gait: 0.0, turn: 0.0, veer_abs: 0.0, odom: 0.0, tof_noise: 0.0, tof_bias_abs: 0.0 }
    }

    /// Narrow: around numbers fitted on the duck.
    pub fn calibrated() -> Self {
        Self { gait: 0.1, turn: 0.12, veer_abs: 0.03, odom: 0.4, tof_noise: 0.4, tof_bias_abs: 0.01 }
    }
}

impl Calib {
    pub fn load(path: &str) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        // A calibration report carries the numbers under "calib".
        let v: serde_json::Value = serde_json::from_str(&text)?;
        let inner = v.get("calib").cloned().unwrap_or(v);
        Ok(serde_json::from_value(inner)?)
    }

    /// A draw around these numbers.
    pub fn sample(&self, s: &Spread, rng: &mut Rng) -> Self {
        let mut c = self.clone();
        let mut rel = |v: &mut f64, k: f64| *v *= 1.0 + k * (2.0 * rng.next() - 1.0);
        rel(&mut c.speed_at_03, s.gait);
        rel(&mut c.yaw_per_unit, s.gait);
        rel(&mut c.back_speed, s.gait);
        rel(&mut c.pulse_gain_mean, s.gait);
        rel(&mut c.turn_left_rad_s, s.turn);
        rel(&mut c.turn_right_rad_s, s.turn);
        rel(&mut c.turn_left_slow_rad_s, s.turn);
        rel(&mut c.spin_rate, s.turn);
        rel(&mut c.odom_xy_per_m, s.odom);
        rel(&mut c.odom_yaw_per_rad, s.odom);
        rel(&mut c.tof_range_sd, s.tof_noise);
        rel(&mut c.tof_dropout, s.tof_noise);
        rel(&mut c.phantom_any_p, s.tof_noise);
        c.straight_veer += s.veer_abs * (2.0 * rng.next() - 1.0);
        c.tof_range_bias += s.tof_bias_abs * (2.0 * rng.next() - 1.0);
        c.stand_keep = (c.stand_keep * (1.0 + 0.5 * s.odom.min(1.0) * (2.0 * rng.next() - 1.0))).clamp(0.05, 0.95);
        c
    }

    /// Turning in place from a standstill, as the gait answers `vyaw`
    /// (the paper twin's `in_place_rate`, its numbers here).
    pub fn in_place_rate(&self, vyaw: f64) -> Option<f64> {
        match vyaw {
            v if v >= 1.45 => Some(self.turn_left_rad_s),
            v if v >= 1.15 => Some(self.turn_left_slow_rad_s),
            v if v <= -1.45 => Some(-self.turn_right_rad_s),
            _ => None,
        }
    }
}
