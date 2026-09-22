//! A depth frame as a 2D scan, from the head FK the `kinematics` crate
//! publishes.
//!
//! In the robotd fork this was `kinematics::tof::Reprojector::flatten`;
//! the released `kinematics` (daemon-v0.14.4) has `project` but not
//! `flatten`, so the half that turns projected zones into a levelled
//! polar scan lives here, beside its one consumer. Behind the
//! `kinematics` feature: the SLAM itself does not need the head.

use kinematics::Quat;
use kinematics::tof::{Posture, Reprojector, Zone};

/// Zones in one ToF frame (8×8).
pub const N_ZONES: usize = 64;

/// A depth frame as a 2D scan — what a mapper consumes.
#[derive(Debug, Clone)]
pub struct FlatScan {
    /// Beam azimuths about the levelled body Z, radians.
    pub angles_body: Vec<f32>,
    /// Horizontal range from the sensor, metres. Parallel to `angles_body`.
    pub ranges: Vec<f32>,
    /// The sensor's levelled body-frame position the ranges start at.
    pub sensor_xy: (f32, f32),
}

/// Reproject one frame and flatten it: every `Hit` zone, levelled by the
/// trunk's gravity, as an azimuth and a horizontal range from the sensor.
/// Floor and too-close returns are already filtered out by `project`.
pub fn flatten(
    rp: &Reprojector,
    ranges_m: &[Option<f64>; N_ZONES],
    head_joints: [f64; 4],
    posture: &Posture,
) -> FlatScan {
    let zones = rp.project(ranges_m, head_joints, posture);
    let level = level_from_gravity(posture.gravity);
    let sensor = level.rotate(rp.sensor_in_trunk(head_joints).pos);

    let mut angles = Vec::with_capacity(N_ZONES);
    let mut ranges = Vec::with_capacity(N_ZONES);
    for zone in zones {
        let Zone::Hit { point, .. } = zone else {
            continue;
        };
        let p = level.rotate(point);
        let (dx, dy) = (p[0] - sensor[0], p[1] - sensor[1]);
        let range = (dx * dx + dy * dy).sqrt();
        if range <= 0.0 {
            continue;
        }
        angles.push(dy.atan2(dx) as f32);
        ranges.push(range as f32);
    }
    FlatScan {
        angles_body: angles,
        ranges,
        sensor_xy: (sensor[0] as f32, sensor[1] as f32),
    }
}

/// The rotation taking the trunk-frame gravity to straight down. Identity for
/// a gravity too small to trust — an IMU that has not converged should level
/// nothing rather than something random. (A copy of `kinematics::tof`'s
/// private helper, which `project` uses for the same levelling.)
fn level_from_gravity(gravity: [f64; 3]) -> Quat {
    let n = (gravity[0] * gravity[0] + gravity[1] * gravity[1] + gravity[2] * gravity[2]).sqrt();
    if n < 0.5 {
        return Quat::IDENTITY;
    }
    let g = [gravity[0] / n, gravity[1] / n, gravity[2] / n];
    let down = [0.0, 0.0, -1.0f64];
    let axis = [
        g[1] * down[2] - g[2] * down[1],
        g[2] * down[0] - g[0] * down[2],
        g[0] * down[1] - g[1] * down[0],
    ];
    let s = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
    let c = g[0] * down[0] + g[1] * down[1] + g[2] * down[2];
    if s < 1e-9 {
        return if c > 0.0 {
            Quat::IDENTITY
        } else {
            // Hanging exactly upside down: any horizontal axis will do.
            Quat::from_axis_angle([1.0, 0.0, 0.0], std::f64::consts::PI)
        };
    }
    Quat::from_axis_angle([axis[0] / s, axis[1] / s, axis[2] / s], s.atan2(c))
}
