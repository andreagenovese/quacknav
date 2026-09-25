//! How sure the mapper is of the pose: a 3×3 covariance over (x, y, yaw) in
//! the map frame, kept as an extended Kalman filter keeps one.
//!
//! - **Predict** with every odometry step: the pose moves by the body-frame
//!   delta, and the uncertainty grows by a noise proportional to the ground
//!   covered and the angle turned (a random walk: variance per metre, per
//!   radian), and is carried through the motion's Jacobian — an error in
//!   heading becomes an error in position as the duck walks.
//! - **Update** with every window the map judges: the scan matcher's normal
//!   matrix `H = JᵀJ` at the pose, over its per-beam residual, is the
//!   information the window carries (`R = σ²·H⁻¹`), fused in information
//!   form. Along a direction the scene does not constrain — the length of a
//!   corridor — `H` has almost nothing, and the variance there keeps growing:
//!   that is the degeneracy the valley test looks for by sliding the pose.
//!
//! This module only keeps the number; the mapper decides what to do with it
//! (see `Mapper::pose_covariance`). robotd's odometry already fuses the
//! legs' contacts with the IMU's heading, so nothing here fuses sensors
//! again: the filter's job is the uncertainty, not the estimate.

use crate::submap::Pose2;

/// A covariance over (x, y, yaw), map frame; metres and radians.
pub type Cov3 = [[f64; 3]; 3];

/// The noise of the motion model and the weight of a scan, as variances.
#[derive(Debug, Clone, Copy)]
pub struct UncertaintyConfig {
    /// Position variance added per metre walked (m²/m): 0.05² is 5 cm of
    /// standard deviation after a metre, 10 after four.
    pub xy_var_per_m: f64,
    /// Heading variance added per radian turned (rad²/rad).
    pub yaw_var_per_rad: f64,
    /// Heading variance added per metre walked (rad²/m).
    pub yaw_var_per_m: f64,
    /// A window's per-beam residual is never believed below this (m): the
    /// map's own noise, which a match that fits "perfectly" still carries.
    pub match_floor_m: f64,
    /// A window composite repeats beams — frames of one stand see the same
    /// wall — so its `H` counts one measurement many times. It is scaled
    /// down to this many independent beams at most.
    pub independent_beams: f64,
    /// A window is fused only with at least this many beams on the map...
    pub min_beams: u32,
    /// ... and a residual at the pose no worse than this (m): a window that
    /// disagrees is the watchdog's business, not evidence of precision.
    pub max_residual_m: f64,
    /// A window is judged against the map without its newest this-many
    /// submaps: the ink the drifting pose has just laid agrees with the
    /// drifting pose, and counting that agreement as information is how a
    /// covariance stays small while the pose walks off (measured: no
    /// correlation at all between the error and the sigma with it at 0).
    /// 0 judges against the whole map. 12 (about three minutes of stands):
    /// on the resumed sessions the sigma then follows the error (r = +0.46
    /// and +0.59 on casa_arredata and house2, against -0.02 and +0.07).
    pub skip_recent_submaps: usize,
}

impl Default for UncertaintyConfig {
    fn default() -> Self {
        Self {
            xy_var_per_m: 0.05 * 0.05,
            yaw_var_per_rad: 0.03 * 0.03,
            yaw_var_per_m: 0.02 * 0.02,
            match_floor_m: 0.08,
            independent_beams: 1.0,
            min_beams: 60,
            max_residual_m: 0.08,
            skip_recent_submaps: 12,
        }
    }
}

/// A diagonal covariance with these standard deviations.
pub fn diagonal(sigma_xy_m: f64, sigma_yaw_rad: f64) -> Cov3 {
    let (a, b) = (sigma_xy_m * sigma_xy_m, sigma_yaw_rad * sigma_yaw_rad);
    [[a, 0.0, 0.0], [0.0, a, 0.0], [0.0, 0.0, b]]
}

/// The motion step: `pose` is the map-frame pose before the step, `delta`
/// the body-frame motion (dx, dy, dyaw). Returns the covariance after it.
pub fn predict(cov: &Cov3, pose: Pose2, delta: Pose2, cfg: &UncertaintyConfig) -> Cov3 {
    let (s, c) = f64::from(pose.2).sin_cos();
    let (dx, dy, dyaw) = (f64::from(delta.0), f64::from(delta.1), f64::from(delta.2));
    // World-frame displacement and its derivative in the heading.
    let (wx, wy) = (c * dx - s * dy, s * dx + c * dy);
    let f = [[1.0, 0.0, -wy], [0.0, 1.0, wx], [0.0, 0.0, 1.0]];
    let d = dx.hypot(dy);
    let q_xy = cfg.xy_var_per_m * d;
    let q_yaw = cfg.yaw_var_per_rad * dyaw.abs() + cfg.yaw_var_per_m * d;
    let mut out = mul(&mul(&f, cov), &transpose(&f));
    out[0][0] += q_xy;
    out[1][1] += q_xy;
    out[2][2] += q_yaw;
    out
}

/// The window's information: `H` scaled to independent beams, over the
/// residual (floored) squared. `None` when the window is not to be fused.
pub fn window_information(hessian: &[[f32; 3]; 3], residual_m: f32, n_used: u32, cfg: &UncertaintyConfig) -> Option<Cov3> {
    if n_used < cfg.min_beams || !residual_m.is_finite() || f64::from(residual_m) > cfg.max_residual_m {
        return None;
    }
    let sigma = f64::from(residual_m).max(cfg.match_floor_m);
    let scale = (cfg.independent_beams / f64::from(n_used)).min(1.0) / (sigma * sigma);
    let mut info = [[0.0; 3]; 3];
    for a in 0..3 {
        for b in 0..3 {
            info[a][b] = f64::from(hessian[a][b]) * scale;
        }
    }
    Some(info)
}

/// Fuse a measurement of the pose, given as information (inverse covariance)
/// about the pose itself: `(Σ⁻¹ + Λ)⁻¹`.
pub fn fuse(cov: &Cov3, info: &Cov3) -> Cov3 {
    let Some(prior_info) = invert(&regularized(cov)) else { return *cov };
    let mut sum = prior_info;
    for a in 0..3 {
        for b in 0..3 {
            sum[a][b] += info[a][b];
        }
    }
    invert(&sum).map(|c| symmetric(&c)).unwrap_or(*cov)
}

/// The covariance a window alone gives: `Λ⁻¹`, with what it cannot see capped
/// at `cap` (a composite facing one wall says nothing along it).
pub fn from_information(info: &Cov3, cap: &Cov3) -> Cov3 {
    let cap_info = invert(cap).unwrap_or([[0.0; 3]; 3]);
    let mut sum = *info;
    for a in 0..3 {
        for b in 0..3 {
            sum[a][b] += cap_info[a][b];
        }
    }
    invert(&sum).map(|c| symmetric(&c)).unwrap_or(*cap)
}

/// What a covariance says, in the units a person reads: the largest position
/// standard deviation (the ellipse's semi-major axis), its direction, the
/// smallest, and the heading's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sigmas {
    pub xy_major_m: f64,
    pub xy_minor_m: f64,
    /// Unit vector of the major axis — the direction the pose is least sure
    /// of (along a corridor, the corridor).
    pub major_axis: (f64, f64),
    pub yaw_rad: f64,
}

pub fn sigmas(cov: &Cov3) -> Sigmas {
    let (a, b, c) = (cov[0][0], cov[0][1], cov[1][1]);
    let tr = a + c;
    let disc = ((a - c) * (a - c) / 4.0 + b * b).sqrt();
    let (l_max, l_min) = ((tr / 2.0 + disc).max(0.0), (tr / 2.0 - disc).max(0.0));
    let axis = if b.abs() > 1e-15 {
        let (vx, vy) = (b, l_max - a);
        let n = vx.hypot(vy);
        (vx / n, vy / n)
    } else if a >= c {
        (1.0, 0.0)
    } else {
        (0.0, 1.0)
    };
    Sigmas { xy_major_m: l_max.sqrt(), xy_minor_m: l_min.sqrt(), major_axis: axis, yaw_rad: cov[2][2].max(0.0).sqrt() }
}

fn mul(a: &Cov3, b: &Cov3) -> Cov3 {
    let mut out = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            out[i][j] = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    out
}

fn transpose(a: &Cov3) -> Cov3 {
    let mut out = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            out[i][j] = a[j][i];
        }
    }
    out
}

fn symmetric(a: &Cov3) -> Cov3 {
    let mut out = *a;
    for i in 0..3 {
        for j in 0..3 {
            out[i][j] = 0.5 * (a[i][j] + a[j][i]);
        }
    }
    out
}

/// A covariance with a floor on its diagonal, so a pose known "exactly"
/// (the origin of a fresh map) still inverts.
fn regularized(a: &Cov3) -> Cov3 {
    let mut out = *a;
    for i in 0..3 {
        out[i][i] = out[i][i].max(1e-8);
    }
    out
}

fn invert(a: &Cov3) -> Option<Cov3> {
    let det = a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1]) - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
        + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]);
    if !det.is_finite() || det.abs() < 1e-30 {
        return None;
    }
    let inv = 1.0 / det;
    Some([
        [
            (a[1][1] * a[2][2] - a[1][2] * a[2][1]) * inv,
            (a[0][2] * a[2][1] - a[0][1] * a[2][2]) * inv,
            (a[0][1] * a[1][2] - a[0][2] * a[1][1]) * inv,
        ],
        [
            (a[1][2] * a[2][0] - a[1][0] * a[2][2]) * inv,
            (a[0][0] * a[2][2] - a[0][2] * a[2][0]) * inv,
            (a[0][2] * a[1][0] - a[0][0] * a[1][2]) * inv,
        ],
        [
            (a[1][0] * a[2][1] - a[1][1] * a[2][0]) * inv,
            (a[0][1] * a[2][0] - a[0][0] * a[2][1]) * inv,
            (a[0][0] * a[1][1] - a[0][1] * a[1][0]) * inv,
        ],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walking_grows_the_uncertainty_and_a_heading_error_spreads_sideways() {
        let cfg = UncertaintyConfig::default();
        // Unsure of the heading only, then a metre straight ahead along x.
        let mut cov = diagonal(0.0, 0.1);
        for _ in 0..10 {
            cov = predict(&cov, (0.0, 0.0, 0.0), (0.1, 0.0, 0.0), &cfg);
        }
        let s = sigmas(&cov);
        // 0.1 rad of heading over a metre is ~10 cm across the path.
        assert!(cov[1][1].sqrt() > 0.09, "sideways {:.3}", cov[1][1].sqrt());
        assert!(cov[0][0].sqrt() < cov[1][1].sqrt(), "the error is across the walk, not along it");
        assert!((s.major_axis.1).abs() > 0.9, "major axis {:?}", s.major_axis);
    }

    #[test]
    fn a_window_facing_one_wall_pins_one_direction_only() {
        // The algebra, not the calibration: a window weighted as 64 beams at
        // 2 cm.
        let cfg = UncertaintyConfig { independent_beams: 64.0, match_floor_m: 0.02, ..UncertaintyConfig::default() };
        let prior = diagonal(0.20, 0.10);
        // A wall normal to x: every beam's gradient is along x; yaw pinned
        // by lever arms. y gets nothing.
        let n = 200.0f32;
        let h = [[n, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 0.0, n * 0.5]];
        let info = window_information(&h, 0.01, 200, &cfg).expect("fused");
        let post = fuse(&prior, &info);
        assert!(post[0][0].sqrt() < 0.03, "x pinned: {:.3}", post[0][0].sqrt());
        assert!((post[1][1].sqrt() - 0.20).abs() < 1e-6, "y untouched: {:.3}", post[1][1].sqrt());
        let s = sigmas(&post);
        assert!(s.major_axis.1.abs() > 0.99, "the weak axis is the wall's: {:?}", s.major_axis);
    }

    #[test]
    fn a_disagreeing_window_says_nothing() {
        let cfg = UncertaintyConfig::default();
        let h = [[500.0, 0.0, 0.0], [0.0, 500.0, 0.0], [0.0, 0.0, 200.0]];
        assert!(window_information(&h, 0.15, 500, &cfg).is_none());
        assert!(window_information(&h, 0.01, 10, &cfg).is_none());
    }
}
