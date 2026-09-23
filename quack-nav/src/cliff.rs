//! Cliff guard: the depth sensor's downward beams, checked against the
//! floor they should hit.
//!
//! A 2D map of walls cannot represent a drop: over a staircase hole the
//! floor beams either come back much longer (the steps below) or not at
//! all, and robotd's reprojection files both away — a long return is still
//! "floor", a missing one is "empty" — so the map shows the hole as
//! unknown at best and, once a wall beyond it has been seen, as free floor.
//! Measured on the MuJoCo twin facing the stairwell: the bottom row read
//! 43–47 cm where the floor was and 100–119 cm or nothing where it was not,
//! against 44 cm expected.
//!
//! So this reads the raw frames itself (tofd's stream is open to the
//! `robot` group, like robotd's), reprojects them through the same head
//! geometry robotd uses (`kinematics::tof`), and calls a beam a **drop**
//! when it looks down enough to expect the floor within reach and the
//! return is either missing or at least [`DEEP_RATIO`] times too long. A
//! frame with fewer than [`MIN_BEAMS`] such beams is noise. Drops are kept
//! for [`MEMORY`] in the body frame, so a standing head sweep accumulates
//! a picture the mapping step can consult before walking.
//!
//! Frames are judged **only while the body stands**. Walking pitches and
//! bounces the trunk, the pose from `robot.state` is not synchronous with
//! the depth frame, and the floor then lands where it is not expected: on
//! the twin a walking duck reported drops on flat floor that a standing
//! one never saw. Whatever was seen before the body moved is forgotten
//! the moment it moves, because it was seen from somewhere else.
//!
//! Caveats, to be settled on hardware: a real sensor also returns nothing
//! on very dark floors, and its far-range noise grows; the thresholds here
//! come from the twin. The guard advises; robotd's safety stays in charge.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Context;
use duck_ipc_proto as proto;
use kinematics::tof::{COLS, Posture, ROWS, Reprojector, Zone};
use serde::Deserialize;

/// A return this many times longer than the floor distance is a drop.
pub const DEEP_RATIO: f64 = 1.5;
/// A missing return counts only where the floor would be within this
/// slant distance — farther out the sensor may simply be out of range.
pub const MAX_FLOOR_M: f64 = 1.2;
/// Beams looking down by less than this (sine of the depression) are not
/// judged: they would meet the floor too far away to trust.
pub const MIN_DOWNWARD: f64 = 0.15;
/// Drops in one frame below this count are noise.
pub const MIN_BEAMS: usize = 2;
/// How long a seen drop stays on the books while the body stands.
/// Where the bottom row itself is over a drop the edge is somewhere
/// between the beak and that beam's floor distance; taken this much short
/// of it (about a row's spacing on the floor).
const EDGE_UNKNOWN_M: f64 = 0.10;
pub const MEMORY: Duration = Duration::from_secs(3);
/// How long frames are kept at all — a head sweep and a little, for the
/// readers that need both sides of the body.
pub const KEEP: Duration = Duration::from_secs(8);

const RECONNECT: Duration = Duration::from_secs(2);
const STATE_HZ: u32 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropKind {
    /// No return where the floor should have been.
    Missing,
    /// A return well beyond the floor: something lower than the floor.
    Deep,
}

/// One beam's verdict, in the body frame at the time of the frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Drop {
    /// Radians, positive to the left, from the body's forward axis.
    pub bearing: f64,
    /// Horizontal distance from the body to where the floor was expected —
    /// the edge is no farther than this.
    pub range_m: f64,
    /// The edge is no *nearer* than this: the floor distance of the
    /// closest beam below this one, in the same column, that still met
    /// the floor. Zero when the bottom row itself is over the drop — the
    /// floor ends somewhere between the beak and `range_m`.
    pub edge_min_m: f64,
    pub kind: DropKind,
}

/// Something standing in a beam's way, in the body frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Obstacle {
    /// Radians, positive to the left, from the body's forward axis.
    pub bearing: f64,
    /// Horizontal distance from the trunk, metres.
    pub range_m: f64,
}

/// Half of the sensor's field of view, radians: what one frame covers
/// around the head's yaw.
pub const HALF_FOV_RAD: f64 = 0.39;

/// What one depth frame said.
#[derive(Debug, Clone)]
pub struct CliffFrame {
    pub seq: u64,
    pub at: Instant,
    pub head_yaw: f64,
    /// Judged while the body walked: the trunk pitches and bobs, so
    /// walls and edge distances from it are not to be trusted — a hole
    /// under a metre is (the floor far below, or gone, is not a thing a
    /// few degrees of pitch invent). Every query here skips such frames
    /// unless it asks for them (`hole_in_lane_walking`).
    pub moving: bool,
    pub drops: Vec<Drop>,
    /// Returns robotd's own reprojection calls obstacles (not floor, not
    /// too close): what is in the way, as far as this frame looked.
    pub obstacles: Vec<Obstacle>,
    /// Beams that met the floor where expected.
    pub floor_beams: usize,
    /// Beams that looked down enough to be judged at all.
    pub judged: usize,
}

/// The body as the geometry needs it, from `robot.state`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyPose {
    /// `[neck_pitch, head_pitch, head_yaw, head_roll]`, measured.
    pub head: [f64; 4],
    /// Projected gravity in the trunk frame; upright is about `[0, 0, -1]`.
    pub gravity: [f64; 3],
    /// Trunk height above the floor, metres.
    pub trunk_z: f64,
    /// The body is walking (robotd's policy label): frames are not judged.
    pub moving: bool,
}

impl BodyPose {
    /// Standing upright, head centred, at the model's rest height.
    pub fn upright(trunk_z: f64) -> Self {
        Self {
            head: [0.0; 4],
            gravity: [0.0, 0.0, -1.0],
            trunk_z,
            moving: false,
        }
    }
}

/// The three fields of `robot.state` the guard reads, taken leniently so
/// a newer robotd (whose other fields moved) still feeds it.
#[derive(Deserialize)]
struct StateBits {
    /// What the head was *commanded* — the sweep and the aim, offsets
    /// from the policy's own posture. Not where the head is.
    head: [f64; 4],
    /// Measured joint angles in [`proto::JOINT_NAMES`] order; the head is
    /// `neck_pitch, head_pitch, head_yaw, head_roll` at 5..9. Empty from a
    /// daemon predating it.
    #[serde(default)]
    joints: Vec<f64>,
    safety: SafetyBits,
    odom: OdomBits,
    /// "walk" while the gait runs; anything else is a standing body.
    #[serde(default)]
    policy: String,
}
impl StateBits {
    /// The head as the reprojection needs it: the measured joints. The
    /// commanded `head` is offsets on top of the policy's posture, and the
    /// walking policy holds the neck and head pitched a good 0.2–0.5 rad
    /// down; projected as level, every floor return of the near rows landed
    /// 7–20 cm above the floor and read as an obstacle 0.37 m ahead — the
    /// phantom that refused every straight leg longer than a second and a
    /// half, on the twin, for a week (found 2026-09-14 when a wake-up's legs
    /// were refused in an empty room). Falls back to the commanded head for
    /// a daemon that sends no joints.
    fn head_joints(&self) -> [f64; 4] {
        const HEAD: [&str; 4] = ["neck_pitch", "head_pitch", "head_yaw", "head_roll"];
        if self.joints.len() < proto::JOINT_NAMES.len() {
            return self.head;
        }
        let mut out = [0.0; 4];
        for (slot, name) in out.iter_mut().zip(HEAD) {
            if let Some(i) = proto::JOINT_NAMES.iter().position(|n| *n == name) {
                *slot = self.joints[i];
            }
        }
        out
    }
}

#[derive(Deserialize)]
struct SafetyBits {
    gravity: [f64; 3],
}
#[derive(Deserialize)]
struct OdomBits {
    position: [f64; 3],
    /// Heading from odometry, radians; present in API v17.
    #[serde(default)]
    yaw: Option<f64>,
}

/// Judge one frame. `None` when the frame is not the 8×8 the geometry
/// knows.
pub fn analyze(
    rp: &Reprojector,
    frame: &proto::TofFrame,
    body: &BodyPose,
    now: Instant,
) -> Option<CliffFrame> {
    const N: usize = ROWS * COLS;
    if frame.distance_mm.len() != N || frame.status.len() != N {
        return None;
    }
    let sensor = rp.sensor_in_trunk(body.head);
    // Unit "down" in the trunk frame, from the IMU.
    let g = body.gravity;
    let gn = (g[0] * g[0] + g[1] * g[1] + g[2] * g[2]).sqrt();
    let down = if gn > 1e-6 {
        [g[0] / gn, g[1] / gn, g[2] / gn]
    } else {
        [0.0, 0.0, -1.0]
    };
    // The sensor's height above the floor: the trunk's, plus how far the
    // sensor sits above the trunk origin along "up".
    let above_floor = body.trunk_z
        - (sensor.pos[0] * down[0] + sensor.pos[1] * down[1] + sensor.pos[2] * down[2]);

    // First pass: every judged beam's verdict, by zone index.
    #[derive(Clone, Copy)]
    enum Verdict {
        Floor(f64),
        Drop(f64, f64, DropKind),
    }
    let mut verdicts: [Option<Verdict>; N] = [None; N];
    let mut floor_beams = 0;
    let mut judged = 0;
    for (i, beam) in rp.beams().iter().enumerate() {
        let dir = sensor.quat.rotate(*beam);
        let downward = dir[0] * down[0] + dir[1] * down[1] + dir[2] * down[2];
        if downward < MIN_DOWNWARD || above_floor <= 0.0 {
            continue;
        }
        let expected = above_floor / downward;
        if expected > MAX_FLOOR_M {
            continue;
        }
        judged += 1;
        let horizontal = (1.0 - downward * downward).max(0.0).sqrt();
        let bearing = dir[1].atan2(dir[0]);
        let range_m = expected * horizontal;
        let valid = matches!(frame.status[i], 5 | 9) && frame.distance_mm[i] > 0;
        if !valid {
            verdicts[i] = Some(Verdict::Drop(bearing, range_m, DropKind::Missing));
            continue;
        }
        let r = f64::from(frame.distance_mm[i]) / 1000.0;
        if r >= DEEP_RATIO * expected {
            verdicts[i] = Some(Verdict::Drop(bearing, range_m, DropKind::Deep));
        } else {
            verdicts[i] = Some(Verdict::Floor(range_m));
            floor_beams += 1;
        }
    }
    // Second pass: a drop's edge is no nearer than the closest beam below
    // it (same column, lower row = nearer the robot) that still met the
    // floor. Rows count from the top of the grid; row 7 looks nearest.
    let mut drops = Vec::new();
    for i in 0..N {
        let Some(Verdict::Drop(bearing, range_m, kind)) = verdicts[i] else {
            continue;
        };
        let (row, col) = (i / COLS, i % COLS);
        let edge_min_m = (row + 1..ROWS)
            .find_map(|r| match verdicts[r * COLS + col] {
                Some(Verdict::Floor(d)) => Some(d),
                _ => None,
            })
            .unwrap_or(0.0);
        drops.push(Drop {
            bearing,
            range_m,
            edge_min_m,
            kind,
        });
    }
    if drops.len() < MIN_BEAMS {
        drops.clear();
    }
    // Obstacles, by robotd's own floor and range filters.
    let mut ranges = [None; N];
    for (i, slot) in ranges.iter_mut().enumerate() {
        if matches!(frame.status[i], 5 | 9) && frame.distance_mm[i] > 0 {
            *slot = Some(f64::from(frame.distance_mm[i]) / 1000.0);
        }
    }
    let posture = Posture {
        gravity: body.gravity,
        trunk_height_m: (body.trunk_z > 0.02).then_some(body.trunk_z),
    };
    let obstacles = rp
        .project(&ranges, body.head, &posture)
        .into_iter()
        .filter_map(|zone| match zone {
            Zone::Hit { point, range } => Some(Obstacle {
                bearing: point[1].atan2(point[0]),
                range_m: range,
            }),
            _ => None,
        })
        .collect();
    Some(CliffFrame {
        moving: body.moving,
        seq: frame.seq,
        at: now,
        head_yaw: body.head[2],
        drops,
        obstacles,
        floor_beams,
        judged,
    })
}

/// Whether the depth stream is there at all.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum StreamState {
    #[default]
    Unknown,
    Serving,
    /// tofd answered but has no sensor (its own word for why).
    Unavailable(String),
}

#[derive(Debug, Clone, Default)]
pub struct CliffStatus {
    pub stream: StreamState,
    /// A `robot.state` has been seen, so frames can be judged.
    pub body_seen: bool,
    /// The newest odometry heading (radians), at the state stream's rate —
    /// what a turn should be closed on, the map's pose coming once a second.
    pub odom_yaw: Option<f64>,
    pub frames: u64,
    /// Frames judged in the last [`MEMORY`].
    pub recent: Vec<CliffFrame>,
}

impl CliffStatus {
    pub fn absorb(&mut self, frame: CliffFrame) {
        let now = frame.at;
        self.frames += 1;
        self.recent.push(frame);
        // Kept for [`KEEP`]: every judgement here filters by [`MEMORY`],
        // and a passage's side boundaries (`tools::passage_here`) look
        // back a whole head sweep.
        self.recent.retain(|f| now.duration_since(f.at) <= KEEP);
    }

    /// The nearest drop seen within [`MEMORY`] inside a cone of
    /// `half_angle` around `bearing` (body frame).
    /// The nearest drop in the body's lane along `heading`: ahead, and
    /// within `half_width` of the line the duck would walk. A cone made a
    /// stairwell beside the path block the path itself.
    pub fn drop_in_lane(&self, now: Instant, heading: f64, half_width: f64) -> Option<Drop> {
        self.recent
            .iter()
            .filter(|f| now.duration_since(f.at) <= MEMORY && !f.moving)
            .flat_map(|f| f.drops.iter())
            .filter(|d| {
                let a = wrap(d.bearing - heading);
                a.cos() > 0.0 && d.range_m * a.sin().abs() <= half_width
            })
            .min_by(|a, b| a.range_m.total_cmp(&b.range_m))
            .copied()
    }

    pub fn drop_within(&self, now: Instant, bearing: f64, half_angle: f64) -> Option<Drop> {
        self.recent
            .iter()
            .filter(|f| now.duration_since(f.at) <= MEMORY && !f.moving)
            .flat_map(|f| f.drops.iter())
            .filter(|d| wrap(d.bearing - bearing).abs() <= half_angle)
            .min_by(|a, b| a.range_m.total_cmp(&b.range_m))
            .copied()
    }

    /// How near the nearest true hole the recent stand frames saw comes
    /// to the body, in metres: its edge where the sensor bounds it, else
    /// (the bottom row itself over the drop, `edge_min_m` zero) a beam's
    /// spacing short of where the floor was expected. A wall's foot — a
    /// drop with an obstacle at its bearing and range — is not a hole.
    /// Reading `edge_min_m` as the distance put every such drop at the
    /// beak: no turn in place was allowed anywhere near the stairwell, and
    /// a duck that stopped beside it could neither turn nor walk (paper
    /// twin, 2026-09-23: 678 refusals on the spot).
    pub fn nearest_hole_m(&self, now: Instant) -> Option<f64> {
        self.recent
            .iter()
            .filter(|f| now.duration_since(f.at) <= MEMORY && !f.moving)
            .flat_map(|f| {
                f.drops.iter().filter(move |d| {
                    !f.obstacles.iter().any(|o| wrap(o.bearing - d.bearing).abs() < 0.2 && (o.range_m - d.range_m).abs() < 0.25)
                })
            })
            .map(|d| if d.edge_min_m > 0.0 { d.edge_min_m } else { (d.range_m - EDGE_UNKNOWN_M).max(0.0) })
            .min_by(f64::total_cmp)
    }

    /// Any drop on the books, nearest first.
    pub fn nearest(&self, now: Instant) -> Option<Drop> {
        self.drop_within(now, 0.0, std::f64::consts::PI)
    }

    /// Whether a recent frame looked along `bearing` (body frame) at all.
    pub fn looked_at(&self, now: Instant, bearing: f64) -> bool {
        self.recent
            .iter()
            .filter(|f| now.duration_since(f.at) <= MEMORY && !f.moving)
            .any(|f| wrap(f.head_yaw - bearing).abs() <= HALF_FOV_RAD)
    }

    /// The nearest obstacle seen within [`MEMORY`] inside a cone of
    /// `half_angle` around `bearing` (body frame).
    /// The nearest obstacle in the body's lane along `heading`: ahead of
    /// the duck and within `half_width` of the line it would walk. A cone
    /// blocked a 0.4 m doorway from half a metre away, its posts being
    /// inside ±23°; a lane the body's width lets the duck through.
    pub fn obstacle_in_lane(
        &self,
        now: Instant,
        heading: f64,
        half_width: f64,
    ) -> Option<Obstacle> {
        self.recent
            .iter()
            .filter(|f| now.duration_since(f.at) <= MEMORY && !f.moving)
            .flat_map(|f| f.obstacles.iter())
            .filter(|o| {
                let d = wrap(o.bearing - heading);
                d.cos() > 0.0 && o.range_m * d.sin().abs() <= half_width
            })
            .min_by(|a, b| a.range_m.total_cmp(&b.range_m))
            .copied()
    }

    /// The nearest true hole in the lane along `heading`, from every
    /// frame of the last `within` — walking ones included — that at
    /// least `min_frames` frames saw: the blind leg's guard, which must
    /// see while the body walks (a pose 0.6 m off along a corridor put
    /// a walking duck into the stairwell with no frame judged since the
    /// last stand, 2026-09-19). A drop with an obstacle at its bearing
    /// and range is a wall's foot, not a hole.
    /// The nearest obstacle in the lane along `heading` within `reach`,
    /// from the frames of the last `within` — walking ones included —
    /// seen by at least `min_frames` of them: something put down in
    /// front of a walking duck (a box, a shoe, a person's foot). The
    /// pitching trunk is worth centimetres at this range, not the
    /// decimetres it is worth on a wall two metres off; `reach` keeps
    /// it near.
    pub fn obstacle_in_lane_walking(&self, now: Instant, heading: f64, half_width: f64, reach: f64, within: Duration, min_frames: usize) -> Option<Obstacle> {
        let mut hits: Vec<Obstacle> = Vec::new();
        for f in self.recent.iter().filter(|f| now.duration_since(f.at) <= within) {
            let nearest = f
                .obstacles
                .iter()
                .filter(|o| {
                    let a = wrap(o.bearing - heading);
                    a.cos() > 0.0 && o.range_m * a.sin().abs() <= half_width && o.range_m < reach
                })
                .min_by(|a, b| a.range_m.total_cmp(&b.range_m));
            if let Some(o) = nearest {
                hits.push(*o);
            }
        }
        if hits.len() < min_frames {
            return None;
        }
        hits.into_iter().min_by(|a, b| a.range_m.total_cmp(&b.range_m))
    }

    pub fn hole_in_lane_walking(&self, now: Instant, heading: f64, half_width: f64, reach: f64, within: Duration, min_frames: usize) -> Option<Drop> {
        let mut hits: Vec<Drop> = Vec::new();
        for f in self.recent.iter().filter(|f| now.duration_since(f.at) <= within) {
            let nearest = f
                .drops
                .iter()
                .filter(|d| {
                    let a = wrap(d.bearing - heading);
                    a.cos() > 0.0 && d.range_m * a.sin().abs() <= half_width && d.edge_min_m < reach
                })
                .filter(|d| {
                    !f.obstacles.iter().any(|o| wrap(o.bearing - d.bearing).abs() < 0.2 && (o.range_m - d.range_m).abs() < 0.25)
                })
                .min_by(|a, b| a.edge_min_m.total_cmp(&b.edge_min_m));
            if let Some(d) = nearest {
                hits.push(*d);
            }
        }
        if hits.len() < min_frames {
            return None;
        }
        hits.into_iter().min_by(|a, b| a.edge_min_m.total_cmp(&b.edge_min_m))
    }

    pub fn obstacle_within(&self, now: Instant, bearing: f64, half_angle: f64) -> Option<Obstacle> {
        self.recent
            .iter()
            .filter(|f| now.duration_since(f.at) <= MEMORY && !f.moving)
            .flat_map(|f| f.obstacles.iter())
            .filter(|o| wrap(o.bearing - bearing).abs() <= half_angle)
            .min_by(|a, b| a.range_m.total_cmp(&b.range_m))
            .copied()
    }
}

fn wrap(a: f64) -> f64 {
    a.sin().atan2(a.cos())
}

/// The guard's two lanes on their own threads, sharing a [`CliffStatus`].
#[derive(Clone)]
pub struct CliffWatch {
    inner: Arc<Mutex<CliffStatus>>,
}

impl CliffWatch {
    /// Start watching `tof_socket` (tofd) with the body's pose from
    /// `robotd_socket`. Both lanes reconnect on loss for the life of the
    /// process; a robot with no depth sensor leaves the status at
    /// `Unavailable` and costs one idle socket.
    pub fn spawn(tof_socket: String, robotd_socket: String) -> Self {
        let inner = Arc::new(Mutex::new(CliffStatus::default()));
        let body: Arc<Mutex<Option<BodyPose>>> = Arc::new(Mutex::new(None));

        let (shared, body_w) = (inner.clone(), body.clone());
        std::thread::Builder::new()
            .name("cliff-body".into())
            .spawn(move || {
                loop {
                    if let Err(e) = body_lane(&robotd_socket, &body_w, &shared) {
                        tracing::debug!(error = %e, "cliff: body lane lost; reconnecting");
                    }
                    std::thread::sleep(RECONNECT);
                }
            })
            .expect("spawning the cliff body lane cannot fail");

        let shared = inner.clone();
        std::thread::Builder::new()
            .name("cliff-tof".into())
            .spawn(move || {
                let rp = Reprojector::alpha();
                loop {
                    if let Err(e) = tof_lane(&tof_socket, &rp, &body, &shared) {
                        tracing::debug!(error = %e, "cliff: depth lane lost; reconnecting");
                    }
                    std::thread::sleep(RECONNECT);
                }
            })
            .expect("spawning the cliff depth lane cannot fail");

        Self { inner }
    }

    /// A guard fed by hand (tests, replays).
    pub fn detached() -> Self {
        Self {
            inner: Arc::new(Mutex::new(CliffStatus::default())),
        }
    }

    pub fn snapshot(&self) -> CliffStatus {
        self.inner.lock().expect("cliff status poisoned").clone()
    }

    pub fn push(&self, frame: CliffFrame) {
        let mut s = self.inner.lock().expect("cliff status poisoned");
        s.stream = StreamState::Serving;
        s.body_seen = true;
        s.absorb(frame);
    }
}

fn connect(path: &str) -> anyhow::Result<UnixStream> {
    let stream = UnixStream::connect(path).with_context(|| format!("connecting to {path}"))?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    Ok(stream)
}

fn send(writer: &mut impl Write, message: &impl serde::Serialize) -> std::io::Result<()> {
    let mut line = serde_json::to_vec(message)?;
    line.push(b'\n');
    writer.write_all(&line)?;
    writer.flush()
}

/// `robot.subscribe` at a modest rate, keeping only the body's pose.
fn body_lane(
    path: &str,
    body: &Mutex<Option<BodyPose>>,
    shared: &Mutex<CliffStatus>,
) -> anyhow::Result<()> {
    let stream = connect(path)?;
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    let mut writer = stream.try_clone()?;
    let mut reader = BufReader::new(stream);
    send(
        &mut writer,
        &proto::Request::call(
            proto::Id::Number(0),
            &proto::Call::RobotSubscribe(proto::SubscribeParams { hz: Some(STATE_HZ) }),
        ),
    )?;
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
        let Ok(bits) = serde_json::from_value::<StateBits>(params) else {
            continue;
        };
        let moving = bits.policy == "walk";
        *body.lock().expect("body pose poisoned") = Some(BodyPose {
            head: bits.head_joints(),
            gravity: bits.safety.gravity,
            trunk_z: bits.odom.position[2],
            moving,
        });
        let mut s = shared.lock().expect("cliff status poisoned");
        s.body_seen = true;
        if bits.odom.yaw.is_some() {
            s.odom_yaw = bits.odom.yaw;
        }
        if moving && s.recent.iter().any(|f| !f.moving) {
            // Seen from somewhere the body no longer is.
            s.recent.retain(|f| f.moving);
        }
    }
}

/// `tof.stream`, every frame judged against the newest body pose.
fn tof_lane(
    path: &str,
    rp: &Reprojector,
    body: &Mutex<Option<BodyPose>>,
    shared: &Mutex<CliffStatus>,
) -> anyhow::Result<()> {
    let stream = connect(path)?;
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    let mut writer = stream.try_clone()?;
    let mut reader = BufReader::new(stream);
    send(
        &mut writer,
        &proto::Request::call(proto::Id::Number(0), &proto::Call::TofStream),
    )?;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            anyhow::bail!("tofd closed the depth stream");
        }
        if let Ok(request) = serde_json::from_str::<proto::Request>(&line) {
            if request.method != proto::method::TOF_FRAME {
                continue;
            }
            let Some(params) = request.params else {
                continue;
            };
            let Ok(frame) = serde_json::from_value::<proto::TofFrame>(params) else {
                continue;
            };
            let Some(pose) = *body.lock().expect("body pose poisoned") else {
                continue;
            };
            if let Some(judged) = analyze(rp, &frame, &pose, Instant::now()) {
                shared.lock().expect("cliff status poisoned").absorb(judged);
            }
            continue;
        }
        if let Ok(response) = serde_json::from_str::<proto::Response>(&line) {
            if let Some(error) = &response.error {
                anyhow::bail!("tofd refused tof.stream: {error}");
            }
            let ack: proto::TofStreamResult = response.result_as()?;
            let state = match ack.unavailable {
                Some(why) => StreamState::Unavailable(why),
                None => StreamState::Serving,
            };
            tracing::info!(state = ?state, "cliff guard: depth stream");
            shared.lock().expect("cliff status poisoned").stream = state;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame whose every judged beam meets the floor exactly where the
    /// geometry expects it, upward beams empty.
    fn floor_frame(rp: &Reprojector, body: &BodyPose) -> proto::TofFrame {
        let sensor = rp.sensor_in_trunk(body.head);
        let above = body.trunk_z + sensor.pos[2];
        let mut distance_mm = vec![0i16; ROWS * COLS];
        let mut status = vec![0u8; ROWS * COLS];
        for (i, beam) in rp.beams().iter().enumerate() {
            let dir = sensor.quat.rotate(*beam);
            let downward = -dir[2];
            if downward > 0.02 {
                distance_mm[i] = (above / downward * 1000.0).round() as i16;
                status[i] = 5;
            }
        }
        proto::TofFrame {
            seq: 1,
            at_us: 0,
            t_ns: 0,
            rows: ROWS as u8,
            cols: COLS as u8,
            distance_mm,
            status,
        }
    }

    #[test]
    fn a_flat_floor_is_no_drop() {
        let rp = Reprojector::alpha();
        let body = BodyPose::upright(0.12);
        let frame = floor_frame(&rp, &body);
        let judged = analyze(&rp, &frame, &body, Instant::now()).unwrap();
        assert!(judged.judged >= 8, "the bottom rows look down: {judged:?}");
        assert_eq!(judged.floor_beams, judged.judged);
        assert!(judged.drops.is_empty());
    }

    #[test]
    fn missing_and_deep_returns_where_the_floor_should_be_are_drops() {
        let rp = Reprojector::alpha();
        let body = BodyPose::upright(0.12);
        let mut frame = floor_frame(&rp, &body);
        // Bottom row, right half (columns 4..8 are the sensor's right):
        // two beams answer nothing, two answer the steps far below.
        let bottom = (ROWS - 1) * COLS;
        frame.status[bottom + 4] = 0;
        frame.status[bottom + 5] = 0;
        frame.distance_mm[bottom + 6] *= 3;
        frame.distance_mm[bottom + 7] *= 3;
        let now = Instant::now();
        let judged = analyze(&rp, &frame, &body, now).unwrap();
        assert_eq!(judged.drops.len(), 4, "{judged:?}");
        assert!(
            judged.drops.iter().all(|d| d.bearing < 0.0),
            "right of centre: {judged:?}"
        );
        assert!(
            judged.drops.iter().all(|d| (0.2..0.8).contains(&d.range_m)),
            "the edge is where the floor was expected: {judged:?}"
        );
        assert_eq!(
            judged
                .drops
                .iter()
                .filter(|d| d.kind == DropKind::Missing)
                .count(),
            2
        );
        assert!(
            judged.drops.iter().all(|d| d.edge_min_m == 0.0),
            "the bottom row over the drop: the edge may be right at the beak"
        );

        // One row up (row 6) over the drop while the bottom row still meets
        // the floor: the edge lies between the two rows' floor distances.
        let mut frame = floor_frame(&rp, &body);
        let row6 = (ROWS - 2) * COLS;
        frame.status[row6 + 3] = 0;
        frame.status[row6 + 4] = 0;
        let judged = analyze(&rp, &frame, &body, now).unwrap();
        assert_eq!(judged.drops.len(), 2, "{judged:?}");
        for d in &judged.drops {
            assert!(d.edge_min_m > 0.2 && d.edge_min_m < d.range_m, "{d:?}");
        }

        let mut status = CliffStatus::default();
        status.absorb(judged);
        // A cone to the right sees it, a cone to the left does not.
        assert!(status.drop_within(now, -0.3, 0.5).is_some());
        assert!(status.drop_within(now, 0.8, 0.4).is_none());
        // And it expires.
        assert!(
            status
                .drop_within(now + MEMORY + Duration::from_secs(1), 0.0, 3.2)
                .is_none()
        );
    }

    #[test]
    fn a_wall_ahead_is_an_obstacle_and_the_frame_says_where_it_looked() {
        let rp = Reprojector::alpha();
        let body = BodyPose::upright(0.12);
        let mut frame = floor_frame(&rp, &body);
        // The middle rows (looking level) return a wall half a metre out.
        for row in 3..5 {
            for col in 0..COLS {
                frame.distance_mm[row * COLS + col] = 500;
                frame.status[row * COLS + col] = 5;
            }
        }
        let now = Instant::now();
        let judged = analyze(&rp, &frame, &body, now).unwrap();
        assert!(judged.obstacles.len() >= 8, "{judged:?}");
        let mut status = CliffStatus::default();
        status.absorb(judged);
        assert!(status.looked_at(now, 0.0));
        assert!(!status.looked_at(now, 1.5));
        let o = status.obstacle_within(now, 0.0, 0.4).expect("a wall ahead");
        assert!((0.4..0.65).contains(&o.range_m), "{o:?}");
        assert!(status.obstacle_within(now, 2.0, 0.4).is_none());
    }

    #[test]
    fn a_walking_body_is_judged_but_marked() {
        // Since 2026-09-19: judged, and flagged `moving` — the standing
        // queries skip it, the walking hole query does not.
        let rp = Reprojector::alpha();
        let mut body = BodyPose::upright(0.12);
        body.moving = true;
        let frame = floor_frame(&rp, &body);
        let judged = analyze(&rp, &frame, &body, Instant::now()).unwrap();
        assert!(judged.moving);
        let mut s = CliffStatus::default();
        s.absorb(judged);
        assert!(!s.looked_at(Instant::now(), 0.0), "a walking frame is not a look");
    }

    #[test]
    fn a_single_odd_beam_is_noise() {
        let rp = Reprojector::alpha();
        let body = BodyPose::upright(0.12);
        let mut frame = floor_frame(&rp, &body);
        frame.status[(ROWS - 1) * COLS + 3] = 0;
        let judged = analyze(&rp, &frame, &body, Instant::now()).unwrap();
        assert!(judged.drops.is_empty());
    }

    #[test]
    fn a_frame_of_the_wrong_shape_is_ignored() {
        let rp = Reprojector::alpha();
        let frame = proto::TofFrame {
            seq: 1,
            at_us: 0,
            t_ns: 0,
            rows: 4,
            cols: 4,
            distance_mm: vec![500; 16],
            status: vec![5; 16],
        };
        assert!(analyze(&rp, &frame, &BodyPose::upright(0.12), Instant::now()).is_none());
    }

    #[test]
    fn state_bits_parse_leniently() {
        let v = serde_json::json!({
            "t": 1.0, "move": {"applied": [0, 0, 0]}, "head": [0.0, 0.1, -0.8, 0.0],
            "safety": {"fallen": false, "gravity": [0.0, 0.0, -1.0], "extra": 1},
            "odom": {"position": [0.5, 0.2, 0.118], "yaw": 0.3}
        });
        let bits: StateBits = serde_json::from_value(v).unwrap();
        assert_eq!(bits.head[2], -0.8);
        assert_eq!(bits.odom.position[2], 0.118);
        assert_eq!(bits.policy, "", "absent policy reads as standing");
        assert_eq!(bits.head_joints(), bits.head, "no joints: the commanded head is all there is");
    }

    /// The reprojection needs where the head is, not what it was told: the
    /// policy pitches the neck and head down on its own, and a floor
    /// projected as if they were level is an obstacle 0.37 m ahead.
    #[test]
    fn the_head_is_the_measured_joints_when_the_daemon_sends_them() {
        let mut joints = vec![0.0; proto::JOINT_NAMES.len()];
        joints[5] = 0.237; // neck_pitch
        joints[6] = 0.494; // head_pitch
        joints[7] = -0.632; // head_yaw
        joints[8] = 0.026; // head_roll
        let v = serde_json::json!({
            "t": 1.0, "move": {"applied": [0, 0, 0]}, "head": [0.0, 0.0, 0.836, 0.0],
            "joints": joints,
            "safety": {"fallen": false, "gravity": [0.0, 0.0, -1.0]},
            "odom": {"position": [0.5, 0.2, 0.116], "yaw": 0.3}
        });
        let bits: StateBits = serde_json::from_value(v).unwrap();
        assert_eq!(bits.head_joints(), [0.237, 0.494, -0.632, 0.026]);
    }
}
