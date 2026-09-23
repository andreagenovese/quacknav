//! The mapper's two inputs, from outside robotd: `robot.state` at the
//! loop's own rate, and tofd's `tof.stream`.
//!
//! In the fork the control loop built [`OdomSample`] itself. Every field
//! of it is on `robot.state` since API v24 except two verdicts, rebuilt
//! here the way robotd reaches them (daemon-v0.14.4, `robotd/src/main.rs`):
//!
//!   - `moving`: the policy step is busy (a skill, a rise, a pick), the
//!     robot is homing or riding a fall, or the smoothed twist it applied
//!     is not exactly zero — robotd's `twist_magnitude() > 0.0`, which
//!     `move.applied` publishes. Of the step labels only `walk`, `stand`,
//!     `sit` and `held` can be still.
//!   - `sitting`: the step label is `sit`.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Context;
use duck_ipc_proto as proto;
use serde::Deserialize;

use super::{Host, OdomSample};

const RECONNECT: Duration = Duration::from_secs(2);

/// The newest body verdicts, for the head sweep.
#[derive(Debug, Clone)]
pub struct Body {
    pub moving: bool,
    pub sitting: bool,
    pub fallen: bool,
    /// The step label, `robot.state.policy`.
    pub policy: String,
    pub at: Instant,
}

pub type SharedBody = Arc<Mutex<Option<Body>>>;

/// Start both feeds; they reconnect on loss for the life of the process.
pub fn spawn(host: Host, robotd_socket: String, tof_socket: String) -> SharedBody {
    let body: SharedBody = Arc::new(Mutex::new(None));

    let (state_host, state_body) = (host.clone(), body.clone());
    std::thread::Builder::new()
        .name("maploc-state".into())
        .spawn(move || {
            let mut said = false;
            loop {
                match state_lane(&robotd_socket, &state_host, &state_body, &mut said) {
                    Ok(()) => {}
                    Err(e) if said => tracing::debug!(error = %e, "maploc: robot.state lost; reconnecting"),
                    Err(e) => tracing::info!(error = %e, "maploc: robotd not reachable yet"),
                }
                std::thread::sleep(RECONNECT);
            }
        })
        .expect("spawning the maploc state feed cannot fail");

    std::thread::Builder::new()
        .name("maploc-tof".into())
        .spawn(move || {
            let mut said = false;
            loop {
                match tof_lane(&tof_socket, &host, &mut said) {
                    Ok(()) => {}
                    Err(e) if said => tracing::debug!(error = %e, "maploc: tofd stream ended; reconnecting"),
                    // Common on a board without the sensor, or before tofd:
                    // said once, then quiet.
                    Err(e) => tracing::info!(error = %e, "maploc: tofd's depth stream not reachable yet"),
                }
                std::thread::sleep(RECONNECT);
            }
        })
        .expect("spawning the maploc tof feed cannot fail");

    body
}

/// What the mapper reads of one `robot.state`; everything defaults, so a
/// robotd that adds fields or predates one still parses.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Tick {
    t_ns: u64,
    odom: Odom,
    joints: Vec<f64>,
    safety: Safety,
    policy: String,
    #[serde(rename = "move")]
    movement: Movement,
    imu: Option<serde_json::Value>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Odom {
    position: [f64; 3],
    yaw: f64,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
struct Safety {
    gravity: [f64; 3],
    fallen: bool,
}

impl Default for Safety {
    fn default() -> Self {
        Self { gravity: [0.0, 0.0, -1.0], fallen: false }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Movement {
    applied: [f64; 3],
}

/// Index of `neck_pitch` in `robot.state.joints` (`JOINT_NAMES` order):
/// the four head joints follow the left leg's five.
const HEAD_JOINTS: usize = 5;

/// robotd's own "is the robot doing something" — see the module doc.
pub fn moving(policy: &str, applied: [f64; 3]) -> bool {
    let twist = applied.iter().map(|v| v * v).sum::<f64>().sqrt();
    twist > 0.0 || !matches!(policy, "walk" | "stand" | "sit" | "held")
}

fn sample(tick: &Tick) -> Option<OdomSample> {
    // robotd fed the mapper only once the orientation filter had converged;
    // a state with no IMU section is a loop that has not read one yet.
    tick.imu.as_ref()?;
    let head = tick.joints.get(HEAD_JOINTS..HEAD_JOINTS + 4)?;
    Some(OdomSample {
        odom: (tick.odom.position[0] as f32, tick.odom.position[1] as f32, tick.odom.yaw as f32),
        gravity: tick.safety.gravity,
        trunk_z: tick.odom.position[2],
        head: [head[0], head[1], head[2], head[3]],
        t_ns: tick.t_ns,
        moving: moving(&tick.policy, tick.movement.applied),
        sitting: tick.policy == "sit",
        fallen: tick.safety.fallen,
    })
}

fn connect(path: &str) -> anyhow::Result<UnixStream> {
    let stream = UnixStream::connect(path).with_context(|| format!("connecting to {path}"))?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    Ok(stream)
}

fn send(writer: &mut impl Write, message: &impl serde::Serialize) -> std::io::Result<()> {
    let mut line = serde_json::to_vec(message)?;
    line.push(b'\n');
    writer.write_all(&line)?;
    writer.flush()
}

/// `robot.subscribe` with no rate: every tick, as the loop handed the fork's
/// mapper every tick.
fn state_lane(path: &str, host: &Host, body: &SharedBody, said: &mut bool) -> anyhow::Result<()> {
    let stream = connect(path)?;
    let mut writer = stream.try_clone()?;
    let mut reader = BufReader::new(stream);
    send(
        &mut writer,
        &proto::Request::call(proto::Id::Number(0), &proto::Call::RobotSubscribe(proto::SubscribeParams { hz: None })),
    )?;
    tracing::info!(socket = path, "maploc: subscribed to robot.state");
    *said = true;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            anyhow::bail!("robotd closed the state stream");
        }
        let Ok(request) = serde_json::from_str::<proto::Request>(&line) else {
            continue;
        };
        if request.method != proto::method::ROBOT_STATE {
            continue;
        }
        let Some(params) = request.params else {
            continue;
        };
        let Ok(tick) = serde_json::from_value::<Tick>(params) else {
            continue;
        };
        let Some(sample) = sample(&tick) else {
            continue;
        };
        *body.lock().expect("maploc body poisoned") = Some(Body {
            moving: sample.moving,
            sitting: sample.sitting,
            fallen: sample.fallen,
            policy: tick.policy,
            at: Instant::now(),
        });
        host.observe(sample);
    }
}

/// `tof.stream`, every frame to the mapper.
fn tof_lane(path: &str, host: &Host, said: &mut bool) -> anyhow::Result<()> {
    let stream = connect(path)?;
    let mut writer = stream.try_clone()?;
    let mut reader = BufReader::new(stream);
    send(
        &mut writer,
        &proto::Request::call(
            proto::Id::Number(1),
            &proto::Call::Hello(proto::HelloParams { api_version: proto::API_VERSION }),
        ),
    )?;
    send(&mut writer, &proto::Request::call(proto::Id::Number(2), &proto::Call::TofStream))?;
    tracing::info!(socket = path, "maploc: connected to tofd's depth stream");
    *said = true;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            anyhow::bail!("tofd closed the depth stream");
        }
        if let Ok(request) = serde_json::from_str::<proto::Request>(&line)
            && let Some(frame) = request.as_tof_frame()
        {
            host.frame(frame);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moving_is_robotds_verdict_rebuilt() {
        assert!(!moving("stand", [0.0; 3]));
        assert!(!moving("walk", [0.0; 3]));
        assert!(!moving("sit", [0.0; 3]));
        assert!(!moving("held", [0.0; 3]));
        // The smoothed twist's tail still counts, as `> 0.0` does in robotd.
        assert!(moving("stand", [2.8e-16, 0.0, 0.0]));
        assert!(moving("walk", [0.1, 0.0, 0.3]));
        for busy in ["rise", "ground_pick", "homing", "limp_fall", "limp_pose", "kick_left", "roulade"] {
            assert!(moving(busy, [0.0; 3]), "{busy}");
        }
    }

    #[test]
    fn a_state_becomes_the_forks_sample() {
        let tick: Tick = serde_json::from_value(serde_json::json!({
            "t_ns": 42,
            "odom": {"position": [0.5, -0.25, 0.117], "yaw": 0.3},
            "joints": [0, 0, 0, 0, 0, 0.1, 0.2, 0.3, 0.4, 0, 0, 0, 0, 0, 0],
            "safety": {"fallen": false, "limp": false, "gravity": [0.0, 0.01, -1.0]},
            "policy": "sit",
            "move": {"requested": [0, 0, 0], "applied": [0, 0, 0]},
            "imu": {"gyro": [0, 0, 0], "quat": [1, 0, 0, 0]},
            "head": [0, 0, 0, 0]
        }))
        .unwrap();
        let s = sample(&tick).expect("a full state");
        assert_eq!(s.odom, (0.5, -0.25, 0.3));
        assert_eq!(s.head, [0.1, 0.2, 0.3, 0.4]);
        assert!((s.trunk_z - 0.117).abs() < 1e-12);
        assert_eq!(s.t_ns, 42);
        assert!(s.sitting && !s.moving && !s.fallen);
        // No IMU yet: the fork's loop fed nothing before the filter settled.
        let cold: Tick = serde_json::from_value(serde_json::json!({"joints": vec![0.0; 15], "policy": "held"})).unwrap();
        assert!(sample(&cold).is_none());
    }
}
