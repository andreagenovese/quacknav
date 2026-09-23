//! The body's own commands: what a walk becomes on the wire once the
//! gait's trim is applied, the timed move with its optional heading
//! hold, and the small helpers every tool needs to read an argument or
//! reach robotd.
//!
//! Split out of quacksat's `tools.rs` on 2026-09-22: the voice satellite
//! and the navigator both send these.

use std::time::{Duration, Instant};

use crate::lane::Control;
use duck_ipc_proto as proto;
use serde_json::Value;

pub const MAX_MOVE_DURATION_S: f64 = 3.0;
/// Pollen's own gamepad commands 0.3 m/s at full deflection (padd's
/// `--max-linear` default), and the walking policy does not step below
/// about 0.25 m/s commanded (measured on the MuJoCo twin: 1 cm in 5 s at
/// 0.2, 53 cm at 0.3). A cap under that made `robot.move` inert.
pub const MAX_SPEED_M_S: f64 = 0.3;
pub const MAX_YAW_RAD_S: f64 = 1.0;
/// Turning from a standstill (vx and vy both 0) is allowed past
/// [`MAX_YAW_RAD_S`]: the walking policy has a dead zone there and does not
/// turn at all below it — 2–4°/s at 0.7 and 0.9 — while above it it turns
/// in place, the body within 4 cm: 30°/s at +1.2, 50–60°/s at ±1.5 (the
/// right side's threshold is higher: −1.2 barely turns). Measured on the
/// MuJoCo twin, fork and daemon-v0.14.4 alike, 2026-09-23
/// (scripts/twin/turnprobe.py).
pub const MAX_TURN_IN_PLACE_RAD_S: f64 = 1.6;
/// What a turn in place asks for: past both sides' thresholds.
pub const TURN_IN_PLACE_RAD_S: f64 = 1.5;
pub const MAX_LOOK_XY_M: f64 = 3.0;
pub const MIN_LOOK_Z_M: f64 = -0.2;
pub const MAX_LOOK_Z_M: f64 = 2.0;
pub const MAX_HEAD_PITCH_RAD: f64 = 0.6;
pub const MAX_HEAD_YAW_RAD: f64 = 1.2;
pub const MAX_HEAD_ROLL_RAD: f64 = 0.5;
/// Intent cadence while a timed move runs (well inside the 500 ms deadman).
pub const MOVE_TICK: Duration = Duration::from_millis(40);
/// A mapping step's stand: robotd's still window needs 0.5 s, a window
/// flushes after 3 s, and the head sweep that turns a 45° wedge into a
/// ~150° composite takes 6 s per triangle.
pub const DEFAULT_STOP_S: f64 = 6.0;
pub const MAX_STOP_S: f64 = 10.0;
/// A mapping step must end at least this far from a mapped wall ahead —
/// the sensor's blind band is 10 cm and the gait wanders.
/// `QK_WALL_MARGIN_M`: 0.18 since 2026-09-16 (was 0.25) (the user's rule: shrink the
/// margins, the refusals must be nearly none) — the flank 8 cm from the
/// wall at the leg's end, and the leg is shortened before it is refused.
/// How far a leg's own yaw rate turns it per unit of command
/// (rad/s per unit, measured on the twin).
pub const YAW_RATE_PER_UNIT: f64 = 0.65;

pub fn step_advance_m(vx: f64, vyaw: f64, walk_s: f64) -> f64 {
    let arc = (vyaw.abs() / MAX_ARC_YAW_RAD_S).min(1.0);
    // Turning costs little forward speed: 0.110 m/s at vyaw 0.7 against
    // 0.121 straight, measured on the human drive (2026-09-07). The
    // quarter this had before let arcs end against walls.
    // `QUACKSAT_GUARD_ARC_FULL=1` judges with the measured advance. Off by
    // default: it is the truth about the gait, but on the paper twin the
    // guard then refuses so much that the full flat drops from 55 to 40 %
    // (falls stayed at zero either way). A debt to settle on MuJoCo.
    let slow = if std::env::var("QUACKSAT_GUARD_ARC_FULL").is_ok_and(|v| v == "1") { 0.1 } else { 0.75 };
    0.4 * vx.abs() * walk_s * (1.0 - slow * arc)
}

/// The yaw rate of a full turning arc.
pub const MAX_ARC_YAW_RAD_S: f64 = 0.7;

/// The yaw a command may carry: more for a turn in place than for a walk.
pub fn yaw_cap(vx: f64, vy: f64) -> f64 {
    if vx == 0.0 && vy == 0.0 { MAX_TURN_IN_PLACE_RAD_S } else { MAX_YAW_RAD_S }
}

pub fn move_params(args: &Value) -> proto::MoveParams {
    let (vx, vy) = (clamp(number(args, "vx"), MAX_SPEED_M_S), clamp(number(args, "vy"), MAX_SPEED_M_S));
    proto::MoveParams { vx, vy, vyaw: clamp(number(args, "vyaw"), yaw_cap(vx, vy)) }
}

/// The `[gait]` corrections, applied last, to what is actually sent.
pub fn trimmed(gait: &crate::gait::GaitConfig, mut params: proto::MoveParams) -> proto::MoveParams {
    params.vyaw = clamp(gait.yaw(params.vx, params.vyaw), yaw_cap(params.vx, params.vy));
    params
}

/// Timed walk: pump the continuous intent for the duration, then go
/// silent — robotd's deadman remains the backstop.
/// The same walk, the heading held: the odometry's yaw read every tick
/// against the heading the leg means to have — where it began plus the
/// steering asked for, integrated — and a TAP the other way when it
/// strays by [`HOLD_THRESHOLD_RAD`]. Measured on the twin (2026-09-18,
/// `straightprobe.py`): walking straight the gait drifts +3.5°/s on
/// average (−0.1 … +10, the trim over-correcting now), 1–14 cm of lateral
/// drift per metre; a tap of ±1.0 on the wire turns about 50°/s × its
/// length (0.2 s → 10°, 0.3 s → 15°, erratic beyond); with a 4° threshold
/// and 0.2 s taps the lateral drift is 1–3 cm/m and the heading ends
/// within ±4°. The user's formula: "if it pulls right, brief taps to the
/// left, and it straightens and goes on". Not for arcs (|vyaw| ≥ 0.5
/// before the trim): those turn on purpose. `QK_HOLD_HEADING=0` is the
/// open loop.
/// A timed `robot.move`, optionally holding its heading by taps: the
/// yaw source is a closure — the cliff guard's odometry heading where
/// there is one — so the body's lane knows nothing of the navigation
/// (the split of 2026-09-22).
pub fn timed_move_held(
    control: &mut Option<Control>,
    params: proto::MoveParams,
    duration_s: f64,
    hold: Option<(&dyn Fn() -> Option<f64>, f64, f64)>,
) -> Result<(), String> {
    let end = Instant::now() + Duration::from_secs_f64(duration_s);
    let started = Instant::now();
    let mut yaw0: Option<f64> = None;
    let mut tap_until: Option<(Instant, f64)> = None;
    while Instant::now() < end {
        let mut send = params;
        if let Some((yaw_now, vyaw_cmd, bias)) = hold
            && params.vx > 0.0
        {
            let now = Instant::now();
            if let Some((until, tap)) = tap_until {
                if now < until {
                    send.vyaw = tap;
                } else {
                    tap_until = None;
                }
            }
            if tap_until.is_none()
                && let Some(yaw) = yaw_now()
            {
                let y0 = *yaw0.get_or_insert(yaw);
                let meant = y0 + bias + YAW_RATE_PER_UNIT * vyaw_cmd * (now - started).as_secs_f64();
                let e = wrap(meant - yaw);
                if e.abs() > HOLD_THRESHOLD_RAD {
                    let tap = HOLD_TAP_WIRE.copysign(e);
                    tap_until = Some((now + Duration::from_secs_f64(HOLD_TAP_S), tap));
                    send.vyaw = tap;
                }
            }
        }
        notify(control, &proto::Call::RobotMove(send))?;
        std::thread::sleep(MOVE_TICK);
    }
    Ok(())
}
/// The heading hold's threshold, tap length and tap size on the wire.
pub const HOLD_THRESHOLD_RAD: f64 = 0.07;
/// A leg steered less than this is straight, and held.
pub const HOLD_STRAIGHT_MAX: f64 = 0.05;
pub const HOLD_TAP_S: f64 = 0.2;
pub const HOLD_TAP_WIRE: f64 = 1.0;
/// `QK_HOLD_HEADING=1` turns the hold on. OFF by default: measured on the
/// twin (2026-09-18) it pays on a 4 s straight walk (1–3 cm/m) and costs
/// on the explorer's legs of 1–1.5 s — a 4° threshold met once a leg by
/// the +3.5°/s drift, and a 10° tap in answer, is a zigzag: the blind
/// six-goal tour went 472–499 s → 634 s held on every non-arc leg, 682 s
/// held on straight legs alone. For long straight legs (a corridor, a
/// human drive) it is the formula to use; the explorer's legs are too
/// short for it as it stands.
pub fn hold_heading() -> bool {
    std::env::var("QK_HOLD_HEADING").is_ok_and(|v| v == "1")
}
pub fn wrap(a: f64) -> f64 {
    a.sin().atan2(a.cos())
}

/// One leg of a stop-and-scan tour: walk, then stand, then say what the
/// stop did to the map. The stand is the point — robotd only inks while
/// the robot is still, and sweeps the head on its own during a stop.
/// What a mapping step will do once the guards have spoken: the
/// command actually sent (trimmed by `[gait]`), how long, the stand after
/// it, and what shortened or steered it. Pure: the real step executes it,
/// the paper twin applies it to its motion model — one set of guards.
pub fn number(args: &Value, key: &str) -> f64 {
    args.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

pub fn clamp(value: f64, limit: f64) -> f64 {
    value.clamp(-limit, limit)
}

pub fn require_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{key} is required"))
}

pub fn with_robot(control: &mut Option<Control>) -> Result<&mut Control, String> {
    control
        .as_mut()
        .ok_or_else(|| "robot unreachable".to_string())
}

pub fn notify(control: &mut Option<Control>, call: &proto::Call) -> Result<(), String> {
    let robot = with_robot(control)?;
    robot.notify(call).map_err(|e| {
        *control = None;
        format!("robot lost: {e}")
    })
}

pub fn request(control: &mut Option<Control>, call: &proto::Call) -> Result<proto::Response, String> {
    let robot = with_robot(control)?;
    robot.request(call).map_err(|e| {
        *control = None;
        format!("robot lost: {e}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_turn_in_place_may_ask_past_the_walking_cap() {
        let still = move_params(&json!({"vx": 0.0, "vyaw": TURN_IN_PLACE_RAD_S}));
        assert_eq!(still.vyaw, TURN_IN_PLACE_RAD_S);
        let walking = move_params(&json!({"vx": 0.3, "vyaw": TURN_IN_PLACE_RAD_S}));
        assert_eq!(walking.vyaw, MAX_YAW_RAD_S);
        let sideways = move_params(&json!({"vy": 0.2, "vyaw": -2.0}));
        assert_eq!(sideways.vyaw, -MAX_YAW_RAD_S);
        // The trim leaves a standstill alone, so a turn in place is sent as asked.
        let gait = crate::gait::GaitConfig { yaw_trim: 0.08, ..Default::default() };
        assert_eq!(trimmed(&gait, still).vyaw, TURN_IN_PLACE_RAD_S);
    }
}
