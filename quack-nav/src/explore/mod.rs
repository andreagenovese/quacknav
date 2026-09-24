//! "Map everything": the duck walks to wherever the known floor meets the
//! unknown, stands there so the stop reaches the map, and repeats until
//! no frontier is left that it can reach (ADR 0005 §2, the guided tour
//! made autonomous).
//!
//! The shape is the one mapping robot vacuums use, in three layers:
//!
//! 1. **The map decides the route.** A costed planner on the occupancy
//!    grid ([`crate::frontier`]) picks the cheapest reachable
//!    frontier and the path there — known floor cheap, unknown dear, so
//!    in mapped space the duck takes the shortest known way and crosses
//!    unknown only to reach a frontier.
//! 2. **The duck follows the path.** A look-ahead point on the path sets
//!    the heading; small errors are corrected gently, large ones with a
//!    tight arc (this gait cannot turn in place). The frontier it is
//!    heading for is kept until it is reached or gone.
//! 3. **The sensor only answers for what the map does not know.** Every
//!    leg is a `robot.map_step` with all its guards. When the depth sensor
//!    refuses a leg — something in the way the map has not inked, a drop —
//!    that spot becomes a wall in a **local obstacle list** the planner
//!    treats as impassable, and the route is replanned around it. No
//!    turning heuristics, no wandering.
//!
//! A background behaviour, not a tool call: `robot.map_explore` starts it
//! and returns; `robot.map_status` narrates; while it runs the
//! conversation's own `robot.move` and `robot.map_step` are refused. When
//! the duck reaches a frontier far from every place it knows, it leaves a
//! **question** — "where are we?" — for the voice backend to ask out loud.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::frontier::{
    path_to,
    ExtraWall, Frontier, MIN_FRONTIER_CELLS, SQUEEZE_INFLATE_M, frontier_cells, inflate_m, path_to_both, route_passable,
    frontiers_with, largest_frontier, waypoint,
};
use crate::cliff::CliffStatus;
use crate::map::{Blocked, Cell, Grid, MapFrame, MapSupport};
use crate::places::Registry;
use crate::{MapConfig, Places};
use serde_json::{Value, json};

use quack_duck::Control;
use crate::tools::{Robot, execute};

/// The stand at each step of the look-around after a fall: the mapper's
/// floor for a still window is six seconds (see `homecoming::STAND_S`).
const RELOCATE_STAND_S: f64 = 6.0;
/// Beside a drop, a way narrower than this is a passage and its aim goes
/// to the middle (see `Job::centred`), by this much at most.
const CENTRE_WIDTH_M: f64 = 0.9;
const CENTRE_MAX_M: f64 = 0.15;
/// Unseals on one spot (within this) before it is a no-go for the job,
/// and how wide a no-go is for the planner.
const NO_GO_AFTER: u32 = 3;
const NO_GO_SAME_M: f64 = 0.30;
const NO_GO_RADIUS_M: f64 = 0.20;

/// What ends the floor on one side of a way (see `Job::side_free`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Wall,
    Drop,
    Unknown,
    Open,
}

/// What the explorer needs of a body: a guarded mapping step, a blind
/// move, the newest map frame, the cliff guard's view, and a clock. The
/// real robot answers over robotd; the paper twin (a kinematic model of
/// the duck in the apartment's boxes) answers in memory, thousands of
/// runs an hour, with the same guards (`tools::plan_step`) and the same
/// explorer code.
pub trait Body {
    /// `robot.map_step`: a guarded, timed walk, then a stand.
    fn step(&mut self, args: &Value) -> Result<Value, String>;
    /// `robot.move`: a blind timed move (no guards — the caller's risk).
    fn blind_move(&mut self, args: &Value) -> Result<Value, String>;
    /// The newest map frame, if any.
    fn frame(&self) -> Option<MapFrame>;
    /// Whether the map's pose can be planned on (tracking, not seated).
    fn pose_trusted(&self) -> bool;
    /// Whether the live map is frozen (robotd's maploc in `localize`
    /// mode: nothing inks, the pose is corrected against the map as
    /// saved). A journey on such a map trusts the planner, see
    /// [`Job::blind`].
    fn frozen_map(&self) -> bool {
        false
    }
    /// The cliff guard's view, if the guard is on.
    fn cliff(&self) -> Option<CliffStatus>;
    /// The clock: wall time for the robot, a virtual one for the twin.
    fn now(&self) -> Instant;
    /// Wait — really, or by advancing the virtual clock.
    fn sleep(&mut self, d: Duration);
}

impl Body for Robot {
    fn step(&mut self, args: &Value) -> Result<Value, String> {
        execute("robot.map_step", args, self)
    }
    fn blind_move(&mut self, args: &Value) -> Result<Value, String> {
        execute("robot.move", args, self)
    }
    fn frame(&self) -> Option<MapFrame> {
        self.places.map.as_ref().and_then(|m| m.snapshot().latest.clone())
    }
    fn pose_trusted(&self) -> bool {
        self.places.map.as_ref().is_some_and(|m| m.snapshot().trusted_pose().is_some())
    }
    fn frozen_map(&self) -> bool {
        self.places.map.as_ref().is_some_and(|m| {
            matches!(&m.snapshot().support, MapSupport::Supported { mode: Some(mode), .. } if mode == "localize")
        })
    }
    fn cliff(&self) -> Option<CliffStatus> {
        self.places.cliff.as_ref().map(|c| c.snapshot())
    }
    fn now(&self) -> Instant {
        Instant::now()
    }
    fn sleep(&mut self, d: Duration) {
        std::thread::sleep(d);
    }
}
use quack_duck::gait::GaitConfig;

/// A frontier is reached within this distance of its nearest cell.
/// Close enough to the standing point: the stand there maps the frontier.
const ARRIVE_M: f64 = 0.30;

/// Stand this long between legs (a window flushes after 3 s) and this
/// long at a frontier (a full head sweep).
const LEG_STOP_S: f64 = 3.0;
/// What the gait covers per second at vx 0.3 (measured on the twin).
const GAIT_M_PER_S: f64 = 0.12;
/// A forward leg must leave this much floor beyond its end (the step's own
/// wall margin plus the beak); a tight arc, which advances a few
/// centimetres and gets a smaller margin from the step, needs less.
const LEG_RESERVE_M: f64 = 0.35;
/// The body is 0.19 m wide (twin's collision hull); with 0.06 m to spare
/// on each side a corridor must be this wide to walk through.
const CORRIDOR_MIN_M: f64 = 0.31;
/// The corridor width is also checked this far along the heading.
const WIDTH_AHEAD_M: f64 = 0.3;
/// A passage narrower than this (and wider than the body) is a doorway:
/// steer onto its axis with this gain, in legs no longer than this.
const GAP_MAX_M: f64 = 0.6;
const GAP_GAIN: f64 = 2.0;
const GAP_LEG_S: f64 = 1.5;
/// Room a doorway leg needs ahead of it: the doorway margin and slack.
const GAP_LEG_RESERVE_M: f64 = 0.20;
/// How far ahead the sensor's obstacles count as doorposts.
const GAP_LOOK_M: f64 = 0.8;
/// The sensor lane in a doorway: the body's half-width plus two centimetres
/// (map_step uses the same when asked with `gap`).
const GAP_LANE_HALF_M: f64 = 0.115;
/// Frontiers farther than this cost more the farther they are (see
/// [`Job::local_score`]): by this share per LOCAL_M beyond; a frontier
/// under this many cells is a sliver and pays this factor, near or far.
const LOCAL_M: f64 = 2.5;
const LOCAL_SLOPE: f64 = 0.35;
const LOCAL_MIN_CELLS: usize = 12;
/// Frontiers with at least this many cells are served first, wherever
/// they are; the slivers (the reborn fragments behind furniture) only
/// when no big one remains — a duck that finished a room does not come
/// back for a fragment while another room is open (measured on the paper
/// twin with bare walls: 12 of 30 runs wandered slivers to the budget).
const BIG_FRONTIER_CELLS: usize = 20;
/// In the sliver phase — no frontier group of that size anywhere on the
/// map, reachable or not — this many rounds (legs, stands, recoveries)
/// without the map growing by [`SLIVER_GROWTH_CELLS`] free cells end the
/// job: what is left is noise the stands keep re-minting, not floor.
const SLIVER_PATIENCE: u32 = 12;
const SLIVER_GROWTH_CELLS: usize = 30;







/// The leg planner's knobs. Each is the measured default; the environment
/// can override one for a measurement run (`QK_<NAME>`), which is how the
/// automatic search on the paper twin explores them. Read once.
fn knob(name: &str, default: f64) -> f64 {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}
fn lane_half_m() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| knob("QK_LANE_HALF_M", LANE_HALF_M))
}
fn leg_reserve_m() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| knob("QK_LEG_RESERVE_M", LEG_RESERVE_M))
}
fn gap_leg_reserve_m() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| knob("QK_GAP_LEG_RESERVE_M", GAP_LEG_RESERVE_M))
}
fn gap_lane_half_m() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| knob("QK_GAP_LANE_HALF_M", GAP_LANE_HALF_M))
}
fn gap_max_m() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| knob("QK_GAP_MAX_M", GAP_MAX_M))
}
fn gap_leg_s() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| knob("QK_GAP_LEG_S", GAP_LEG_S))
}













const SLIVER_PENALTY: f64 = 1.5;
/// Room a turn needs on the side it swings to: half the body and the
/// few centimetres a tight arc advances.
const TURN_ROOM_M: f64 = 0.30;
/// Obstacles the sensor reports within this much of the line the duck
/// would walk are in the way: half the body plus a little (the mapping
/// step uses the same lane). A ±23° cone blocked doorways from half a
/// metre away, their posts being inside it.
const LANE_HALF_M: f64 = 0.16;
/// An obstacle closer than this dead ahead is "nose against it": the one
/// case where a blind step back is the lesser evil.
const NOSE_STUCK_M: f64 = 0.30;



/// A heading counts as open floor only with at least this much known free
/// floor along it.
const SPACE_MIN_M: f64 = 0.6;
const DETOUR_SUSPECT: f64 = 2.5;
const DETOUR_NEAR_M: f64 = 2.5;
const DETOUR_DOUBTS_MAX: u32 = 3;
const STUCK_MAX: u32 = 3;
/// A "sealed in" attempt counts toward [`STUCK_MAX`] only this long after
/// the previous one, or once the body has moved this far since.
const STUCK_GAP_S: f64 = 30.0;
const STUCK_MOVE_M: f64 = 0.20;
/// Refusals in a row, no leg between, before backing out of the spot.
const REFUSAL_STREAK_MAX: u32 = 6;
/// Turns in place without a leg between them before the spot is treated
/// as a refusal: a turn is cheap, but not the answer forever.
const SPINS_MAX: u32 = 3;
/// How many times a run may forget every local obstacle at once because
/// they, and not the map, seal the rest of the flat away (a doorpost and
/// a cabinet corner sealed a 0.42 m door from 2.4 m away in run 59;
/// the unseal recovery only forgets what is near the duck).
const GLOBAL_FORGETS_MAX: u32 = 3;
/// Obstacles recorded farther than this from the duck may be forgotten
/// to unseal the map; nearer ones are what the sensor just saw.
const FORGET_FAR_M: f64 = 1.0;
/// Played past the commanded time by this much: the gait keeps going.
const DROP_PATH_EXTRA_S: f64 = 0.7;
/// A kick or a leg is not taken toward floor the guard has not looked at
/// within its memory: the map cannot show a hole, the books can be wrong
/// or just struck, and the sensor is what is left. Wait for a frame this
/// long (the head is sweeping), then give the manoeuvre up.
const LOOK_WAIT_S: f64 = 2.0;
/// A frontier that survives a full stand at it (a window, a hole) is not
/// worth a second visit — that one, not everything near it.
const BLOCK_VISITED_M: f64 = 0.25;
/// Refusals on the way to the same frontier before it is left alone.
const REFUSALS_PER_TARGET: u32 = 4;
/// Ask for a name only this far from every known place and earlier ask.
const ASK_MIN_M: f64 = 2.0;
/// A pose that moved more than this plus what the gait could have walked
/// since the last frame is the map moving, not the duck.
const JUMP_SLACK_M: f64 = 0.35;
/// Stands in a row whose poses agree within [`SETTLED_M`] before a moved
/// map is trusted again.
const SETTLE_STANDS: u32 = 2;
const SETTLED_M: f64 = 0.15;
/// A watch job: how long the body stands still before its stand is
/// booked, and the watch's tick.
const WATCH_STILL_S: f64 = 2.0;
const WATCH_TICK: Duration = Duration::from_millis(500);
/// What a sealed rim's way round adds to a journey's budget, once.
const GO_ROUND_EXTRA_S: f64 = 300.0;
/// An untrusted pose is "stable" after this long without moving, and
/// its fit (`fit.rs`) must be under this to map on.
const STABLE_UNTRUSTED_S: f64 = 20.0;
const STABLE_UNTRUSTED_FIT_M: f64 = 0.10;
/// Map-versus-sensor agreement at stands: obstacles the sensor reports
/// within this range, in directions where the map has a wall, either sit
/// on it (within this tolerance) or beyond it. At least this many beyond,
/// and three times the ones on it, make a stand "disagreeing" (a true
/// pose still sees past a mapped wall corner here and there: with 3 and
/// half, nine false alarms in a run); this
/// many in a row earn a panorama, this many end the job as lost.
const AGREE_RANGE_M: f64 = 1.5;
const AGREE_TOL_M: f64 = 0.35;
const DISAGREE_MIN: usize = 10;
/// Where the route met what the sensor sees: the route point, what it
/// was, and where that is on the map.
type RouteConflict = ((f64, f64), &'static str, (f64, f64));
const DISTRUST_PANORAMA: u32 = 2;
const DISTRUST_FAIL: u32 = 6;
/// Pause after a refusal: the map has not changed yet.
const AFTER_REFUSAL: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Idle,
    Running,
    Done,
    Stopped,
    Failed,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            State::Idle => "idle",
            State::Running => "running",
            State::Done => "done",
            State::Stopped => "stopped",
            State::Failed => "failed",
        }
    }
}

/// A "where are we?" the voice side should ask.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Question {
    pub pose: (f64, f64, f64),
    pub since: Instant,
}

#[derive(Debug, Clone)]
pub struct ExploreStatus {
    pub state: State,
    pub reason: Option<String>,
    pub started: Option<Instant>,
    pub finished: Option<Instant>,
    pub legs: u32,
    pub refusals: u32,
    pub frontiers_left: usize,
    pub target: Option<(f64, f64)>,
    pub target_distance_m: Option<f64>,
    pub visited: u32,
    /// Obstacles the sensor met that the map has not inked.
    pub local_obstacles: usize,
    /// Their world coordinates and radii, for the record.
    pub local: Vec<((f64, f64), f64)>,
    /// Where the body has been (see `Job::trail`): the next job inherits it.
    pub trail: Vec<(f64, f64)>,
    /// The map's lanes from the ground book (`ExploreHandle::map_named`):
    /// passable to the planner, but not a place this body has stood —
    /// merged into the trail they struck every rim point the guard
    /// re-booked (rim2, 2026-09-17: twelve of thirteen, by lane points
    /// 10–15 cm from them), so the book could never regain its rim.
    pub lanes: Vec<(f64, f64)>,
    /// Part of the trail was walked blind (a journey on a frozen map, no
    /// guard on the legs): it strikes no drop and lays no lane. Standing
    /// "there" proves nothing when nothing looked, and the pose while
    /// walking is 10–17 cm out (tour2, 2026-09-16: the west rim of the
    /// stairwell — eight drops — struck in seven seconds by a body truly
    /// 9–17 cm from the rim, believed on it).
    pub blind: bool,
    /// The route planned on the last pass, the point the current leg aims
    /// at and the journey's goal — for an overlay to draw where the duck
    /// thinks it will pass, beside where it really does.
    pub route: Vec<(f64, f64)>,
    /// Dijkstra's own route before it was pulled taut, for the overlay.
    pub route_raw: Vec<(f64, f64)>,
    pub aim: Option<(f64, f64)>,
    pub goal: Option<(f64, f64)>,
    /// The trusted floor's cells (5 cm), for an overlay to draw.
    pub trusted: Vec<(f64, f64)>,
    /// The pose's fit at the last stand (see `fit.rs`): median metres
    /// from the seen obstacles to the mapped walls, matched, judged.
    pub fit: Option<(f64, usize, usize)>,
    pub pending_question: Option<Question>,
    pub questions_asked: u32,
}

impl Default for ExploreStatus {
    fn default() -> Self {
        Self {
            state: State::Idle,
            reason: None,
            started: None,
            finished: None,
            legs: 0,
            refusals: 0,
            frontiers_left: 0,
            target: None,
            target_distance_m: None,
            visited: 0,
            local_obstacles: 0,
            local: Vec::new(),
            trail: Vec::new(),
            lanes: Vec::new(),
            blind: false,
            route: Vec::new(),
            route_raw: Vec::new(),
            trusted: Vec::new(),
            fit: None,
            aim: None,
            goal: None,
            pending_question: None,
            questions_asked: 0,
        }
    }
}

impl ExploreStatus {
    /// Reset for a new job, handing back what outlives the old one: the
    /// drops on the books — the map cannot show a stairwell, and a job
    /// that starts with none walked into one on its first passage (run
    /// 67, second segment) — and the walked trail, so the doors it walked
    /// through stay doors. Both are read BEFORE the reset: read after it
    /// they were always empty, every job started blind, and the hole
    /// booked twelve times on the way to the living room was gone on the
    /// way to the bathroom (fresh1, 2026-09-15 — the duck fell in).
    fn begin_job(s: &mut ExploreStatus) -> (Vec<ExtraWall>, Vec<(f64, f64)>) {
        let drops: Vec<ExtraWall> = s.local.iter().copied().filter(|(_, r)| *r >= DROP_RADIUS_M).collect();
        let trail = s.trail.clone();
        *s = ExploreStatus {
            state: State::Running,
            started: Some(Instant::now()),
            local: drops.clone(),
            trail: trail.clone(),
            lanes: s.lanes.clone(),
            blind: s.blind,
            ..ExploreStatus::default()
        };
        (drops, trail)
    }

    pub fn to_json(&self) -> Value {
        let elapsed = self.started.map(|s| {
            self.finished
                .unwrap_or_else(Instant::now)
                .duration_since(s)
                .as_secs()
        });
        json!({
            "state": self.state.as_str(),
            "reason": self.reason,
            "elapsed_s": elapsed,
            "legs": self.legs,
            "refusals": self.refusals,
            "frontiers_left": self.frontiers_left,
            "target": self.target.map(|(x, y)| json!({"x": round2(x), "y": round2(y)})),
            "target_distance_m": self.target_distance_m.map(round2),
            "frontiers_visited": self.visited,
            "local_obstacles": self.local_obstacles,
            "local": self.local.iter().map(|((x, y), r)| json!([round2(*x), round2(*y), round2(*r)])).collect::<Vec<_>>(),
            "route": self.route.iter().map(|(x, y)| json!([round2(*x), round2(*y)])).collect::<Vec<_>>(),
            "route_raw": self.route_raw.iter().map(|(x, y)| json!([round2(*x), round2(*y)])).collect::<Vec<_>>(),
            "trusted": self.trusted.iter().map(|(x, y)| json!([round2(*x), round2(*y)])).collect::<Vec<_>>(),
            "fit": self.fit.map(|(m, a, b)| json!({"m": (m * 1000.0).round() / 1000.0, "matched": a, "judged": b})),
            "aim": self.aim.map(|(x, y)| json!([round2(x), round2(y)])),
            "goal": self.goal.map(|(x, y)| json!([round2(x), round2(y)])),
            "question_pending": self.pending_question.is_some(),
        })
    }
}

/// The job's handle: shared status, a stop flag, and the one place the
/// voice side takes a pending question from.
#[derive(Clone, Default)]
pub struct ExploreHandle {
    status: Arc<Mutex<ExploreStatus>>,
    stop: Arc<AtomicBool>,
    /// The ground book: the drops on the books, kept on disk per saved
    /// map (see [`ExploreHandle::map_named`]).
    ground: Arc<Mutex<Ground>>,
}

/// Where the drops of each saved map are kept, and which map is live.
/// A map from the library shows floor where the stairwell is — the
/// mapper cannot see a hole — and a fresh boot on it planned its first
/// route straight through the hole, the books being empty until the
/// sensor refused there again (house1, 2026-09-15). So the books outlive
/// the process: written under the map's name when a job ends or the map
/// is saved, read back when that map is loaded or adopted.
#[derive(Default)]
struct Ground {
    path: Option<std::path::PathBuf>,
    map: Option<String>,
}

impl ExploreHandle {
    pub fn new() -> Self {
        Self::default()
    }

    /// Keep the ground book beside the places registry: `ground.json`.
    pub fn with_ground(self, places_path: &str) -> Self {
        let path = std::path::Path::new(places_path).with_file_name("ground.json");
        self.ground.lock().expect("ground poisoned").path = Some(path);
        self
    }

    fn ground_file(&self) -> serde_json::Map<String, Value> {
        let path = self.ground.lock().expect("ground poisoned").path.clone();
        path.and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default()
    }

    /// The saved map `name` is the live one now: its drops come onto the
    /// books, whatever was there goes (it named places on another map),
    /// and the trail with it.
    pub fn map_named(&self, name: &str) {
        let drops: Vec<((f64, f64), f64)> = self
            .ground_file()
            .get(name)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|p| {
                        let v = p.as_array()?;
                        Some(((v.first()?.as_f64()?, v.get(1)?.as_f64()?), v.get(2)?.as_f64()?))
                    })
                    .collect()
            })
            .unwrap_or_default();
        // A drop's radius is DROP_RADIUS_M; wider ones are obstacles an old
        // rule booked a centimetre too wide and so saved as drops (see
        // `LOW_BOOK_RADIUS_MAX_M`). They are not drops: left out.
        let before = drops.len();
        let drops: Vec<((f64, f64), f64)> = drops.into_iter().filter(|(_, r)| *r <= DROP_RADIUS_M + 0.005).collect();
        if drops.len() < before {
            tracing::info!(map = name, left_out = before - drops.len(), "map explore: obstacles saved as drops by the old rule; left out of the books");
        }
        // The passages walked on this map: trail points beside its drops,
        // kept as lanes — cells the body stood on, passable to the planner
        // whatever the margins say. A 0.54 m passage with a 0.25 m drop
        // margin has four centimetres of slack and a rim booked five
        // centimetres inside seals it for the planner though the passage
        // law walks it fine (loc5, 2026-09-16); once walked, it stays open.
        let lanes: Vec<(f64, f64)> = self
            .ground_file()
            .get(&format!("{name}.lanes"))
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|p| {
                        let v = p.as_array()?;
                        Some((v.first()?.as_f64()?, v.get(1)?.as_f64()?))
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.ground.lock().expect("ground poisoned").map = Some(name.to_string());
        let mut s = self.status.lock().expect("explore status poisoned");
        tracing::info!(map = name, drops = drops.len(), lanes = lanes.len(), "map explore: the ground book for this map is on the books");
        s.local = drops;
        s.trail.clear();
        s.lanes = lanes;
        s.blind = false;
    }

    /// The live map has this name now (`robot.map_save`): the books stay
    /// as they are, they are its.
    pub fn name_live_map(&self, name: &str) {
        self.ground.lock().expect("ground poisoned").map = Some(name.to_string());
    }

    /// Write the drops on the books under the live map's name.
    pub fn keep_ground(&self) {
        let (path, map) = {
            let g = self.ground.lock().expect("ground poisoned");
            (g.path.clone(), g.map.clone())
        };
        let (Some(path), Some(map)) = (path, map) else { return };
        // Struck against the whole trail, not the one stand: a rim point
        // the body has since walked past within STRIKE_M was booked
        // wrong, and kept it would seal the passage for every boot to
        // come (full2, 2026-09-15: 48 drops back on the books, the first
        // route 10.8 m round the house for a 1.75 m goal).
        let status = self.status();
        // A blind trail (see `ExploreStatus::blind`) strikes nothing.
        let trail: &[(f64, f64)] = if status.blind { &[] } else { &status.trail };
        let drops: Vec<Value> = status
            .local
            .iter()
            .filter(|(_, r)| *r >= DROP_RADIUS_M)
            .filter(|(p, _)| !trail.iter().any(|t| dist2(*t, *p) < STRIKE_M))
            .map(|((x, y), r)| json!([round2(*x), round2(*y), round2(*r)]))
            .collect();
        // The lanes: every trail point within LANE_KEEP_M of a kept drop,
        // merged with the ones already in the book (walked on an earlier
        // day), deduplicated to the trail's own step.
        let kept: Vec<(f64, f64)> = drops
            .iter()
            .filter_map(|d| Some((d.get(0)?.as_f64()?, d.get(1)?.as_f64()?)))
            .collect();
        let mut lanes: Vec<(f64, f64)> = self
            .ground_file()
            .get(&format!("{map}.lanes"))
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|p| {
                        let v = p.as_array()?;
                        Some((v.first()?.as_f64()?, v.get(1)?.as_f64()?))
                    })
                    .collect()
            })
            .unwrap_or_default();
        for t in trail.iter().filter(|t| kept.iter().any(|d| dist2(*d, **t) < LANE_KEEP_M)) {
            if !lanes.iter().any(|l| dist2(*l, *t) < TRAIL_STEP_M * 0.5) {
                lanes.push(*t);
            }
        }
        let n_lanes = lanes.len();
        let mut file = self.ground_file();
        let n = drops.len();
        file.insert(map.clone(), Value::Array(drops));
        file.insert(
            format!("{map}.lanes"),
            Value::Array(lanes.iter().map(|(x, y)| json!([round2(*x), round2(*y)])).collect()),
        );
        match std::fs::write(&path, serde_json::to_string_pretty(&Value::Object(file)).unwrap_or_default()) {
            Ok(()) => tracing::info!(map, drops = n, lanes = n_lanes, path = %path.display(), "map explore: ground book kept"),
            Err(e) => tracing::warn!(error = %e, path = %path.display(), "map explore: the ground book could not be written"),
        }
    }

    pub fn status(&self) -> ExploreStatus {
        self.status.lock().expect("explore status poisoned").clone()
    }

    pub fn running(&self) -> bool {
        self.status().state == State::Running
    }

    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    /// Forget the drops on the books and the walked trail.
    ///
    /// Both are coordinates, and after the map underneath them is traded
    /// for another one (`robot.map_adopt`) they name places the duck is no
    /// longer anywhere near. Carrying them over would put a remembered
    /// stairwell in the middle of a room.
    pub fn forget_ground(&self) {
        let mut s = self.status.lock().expect("explore status poisoned");
        s.local.clear();
        s.trail.clear();
        s.lanes.clear();
        s.blind = false;
    }

    /// The pending question, cleared — whoever takes it asks it.
    pub fn take_question(&self) -> Option<Question> {
        let mut s = self.status.lock().expect("explore status poisoned");
        let q = s.pending_question.take();
        if q.is_some() {
            s.questions_asked += 1;
        }
        q
    }

    /// Start the job. `known` are the places already named (so the duck
    /// does not ask again for them); `max_s` bounds the run; `ask` says
    /// whether to leave questions at all. Refused while one is running.
    /// Walk to one point on the map already built, in the background, with
    /// the mapping job's own legs and guards: `go_to`. The books' drops and
    /// the walked trail carry over, as they do between mapping jobs.
    #[allow(clippy::too_many_arguments)]
    pub fn start_goto(
        &self,
        robotd_socket: &str,
        places: &Places,
        goal: (f64, f64),
        max_s: f64,
        turn: f64,
        gait: GaitConfig,
    ) -> Result<(), String> {
        self.start_job(robotd_socket, places, max_s, gait, move |drops, trail| {
            let mut job = Job::to_goal(goal, max_s, turn, Instant::now());
            job.ground_drops = drops.iter().filter(|(_, r)| *r >= DROP_RADIUS_M).count();
            job.local = drops;
            job.trail = trail;
            job
        })
    }

    /// A watch (see `Job::watch`): books what the stands see while
    /// somebody else drives.
    pub fn watch(&self, robotd_socket: &str, places: &Places, max_s: f64, gait: GaitConfig) -> Result<(), String> {
        self.start_job(robotd_socket, places, max_s, gait, move |drops, trail| {
            let mut job = Job::watch(max_s, Instant::now());
            job.ground_drops = drops.iter().filter(|(_, r)| *r >= DROP_RADIUS_M).count();
            job.local = drops;
            job.trail = trail;
            job
        })
    }

    pub fn start(
        &self,
        robotd_socket: &str,
        places: &Places,
        known: Vec<(f64, f64)>,
        max_s: f64,
        ask: bool,
        turn: f64,
        gait: GaitConfig,
    ) -> Result<(), String> {
        self.start_job(robotd_socket, places, max_s, gait, move |drops, trail| {
            let mut job = Job::new(known, max_s, ask, turn, Instant::now());
            job.ground_drops = drops.iter().filter(|(_, r)| *r >= DROP_RADIUS_M).count();
            job.local = drops;
            job.trail = trail;
            job
        })
    }

    /// The part every background job shares: the preconditions, its own
    /// request lane, the books and the trail carried over, the thread.
    fn start_job(
        &self,
        robotd_socket: &str,
        places: &Places,
        _max_s: f64,
        gait: GaitConfig,
        build: impl FnOnce(Vec<ExtraWall>, Vec<(f64, f64)>) -> Job + Send + 'static,
    ) -> Result<(), String> {
        let drops: Vec<ExtraWall>;
        let lanes: Vec<(f64, f64)>;
        let trail: Vec<(f64, f64)>;
        {
            let mut s = self.status.lock().expect("explore status poisoned");
            if s.state == State::Running {
                return Err("the duck is already exploring".into());
            }
            let Some(map) = &places.map else {
                return Err("this satellite has no map lane ([map] enabled = false)".into());
            };
            if map.snapshot().latest.is_none() {
                return Err("no map yet: robotd is unreachable or has not sent a map frame".into());
            }
            (drops, trail) = ExploreStatus::begin_job(&mut s);
            lanes = s.lanes.clone();
        }
        self.stop.store(false, Ordering::Relaxed);
        let control = Control::connect(robotd_socket).map_err(|e| {
            self.finish(State::Failed, format!("robotd unreachable: {e}"));
            format!("robotd unreachable: {e}")
        })?;
        // The job's own robot: its own request lane, the shared map and
        // cliff lanes, and an idle explore handle so its own steps are not
        // refused as "someone else is driving".
        let mut robot = Robot {
            control: Some(control),
            places: Places {
                map: places.map.clone(),
                registry: Registry::in_memory(),
                cliff: places.cliff.clone(),
                explore: ExploreHandle::new(),
                robotd_socket: robotd_socket.to_owned(),
                map_socket: places.map_socket.clone(),
                map_config: MapConfig::default(),
                gait,
            },
        };
        let handle = self.clone();
        std::thread::Builder::new()
            .name("map-explore".into())
            .spawn(move || {
                let mut job = build(drops, trail);
                job.lanes = lanes;
                let (state, reason) = job.run(&handle, &mut robot);
                handle.finish(state, reason);
            })
            .map_err(|e| format!("cannot start the exploration thread: {e}"))?;
        Ok(())
    }

    fn finish(&self, state: State, reason: String) {
        let mut s = self.status.lock().expect("explore status poisoned");
        s.state = state;
        s.reason = Some(reason.clone());
        s.finished = Some(Instant::now());
        s.target = None;
        s.target_distance_m = None;
        tracing::info!(state = state.as_str(), reason, "map explore: finished");
        drop(s);
        self.keep_ground();
    }

    fn update(&self, f: impl FnOnce(&mut ExploreStatus)) {
        f(&mut self.status.lock().expect("explore status poisoned"));
    }
}

/// The job's state between iterations.
pub struct Job {
    known: Vec<(f64, f64)>,
    max_s: f64,
    ask: bool,
    /// The hand to turn to when blocked: -1 right, +1 left ([`MapConfig::turn_sign`]).
    turn: f64,
    started: Instant,
    /// Visited frontier spots: never chosen again.
    visited: Vec<((f64, f64), f64)>,
    /// Frontier spots refused too often: left alone until nothing else
    /// is left, then forgotten once.
    refused: Vec<((f64, f64), f64)>,
    /// Drops whose leg the books refused: the planner keeps wider of them
    /// from then on (see [`Job::refusal`], "on the books").
    widened: Vec<(f64, f64)>,
    /// Drops the guard refused for over and over: sealed for the planner
    /// (see `DROP_SEAL_M`), the route goes round.
    sealed: Vec<(f64, f64)>,
    /// The drop that last refused a leg from the books (`guarded_step`).
    books_refusal: std::cell::Cell<Option<(f64, f64)>>,
    refused_cleared: bool,
    /// Where the refused list was last cleared: it is cleared again only
    /// once the body has moved [`REARM_DIST_M`] from there.
    refused_cleared_at: Option<(f64, f64)>,
    /// What the sensor met that the map has not inked: walls for the planner.
    local: Vec<ExtraWall>,
    /// Where the body has been, a point every [`TRAIL_STEP_M`]: lanes the
    /// planner may always use. The body's own passage is the one fact
    /// about passability the map, the margins and the books cannot deny
    /// — the door the duck walked in through stays a door.
    trail: Vec<(f64, f64)>,
    /// The frontier being pursued, and how many refusals on the way to it.
    target: Option<((f64, f64), u32)>,
    /// Where the duck was told to go, when this job is a `go_to` and not a
    /// mapping job: the planner aims here instead of at a frontier and the
    /// job ends on arrival. Everything else — the legs, the guards, the
    /// books, the recoveries — is the mapping job's own.
    goal: Option<(f64, f64)>,
    /// When the last nose-stuck step back happened: one per [`BACK_EVERY`].
    last_back: Option<Instant>,
    /// The yaw of the last leg that walked: a step back that mirrors it
    /// retraces the way in.
    last_leg_vyaw: f64,
    /// The aim of the last leg planned: what a step back re-orients to.
    last_aim: Option<(f64, f64)>,
    /// Fast mode: when and where the body last stood for its pose.
    last_pose_stand: Option<(Instant, (f64, f64))>,
    /// The live map is frozen (`Body::frozen_map`), read each turn.
    frozen: bool,
    /// The mode's policy (see `mode.rs`), kept in step with the mode.
    policy: Policy,
    /// The floor the duck knows (see `trusted.rs`).
    trusted: TrustedFloor,
    /// The pose's fit at the last stand, metres (see `fit.rs`).
    last_fit: Option<f64>,
    /// The ground book's lanes (see `ExploreStatus::lanes`).
    lanes: Vec<(f64, f64)>,
    /// Legs in a row that did not move the duck (see `STALLED_M`).
    stalls_in_row: u32,
    /// A watch: the job commands nothing — somebody else drives (a human
    /// at `teleop.py`) — and books what the stands see, as the legs'
    /// stands do; the ground book is written when it ends. The guided
    /// drive that writes the books (the user's, 2026-09-21: "a small
    /// guided drive round the stairwell, and we refresh the drops").
    watch: bool,
    watch_booked: bool,
    /// How many drops the ground book brought (see `remember_local`),
    /// and whether the "journeys book none" note was logged.
    ground_drops: usize,
    frozen_drops_noted: bool,
    /// Legs in a row the guard refused for a drop (see `DROP_REFUSALS_SEAL`),
    /// and when and where the last one counted.
    drop_refusals_in_row: u32,
    last_drop_refusal: Option<(Instant, (f64, f64))>,
    /// "A passage too narrow" refusals in a row without a leg between.
    passage_refusals: u32,
    /// Whether the passage law last read the sides at the mouth ahead
    /// (see `passage`).
    passage_at_mouth: bool,
    /// The last `passage()` found the way beside a drop narrower than the
    /// body, its drift and the pose's margin: the leg is not to be walked.
    passage_narrow: bool,
    /// Narrow-passage refusals in a row, and where the body stood.
    narrow_refusals: (u32, (f64, f64)),
    /// A fall was seen and the pose has not been trusted for
    /// [`BOOKS_AFTER_FALL_S`] since (the instant it was trusted again, if
    /// it is): no drop goes on the books meanwhile (see `drops_bookable`).
    fell: Option<Option<Instant>>,
    /// Steps of the look-around after a fall (see `relocate_step`).
    relocate_steps: u32,
    /// Moves off a rim in a row (see `off_the_rim`).
    rim_offs: u32,
    /// Spots the job got stuck on again and again: the planner keeps off
    /// them for the rest of the job (see `unseal`).
    no_go: Vec<(f64, f64)>,
    /// Where the last unseals happened, and how many in a row there.
    unseals_here: Option<((f64, f64), u32)>,
    /// Whether a drop may go on the books now: the pose trusted, the duck
    /// on its feet, no fall pending confirmation. Set each turn.
    drops_bookable: bool,
    /// Turns in place refused beside a drop in a row (the kick refused,
    /// no way back) without a leg between (see `TURNS_REFUSED_SEAL`).
    turns_refused_at_drop: u32,
    /// The budget was grown once for a go-round (see `GO_ROUND_EXTRA_S`).
    budget_extended: bool,
    /// The arrival stand was taken once (see `GOAL_FIT_M`).
    goal_confirmed: bool,
    /// The journey's route as kept between plans (raw, pulled), when it
    /// was planned, and the books' size then (see `KEEP_ROUTE_S`).
    kept_route: Option<(Vec<(f64, f64)>, Vec<(f64, f64)>, Instant, usize)>,
    /// A refusal since the route was planned: plan again.
    refused_since_plan: bool,
    lost_since: Option<Instant>,
    /// Consecutive "sealed in" recoveries without a leg in between.
    stuck: u32,
    /// When and where the last one was counted: another counts only
    /// after [`STUCK_GAP_S`] or [`STUCK_MOVE_M`] of the body's own motion.
    last_unseal: Option<(Instant, (f64, f64))>,
    /// Refusals since the last leg that walked.
    since_leg: u32,
    /// Turns in place since the last leg that walked.
    spins_since_leg: u32,
    /// Times every local obstacle was forgotten to reach a sealed-off frontier.
    global_forgets: u32,
    /// Sliver phase bookkeeping: free cells when the phase began (or the
    /// last growth), and slivers served since without growth.
    sliver_free_cells: Option<usize>,
    sliver_served: u32,
    /// The axis of the passage being walked, kept while drops stay near:
    /// the planned path runs out before the passage does.
    passage_axis: Option<f64>,
    /// How far the axis has been turned away from the drops (radians,
    /// signed), see [`PASSAGE_BIAS_STEP_RAD`].
    passage_bias: f64,
    /// The pose seen last, to notice the map moving under the duck.
    last_pose: Option<((f64, f64), Instant)>,
    /// Stands still owed before the pose is trusted again.
    unsettled: u32,
    /// Where a panorama was taken: not again within [`PANO_MIN_GAP_M`].
    panoramas: Vec<(f64, f64)>,
    /// When the duck last stood somewhere it had already mapped, on
    /// purpose. `None` until the first one is due.
    anchored_at: Option<Instant>,
    /// Where it is going back to, while it is going.
    anchor: Option<(f64, f64)>,
    /// Stands in a row at which the sensor and the map disagreed.
    distrust: u32,
    /// Stands in a row at which the route contradicted the sensor.
    route_conflicts: u32,
    /// The last conflict's stand and point, and how often it repeated.
    route_repeat: Option<((f64, f64), (f64, f64), u32)>,
    /// The aim the last leg steered at, with the stand it was chosen for
    /// ([`AIM_REACHED_M`]).
    aim: Option<((f64, f64), (f64, f64))>,
    /// The aim the duck is committed to walking straight at; `None` while
    /// turning ([`GO_EXIT_RAD`]).
    going: Option<(f64, f64)>,
    /// Times this job has doubted a detour of its own making
    /// ([`DETOUR_SUSPECT`]), capped at [`DETOUR_DOUBTS_MAX`].
    doubts: u32,
    /// The route length planned on the last pass of this journey, and how
    /// many times the short way has been insisted on (see [`ROUTE_JUMP`]).
    last_route_m: Option<f64>,
    insisted: u32,
}




/// The pose after a leg, from the newest frame (else the one before it).
fn pose_after(robot: &dyn Body, before: (f64, f64, f64)) -> (f64, f64, f64) {
    robot.frame().map(|f| f.pose()).unwrap_or(before)
}

fn stand(robot: &mut dyn Body, stop_s: f64) -> Result<Value, String> {
    robot.step(&json!({"walk_s": 0, "stop_s": stop_s}))
}

fn dist2((x, y): (f64, f64), (tx, ty): (f64, f64)) -> f64 {
    ((tx - x).powi(2) + (ty - y).powi(2)).sqrt()
}

fn wrap(a: f64) -> f64 {
    a.sin().atan2(a.cos())
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}


mod books;
mod fit;
mod gait;
pub(crate) use gait::turn_in_place_on;
mod guarded;
mod journey;
mod mapping;
mod mode;
mod recover;
mod trusted;
use books::*;
use gait::*;
use guarded::*;
use journey::*;
use mapping::*;
use mode::*;
use trusted::*;

impl Job {
    pub fn new(known: Vec<(f64, f64)>, max_s: f64, ask: bool, turn: f64, now: Instant) -> Self {
        Self {
            known,
            max_s,
            ask,
            turn: if turn < 0.0 { -1.0 } else { 1.0 },
            started: now,
            visited: Vec::new(),
            refused: Vec::new(),
            widened: Vec::new(),
            sealed: Vec::new(),
            books_refusal: std::cell::Cell::new(None),
            goal: None,
            refused_cleared: false,
            refused_cleared_at: None,
            local: Vec::new(),
            target: None,
            last_back: None,
            last_leg_vyaw: 0.0,
            last_aim: None,
            last_pose_stand: None,
            frozen: false,
            policy: Policy::for_mode(Mode::Mapping),
            trusted: TrustedFloor::default(),
            last_fit: None,
            lanes: Vec::new(),
            stalls_in_row: 0,
            watch: false,
            watch_booked: false,
            ground_drops: 0,
            frozen_drops_noted: false,
            drop_refusals_in_row: 0,
            last_drop_refusal: None,
            passage_refusals: 0,
            turns_refused_at_drop: 0,
            passage_at_mouth: false,
            passage_narrow: false,
            narrow_refusals: (0, (f64::NAN, f64::NAN)),
            fell: None,
            relocate_steps: 0,
            rim_offs: 0,
            no_go: Vec::new(),
            unseals_here: None,
            drops_bookable: true,
            budget_extended: false,
            goal_confirmed: false,
            kept_route: None,
            refused_since_plan: false,
            lost_since: None,
            stuck: 0,
            last_unseal: None,
            since_leg: 0,
            spins_since_leg: 0,
            global_forgets: 0,
            passage_axis: None,
            trail: Vec::new(),
            sliver_free_cells: None,
            sliver_served: 0,
            passage_bias: 0.0,
            last_pose: None,
            unsettled: 0,
            panoramas: Vec::new(),
            distrust: 0,
            anchored_at: None,
            anchor: None,
            aim: None,
            going: None,
            doubts: 0,
            route_conflicts: 0,
            route_repeat: None,
            last_route_m: None,
            insisted: 0,
        }
    }
    /// A watch job (see the `watch` field).
    pub fn watch(max_s: f64, now: Instant) -> Self {
        let mut job = Job::new(Vec::new(), max_s, false, 1.0, now);
        job.watch = true;
        job
    }

    /// One turn of a watch: the trail follows the body; standing still
    /// [`WATCH_STILL_S`], the drops the frames vote for go on the books
    /// once per stand, and what the stand saw becomes trusted floor.
    fn watch_turn(&mut self, handle: &ExploreHandle, robot: &mut dyn Body, frame: &MapFrame) {
        let (x, y, _) = frame.pose();
        if !robot.pose_trusted() {
            return;
        }
        let here = (x, y);
        match self.last_pose {
            Some((p, _)) if dist2(p, here) >= TRAIL_STEP_M => {
                self.walked(p, here);
                self.last_pose = Some((here, robot.now()));
                self.watch_booked = false;
            }
            None => {
                self.last_pose = Some((here, robot.now()));
            }
            Some((_, since)) => {
                if !self.watch_booked && (robot.now() - since).as_secs_f64() >= WATCH_STILL_S {
                    let before = self.local.len();
                    self.record_drops(robot);
                    self.trust_seen(robot);
                    self.watch_booked = true;
                    let (local, trail) = (self.local.clone(), self.trail.clone());
                    let cells = self.trusted.cells();
                    tracing::info!(at = ?(format!("{x:.2}"), format!("{y:.2}")), booked = self.local.len() - before, drops = self.local.iter().filter(|(_, r)| *r >= DROP_RADIUS_M).count(), "map explore: watch — a stand, the books");
                    handle.update(|s| {
                        s.local_obstacles = local.len();
                        s.local = local;
                        s.trail = trail;
                        s.trusted = cells;
                        s.legs += 1;
                    });
                }
            }
        }
    }

    pub fn run(&mut self, handle: &ExploreHandle, robot: &mut dyn Body) -> (State, String) {
        let mut turn_began = robot.now();
        loop {
            let turn_s = (robot.now() - turn_began).as_secs_f64();
            if turn_s > 0.2 {
                tracing::info!(turn_s = format!("{turn_s:.1}"), "map explore: turn");
            }
            turn_began = robot.now();
            if handle.stop.load(Ordering::Relaxed) {
                return (State::Stopped, "stopped on request".into());
            }
            if (robot.now() - self.started).as_secs_f64() > self.max_s {
                return (
                    State::Done,
                    format!("time budget of {:.0} s spent", self.max_s),
                );
            }
            let Some(frame) = robot.frame() else {
                robot.sleep(WAIT);
                continue;
            };
            let frozen = robot.frozen_map();
            if frozen != self.frozen {
                tracing::info!(frozen, "map explore: the live map is {}", if frozen { "frozen: a journey trusts the planner" } else { "live: guards and stands" });
                self.frozen = frozen;
            }
            self.note_fall(robot, &frame);
            if self.watch {
                self.watch_turn(handle, robot, &frame);
                robot.sleep(WATCH_TICK);
                continue;
            }
            if self.blind() {
                handle.update(|s| s.blind = true);
            }
            self.resolve_policy();
            // An untrusted pose that does not move, with a good fit (the
            // seen walls on the mapped ones), is maploc's watchdog on
            // furniture, not a lost duck (explmap4, 2026-09-19: four
            // minutes standing in the bedroom, the pose 12 cm right).
            // After a first stand it walks on — guarded, every leg judged
            // by the sensor — and maploc catches up when it can.
            let stable_untrusted = !robot.pose_trusted()
                && !frame.seated
                && self.lost_since.is_some_and(|s| (robot.now() - s).as_secs_f64() > STABLE_UNTRUSTED_S)
                && self.last_pose.is_some_and(|(p, _)| dist2(p, (frame.x, frame.y)) < SETTLED_M)
                && self.last_fit.is_some_and(|f| f < STABLE_UNTRUSTED_FIT_M)
                // Never after a fall: the fit there is the fit of a pose
                // nobody can vouch for.
                && self.fell.is_none();
            if stable_untrusted {
                tracing::info!(fit = ?self.last_fit, "map explore: the pose is untrusted but stable and fits the map; mapping on, guarded");
            }
            if !robot.pose_trusted() && !stable_untrusted {
                let now = robot.now();
                let since = *self.lost_since.get_or_insert(now);
                if now - since > LOST_PATIENCE {
                    return (
                        State::Failed,
                        if frame.seated {
                            "the duck is seated or fallen and did not get up".into()
                        } else {
                            "the duck could not find its position again".into()
                        },
                    );
                }
                if self.fell.is_some() && !frame.seated {
                    // Up again after a fall: finding the pose is the first
                    // thing, before any leg of the job — look around.
                    self.relocate_step(robot);
                } else {
                    // Standing still is what relocalization needs.
                    let _ = stand(robot, self.turn_stand_s());
                }
                continue;
            }
            let lost_just_now = if stable_untrusted { false } else { self.lost_since.take().is_some() };
            if stable_untrusted {
                // The stand for the fit, each turn, so the judgement is fresh.
                let _ = stand(robot, self.turn_stand_s());
                if let Ok(g) = frame.grid() {
                    self.measure_fit(robot, handle, &g, frame.pose());
                }
            }
            let pose = frame.pose();
            let (x, y, _yaw) = pose;
            // The map can move under the duck: a loop closure or a
            // relocalization that lands on the wrong spot shifts the pose
            // by metres between two frames while the body walked a leg at
            // most. Plans made on such a pose are wrong, so are the local
            // obstacles recorded in the old one: forget them, and stand
            // until two stands in a row agree on where the duck is.
            let now = robot.now();
            if let Some((prev, at)) = self.last_pose {
                let allowed = JUMP_SLACK_M + GAIT_M_PER_S * (now - at).as_secs_f64();
                let moved = dist2(prev, (x, y));
                if moved > allowed {
                    // On a frozen map the books are the map's own (the
                    // ground book, in the map's frame): a pose that
                    // jumped is wrong, the rim is where it was. Clearing
                    // them there would send the next blind leg over the
                    // stairwell with nothing on the books (the lost-in-
                    // localize rule, 2026-09-19). On a live map the
                    // books were written in the frame that moved: gone.
                    if self.frozen {
                        tracing::info!(moved, allowed, "map explore: the pose jumped on the frozen map; the books stay, settling");
                    } else {
                        tracing::info!(moved, allowed, "map explore: the map moved under the duck, settling");
                        self.local.clear();
                    }
                    self.target = None;
                    self.kept_route = None;
                    self.unsettled = SETTLE_STANDS;
                }
            }
            if lost_just_now {
                self.unsettled = SETTLE_STANDS;
            }
            self.last_pose = Some(((x, y), now));
            if self.unsettled > 0 {
                let before = (x, y);
                let _ = stand(robot, self.turn_stand_s());
                let after = robot.frame().map(|f| (f.x, f.y));
                if after.is_some_and(|a| dist2(before, a) < SETTLED_M) {
                    self.unsettled -= 1;
                }
                continue;
            }
            let Ok(grid) = frame.grid() else {
                robot.sleep(WAIT);
                continue;
            };

            // A false pose shows itself at a stand: the sensor sees walls
            // where the map, from where it thinks the duck is, shows open
            // floor — or sees through walls the map has. The map moving
            // under the duck is caught by the jump guard above; a pose
            // that is wrong and *stable* is caught only here.
            if let Some(cliff) = robot.cliff() {
                let (agree, disagree) = map_sensor_agreement(&grid, &cliff, pose);
                if disagree >= DISAGREE_MIN && disagree >= 3 * agree {
                    self.distrust += 1;
                    tracing::info!(agree, disagree, streak = self.distrust, "map explore: map and sensor disagree");
                } else if agree > 0 {
                    self.distrust = 0;
                }
                if self.distrust >= DISTRUST_FAIL {
                    return (
                        State::Failed,
                        format!(
                            "position lost: the map and the depth sensor disagreed at {} stands in a row",
                            self.distrust
                        ),
                    );
                }
                if self.distrust == DISTRUST_PANORAMA {
                    // A full look around gives the mapper its best chance
                    // to close a loop and put the duck back where it is.
                    self.distrust += 1;
                    self.panorama(handle, robot, (x, y));
                    self.unsettled = SETTLE_STANDS;
                    continue;
                }
            }

            // 0. Somewhere mostly unknown around: look all around first.
            if self.wants_panorama(&grid, pose) {
                self.panorama(handle, robot, (x, y));
                // Seen all around: set off toward the most open floor.
                self.head_for_space(robot, &grid);
                continue;
            }

            // 0b. Every so often, go back to somewhere already mapped and
            // stand there. A loop closes where the duck revisits, and
            // exploring alone never revisits on purpose — see
            // `ANCHOR_EVERY_S`.
            if reanchor() && self.goal.is_none() {
                let due = self
                    .anchored_at
                    .is_none_or(|t| (robot.now() - t).as_secs_f64() >= ANCHOR_EVERY_S);
                if self.anchor.is_none() && due {
                    if let Some(p) = self.anchor_target((x, y)) {
                        tracing::info!(at = ?(x, y), back_to = ?p, "map explore: going back to close a loop");
                        self.anchor = Some(p);
                    } else {
                        // Nothing old enough within reach; ask again
                        // shortly, not after the whole interval.
                        tracing::debug!(at = ?(x, y), "map explore: nowhere old to go back to yet");
                        self.anchored_at = Some(
                            robot.now()
                                - Duration::from_secs_f64(ANCHOR_EVERY_S - ANCHOR_RETRY_S),
                        );
                    }
                }
            }
            if let Some(anchor) = self.anchor {
                if dist2((x, y), anchor) < ANCHOR_ARRIVE_M {
                    tracing::info!(at = ?(x, y), "map explore: standing at the old place");
                    let _ = stand(robot, ANCHOR_STAND_S);
                    self.anchor = None;
                    self.anchored_at = Some(robot.now());
                    continue;
                }
                let walls = self.planner_walls();
                let lanes = self.lanes();
                match path_to(&grid, x, y, anchor, &walls, inflate_m(), &lanes) {
                    Some(path) => {
                        let f = Frontier {
                            cells: 0,
                            centroid: anchor,
                            target: anchor,
                            stand: anchor,
                            distance_m: path.len() as f64 * grid.cell_m,
                            cost: 0,
                            score: 0.0,
                            path,
                        };
                        if let Some(verdict) = self.walk_leg(handle, robot, &grid, pose, &f) {
                            return verdict;
                        }
                        continue;
                    }
                    None => {
                        // The way back is blocked or the map moved: not
                        // worth fighting for, the frontier work matters
                        // more.
                        tracing::info!("map explore: no way back to the old place; carrying on");
                        self.anchor = None;
                        self.anchored_at = Some(robot.now());
                    }
                }
            }

            // 1. The map decides where to go.
            let blocked: Vec<((f64, f64), f64)> = self
                .visited
                .iter()
                .chain(self.refused.iter())
                .copied()
                .collect();
            // In a pocket the usual margin says has no way out, plan with
            // the body's own half-width instead.
            let inflate = if self.stuck >= 2 { SQUEEZE_INFLATE_M } else { inflate_m() };
            self.strike_drops_under((x, y));
            if self.fast() {
                let now = robot.now();
                let due = match self.last_pose_stand {
                    None => true,
                    Some((t, p)) => (now - t).as_secs_f64() >= FAST_POSE_EVERY_S || dist2(p, (x, y)) >= FAST_POSE_EVERY_M,
                };
                // An obstacle in the lane: one stand to look at it, then
                // the plan goes on — not a stand at every plan while it
                // is still there (fast2 stood every two seconds beside a
                // door jamb, 0.3 m from the goal, to the budget).
                let recent = self.last_pose_stand.is_some_and(|(t, _)| (now - t).as_secs_f64() < FAST_AHEAD_EVERY_S);
                let ahead = !recent
                    && robot
                        .cliff()
                        .and_then(|c| c.obstacle_in_lane(robot.now(), 0.0, lane_half_m()))
                        .is_some_and(|o| o.range_m < FAST_STOP_AHEAD_M);
                if due || ahead {
                    tracing::info!(due, ahead, "map explore: fast: a stand for the pose");
                    let _ = stand(robot, FAST_POSE_STAND_S);
                    self.last_pose_stand = Some((robot.now(), (x, y)));
                }
            }
            let walls = self.planner_walls();
            self.walked((x, y), (x, y));
            let lanes = self.lanes();
            if let Some(goal) = self.goal {
                if dist2((x, y), goal) < GOAL_ARRIVE_M {
                    // A stand, and "arrived" judged on the pose AFTER it:
                    // a pose 0.6 m off along a corridor reached the goal's
                    // coordinates 0.5 m from the goal without knowing
                    // (lost6, 2026-09-19). The stand gives maploc its
                    // window; if its correction moves the pose off the
                    // goal, the journey goes on from there — once (the
                    // fit was tried as the judge and failed every arrival
                    // at 0.16–0.20 for nothing, house18tour: it does not
                    // correlate with the truth, point 1).
                    let _ = stand(robot, FRONTIER_STOP_S);
                    if !self.goal_confirmed {
                        self.goal_confirmed = true;
                        if let Some(f) = robot.frame() {
                            let left = dist2((f.x, f.y), goal);
                            if left >= GOAL_ARRIVE_M {
                                tracing::info!(left_m = format!("{left:.2}"), "map explore: at the goal's coordinates, but the stand moved the pose off it; going on");
                                self.kept_route = None;
                                continue;
                            }
                        }
                    }
                    return (State::Done, format!("arrived at ({:.2}, {:.2})", goal.0, goal.1));
                }
                // At the mouth of a passage the turn beside the drop is
                // what fails, not the leg: the kick refused and no way
                // back, over and over (goround2, 2026-09-20: 172 times
                // in ten minutes, and no seal, since only a LEG refused
                // for the drop counted). So many turns refused beside a
                // drop without a leg between seal the rim as the legs do.
                if self.policy.seal && self.turns_refused_at_drop >= TURNS_REFUSED_SEAL {
                    self.turns_refused_at_drop = 0;
                    let seen = robot.cliff();
                    let at = seen
                        .as_ref()
                        .and_then(|c| c.nearest(robot.now()))
                        .map(|d| {
                            let b = _yaw + d.bearing;
                            (x + d.range_m.max(0.2) * b.cos(), y + d.range_m.max(0.2) * b.sin())
                        })
                        .or_else(|| {
                            self.local.iter().map(|p| p.0).min_by(|a, b| dist2(*a, (x, y)).total_cmp(&dist2(*b, (x, y))))
                        });
                    if let Some(at) = at {
                        tracing::info!(at = ?at, "map explore: the turn beside the drop refused over and over; the rim is sealed as a refused leg would seal it");
                        self.seal_rim(at);
                        self.kept_route = None;
                        continue;
                    }
                }
                // The route kept from the last plan, trimmed to the body,
                // when nothing calls for a new one.
                let lanes_owned: Vec<(f64, f64)> = lanes.to_vec();
                let lanes: &[(f64, f64)] = &lanes_owned;
                let books_now = self.local.len();
                let refused_since = self.refused_since_plan;
                let kept = if keep_route() {
                    self.kept_route.take().and_then(|(raw, pulled, at, books)| {
                        let fresh = (robot.now() - at).as_secs_f64() < KEEP_ROUTE_S;
                        let same_books = books == books_now;
                        let (near_i, near_d) = pulled
                            .iter()
                            .enumerate()
                            .map(|(i, p)| (i, dist2(*p, (x, y))))
                            .min_by(|a, b| a.1.total_cmp(&b.1))
                            .unwrap_or((0, f64::INFINITY));
                        let trimmed: Vec<(f64, f64)> = pulled[near_i..].to_vec();
                        let ok = fresh
                            && same_books
                            && !refused_since
                            && near_d <= KEEP_ROUTE_OFF_M
                            && trimmed.len() >= 2
                            && route_passable(&grid, &trimmed, 1.0, &walls, inflate, lanes);
                        if ok {
                            Some((raw, trimmed, at, books))
                        } else {
                            tracing::info!(fresh, same_books, refused = refused_since, off_m = format!("{near_d:.2}"), "map explore: planning the route anew");
                            None
                        }
                    })
                } else {
                    None
                };
                let (raw, mut path, planned_at, books_then) = match kept {
                    Some((raw, path, at, books)) => (raw, path, at, books),
                    None => {
                        let Some((raw, path)) = path_to_both(&grid, x, y, goal, &walls, inflate, lanes) else {
                            if self.stuck < STUCK_MAX {
                                self.unseal(robot, &grid, (x, y), "no way to the goal from here");
                                continue;
                            }
                            return (State::Failed, format!("no way to ({:.2}, {:.2}) on the map", goal.0, goal.1));
                        };
                        self.refused_since_plan = false;
                        (raw, path, robot.now(), self.local.len())
                    }
                };
                self.kept_route = Some((raw.clone(), path.clone(), planned_at, books_then));
                // The short way, insisted on (see `ROUTE_JUMP`).
                let long_m = path.len() as f64 * grid.cell_m;
                if inflate > SQUEEZE_INFLATE_M
                    && route_jump() > 0.0
                    && self.last_route_m.is_none_or(|last| long_m > route_jump() * last)
                    && let Some(squeezed) = path_to(&grid, x, y, goal, &walls, SQUEEZE_INFLATE_M, lanes)
                    && keep_the_short_way(self.last_route_m, long_m, Some(squeezed.len() as f64 * grid.cell_m), self.insisted)
                {
                    self.insisted += 1;
                    tracing::info!(
                        at = ?(x, y),
                        long_m = format!("{long_m:.2}"),
                        short_m = format!("{:.2}", squeezed.len() as f64 * grid.cell_m),
                        last_m = format!("{:.2}", self.last_route_m.unwrap_or(0.0)),
                        insisted = self.insisted,
                        "map explore: the route jumped; keeping the short way with the body's own width"
                    );
                    path = squeezed;
                }
                self.last_route_m = Some(path.len() as f64 * grid.cell_m);
                let f = Frontier {
                    cells: 0,
                    centroid: goal,
                    target: goal,
                    stand: goal,
                    distance_m: path.len() as f64 * grid.cell_m,
                    cost: 0,
                    score: 0.0,
                    path,
                };
                // The route as planned on this pass. A journey that doubles
                // back leaves its trace here: the length jumping down when a
                // shorter way opens, or the detour ratio swinging as two
                // routes trade places. Measured 2026-09-12, the journey that
                // goes wrong spends its first minute walking *away* from the
                // goal (distance 2.58 → 3.06 m) while the good one falls
                // straight to it — and a plan is the only thing that can
                // send it the wrong way on purpose.
                let straight_now = dist2((x, y), goal);
                tracing::info!(
                    at = ?(x, y),
                    route_m = format!("{:.2}", f.distance_m),
                    straight_m = format!("{:.2}", straight_now),
                    detour = format!("{:.2}", f.distance_m / straight_now.max(0.01)),
                    "map explore: route to the goal"
                );
                if self.doubt_the_detour(robot, &grid, (x, y), goal, &f, straight_now, inflate) {
                    continue;
                }
                let (local, trail, left) = (self.local.clone(), self.trail.clone(), straight_now);
                let route = f.path.clone();
                handle.update(|s| {
                    s.trail = trail;
                    s.frontiers_left = 1;
                    s.target = Some(goal);
                    s.target_distance_m = Some(left);
                    s.local_obstacles = local.len();
                    s.local = local;
                    s.route = route;
                    s.route_raw = raw;
                    s.goal = Some(goal);
                });
                if let Some(verdict) = self.walk_leg(handle, robot, &grid, pose, &f) {
                    return verdict;
                }
                continue;
            }
            let mut fs = frontiers_with(&grid, x, y, &blocked, &walls, inflate, &lanes);
            // Big frontiers first, wherever they are; slivers last, and
            // not for ever: once no big group is left anywhere on the map,
            // the job ends as soon as the map stops growing — the slivers
            // are edges the stands keep re-minting, and chasing them is
            // what kept the twin walking to the budget on a finished map.
            if largest_frontier(&grid, &walls) >= BIG_FRONTIER_CELLS {
                self.sliver_free_cells = None;
                self.sliver_served = 0;
            } else {
                let free = grid.counts().1;
                self.sliver_served += 1;
                match self.sliver_free_cells {
                    Some(base) if free >= base + SLIVER_GROWTH_CELLS => {
                        self.sliver_free_cells = Some(free);
                        self.sliver_served = 0;
                    }
                    Some(_) if self.sliver_served > SLIVER_PATIENCE => {
                        return (
                            State::Done,
                            format!(
                                "only slivers remain and the map stopped growing: {} submaps, {} windows, {} loops",
                                frame.n_submaps, frame.windows, frame.n_loops
                            ),
                        );
                    }
                    Some(_) => {}
                    None => self.sliver_free_cells = Some(free),
                }
            }
            let big: Vec<Frontier> = fs.iter().filter(|f| f.cells >= BIG_FRONTIER_CELLS).cloned().collect();
            if !big.is_empty() {
                fs = big;
            }
            let Some(f) = self.pick(&fs) else {
                if !self.refused.is_empty() && !self.refused_cleared {
                    // Frontiers refused earlier may be reachable from here.
                    self.refused.clear();
                    self.refused_cleared = true;
                    self.refused_cleared_at = Some((x, y));
                    continue;
                }
                // Nothing reachable *from here* is not the same as nothing
                // left. Frontier cells still on the map mean the duck is
                // sealed in — by the local obstacles it collected, by the
                // margin, or by a map that moved under it — so forget the
                // obstacles around, step back and turn, stand so the map
                // gets a scan, and try again; a few times, then say so.
                let cells_left = frontier_cells(&grid);
                // Sealed off by obstacles on the books far from the duck,
                // not by the map: what was recorded far away is stale
                // knowledge by now (the map has had stands there since), so
                // forget those and plan again — the near ones stay, and the
                // sensor records again whatever is really there.
                if cells_left >= MIN_FRONTIER_CELLS && self.global_forgets < GLOBAL_FORGETS_MAX {
                    let kept: Vec<ExtraWall> = self
                        .local
                        .iter()
                        .copied()
                        .filter(|(p, r)| *r >= DROP_RADIUS_M || dist2(*p, (x, y)) <= FORGET_FAR_M)
                        .collect();
                    let kept_walls: Vec<ExtraWall> = kept
                        .iter()
                        .map(|(p, r)| if *r >= DROP_RADIUS_M { (*p, self.policy.drop_plan_radius_m) } else { (*p, *r) })
                        .collect();
                    if kept.len() < self.local.len()
                        && !frontiers_with(&grid, x, y, &blocked, &kept_walls, inflate, &self.lanes()).is_empty()
                    {
                        self.global_forgets += 1;
                        tracing::info!(
                            forgotten = self.local.len() - kept.len(),
                            "map explore: obstacles recorded far away seal the rest of the map; forgetting them"
                        );
                        self.local = kept;
                        continue;
                    }
                }
                if cells_left >= MIN_FRONTIER_CELLS && self.stuck < STUCK_MAX {
                    let why = if frontiers_with(&grid, x, y, &blocked, &[], SQUEEZE_INFLATE_M, &self.lanes()).is_empty() {
                        "no way out of here on the map"
                    } else {
                        "sealed in by local obstacles"
                    };
                    self.unseal(robot, &grid, (x, y), why);
                    continue;
                }
                let why = if cells_left >= MIN_FRONTIER_CELLS {
                    "stuck: frontiers remain but none is reachable from here"
                } else {
                    "no frontier left"
                };
                return (
                    State::Done,
                    format!(
                        "{why}: {} submaps, {} windows, {} loops",
                        frame.n_submaps, frame.windows, frame.n_loops
                    ),
                );
            };
            let to_target = dist2((x, y), f.stand);
            let local = self.local.clone();
            let trail = self.trail.clone();
            let route = f.path.clone();
            handle.update(|s| {
                s.trail = trail;
                s.frontiers_left = fs.len();
                s.target = Some(f.target);
                s.target_distance_m = Some(to_target);
                s.local_obstacles = local.len();
                s.local = local;
                s.route = route;
                s.goal = Some(f.target);
            });

            if to_target < ARRIVE_M {
                self.arrive(handle, robot, &f, pose);
                continue;
            }

            if let Some(verdict) = self.walk_leg(handle, robot, &grid, pose, &f) {
                return verdict;
            }
            continue;
        }
    }
    fn walk_leg(
        &mut self,
        handle: &ExploreHandle,
        robot: &mut dyn Body,
        grid: &Grid,
        pose: (f64, f64, f64),
        f: &Frontier,
    ) -> Option<(State, String)> {
        let iteration_began = robot.now();
        let (x, y, yaw) = pose;
        let to_target = dist2((x, y), f.stand);
        if switch("QK_RIM_OFF").unwrap_or(true) && self.off_the_rim(robot, pose) {
            let _ = stand(robot, self.turn_stand_s());
            return None;
        }
        // The middle level: the route against the sensor, before a leg.
        if self.route_contradicted(&*robot, pose, &f.path) {
            let _ = stand(robot, self.turn_stand_s());
            return None;
        }
            // 2. Go straight for the standing point when the straight line
            // to it is clear for the body; follow the grid path — which
            // zigzags by nature — only when something is in the way.
            // Steering is for getting around obstacles, not for tracing
            // cells.
            let straight = (f.stand.1 - y).atan2(f.stand.0 - x);
            let look = to_target.min(straight_look_m());
            // How far along the path to aim when the straight line is not
            // clear: a stride while mapping, a metre on a journey (see
            // [`GOAL_LOOKAHEAD_M`]).
            let ahead_m = if self.goal.is_some() { goal_lookahead_m(self.follow()) } else { lookahead_m() };
            // Beside a drop, booked or seen, the route itself: the aim a
            // step along it, neither the string pulled nor an old aim
            // held. The grid path keeps the planner's margin from the
            // books; a straight line to a point two metres on did not —
            // the aim of the fall of 2026-09-23 was held 2 m ahead from
            // before the stairwell to its rim, the green line bending
            // round it unwalked (the user's eye).
            let near_drop = self.drop_within_any(&*robot, STRING_NEAR_DROP_M).is_some();
            let aim = if !self.follow()
                && !near_drop
                && grid.lane_clear(x, y, straight, look, lane_half_m())
                && self.clear_of_local(x, y, straight, look, lane_half_m())
            {
                f.stand
            } else if smooth_path() && !near_drop {
                self.farthest_clear(grid, (x, y), &f.path)
                    .or_else(|| waypoint(&f.path, ahead_m, grid.cell_m))
                    .unwrap_or(f.stand)
            } else {
                waypoint(&f.path, ahead_m, grid.cell_m).unwrap_or(f.stand)
            };
            // Beside a drop, the middle of the way: the aim slid across
            // the heading to where the wall (or the thing) on one side and
            // the rim on the other are as far. The planner keeps the rim
            // wider than a wall (the drop's radius and the widening), so
            // its route between them runs along the wall: at house2's
            // living-room door, between the jamb and the stairwell's west
            // rim (0.54 m), the body walked 7 cm off the jamb, the sensor
            // saw it in the lane, and "no room" 75 times on the spot
            // (2026-09-24).
            let aim = if near_drop && switch("QK_CENTRE").unwrap_or(true) { self.centred(grid, (x, y), aim) } else { aim };
            // Hold it unless it is reached, blocked, or bettered — only on a
            // journey; a mapping job's aim is its frontier's business.
            let aim = if self.goal.is_some() && hold_aim_enabled() && !near_drop {
                self.hold_aim(grid, (x, y), yaw, aim, f.stand)
            } else {
                aim
            };
            // Never an aim behind the beak: the waypoint counts cells from
            // the route's start, and a body beside or past that start got
            // an aim behind it, and walked round it (the user's eye,
            // 2026-09-16). The first route point ahead, 0.2 m out, instead.
            let aim = if wrap((aim.1 - y).atan2(aim.0 - x) - yaw).abs() > std::f64::consts::FRAC_PI_2 {
                f.path
                    .iter()
                    .copied()
                    .find(|p| dist2(*p, (x, y)) >= 0.2 && wrap((p.1 - y).atan2(p.0 - x) - yaw).abs() <= std::f64::consts::FRAC_PI_2)
                    .unwrap_or(aim)
            } else {
                aim
            };
            let err = wrap((aim.1 - y).atan2(aim.0 - x) - yaw);
            // Stuck beside a drop: no turn in place this near the rim, the
            // kick refused, no way back — and the duck stood there, while
            // the standing gait crept it toward the hole, until it fell in
            // (casa_arredata, 2026-09-23: 49 refusals in three minutes
            // 0.12 m from the stairwell, 6 cm crept, a fall standing
            // still). Never wait at a rim: the way on ahead, guarded, out
            // of its reach to turn there; else the aim is given up.
            if self.turns_refused_at_drop >= TURNS_REFUSED_ESCAPE {
                self.turns_refused_at_drop = 0;
                self.going = None;
                let leg = json!({"vx": 0.3, "vyaw": 0.0, "walk_s": 1.5, "stop_s": self.turn_stand_s(), "gap": true, "steer": false,
                                 "passage": passage_lane(), "cliff_margin_m": passage_cliff_margin_m()});
                let walked = self.guarded_step(robot, pose, &leg);
                tracing::info!(at = ?(x, y, yaw), walked = walked.is_ok(), why = walked.as_ref().err().map(String::as_str).unwrap_or(""),
                               "map explore: stuck beside a drop, no turn and no way back; the way on ahead, to turn out of the rim's reach");
                if walked.is_err() {
                    if self.goal.is_some() {
                        return Some((State::Failed, "stuck beside a drop: no turn there, no way back and none ahead".into()));
                    }
                    if let Some((t, _)) = self.target.take() {
                        self.refused.push((t, BLOCK_REFUSED_M));
                    }
                }
                return None;
            }
            // Turn, then go — and stay going until the error is well past
            // the entry, not merely past it (see `GO_EXIT_RAD`).
            let going = commit() && self.gate(aim, err);
            // Which aim, and how far off the nose it is. The journey that
            // goes wrong without any detour walks three to five times its
            // own route in loops (2026-09-12: 16.9 m for a 5.2 m route, the
            // plan sane throughout), and an aim that flips between the
            // stand and a waypoint is the only thing that can draw a loop.
            // `by` says which branch chose it, so a flip shows as that word
            // changing back and forth while the duck goes nowhere.
            handle.update(|s| s.aim = Some(aim));
            self.last_aim = Some(aim);
            if self.goal.is_some() {
                tracing::info!(
                    at = ?(x, y),
                    aim = ?(format!("{:.2}", aim.0), format!("{:.2}", aim.1)),
                    by = if aim == f.stand { "straight" } else { "path" },
                    err_deg = format!("{:.0}", err.to_degrees()),
                    phase = if !commit() { "legacy" } else if going { "go" } else { "turn" },
                    "map explore: aim"
                );
            }
            // How long every leg below stands afterwards: three seconds
            // while mapping, none while hurrying to a goal over floor
            // already mapped (bar every fifth).
            let stop_s = self.stop_s(handle.status().legs);

            // 2b. A passage beside a drop: onto its axis, then straight.
            let mut passage_leg: Option<Value> = None;
            let mut aim = aim;
            let mut err = err;
            if let Some((axis0, offset, drop_side)) = self.passage(&*robot, grid, pose, &f.path, Some(f.stand)) {
                // Try D: with the wall's line exact, the heading held is
                // the line's direction bent toward the line itself — a
                // pursuit point PASSAGE_PURSUIT_M ahead on it — so the
                // yaw carries the body onto the line. Without it the
                // 15° the nearest-ray axis was skewed by did that by
                // accident on the paper twin, and the exact axis alone
                // walked 8 cm from the rim and was refused (kmouth-D
                // 3/30 against 24/30, 2026-09-20).
                let pursuit = if self.policy.wall_fit { (offset / PASSAGE_PURSUIT_M).atan().clamp(-PASSAGE_PURSUIT_MAX_RAD, PASSAGE_PURSUIT_MAX_RAD) } else { 0.0 };
                let axis = wrap(axis0 + self.passage_bias + pursuit);
                let e = wrap(axis - yaw);
                let (nx, ny) = (-axis.sin(), axis.cos());
                // Point 2, try B (`QK_MOUTH_AIM`): off the centre line at
                // the mouth, the aim is the line itself 0.4 m ahead — it
                // enters aligned or not at all.
                let converge = self.policy.mouth_aim && self.passage_at_mouth && offset.abs() > 0.12;
                if converge {
                    let a = (x + 0.4 * axis.cos() + offset * nx, y + 0.4 * axis.sin() + offset * ny);
                    err = wrap((a.1 - y).atan2(a.0 - x) - yaw);
                    aim = a;
                    self.last_aim = Some(a);
                    handle.update(|s| s.aim = Some(a));
                    tracing::info!(at = ?(x, y, yaw), aim = ?(format!("{:.2}", a.0), format!("{:.2}", a.1)), err_deg = format!("{:.0}", err.to_degrees()), offset = format!("{offset:.2}"), "map explore: passage beside a drop: the aim before the mouth, on the centre line");
                } else if e.abs() > PASSAGE_ALIGN_RAD && self.spins_since_leg < SPINS_MAX {
                    self.spins_since_leg += 1;
                    self.going = None;
                    tracing::info!(at = ?(x, y, yaw), axis, offset, "map explore: passage beside a drop: aligning to its axis");
                    let ok = self.align(robot, axis);
                    tracing::info!(ok, "map explore: passage beside a drop: alignment");
                    let _ = stand(robot, self.turn_stand_s());
                    return None;
                }
                if converge {
                    // The leg to the aim is the ordinary one, below.
                } else {
                    // Centring steers gently; if even that bends the path onto a
                    // drop, hold the axis and let the next stand re-measure.
                    let mut vyaw = (0.6 * e + PASSAGE_GAIN * offset).clamp(-0.15, 0.15);
                    // The held leg (the user's, 2026-09-18): aligned to the
                    // wall, one long straight leg with the heading held by
                    // taps — the heading it means to have is the axis, bent
                    // a little toward the line it should be on — instead of
                    // short steered legs. `QK_PASSAGE_HELD=0` for the latter.
                    let (walk_s, held) = if self.policy.held_leg { (PASSAGE_HELD_LEG_S.min(drop_leg_s()), true) } else { (PASSAGE_LEG_S, false) };
                    let bias = if held { (e + (PASSAGE_GAIN * offset).clamp(-0.3, 0.3)).clamp(-0.4, 0.4) } else { 0.0 };
                    if held {
                        vyaw = 0.0;
                    }
                    let mut leg = json!({"vx": 0.3, "vyaw": vyaw, "walk_s": walk_s, "stop_s": stop_s, "gap": true, "steer": false, "passage": passage_lane(), "cliff_margin_m": passage_cliff_margin_m(), "hold": held, "hold_bias": bias});
                    if self.drop_on_path(pose, &leg).is_some() {
                        vyaw = if held { 0.0 } else { (0.6 * e).clamp(-0.1, 0.1) };
                        leg = json!({"vx": 0.3, "vyaw": vyaw, "walk_s": walk_s, "stop_s": stop_s, "gap": true, "steer": false, "passage": passage_lane(), "cliff_margin_m": passage_cliff_margin_m(), "hold": held, "hold_bias": if held { e.clamp(-0.2, 0.2) } else { 0.0 }});
                    }
                    if self.drop_on_path(pose, &leg).is_some() && drop_side != 0.0 {
                        // Still onto a drop: turn the axis away from it and
                        // plan again from the top (a spin if the turn is big).
                        let bias = (self.passage_bias - drop_side * PASSAGE_BIAS_STEP_RAD)
                            .clamp(-PASSAGE_BIAS_MAX_RAD, PASSAGE_BIAS_MAX_RAD);
                        if bias != self.passage_bias {
                            self.passage_bias = bias;
                            tracing::info!(at = ?(x, y, yaw), bias, "map explore: passage beside a drop: axis turned away from the drop");
                            return None;
                        }
                    }
                    tracing::info!(at = ?(x, y, yaw), axis, offset, vyaw, "map explore: passage beside a drop: straight leg");
                    passage_leg = Some(leg);
                }
            }
            // 2b'. Beside a drop and too narrow for the pose's margin: not
            // walked. The drops beside it are widened for the planner (the
            // guard's refusal does the same) and the route is planned again;
            // with no way round the journey fails rather than brushing the
            // rim (the twin, 2026-09-23: three falls in that passage).
            if self.passage_narrow {
                let near: Vec<(f64, f64)> = self
                    .local
                    .iter()
                    .filter(|(p, r)| *r >= DROP_RADIUS_M && dist2(*p, (x, y)) < 0.6)
                    .map(|(p, _)| *p)
                    .collect();
                self.widened.extend(near.iter().copied());
                self.going = None;
                let (n, at) = self.narrow_refusals;
                let n = if dist2(at, (x, y)) < 0.10 { n + 1 } else { 1 };
                self.narrow_refusals = (n, (x, y));
                tracing::info!(at = ?(x, y, yaw), widened = near.len(), in_a_row = n, "map explore: planning around the narrow passage");
                if n >= NARROW_REFUSALS_MAX {
                    self.narrow_refusals = (0, (f64::NAN, f64::NAN));
                    if let Some((t, _)) = self.target.take() {
                        // Exploring: this frontier is not reachable safely today.
                        self.refused.push((t, BLOCK_REFUSED_M));
                        tracing::info!("map explore: the narrow passage is no way; the frontier is dropped");
                    } else {
                        return Some((State::Failed, "no safe way past the drop: the passage beside it is narrower than the body and the pose's margin".into()));
                    }
                }
                let _ = stand(robot, self.turn_stand_s());
                return None;
            }
            // 2c. The aim well off the nose: turn in place to it first,
            // closed on the yaw, then plan again — the human driver aligns
            // before entering, then goes straight (house1, 2026-09-15).
            if passage_leg.is_none() && spin_rad(self.follow()) > 0.0 && err.abs() > spin_rad(self.follow()) && self.spins_since_leg < SPINS_MAX {
                self.spins_since_leg += 1;
                self.going = None;
                let heading = (aim.1 - y).atan2(aim.0 - x);
                let ok = self.align(robot, heading);
                tracing::info!(at = ?(x, y, yaw), err_deg = format!("{:.0}", err.to_degrees()), ok, "map explore: aim off the nose, turning in place to it");
                let _ = stand(robot, self.turn_stand_s());
                return None;
            }
            let leg = passage_leg.or_else(|| self.leg(robot, grid, pose, aim, err, going, handle.status().legs));
            if let Some(sign) = leg.as_ref().and_then(|l| l.get("spin")).and_then(Value::as_f64)
                && self.spins_since_leg < SPINS_MAX
            {
                // No room ahead for a leg, the way on is elsewhere: what is
                // in the way goes on the books, then turn in place toward
                // it — an arc needs the room a wall under the beak denies.
                self.spins_since_leg += 1;
                self.going = None;
                let want = leg
                    .as_ref()
                    .and_then(|l| l.get("want"))
                    .and_then(Value::as_f64)
                    .unwrap_or(NO_ROOM_TURN_RAD);
                self.note_obstacle_ahead(robot, grid, pose);
                // By the route, not by a fixed quarter turn toward the
                // job's hand: the obstacle is ahead, the route says which
                // way round and how far (the user's question, 2026-09-15).
                // The quarter turn stays for an aim already on the nose —
                // the route itself runs into what was just booked, and
                // the next plan will say where.
                if spin_rad(self.follow()) > 0.0 && err.abs() > deadband_rad() {
                    let ok = self.align(robot, (aim.1 - y).atan2(aim.0 - x));
                    tracing::info!(at = ?(x, y, yaw), err_deg = format!("{:.0}", err.to_degrees()), ok, "map explore: no room ahead, turning in place to the aim");
                    let _ = stand(robot, self.turn_stand_s());
                } else if let Some(h) = self.route_heading_anew(grid, (x, y), self.goal.unwrap_or(f.stand))
                    && wrap(h - yaw).abs() > deadband_rad()
                {
                    // The aim on the nose runs into what was just booked:
                    // the route planned again with it says which way and
                    // how far — no fixed quarter or eighth turn (the
                    // user's, 2026-09-23: "the angle from the Dijkstra
                    // route, that is all").
                    self.kept_route = None;
                    let ok = self.align(robot, h);
                    tracing::info!(at = ?(x, y, yaw), err_deg = format!("{:.0}", wrap(h - yaw).to_degrees()), ok, "map explore: no room ahead, turning in place to the route planned anew");
                    let _ = stand(robot, self.turn_stand_s());
                } else {
                    tracing::info!(at = ?(x, y, yaw), sign, want, "map explore: no room ahead, turning in place");
                    self.spin(robot, sign, want);
                }
                return None;
            }
            let leg = leg.filter(|l| l.get("spin").is_none());
            let Some(leg) = leg else {
                // No room for any leg: whatever is there is an obstacle the
                // map does not know — record it and replan, as a refusal.
                handle.update(|s| s.refusals += 1);
                // What leaves no room: a drop's edge ahead is a drop's
                // refusal — booked and widened for the planner, so the
                // route stops asking for that lane (paper twin seed 4,
                // 2026-09-17: "no room" 43 times on the same spot, the
                // route through the passage the sensor kept refusing).
                // ... and only when the drop is what bounds the room: a
                // wall's end 0.13 m ahead with a rim 0.56 m beyond was
                // blamed on the rim six times on the spot, widened,
                // sealed and sent round the house (rimD3, 2026-09-20).
                let wall_m = {
                    let c = grid.clearance(x, y, yaw, 3.0);
                    if c.by == Blocked::Wall { c.free_m } else { f64::INFINITY }
                };
                let thing_m = robot
                    .cliff()
                    .and_then(|c| c.obstacle_in_lane(robot.now(), 0.0, gap_lane_half_m()))
                    .map_or(f64::INFINITY, |o| o.range_m);
                let drop_ahead = robot
                    .cliff()
                    .and_then(|c| c.drop_in_lane(robot.now(), 0.0, lane_half_m()))
                    .filter(|d| d.edge_min_m < 0.6 && d.edge_min_m - 0.12 <= wall_m.min(thing_m));
                let e = match drop_ahead {
                    Some(d) => format!(
                        "a drop — stairs or a hole — begins {:.2}–{:.2} m ahead, {:.0}° {}: no room for a leg",
                        d.edge_min_m, d.range_m, d.bearing.to_degrees().abs(), if d.bearing >= 0.0 { "left" } else { "right" }
                    ),
                    None if wall_m <= thing_m => format!("a wall on the map {wall_m:.2} m ahead: no room for a leg"),
                    None => format!("the depth sensor sees something {thing_m:.2} m ahead (no room for a leg)"),
                };
                let room = self.room_ahead(robot, grid, pose);
                tracing::info!(at = ?(x, y, yaw), target = ?f.target, room_m = room.0, gap = room.1, error = %e, "map explore: no room");
                if let Some(verdict) = self.refusal(robot, grid, pose, &e) {
                    return Some(verdict);
                }
                robot.sleep(AFTER_REFUSAL);
                return None;
            };
            tracing::debug!(at = ?(x, y, yaw), aim = ?aim, err, target = ?f.target, cells = f.cells, path = ?&f.path[..f.path.len().min(8)], leg = %leg, "map explore: leg");
            // A hurrying leg (no stand after it) walks on the frames the
            // guard has, and the guard judges frames only while the body
            // stands: a leg toward floor it has not looked at within its
            // memory is a leg it cannot refuse. The map cannot show a
            // hole and the books can be empty — full8 (2026-09-15) fell
            // into the stairwell on two such legs in a row, the pose 11°
            // off. Wait for a frame along the nose; none, then stand
            // properly and plan again.
            if leg.get("stop_s").and_then(Value::as_f64).unwrap_or(LEG_STOP_S) < 1.0
                && leg.get("vx").and_then(Value::as_f64).unwrap_or(0.0) > 0.0
                && !self.looked_ahead(robot)
            {
                tracing::info!(at = ?(x, y, yaw), "map explore: not looked ahead for this hurrying leg; standing first");
                let _ = stand(robot, self.turn_stand_s());
                return None;
            }
            let leg_began = robot.now();
            let plan_s = (leg_began - iteration_began).as_secs_f64();
            let stepped = self.guarded_step(robot, pose, &leg);
            tracing::info!(
                plan_s = format!("{plan_s:.1}"),
                step_s = format!("{:.1}", (robot.now() - leg_began).as_secs_f64()),
                walk_s = leg.get("walk_s").and_then(serde_json::Value::as_f64).unwrap_or(0.0),
                stop_s = leg.get("stop_s").and_then(serde_json::Value::as_f64).unwrap_or(0.0),
                ok = stepped.is_ok(),
                phase = leg.get("phase").and_then(serde_json::Value::as_str).unwrap_or("-"),
                "map explore: leg timing"
            );
            match stepped {
                Ok(_) => {
                    self.passage_bias *= 0.5;
                    self.measure_fit(robot, handle, grid, pose_after(robot, pose));
                    self.record_drops(robot);
                    self.trust_seen(robot);
                    let cells = self.trusted.cells();
                    handle.update(|s| s.trusted = cells);
                    // A leg that walked is progress: frontiers refused from
                    // an older spot may be reachable from the next one, so
                    // the one-shot clearing of the refused list is re-armed.
                    // Left armed once for good, a run that refused every
                    // frontier once ended "stuck" in the passage beside the
                    // stairwell with eight of them reachable (paper twin,
                    // 16 of 30 seeds).
                    // (`QUACKSAT_REFUSED_REARM=0` keeps the clearing one-shot
                    // per job, for measuring: on MuJoCo the room stays grew
                    // from 5–11 to 15–32 minutes with the re-arm on the books.)
                    match refused_rearm() {
                        0 => {}
                        1 => self.refused_cleared = false,
                        _ => {
                            if self.refused_cleared_at.is_none_or(|p| dist2(p, (x, y)) >= REARM_DIST_M) {
                                self.refused_cleared = false;
                            }
                        }
                    }
                    // A leg that did not move the duck is a bump, not a leg.
                    let after = robot.frame().map(|f| f.pose());
                    let stalled = after.is_some_and(|(ax, ay, ayaw)| {
                        dist2((ax, ay), (x, y)) < STALLED_M && wrap(ayaw - yaw).abs() < STALLED_RAD
                    });
                    if stalled {
                        // Once is the gait, not a bump: the first leg after
                        // a turn in place or a stand walks a few centimetres
                        // in its 1.5 s (tour1, 2026-09-16: four "stalls" in
                        // the doorways, each right after a turn to the aim,
                        // each booking a phantom at the nose that bent the
                        // route in the last centimetres and left a mark on
                        // the books). A bump is a stall that repeats.
                        self.stalls_in_row += 1;
                        if self.stalls_in_row < 2 {
                            tracing::info!(at = ?(x, y, yaw), "map explore: a leg that barely moved: the gait warming up, or a bump — once more to tell");
                            return None;
                        }
                        handle.update(|s| s.refusals += 1);
                        let e = "the leg did not move the duck: nose or flank against something the sensor cannot see";
                        tracing::info!(at = ?(x, y, yaw), leg = %leg, "map explore: stalled");
                        if let Some(verdict) = self.refusal(robot, grid, pose, e) {
                            return Some(verdict);
                        }
                        return None;
                    }
                    self.stuck = 0;
                    self.since_leg = 0;
                    self.spins_since_leg = 0;
                    self.stalls_in_row = 0;
                    self.drop_refusals_in_row = 0;
                    self.passage_refusals = 0;
                    self.turns_refused_at_drop = 0;
                    // A leg walked is progress on the way chosen: the
                    // short way may be insisted on afresh at the next
                    // jump; only refusals in a row give it up.
                    self.insisted = 0;
                    if let Some((ax, ay, _)) = after {
                        self.walked((x, y), (ax, ay));
                    }
                    self.last_leg_vyaw = leg.get("vyaw").and_then(Value::as_f64).unwrap_or(0.0);
                    handle.update(|s| s.legs += 1);
                }
                // 3. The sensor answers for what the map does not know.
                Err(e) => {
                    handle.update(|s| s.refusals += 1);
                    self.refused_since_plan = true;
                    tracing::info!(at = ?(x, y, yaw), target = ?f.target, error = %e, "map explore: step refused");
                    if let Some(verdict) = self.refusal(robot, grid, pose, &e) {
                        return Some(verdict);
                    }
                    robot.sleep(AFTER_REFUSAL);
                }
            }
        None
    }
    /// Arrived: face it if it is off to the side (the sweep covers what is
    /// roughly ahead), a full stand maps it, then it is never chosen again.
    fn arrive(
        &mut self,
        handle: &ExploreHandle,
        robot: &mut dyn Body,
        f: &Frontier,
        pose: (f64, f64, f64),
    ) {
        let (x, y, yaw) = pose;
        let bearing = wrap((f.target.1 - y).atan2(f.target.0 - x) - yaw);
        if bearing.abs() > curve_rad() {
            let _ = robot.step(&json!({"vx": 0.3, "vyaw": 0.7_f64.copysign(bearing), "walk_s": 1.5, "stop_s": 1.0}));
        }
        let _ = stand(robot, FRONTIER_STOP_S);
        self.visited.push((f.target, BLOCK_VISITED_M));
        self.target = None;
        let far_from_names = self.ask && self.known.iter().all(|&k| dist2((x, y), k) > ASK_MIN_M);
        handle.update(|s| {
            s.visited += 1;
            if far_from_names && s.pending_question.is_none() {
                s.pending_question = Some(Question {
                    pose,
                    since: robot.now(),
                });
            }
        });
        if far_from_names {
            // Whether or not somebody answers, do not ask again here.
            self.known.push((x, y));
        }
    }
    /// The farthest point on `path` the body can reach in a straight line
    /// from `from`, within the straight-line horizon.
    ///
    /// Searched from the far end back, so the first clear one is the
    /// farthest, and both walls and the drops on the books have to be
    /// clear of the lane the body would sweep. `None` when not even the
    /// first step is clear — then the caller falls back to the old fixed
    /// waypoint, which is what the guards will refuse or shorten anyway.
    /// One step of the search for the pose after a fall: a stand, so the
    /// head sweeps and the mapper has its still window; then a turn in
    /// place of an eighth, so the next stand sees elsewhere; after a
    /// whole circle, a short guarded leg ahead to a new viewpoint. Up to
    /// [`LOST_PATIENCE`], then the job gives up — no leg of the job itself
    /// is walked on a pose the fall made a guess of (the user's,
    /// 2026-09-24: "after getting up, relocalizing is the first thing").
    fn relocate_step(&mut self, robot: &mut dyn Body) {
        self.relocate_steps += 1;
        if self.relocate_steps == 1 {
            tracing::info!("map explore: up again after a fall; looking around for the pose before anything else");
        }
        let _ = stand(robot, RELOCATE_STAND_S);
        if robot.pose_trusted() {
            return;
        }
        if self.relocate_steps % (PANO_STEPS + 1) == 0 {
            let leg = json!({"vx": 0.3, "vyaw": 0.0, "walk_s": 1.0, "stop_s": 0.0});
            let walked = match robot.frame() {
                Some(f) => self.guarded_step(robot, f.pose(), &leg).is_ok(),
                None => false,
            };
            tracing::info!(walked, steps = self.relocate_steps, "map explore: after a fall, a circle seen and no pose; a short leg to see from elsewhere");
        } else {
            let turned = self.turn_in_place(robot, 1.0, PANO_STEP_RAD);
            tracing::info!(turned_deg = ?turned.map(|t| format!("{:.0}", t.to_degrees())), steps = self.relocate_steps, "map explore: after a fall, turning to look elsewhere");
        }
    }

    /// How far the floor goes from `p` along `heading`, up to `max_m`, and
    /// what ends it: a wall on the map, a booked drop's radius, unknown
    /// map, or nothing within reach.
    fn side_free(&self, grid: &Grid, p: (f64, f64), heading: f64, max_m: f64) -> (f64, Side) {
        let (c, s) = (heading.cos(), heading.sin());
        let mut d = 0.0;
        while d < max_m {
            let q = (p.0 + d * c, p.1 + d * s);
            if self.local.iter().any(|(b, r)| *r >= DROP_RADIUS_M && dist2(*b, q) < *r) {
                return (d, Side::Drop);
            }
            match grid.at(q.0, q.1) {
                Some(Cell::Free) => {}
                Some(Cell::Wall) => return (d, Side::Wall),
                _ => return (d, Side::Unknown),
            }
            d += 0.025;
        }
        (max_m, Side::Open)
    }

    /// `aim` slid across the way from `from` to it, into the middle of a
    /// passage beside a drop (see the use in `walk_leg`): only where a
    /// booked drop ends one side and a wall on the map the other, less
    /// than [`CENTRE_WIDTH_M`] apart — half the difference, at most
    /// [`CENTRE_MAX_M`]. An unknown side is no side: sliding toward it
    /// cost the paper twin two journeys in thirty (2026-09-24).
    fn centred(&self, grid: &Grid, from: (f64, f64), aim: (f64, f64)) -> (f64, f64) {
        let h = (aim.1 - from.1).atan2(aim.0 - from.0);
        let (l, lk) = self.side_free(grid, aim, h + std::f64::consts::FRAC_PI_2, CENTRE_WIDTH_M);
        let (r, rk) = self.side_free(grid, aim, h - std::f64::consts::FRAC_PI_2, CENTRE_WIDTH_M);
        let beside_drop = matches!((lk, rk), (Side::Drop, Side::Wall) | (Side::Wall, Side::Drop));
        if !beside_drop || l + r >= CENTRE_WIDTH_M {
            return aim;
        }
        let shift = ((l - r) / 2.0).clamp(-CENTRE_MAX_M, CENTRE_MAX_M);
        if shift.abs() < 0.03 {
            return aim;
        }
        let a = (aim.0 - shift * h.sin(), aim.1 + shift * h.cos());
        tracing::info!(left_m = format!("{l:.2}"), right_m = format!("{r:.2}"), shift_m = format!("{shift:.2}"), "map explore: beside a drop, the aim to the middle of the way");
        a
    }

    /// The heading of a route to `to` planned now, with the books as they
    /// stand: toward its point a step along. `None` when there is none.
    fn route_heading_anew(&self, grid: &Grid, from: (f64, f64), to: (f64, f64)) -> Option<f64> {
        let path = path_to(grid, from.0, from.1, to, &self.planner_walls(), inflate_m(), &self.lanes())?;
        let p = waypoint(&path, goal_lookahead_m(true), grid.cell_m)?;
        (dist2(p, from) >= grid.cell_m).then(|| (p.1 - from.1).atan2(p.0 - from.0))
    }
    fn farthest_clear(
        &self,
        grid: &Grid,
        from: (f64, f64),
        path: &[(f64, f64)],
    ) -> Option<(f64, f64)> {
        let horizon = straight_look_m().min(string_pull_m());
        for p in path.iter().rev() {
            let d = dist2(from, *p);
            if d > horizon || d < grid.cell_m {
                continue;
            }
            let heading = (p.1 - from.1).atan2(p.0 - from.0);
            if grid.lane_clear(from.0, from.1, heading, d, lane_half_m())
                && self.clear_of_local(from.0, from.1, heading, d, lane_half_m())
            {
                return Some(*p);
            }
        }
        None
    }
    /// One leg toward `aim`: straight with a gentle correction, a curve,
    /// or a tight arc by heading error; sized to the floor the map and the
    /// sensor say is ahead. `None` when there is no room for any leg.
    fn leg(
        &self,
        robot: &dyn Body,
        grid: &Grid,
        pose: (f64, f64, f64),
        aim: (f64, f64),
        err: f64,
        going: bool,
        legs: u32,
    ) -> Option<Value> {
        let stop_s = self.stop_s(legs);
        let (x, y, yaw) = pose;
        let straight = if commit() { going } else { err.abs() <= straight_rad() };
        // The guarded journey's dense stops beside the drops (see
        // `DROP_STAND_S`): mapping and the blind journey keep their legs.
        let leg_cap = if self.policy.mode == Mode::JourneyGuarded && self.drop_within(DROP_STAND_NEAR_M) { drop_leg_s().min(3.0) } else { 3.0 };
        let (vyaw, wanted_s, arc) = if straight {
            let walk_s = (dist2((x, y), aim) / GAIT_M_PER_S).clamp(1.0, if self.follow() { 1.5 } else { leg_cap });
            let correction = if err.abs() < deadband_rad() {
                0.0
            } else if commit() {
                // Sized to the leg: 0.6·err over a 3 s leg turned 0.45 rad
                // for a 0.25 rad error, a sign flip by construction.
                (err / walk_s).clamp(-0.2, 0.2)
            } else {
                (0.6 * err).clamp(-0.2, 0.2)
            };
            (correction, walk_s, false)
        } else if err.abs() <= curve_rad() {
            // Turn by the error you have, not by a fixed amount. A flat
            // 0.5 rad/s for 1.5 s is 43°, which overshoots a 20° error and
            // undershoots a 55° one; either way the next leg corrects back
            // and the duck weaves across its own aim line. Measured in the
            // empty flat (2026-09-13): a leg begins 33° off its aim at the
            // median, 40 % of them more than 45° off, and the error changes
            // sign on a third of legs — with nothing in the house to blame,
            // the duck walks 2.13 m for every metre it makes good.
            // `QK_PROP_TURN=0` restores the flat rate.
            let walk_s = 1.5;
            let vyaw = if prop_turn() {
                (err / walk_s).clamp(-0.7, 0.7)
            } else {
                0.5_f64.copysign(err)
            };
            (vyaw, walk_s, false)
        } else {
            (
                0.7_f64.copysign(err),
                (err.abs() / 0.5).clamp(1.5, 3.0),
                true,
            )
        };
        // Width first: the corridor ahead must take the body. Mapped walls
        // on both sides, measured at the body and a little way along the
        // heading; one wall, or unknown floor, is not a corridor. A passage
        // narrower than gap_max_m() is a doorway, and a doorway is judged
        // with the body's own lane and margins — with the corridor's, the
        // posts of a 0.42 m door were always "in the way".
        let mut width_m = f64::INFINITY;
        for along in [0.0, WIDTH_AHEAD_M] {
            let (px, py) = (x + along * yaw.cos(), y + along * yaw.sin());
            let l = grid.clearance(px, py, yaw + std::f64::consts::FRAC_PI_2, 1.0);
            let r = grid.clearance(px, py, yaw - std::f64::consts::FRAC_PI_2, 1.0);
            if l.by == Blocked::Wall && r.by == Blocked::Wall {
                width_m = width_m.min(l.free_m + r.free_m);
            }
        }
        // The sensor's own word on a doorway: something on both sides
        // within gap_max_m() of each other, ahead and near — the map may not
        // have inked a low cabinet the sensor sees as a doorpost.
        let mut sensor_gap: Option<(f64, f64)> = None;
        if let Some(cliff) = robot.cliff() {
            let (mut left, mut right) = (f64::INFINITY, f64::INFINITY);
            for o in cliff.recent.iter().filter(|f| !f.moving).flat_map(|f| f.obstacles.iter()) {
                if o.range_m > GAP_LOOK_M || o.bearing.abs() > 1.05 {
                    continue;
                }
                let lateral = o.range_m * o.bearing.sin();
                if lateral > 0.0 { left = left.min(lateral) } else { right = right.min(-lateral) }
            }
            if left + right < gap_max_m() {
                sensor_gap = Some((left, right));
            }
        }
        let gap = width_m < gap_max_m() || sensor_gap.is_some();
        let (lane, leg_reserve, arc_reserve) = if gap {
            (gap_lane_half_m(), gap_leg_reserve_m(), gap_leg_reserve_m())
        } else {
            (lane_half_m(), leg_reserve_m(), arc_reserve_m())
        };
        // Room ahead: the nearer of a mapped wall and what the sensor sees.
        let mut room_m = f64::INFINITY;
        let ahead = grid.clearance(x, y, yaw, 3.0);
        if ahead.by == Blocked::Wall {
            room_m = room_m.min(ahead.free_m);
        }
        if let Some(cliff) = robot.cliff()
            && let Some(o) = cliff.obstacle_in_lane(robot.now(), 0.0, lane)
            && o.range_m < room_m
        {
            room_m = o.range_m;
        }
        // A tight arc advances almost as much as a straight leg: 0.110
        // m/s against 0.121 at vyaw 0.7, measured on the human drive
        // (2026-09-07) — not the quarter the model had, which granted arcs
        // with 0.33 m of room that carried the body into the wall (the
        // user's observation). With the room an arc really needs, the
        // turn in place (kick, then spin) is what is left near a wall.
        let arc_advance = if arc_full() { GAIT_M_PER_S * ARC_ADVANCE_FRAC } else { GAIT_M_PER_S / 4.0 };
        let arc_room_s = ((room_m - arc_reserve) / arc_advance).max(0.0);
        let straight_room_s = ((room_m - leg_reserve) / GAIT_M_PER_S).max(0.0);
        // Tight quarters — a doorway, a corridor under TIGHT_WIDTH_M, less
        // than TIGHT_ROOM_M ahead: a heading change beyond SPIN_ERR_RAD is
        // a turn in place (the kick, then the spin closed on the yaw), not
        // an arc; an arc there advances 0.11 m/s while it turns and ends
        // where the wall is (the user's rule, 2026-09-07).
        let tight = if tight_gap_only() { gap } else { gap || width_m < TIGHT_WIDTH_M || room_m < TIGHT_ROOM_M };
        if spin_tight() && tight && err.abs() > spin_err_rad() {
            return Some(json!({"spin": err.signum(), "want": err.abs()}));
        }
        if arc {
            if arc_room_s < 1.0 {
                return Some(json!({"spin": err.signum(), "want": err.abs()}));
            }
            let walk_s = wanted_s.min(arc_room_s);
            return Some(json!({"vx": 0.3, "vyaw": vyaw, "walk_s": walk_s, "stop_s": stop_s, "centre": true, "phase": if going { "go" } else { "turn" }}));
        }
        // In a doorway: steer onto its axis, take short steps, and let
        // the mapping step know (`gap`) so its margins shrink too.
        let mut vyaw = vyaw;
        if gap {
            let (l, r) = match sensor_gap {
                Some(lr) => lr,
                None => {
                    let (px, py) = (x + WIDTH_AHEAD_M * yaw.cos(), y + WIDTH_AHEAD_M * yaw.sin());
                    (
                        grid.clearance(px, py, yaw + std::f64::consts::FRAC_PI_2, 1.0).free_m,
                        grid.clearance(px, py, yaw - std::f64::consts::FRAC_PI_2, 1.0).free_m,
                    )
                }
            };
            // More room on the left: the axis is to the left, steer left.
            vyaw = (vyaw + (GAP_GAIN * (l - r) / 2.0).clamp(-0.3, 0.3)).clamp(-0.5, 0.5);
        }
        // The way to turn when there is no room to go on: the configured
        // hand, or (a switch, for the MuJoCo bedroom-exit test) toward
        // the aim — a duck in the twin's bedroom spun left and right at
        // the doorway for ten minutes (run 71), but on the paper twin the
        // aim-side turn measured worse (see `turn_to_aim`).
        let no_room_turn = |this: &Self| {
            if turn_to_aim() && err.abs() > deadband_rad() {
                0.7_f64.copysign(err)
            } else {
                this.turn_toward(grid, pose)
            }
        };
        if width_m < CORRIDOR_MIN_M {
            // Too narrow to go on: turn — an arc with room, in place without.
            let away = no_room_turn(self);
            if arc_room_s < 1.0 {
                return Some(json!({"spin": away.signum(), "want": NO_ROOM_TURN_RAD}));
            }
            return Some(json!({"vx": 0.3, "vyaw": away, "walk_s": 1.5, "stop_s": stop_s}));
        }
        if straight_room_s >= 1.0 {
            let walk_s = wanted_s.min(straight_room_s).min(if gap { gap_leg_s() } else { 3.0 });
            return Some(json!({"vx": 0.3, "vyaw": vyaw, "walk_s": walk_s, "stop_s": stop_s, "centre": !gap, "gap": gap, "phase": if going { "go" } else { "turn" }}));
        }
        // On the trail: the body has already been where this leg goes,
        // at the body's width, so the leg is judged with the doorway's
        // reserve, not the corridor's — a duck that walked into a room
        // walks out of it the same way. The cliff guard and the sensor's
        // obstacle in the narrow lane still have their say (a chair moved
        // since is a chair). Without it, a bedroom on MuJoCo cost a
        // quarter of an hour of left-right spins at its door (run 71):
        // straight legs want 0.47 m of room, furniture leaves less.
        if trail_leg_enabled() && self.on_trail(pose, TRAIL_LEG_M) {
            let trail_room_s = ((room_m - gap_leg_reserve_m()) / GAIT_M_PER_S).max(0.0);
            if trail_room_s >= TRAIL_LEG_MIN_S {
                let walk_s = wanted_s.min(trail_room_s).min(gap_leg_s());
                return Some(json!({"vx": 0.3, "vyaw": vyaw, "walk_s": walk_s, "stop_s": stop_s, "centre": false, "gap": true, "steer": false, "phase": if going { "go" } else { "turn" }}));
            }
        }
        // No room to go on, room to turn: the bumper move — a tight arc
        // (toward the aim, or the configured hand), then replan from the
        // new heading.
        let away = no_room_turn(self);
        if arc_room_s >= 1.0 {
            return Some(json!({"vx": 0.3, "vyaw": away, "walk_s": 1.5, "stop_s": stop_s}));
        }
        // No room even for a tight arc: turn in place.
        Some(json!({"spin": away.signum(), "want": NO_ROOM_TURN_RAD}))
    }
    /// Whether the straight lane from `(x, y)` along `heading` for `len_m`
    /// passes clear of every obstacle and drop the sensor put on the
    /// books. The map's walls are [`Grid::lane_clear`]'s business; a hole
    /// is not on the map, and "straight when clear" aimed across the
    /// stairwell for exactly that.
    fn clear_of_local(&self, x: f64, y: f64, heading: f64, len_m: f64, half_w: f64) -> bool {
        let (dx, dy) = (heading.cos(), heading.sin());
        self.local.iter().all(|((px, py), r)| {
            let along = ((px - x) * dx + (py - y) * dy).clamp(0.0, len_m);
            dist2((x + along * dx, y + along * dy), (*px, *py)) > half_w + r
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontier::INFLATE_M;
    use crate::map::Cell;

    /// A low thing booked ahead, however far it is pushed, stays an
    /// obstacle: the books tell the two apart by the radius alone.
    #[test]
    fn a_low_thing_booked_ahead_is_never_a_drop() {
        let widest = (OBSTACLE_RADIUS_M + guarded::LOW_BOOK_PUSH_M / 2.0).min(guarded::LOW_BOOK_RADIUS_MAX_M);
        assert!(widest < DROP_RADIUS_M, "{widest}");
    }

    /// A flat with one wall across it and two doorways: a near one at
    /// x ≈ 3.0 and a far one at x ≈ 0.6. Everything else is known floor.
    fn two_doors() -> Grid {
        let (rows, cols, cell_m) = (40, 60, 0.1);
        let mut cells = vec![Cell::Free; rows * cols];
        let wall_row = 20; // y ≈ 2.0
        for col in 0..cols {
            let near = (28..=32).contains(&col);
            let far = (4..=8).contains(&col);
            if !near && !far {
                cells[wall_row * cols + col] = Cell::Wall;
            }
        }
        Grid { rows, cols, x_min: 0.0, y_min: 0.0, cell_m, cells }
    }

    /// The measured case (2026-09-12): a stall writes an obstacle in the
    /// near doorway, the planner walks the duck round the whole flat, and
    /// nothing forgets the guess because a route still exists.
    #[test]
    fn a_guess_in_the_doorway_is_ours_to_doubt() {
        let grid = two_doors();
        let (at, goal) = ((3.0, 1.5), (3.0, 2.6));
        let phantom = vec![((3.0, 1.75), OBSTACLE_RADIUS_M)];
        let walls: Vec<ExtraWall> = phantom.iter().map(|(p, r)| (*p, *r)).collect();
        let route = path_to(&grid, at.0, at.1, goal, &walls, INFLATE_M, &[])
            .expect("the far door is still open");
        let route_m = route.len() as f64 * grid.cell_m;
        let straight_m = dist2(at, goal);
        assert!(
            route_m > DETOUR_SUSPECT * straight_m,
            "the guess should send the duck the long way: {route_m:.2} m for {straight_m:.2} m"
        );
        let bare = Job::ours_to_doubt(&grid, &phantom, at, goal, route_m, INFLATE_M, &[])
            .expect("the detour is the duck's own doing");
        assert!(bare.len() as f64 * grid.cell_m < route_m / DETOUR_SUSPECT);
    }

    /// The other half of the rule: when the near door is walled on the map
    /// itself, the long way round is the map's doing and forgetting our own
    /// books would not help. Nothing to doubt.
    #[test]
    fn a_wall_on_the_map_is_not_ours_to_doubt() {
        let mut grid = two_doors();
        for col in 28..=32 {
            grid.cells[20 * grid.cols + col] = Cell::Wall;
        }
        let (at, goal) = ((3.0, 1.5), (3.0, 2.6));
        let elsewhere = vec![((0.2, 3.5), OBSTACLE_RADIUS_M)];
        let walls: Vec<ExtraWall> = elsewhere.iter().map(|(p, r)| (*p, *r)).collect();
        let route = path_to(&grid, at.0, at.1, goal, &walls, INFLATE_M, &[])
            .expect("the far door is open");
        let route_m = route.len() as f64 * grid.cell_m;
        assert!(
            Job::ours_to_doubt(&grid, &elsewhere, at, goal, route_m, INFLATE_M, &[]).is_none(),
            "forgetting an obstacle nowhere near the way should change nothing"
        );
    }

    /// One frame's "Missing" is not a hole; two frames' is. Edges of
    /// obstacles go on the books from one frame as before.
    #[test]
    fn a_hole_needs_two_frames_to_be_believed() {
        let once = vec![(0usize, ((1.0, 1.0), DROP_RADIUS_M)), (0, ((1.1, 1.0), DROP_RADIUS_M))];
        assert!(Job::vote_drops(&once).is_empty(), "a single frame's hole stays off the books");
        let twice = vec![(0usize, ((1.0, 1.0), DROP_RADIUS_M)), (1, ((1.03, 1.02), DROP_RADIUS_M))];
        assert_eq!(Job::vote_drops(&twice).len(), 2);
        let edge = vec![(0usize, ((2.0, 2.0), OBSTACLE_RADIUS_M))];
        assert_eq!(Job::vote_drops(&edge).len(), 1, "an obstacle edge is cheap to keep");
    }

    /// Standing on a booked drop strikes it, and the reach booked behind
    /// it; an obstacle edge and a far drop stay.
    /// A body that only has a cliff guard's view: what the middle level needs.
struct Seeing(crate::cliff::CliffStatus, Instant);
impl Body for Seeing {
    fn step(&mut self, _: &Value) -> Result<Value, String> {
        Ok(json!({}))
    }
    fn blind_move(&mut self, _: &Value) -> Result<Value, String> {
        Ok(json!({}))
    }
    fn frame(&self) -> Option<crate::map::MapFrame> {
        None
    }
    fn pose_trusted(&self) -> bool {
        true
    }
    fn cliff(&self) -> Option<crate::cliff::CliffStatus> {
        Some(self.0.clone())
    }
    fn now(&self) -> Instant {
        self.1
    }
    fn sleep(&mut self, _: Duration) {}
}

#[test]
fn the_route_is_judged_against_the_sensor_before_a_leg() {
    use crate::cliff::{CliffFrame, Drop, DropKind, Obstacle};
    let now = Instant::now();
    // The sensor, head straight ahead: a wall 0.4 m ahead, 0.3 rad left,
    // and a drop whose edge begins 0.5 m ahead to the right (bearing −0.3).
    let frame = CliffFrame {
        moving: false,
        seq: 1,
        at: now,
        head_yaw: 0.0,
        drops: vec![Drop { bearing: -0.3, range_m: 0.6, edge_min_m: 0.5, kind: DropKind::Missing }],
        obstacles: vec![Obstacle { bearing: 0.3, range_m: 0.4 }],
        floor_beams: 40,
        judged: 60,
    };
    let status = crate::cliff::CliffStatus { recent: vec![frame], ..Default::default() };
    let body = Seeing(status, now);
    let job = Job::new(vec![], 60.0, false, 0.7, now);
    let pose = (1.0, 1.0, 0.0);
    // Straight ahead: into the wall at 0.4 m.
    let ahead: Vec<(f64, f64)> = (1..=12).map(|i| (1.0 + i as f64 * 0.05, 1.0)).collect();
    let hit = job.route_vs_sensor(&body, pose, &ahead).expect("the wall ahead");
    assert!(hit.1.starts_with("an obstacle"), "{}", hit.1);
    // Bearing −0.3 (to the right), out to 0.6 m: past the drop's edge.
    let right: Vec<(f64, f64)> = (1..=12).map(|i| (1.0 + i as f64 * 0.05 * 0.955, 1.0 - i as f64 * 0.05 * 0.296)).collect();
    let hit = job.route_vs_sensor(&body, pose, &right).expect("the drop's edge");
    assert!(hit.1.starts_with("a drop"), "{}", hit.1);
    // Bearing +0.6 (well to the left): outside the wedge the head looked at — no word.
    let left: Vec<(f64, f64)> = (1..=12).map(|i| (1.0 + i as f64 * 0.05 * 0.825, 1.0 + i as f64 * 0.05 * 0.565)).collect();
    assert!(job.route_vs_sensor(&body, pose, &left).is_none());
    // Going backwards: nothing ahead of the body on the route — no word.
    let back: Vec<(f64, f64)> = (1..=6).map(|i| (1.0 - i as f64 * 0.05, 1.0)).collect();
    assert!(job.route_vs_sensor(&body, pose, &back).is_none());
}

#[test]
fn the_short_way_is_kept_twice_then_the_long_one_believed() {
    // Last route 2.4 m; the plan now says 9.6 m, squeezed 2.6 m: keep it.
    assert!(keep_the_short_way(Some(2.4), 9.6, Some(2.6), 0));
    assert!(keep_the_short_way(Some(2.4), 9.6, Some(2.6), 1));
    // Insisted twice already: the long way it is.
    assert!(!keep_the_short_way(Some(2.4), 9.6, Some(2.6), ROUTE_INSIST_MAX));
    // A modest change is not a jump; a squeezed route that is itself long
    // is no short way; no squeezed route, nothing to keep.
    assert!(!keep_the_short_way(Some(2.4), 3.5, Some(2.6), 0));
    assert!(!keep_the_short_way(Some(2.4), 9.6, Some(6.0), 0));
    assert!(!keep_the_short_way(Some(2.4), 9.6, None, 0));
    // The first plan of a journey: the squeezed route is the yardstick.
    assert!(keep_the_short_way(None, 9.6, Some(2.6), 0));
    assert!(!keep_the_short_way(None, 3.5, Some(2.6), 0));
}

#[test]
fn the_ground_book_keeps_the_drops_the_body_never_walked_over() {
    let dir = std::env::temp_dir().join(format!("quacksat-ground-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let places = dir.join("places.json");
    let h = ExploreHandle::new().with_ground(places.to_str().unwrap());
    h.name_live_map("twin");
    h.update(|s| {
        s.local = vec![((1.0, 1.0), DROP_RADIUS_M), ((3.0, 3.0), DROP_RADIUS_M), ((5.0, 5.0), OBSTACLE_RADIUS_M)];
        // The body's centre 4 cm from the first drop: it stood on it.
        s.trail = vec![(0.0, 0.0), (1.03, 1.02), (3.2, 3.1)];
    });
    h.keep_ground();
    // A fresh handle, the same file: the map's drops come back, minus the
    // one the body walked over and the obstacle edge; the trail point
    // beside the kept drop comes back as a lane — a lane, not a trail:
    // nothing this body has walked yet (rim2, 2026-09-17).
    let h2 = ExploreHandle::new().with_ground(places.to_str().unwrap());
    h2.map_named("twin");
    assert_eq!(h2.status().local, vec![((3.0, 3.0), DROP_RADIUS_M)]);
    assert_eq!(h2.status().lanes, vec![(3.2, 3.1)]);
    assert!(h2.status().trail.is_empty());
    // A drop the guard books beside that lane survives the next keep:
    // the lane is not a place the body stood.
    h2.update(|s| s.local.push(((3.25, 3.0), DROP_RADIUS_M)));
    h2.keep_ground();
    let h3 = ExploreHandle::new().with_ground(places.to_str().unwrap());
    h3.map_named("twin");
    assert_eq!(h3.status().local.len(), 2, "the re-booked rim point stays");
    h2.map_named("other");
    assert!(h2.status().local.is_empty());
    assert!(h2.status().lanes.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_new_job_inherits_the_drops_and_the_trail() {
    let mut s = ExploreStatus { state: State::Done, ..ExploreStatus::default() };
    s.local = vec![((-0.16, -0.60), DROP_RADIUS_M), ((1.0, 1.0), OBSTACLE_RADIUS_M)];
    s.trail = vec![(0.0, 0.0), (0.5, 0.0)];
    let (drops, trail) = ExploreStatus::begin_job(&mut s);
    assert_eq!(drops, vec![((-0.16, -0.60), DROP_RADIUS_M)], "holes carry over, obstacle edges do not");
    assert_eq!(trail, vec![(0.0, 0.0), (0.5, 0.0)]);
    assert_eq!(s.state, State::Running);
    assert_eq!(s.local, drops, "the fresh status shows them too");
    assert_eq!(s.trail, trail);
}

#[test]
    fn standing_on_a_booked_drop_strikes_it() {
        let mut job = Job::new(vec![], 60.0, false, 0.7, Instant::now());
        job.local = vec![
            ((0.0, 0.0), DROP_RADIUS_M),
            ((0.1, 0.0), DROP_RADIUS_M),   // the reach behind the rim
            ((0.2, 0.0), DROP_RADIUS_M),
            ((0.05, 0.05), OBSTACLE_RADIUS_M),
            ((3.0, 3.0), DROP_RADIUS_M),
        ];
        job.strike_drops_under((0.02, 0.01));
        // What is under the body goes — the rim point 2 cm off; the reach
        // point 8 cm off is not under the body (STRIKE_M is 6 cm, the
        // user's rule, 2026-09-21: only where it walked with its body).
        assert_eq!(job.local.len(), 4, "{:?}", job.local);
        assert!(!job.local.iter().any(|(p, _)| *p == (0.0, 0.0)));
        assert!(job.local.iter().any(|(p, _)| *p == (0.1, 0.0)));
        assert!(job.local.iter().any(|(p, _)| *p == (0.2, 0.0)));
        assert!(job.local.iter().any(|(p, _)| *p == (3.0, 3.0)));
        assert!(job.local.iter().any(|(_, r)| *r == OBSTACLE_RADIUS_M));
    }

    /// A drop is never doubted: the map cannot show a stairwell, so a
    /// route that exists only by forgetting one is not a route.
    #[test]
    fn a_drop_is_never_ours_to_doubt() {
        let grid = two_doors();
        let (at, goal) = ((3.0, 1.5), (3.0, 2.6));
        let stairs = vec![((3.0, 1.75), DROP_RADIUS_M)];
        assert!(Job::ours_to_doubt(&grid, &stairs, at, goal, 99.0, INFLATE_M, &[]).is_none());
    }
}
