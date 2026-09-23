//! The head sweep, which robotd's control loop did for the fork's mapper.
//!
//! A stop that keeps whatever 45° wedge the beak happens to face throws most
//! of the stop away, and relocalization through one static wedge aliases
//! onto any wall at the same range; a slow pan hands the accumulator a
//! ~150° composite instead (the prototype panned at every stop, and its map
//! quality came from that width). The rule is the fork's, unchanged:
//! sweep while standing — every stop, and any time the pose is suspect —
//! and in `continuous` while walking too, narrower. A triangle wave on
//! head yaw, ±0.9 rad over 6 s standing, ±0.6 on the move.
//!
//! From outside the loop that is a `robot.head` notification at 20 Hz;
//! robotd's head slot is sticky and slewed, so the steps arrive smooth.
//! When the sweep ends the head is handed back to centre.
//!
//! The slot is shared — whoever wrote last wins — and robotd's loop, which
//! swept at every stop, beat everybody: a `robot.look` asked of a standing
//! duck lasted one tick, and a voice satellite's thinking sway fought the
//! pan. Here the head is the navigation's only while it needs it:
//!
//!   - **while the navigation drives** (an explore job, a journey, the
//!     homecoming's exploring) **or the mapper searches for its pose** —
//!     a duck standing in the living room keeps its head still;
//!   - **and nobody else is using it.** robotd publishes the commanded
//!     head; the sweep only ever writes yaw, so a pitch, a neck pitch or a
//!     roll there is somebody else's (the thinking pose tilts its roll, a
//!     look pitches the head). The sweep stands aside while one is there
//!     and [`YIELD`] after it goes, and hands nothing back meanwhile. A
//!     pure-yaw pose from elsewhere is the one it cannot tell from its own.

use std::time::{Duration, Instant};

use duck_ipc_proto as proto;

use super::Host;
use super::feed::SharedBody;
use crate::config::MaplocMode;

const TICK: Duration = Duration::from_millis(50);
/// A body verdict older than this is no verdict: the state stream is down.
const STALE: Duration = Duration::from_secs(1);
const SWEEP_STANDING_RAD: f64 = 0.9;
/// The full ±0.9 rad leaves the lane ahead unseen for seconds at a time,
/// and the stairs are seen through this head.
const SWEEP_MOVING_RAD: f64 = 0.6;
const PERIOD_S: f64 = 6.0;
/// How long the sweep stays out of the way after somebody else's pose left
/// the head: a thinking sway recentres when the answer comes, and the
/// answer is often followed by the next question.
const YIELD: Duration = Duration::from_secs(5);
/// A commanded pitch or roll beyond this is not the sweep's (it writes 0,
/// and robotd's slew brings its own writes to 0 within a fraction of this).
const FOREIGN_RAD: f64 = 0.03;

pub fn spawn(host: Host, body: SharedBody, robotd_socket: String) {
    std::thread::Builder::new()
        .name("maploc-sweep".into())
        .spawn(move || {
            let mut lane: Option<quack_duck::Control> = None;
            let mut sweep_t = 0.0f64;
            let mut was_sweeping = false;
            let mut last = Instant::now();
            let mut foreign_until: Option<Instant> = None;
            loop {
                std::thread::sleep(TICK);
                let now = Instant::now();
                let dt = now.duration_since(last).as_secs_f64();
                last = now;

                let body = body.lock().expect("maploc body poisoned").clone();
                let continuous = host.mode() == MaplocMode::Continuous;
                let fresh = body.as_ref().filter(|b| now.duration_since(b.at) < STALE);
                let wanted = host.driving() || host.searching();
                if fresh.is_some_and(|b| foreign(b.commanded_head)) {
                    if wanted && foreign_until.is_none_or(|t| t <= now) {
                        tracing::info!("maploc: somebody else is posing the head; the sweep stands aside");
                    }
                    foreign_until = Some(now + YIELD);
                }
                let yielding = foreign_until.is_some_and(|t| t > now);
                let sweeping = !yielding
                    && wanted
                    && fresh.is_some_and(|b| sweeps(&b.policy, b.moving, b.sitting, b.fallen, continuous));
                let yaw = if sweeping {
                    sweep_t += dt;
                    let moving = body.as_ref().is_some_and(|b| b.moving);
                    (if moving { SWEEP_MOVING_RAD } else { SWEEP_STANDING_RAD }) * triangle(sweep_t)
                } else {
                    sweep_t = 0.0;
                    if !was_sweeping {
                        continue;
                    }
                    was_sweeping = false;
                    if yielding {
                        continue; // the head is somebody else's: nothing handed back
                    }
                    0.0 // hand the head back to centre, once
                };
                was_sweeping = sweeping;

                if lane.is_none() {
                    lane = quack_duck::Control::connect(&robotd_socket).ok();
                }
                let head = proto::Call::RobotHead(proto::HeadParams {
                    neck_pitch: 0.0,
                    head_pitch: 0.0,
                    head_yaw: yaw,
                    head_roll: 0.0,
                });
                if let Some(control) = lane.as_mut()
                    && control.notify(&head).is_err()
                {
                    lane = None;
                }
            }
        })
        .expect("spawning the maploc sweep cannot fail");
}

/// The fork's gate, with one guard it had implicitly: the loop only swept a
/// robot its policy was driving, so a held or seated duck is left alone.
/// The fork also spelled a `searching ||` term, which the "standing, or
/// continuous" term beside it made moot: a suspect pose sweeps at the
/// stops like any other, and never on the move outside `continuous`.
fn sweeps(policy: &str, moving: bool, sitting: bool, fallen: bool, continuous: bool) -> bool {
    let driving = matches!(policy, "stand" | "walk");
    driving && !sitting && !fallen && (!moving || continuous)
}

/// Somebody else's pose: a commanded neck pitch, head pitch or roll, which
/// the sweep never writes.
fn foreign(commanded: [f64; 4]) -> bool {
    [commanded[0], commanded[1], commanded[3]].iter().any(|v| v.abs() > FOREIGN_RAD)
}

/// A triangle wave in [-1, 1] with a [`PERIOD_S`] period, starting from
/// centre: slow enough that every wall cell stays in view for the
/// accumulator's 3-frame vote.
fn triangle(t: f64) -> f64 {
    let phase = (t / PERIOD_S + 0.25).fract();
    if phase < 0.5 { 4.0 * phase - 1.0 } else { 3.0 - 4.0 * phase }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_head_sweeps_at_stops_only_unless_continuous() {
        assert!(sweeps("stand", false, false, false, false));
        assert!(!sweeps("walk", true, false, false, false), "not while walking");
        assert!(sweeps("walk", true, false, false, true), "continuous sweeps on the move");
        assert!(!sweeps("sit", false, true, false, false));
        assert!(!sweeps("held", false, false, false, false));
        assert!(!sweeps("stand", false, false, true, false));
    }

    #[test]
    fn a_pose_the_sweep_never_writes_is_somebody_elses() {
        assert!(!foreign([0.0, 0.0, 0.9, 0.0]), "the sweep's own");
        assert!(!foreign([0.0, 0.0, -0.4, 0.01]), "its own, slewing to zero");
        assert!(foreign([0.0, 0.0, 0.15, 0.10]), "the thinking pose's roll");
        assert!(foreign([0.0, 0.35, 0.2, 0.0]), "a look's pitch");
        assert!(foreign([-0.2, 0.0, 0.0, 0.0]), "a neck pitch");
    }

    #[test]
    fn the_wave_starts_at_centre_and_spans_both_sides() {
        assert!(triangle(0.0).abs() < 1e-12);
        assert!((triangle(1.5) - 1.0).abs() < 1e-12);
        assert!((triangle(4.5) + 1.0).abs() < 1e-12);
    }
}
