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
//! 2. **The duck follows the path** on the journey's loop (`navigate.rs`):
//!    the stick's legs (`stick.rs`), a point of the route a little ahead,
//!    a turn in place to it when it is well off the nose. The frontier it
//!    is heading for is kept until it is reached or gone. (The explorer's
//!    own guarded legs, `QK_EXPLORE_NAV=0`, were removed on 2026-09-30.)
//! 3. **The sensor only answers for what the map does not know.** What
//!    the depth sensor sees that the map has not inked — a thing in the
//!    way, a drop — goes into a **local obstacle list** the planner
//!    treats as impassable, and the route is replanned around it.
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
    frontiers_with, largest_frontier,
};
use crate::cliff::CliffStatus;
use crate::map::{Blocked, Cell, Grid, MapFrame, MapSupport};
use crate::places::Registry;
use crate::{MapConfig, Places};
use serde_json::{Value, json};

use quack_duck::Control;
use crate::tools::{Robot, execute};

/// The stand at each step of the look-around after a fall: the mapper's
/// floor for a still window is six seconds (see `homecoming/search.rs`, `STAND_S`).
const RELOCATE_STAND_S: f64 = 6.0;
/// A session that ends with only unreachable frontiers left and less than
/// this much unknown floor within reach of them finds the house done.
const DONE_LEFT_M2: f64 = 2.0;
/// The session's end finds the house mapped when no frontier is within
/// reach and no piece of unknown floor left is this big (see where the
/// end is decided: the finished maps of three houses 1.0–3.9 m², first
/// sessions still exploring 5.5–7.3).
const DONE_PIECE_M2: f64 = 4.5;
/// A session that ends with this share of the house mapped finds it done.
const DONE_SHARE: f64 = 0.95;
/// How often a session looks at the battery.
const BATTERY_EVERY_S: f64 = 30.0;
/// Unseals on one spot (within this) before it is a no-go for the job,
/// and how wide a no-go is for the planner.
const NO_GO_AFTER: u32 = 3;
const NO_GO_SAME_M: f64 = 0.30;
const NO_GO_RADIUS_M: f64 = 0.20;

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
    /// The battery level robotd reports (`robot.health`), when it knows.
    fn battery_percent(&mut self) -> Option<f64> {
        None
    }
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
        let frame = self.places.map.as_ref().and_then(|m| m.snapshot().latest.clone())?;
        // The oracle (see `oracle.rs`): the true map and pose, when set.
        Some(match crate::oracle::oracle() {
            Some(o) => o.apply(frame),
            None => frame,
        })
    }
    fn pose_trusted(&self) -> bool {
        self.places.map.as_ref().is_some_and(|m| m.snapshot().trusted_pose().is_some())
    }
    fn frozen_map(&self) -> bool {
        self.places.map.as_ref().is_some_and(|m| {
            let snap = m.snapshot();
            matches!(&snap.support, MapSupport::Supported { mode: Some(mode), .. } if mode == "localize")
                || snap.latest.as_ref().is_some_and(|f| f.frozen)
        })
    }
    fn cliff(&self) -> Option<CliffStatus> {
        self.places.cliff.as_ref().map(|c| c.snapshot())
    }
    fn battery_percent(&mut self) -> Option<f64> {
        let control = self.control.as_mut()?;
        let response = control.request(&duck_ipc_proto::Call::RobotHealth).ok()?;
        response.result.as_ref()?.get("battery")?.get("percent")?.as_f64()
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
/// long at a frontier (a full head sweep). Also the stick's stand while
/// exploring: without it (`QK_MAP_STAND_S=0`, removed 2026-09-30) the
/// position was lost after two minutes.
const LEG_STOP_S: f64 = 3.0;
/// What the gait covers per second at vx 0.3 (measured on the twin).
const GAIT_M_PER_S: f64 = 0.12;
/// A passage narrower than this (and wider than the body) is a doorway.
const GAP_MAX_M: f64 = 0.6;
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
/// automatic search on the paper twin explores them. Read at every call
/// (the daemon's environment does not change while it runs).
fn knob(name: &str, default: f64) -> f64 {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}
fn lane_half_m() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| knob("QK_LANE_HALF_M", LANE_HALF_M))
}
fn gap_lane_half_m() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| knob("QK_GAP_LANE_HALF_M", GAP_LANE_HALF_M))
}
fn gap_max_m() -> f64 {
    static V: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *V.get_or_init(|| knob("QK_GAP_MAX_M", GAP_MAX_M))
}













const SLIVER_PENALTY: f64 = 1.5;
/// Obstacles the sensor reports within this much of the line the duck
/// would walk are in the way: half the body plus a little (the mapping
/// step uses the same lane). A ±23° cone blocked doorways from half a
/// metre away, their posts being inside it.
const LANE_HALF_M: f64 = 0.16;



/// A heading counts as open floor only with at least this much known free
/// floor along it.
const SPACE_MIN_M: f64 = 0.6;
const STUCK_MAX: u32 = 3;
/// A "sealed in" attempt counts toward [`STUCK_MAX`] only this long after
/// the previous one, or once the body has moved this far since.
const STUCK_GAP_S: f64 = 30.0;
const STUCK_MOVE_M: f64 = 0.20;
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
/// Ask for a name only this far from every known place and earlier ask.
const ASK_MIN_M: f64 = 2.0;
/// A pose that moved more than this plus what the gait could have walked
/// since the last frame is the map moving, not the duck.
const JUMP_SLACK_M: f64 = 0.35;
/// An untrusted pose walks on (guarded) only farther than this from every
/// drop on the books — see the stable-untrusted rule in `Job::run`.
const UNTRUSTED_DROP_NEAR_M: f64 = 1.0;
/// No revisit to close a loop starts nearer a booked drop than this
/// (`QK_ANCHOR_DROP_M`, 0 for the old timing).
const ANCHOR_NOT_NEAR_DROP_DEFAULT_M: f64 = 1.0;
/// Stands in a row whose poses agree within [`SETTLED_M`] before a moved
/// map is trusted again.
const SETTLE_STANDS: u32 = 2;
const SETTLED_M: f64 = 0.15;
/// A watch job: how long the body stands still before its stand is
/// booked, and the watch's tick.
const WATCH_STILL_S: f64 = 2.0;
const WATCH_TICK: Duration = Duration::from_millis(500);
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
const DISTRUST_PANORAMA: u32 = 2;
const DISTRUST_FAIL: u32 = 6;

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
    /// The map's exploration so far, session after session (see
    /// `ExploreHandle::end_session`), as the ground book keeps it.
    pub progress: Option<Value>,
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
            progress: None,
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
            // The map's progress is the map's, not the job's: reset here, a
            // map declared complete was complete no more after one go_to,
            // and "explore" set out again.
            progress: s.progress.clone(),
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
            "progress": self.progress,
        })
    }
}

/// The job's handle: shared status, a stop flag, and the one place the
/// voice side takes a pending question from.
#[derive(Clone, Default)]
pub struct ExploreHandle {
    status: Arc<Mutex<ExploreStatus>>,
    /// The session the running job is, when it is one (see [`Session`]).
    session: Arc<Mutex<Option<Session>>>,
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
    /// The live map is the boot's search on a fresh map, not the house:
    /// nothing is saved or declared under the house's name until a saved
    /// map is adopted (see [`ExploreHandle::map_searching`]).
    searching: bool,
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

    fn write_ground_file(&self, file: serde_json::Map<String, Value>) {
        let Some(path) = self.ground.lock().expect("ground poisoned").path.clone() else { return };
        if let Err(e) = std::fs::write(&path, serde_json::to_string_pretty(&Value::Object(file)).unwrap_or_default()) {
            tracing::warn!(error = %e, path = %path.display(), "map explore: the ground book could not be written");
        }
    }

    /// The user says the exploration is complete (`robot.map_explore`
    /// `complete`): the map `name` is declared done whatever its share —
    /// the share is kept beside the verdict — and nothing explores it
    /// again unless a new map is asked for.
    pub fn declare_done(&self, name: &str, percent: f64) -> Value {
        let mut file = self.ground_file();
        let mut progress = file.get(&format!("{name}.progress")).cloned().unwrap_or(json!({}));
        progress["done"] = json!(true);
        progress["declared_by_user"] = json!(true);
        progress["percent"] = json!(percent.round());
        file.insert(format!("{name}.progress"), progress.clone());
        self.write_ground_file(file);
        self.name_live_map(name);
        self.update(|s| s.progress = Some(progress.clone()));
        tracing::info!(map = name, percent = format!("{percent:.0}"), "map explore: the user declares the exploration complete");
        progress
    }

    /// The name of the map whose books are on the books now, if any.
    pub fn map_name(&self) -> Option<String> {
        self.ground.lock().expect("ground poisoned").map.clone()
    }

    /// The boot could not find itself on the saved map and wiped the live
    /// one to search from nothing: the live map is no house's until a saved
    /// map is adopted (`map_named`). Its books and trail are the search's;
    /// the saved map's stay on disk.
    pub fn map_searching(&self) {
        {
            let mut g = self.ground.lock().expect("ground poisoned");
            g.map = None;
            g.searching = true;
        }
        let mut s = self.status.lock().expect("explore status poisoned");
        s.local.clear();
        s.trail.clear();
        s.lanes.clear();
        s.progress = None;
        s.blind = false;
    }

    /// Whether the live map is the boot's search (see `map_searching`).
    pub fn searching(&self) -> bool {
        self.ground.lock().expect("ground poisoned").searching
    }

    /// A new exploration of `name` from nothing (`robot.map_explore`
    /// `fresh`): the books, the trail and the progress go; the saved map
    /// and its book stay on disk until the new session saves over them.
    pub fn fresh_map(&self, name: &str) {
        let mut file = self.ground_file();
        file.remove(&format!("{name}.progress"));
        self.write_ground_file(file);
        {
            let mut g = self.ground.lock().expect("ground poisoned");
            g.map = None;
            g.searching = false;
        }
        let mut s = self.status.lock().expect("explore status poisoned");
        s.local.clear();
        s.trail.clear();
        s.lanes.clear();
        s.progress = None;
        s.blind = false;
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
        // rule booked a centimetre too wide and so saved as drops (the low
        // thing pushed ahead by the explorer's old legs, 2026-09-23). They
        // are not drops: left out.
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
        {
            let mut g = self.ground.lock().expect("ground poisoned");
            g.map = Some(name.to_string());
            g.searching = false;
        }
        let mut s = self.status.lock().expect("explore status poisoned");
        tracing::info!(map = name, drops = drops.len(), lanes = lanes.len(), "map explore: the ground book for this map is on the books");
        s.local = drops;
        s.trail.clear();
        s.lanes = lanes;
        s.progress = self.ground_file().get(&format!("{name}.progress")).cloned();
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
        gait: GaitConfig,
    ) -> Result<(), String> {
        self.start_job(robotd_socket, places, max_s, gait, move |drops, trail| {
            let mut job = Job::to_goal(goal, max_s, Instant::now());
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

    #[allow(clippy::too_many_arguments)]
    pub fn start(
        &self,
        robotd_socket: &str,
        places: &Places,
        known: Vec<(f64, f64)>,
        max_s: f64,
        ask: bool,
        gait: GaitConfig,
        session: Option<Session>,
    ) -> Result<(), String> {
        *self.session.lock().expect("session poisoned") = session.clone();
        self.start_job(robotd_socket, places, max_s, gait, move |drops, trail| {
            let mut job = Job::new(known, max_s, ask, Instant::now());
            job.battery_min_pct = session.as_ref().map(|s| s.battery_min_pct);
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
                let began = Instant::now();
                let (state, reason) = job.run(&handle, &mut robot);
                let session = handle.session.lock().expect("session poisoned").take();
                if let Some(session) = session
                    && job.goal.is_none()
                {
                    handle.end_session(&mut robot, &session, &reason, began.elapsed().as_secs_f64());
                }
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

/// A session of a progressive exploration (the user's idea, 2026-09-24:
/// the duck explores as far as a charge takes it, and after the next one
/// finds the map, finds itself on it, and goes on where it stopped, until
/// no large area is left): the map it saves itself under at the end, and
/// the battery level that ends it.
#[derive(Clone, Debug)]
pub struct Session {
    pub save_as: String,
    pub battery_min_pct: f64,
}

/// A session's progress: its share of floor the map knows against the
/// floor it knows plus the unknown still reachable from a frontier inside
/// the map's walls. Unknown pockets walled in on every side (the inside
/// of a sofa, a box) are not left to explore, and are not counted.
/// The largest piece of unknown, in m², that touches known floor within
/// the walls' box: under a bed, inside a hole — or a room not seen yet.
pub(crate) fn largest_unknown_piece_m2(grid: &Grid) -> f64 {
    let (rows, cols) = (grid.rows, grid.cols);
    let (mut r0, mut r1, mut c0, mut c1) = (rows, 0, cols, 0);
    for r in 0..rows {
        for c in 0..cols {
            if grid.cell(r, c) == Some(Cell::Wall) {
                r0 = r0.min(r); r1 = r1.max(r); c0 = c0.min(c); c1 = c1.max(c);
            }
        }
    }
    if r0 > r1 {
        return 0.0;
    }
    let mut seen = vec![false; rows * cols];
    let mut best = 0usize;
    for r in r0..=r1 {
        for c in c0..=c1 {
            if seen[r * cols + c] || grid.cell(r, c) != Some(Cell::Unknown) {
                continue;
            }
            seen[r * cols + c] = true;
            let (mut stack, mut n, mut touches) = (vec![(r, c)], 0usize, false);
            while let Some((rr, cc)) = stack.pop() {
                n += 1;
                for (dr, dc) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                    let (a, b) = (rr as i64 + dr, cc as i64 + dc);
                    if a < r0 as i64 || b < c0 as i64 || a > r1 as i64 || b > c1 as i64 {
                        continue;
                    }
                    let (a, b) = (a as usize, b as usize);
                    match grid.cell(a, b) {
                        Some(Cell::Free) => touches = true,
                        Some(Cell::Unknown) if !seen[a * cols + b] => {
                            seen[a * cols + b] = true;
                            stack.push((a, b));
                        }
                        _ => {}
                    }
                }
            }
            if touches {
                best = best.max(n);
            }
        }
    }
    best as f64 * grid.cell_m * grid.cell_m
}

pub(crate) fn explored_share(grid: &Grid) -> (f64, usize, usize) {
    let (rows, cols) = (grid.rows, grid.cols);
    let (mut r0, mut r1, mut c0, mut c1) = (rows, 0, cols, 0);
    for r in 0..rows {
        for c in 0..cols {
            if grid.cell(r, c) == Some(Cell::Wall) {
                r0 = r0.min(r); r1 = r1.max(r); c0 = c0.min(c); c1 = c1.max(c);
            }
        }
    }
    let free = grid.counts().1;
    if r0 > r1 {
        return (0.0, free, 0);
    }
    // Unknown cells within the walls' box, reached from a frontier cell.
    let mut seen = vec![false; rows * cols];
    let mut stack = Vec::new();
    for r in r0..=r1 {
        for c in c0..=c1 {
            if grid.cell(r, c) != Some(Cell::Free) {
                continue;
            }
            for (dr, dc) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                let (rr, cc) = (r as i64 + dr, c as i64 + dc);
                if rr < r0 as i64 || cc < c0 as i64 || rr > r1 as i64 || cc > c1 as i64 {
                    continue;
                }
                let i = rr as usize * cols + cc as usize;
                if grid.cell(rr as usize, cc as usize) == Some(Cell::Unknown) && !seen[i] {
                    seen[i] = true;
                    stack.push((rr as usize, cc as usize));
                }
            }
        }
    }
    let mut open = 0usize;
    while let Some((r, c)) = stack.pop() {
        open += 1;
        for (dr, dc) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
            let (rr, cc) = (r as i64 + dr, c as i64 + dc);
            if rr < r0 as i64 || cc < c0 as i64 || rr > r1 as i64 || cc > c1 as i64 {
                continue;
            }
            let i = rr as usize * cols + cc as usize;
            if grid.cell(rr as usize, cc as usize) == Some(Cell::Unknown) && !seen[i] {
                seen[i] = true;
                stack.push((rr as usize, cc as usize));
            }
        }
    }
    let share = free as f64 / (free + open).max(1) as f64;
    (share, free, open)
}

impl ExploreHandle {
    /// A session ended: its progress goes on the books under the map's
    /// name, and the map is saved under it — refused by the mapper while
    /// the duck does not know where it is, and then the session is not
    /// counted either (the map as it was saved last stays the map).
    fn end_session(&self, robot: &mut Robot, session: &Session, reason: &str, secs: f64) {
        let name = session.save_as.clone();
        let saved = crate::tools::map_library(&robot.places.map_socket, "robot.map_save", Some(json!({"name": name})));
        if let Err(e) = &saved {
            tracing::warn!(map = name, error = %e, "map explore: the session's map not saved; the session is not counted");
            return;
        }
        self.name_live_map(&name);
        let (share, free, open) = robot.frame().and_then(|f| f.grid().ok()).map_or((0.0, 0, 0), |g| explored_share(&g));
        // Done: nothing left, or only what cannot be reached and is small —
        // a strip behind a sofa, a hole's inside — else a house with one
        // unreachable sliver is never done.
        let left_m2 = open as f64 * 0.0025;
        // ... or the share mapped this high: the estimate errs low (on the
        // twin 7–20 points under the truth once a session is done), so 95 %
        // reported is the house — casa_libera, 2026-09-24: 96 % reported,
        // 99–100 % in every room, and another session spent on slivers.
        let done = reason.contains("no frontier")
            || (reason.contains("none is reachable") && left_m2 < DONE_LEFT_M2)
            || share >= DONE_SHARE;
        let mut file = self.ground_file();
        let before = file.get(&format!("{name}.progress")).cloned().unwrap_or(json!({}));
        let sessions = before.get("sessions").and_then(Value::as_u64).unwrap_or(0) + 1;
        let explore_s = before.get("explore_s").and_then(Value::as_f64).unwrap_or(0.0) + secs;
        let cell = robot.frame().and_then(|f| f.grid().ok()).map_or(0.05, |g| g.cell_m);
        // The user's "exploration complete" is final: a session that ends
        // after it (the stop it asked for) does not undo it.
        let declared = before.get("declared_by_user").and_then(Value::as_bool).unwrap_or(false);
        let done = done || declared;
        let progress = json!({
            "declared_by_user": declared,
            "sessions": sessions,
            "explore_s": explore_s.round(),
            "percent": (share * 100.0).round(),
            "known_floor_m2": (free as f64 * cell * cell * 10.0).round() / 10.0,
            "left_m2": (open as f64 * cell * cell * 10.0).round() / 10.0,
            "done": done,
            "last_reason": reason,
        });
        tracing::info!(map = name, sessions, percent = format!("{:.0}", share * 100.0), done, "map explore: the session saved; the exploration so far");
        file.insert(format!("{name}.progress"), progress.clone());
        self.write_ground_file(file);
        self.update(|s| s.progress = Some(progress));
    }
}

/// The job's state between iterations.
pub struct Job {
    known: Vec<(f64, f64)>,
    max_s: f64,
    ask: bool,
    started: Instant,
    /// Visited frontier spots: never chosen again.
    visited: Vec<((f64, f64), f64)>,
    /// Frontier spots refused too often: left alone until nothing else
    /// is left, then forgotten once.
    refused: Vec<((f64, f64), f64)>,
    refused_cleared: bool,
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
    /// A watch: the job commands nothing — somebody else drives (a human
    /// at `teleop.py`) — and books what the stands see, as the legs'
    /// stands do; the ground book is written when it ends. The guided
    /// drive that writes the books (the user's, 2026-09-21: "a small
    /// guided drive round the stairwell, and we refresh the drops").
    watch: bool,
    watch_booked: bool,
    /// How many drops the ground book brought (see `remember_local`).
    ground_drops: usize,
    /// Steps of the stick (see `stick.rs`), for its stands.
    stick_steps: u32,
    /// The stick's last pose, and its steps in a row that did not move it.
    stick_last: Option<(f64, f64)>,
    stick_stalls: u32,
    /// Odometry walked since the stick's last stand, and where it was.
    stick_since_stand: f64,
    stick_odom_at: Option<(f64, f64)>,
    /// How long the stick stands, and whether it is careful beside the
    /// drops (the exploration's travel; see `Job::travel`).
    stick_stand_s: f64,
    stick_careful: bool,
    /// Whether the stick writes the books at its stands (the exploration's
    /// travel).
    stick_books: bool,
    /// Where the stick's hole guard last saw a hole (see `Job::travel`).
    stick_hole_at: Option<(f64, f64)>,
    /// Walls looked at from near, and how many (see `close_look.rs`).
    looked: Vec<(f64, f64)>,
    close_looks: u32,
    last_close_look: Option<Instant>,
    /// The holes gone round, by centre, and which of their four sides have
    /// been looked from (see `rim_tour.rs`).
    rim_sides: Vec<((f64, f64), u8)>,
    rim_looks: u32,
    last_rim_look: Option<Instant>,
    /// A fall was seen and the pose has not been trusted for
    /// [`BOOKS_AFTER_FALL_S`] since (the instant it was trusted again, if
    /// it is): no drop goes on the books meanwhile (see `drops_bookable`).
    fell: Option<Option<Instant>>,
    /// Steps of the look-around after a fall (see `relocate_step`).
    relocate_steps: u32,
    /// Moves off a rim in a row (see `off_the_rim`).
    rim_offs: u32,
    /// A session of a progressive exploration ends at this battery level,
    /// checked every [`BATTERY_EVERY_S`].
    battery_min_pct: Option<f64>,
    battery_checked: Option<Instant>,
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
    /// The arrival stand was taken once (see `GOAL_FIT_M`).
    goal_confirmed: bool,
    /// The journey's route as kept between plans (raw, pulled), when it
    /// was planned, and the books' size then (see `KEEP_ROUTE_S`).
    kept_route: Option<(Vec<(f64, f64)>, Vec<(f64, f64)>, Instant, usize)>,
    lost_since: Option<Instant>,
    /// "Sealed in" recoveries in this job (see `unseal`).
    stuck: u32,
    /// When and where the last one was counted: another counts only
    /// after [`STUCK_GAP_S`] or [`STUCK_MOVE_M`] of the body's own motion.
    last_unseal: Option<(Instant, (f64, f64))>,
    /// Times every local obstacle was forgotten to reach a sealed-off frontier.
    global_forgets: u32,
    /// Sliver phase bookkeeping: free cells when the phase began (or the
    /// last growth), and slivers served since without growth.
    sliver_free_cells: Option<usize>,
    sliver_served: u32,
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
mod close_look;
mod navigate;
mod rim_tour;
mod mode;
mod recover;
mod stick;
mod trusted;
use books::*;
use gait::*;
use guarded::*;
use journey::*;
use mapping::*;
use mode::*;
use close_look::*;
use navigate::*;
use rim_tour::*;
use stick::*;
use trusted::*;

impl Job {
    pub fn new(known: Vec<(f64, f64)>, max_s: f64, ask: bool, now: Instant) -> Self {
        Self {
            known,
            max_s,
            ask,
            started: now,
            visited: Vec::new(),
            refused: Vec::new(),
            goal: None,
            refused_cleared: false,
            local: Vec::new(),
            target: None,
            last_back: None,
            last_pose_stand: None,
            frozen: false,
            policy: Policy::for_mode(Mode::Mapping),
            trusted: TrustedFloor::default(),
            last_fit: None,
            lanes: Vec::new(),
            watch: false,
            watch_booked: false,
            ground_drops: 0,
            turns_refused_at_drop: 0,
            stick_steps: 0,
            stick_last: None,
            stick_stalls: 0,
            stick_since_stand: 0.0,
            stick_odom_at: None,
            stick_stand_s: STICK_STAND_S,
            stick_careful: false,
            stick_books: false,
            stick_hole_at: None,
            looked: Vec::new(),
            close_looks: 0,
            last_close_look: None,
            rim_sides: Vec::new(),
            rim_looks: 0,
            last_rim_look: None,
            fell: None,
            relocate_steps: 0,
            rim_offs: 0,
            battery_min_pct: None,
            battery_checked: None,
            no_go: Vec::new(),
            unseals_here: None,
            drops_bookable: true,
            goal_confirmed: false,
            kept_route: None,
            lost_since: None,
            stuck: 0,
            last_unseal: None,
            global_forgets: 0,
            trail: Vec::new(),
            sliver_free_cells: None,
            sliver_served: 0,
            last_pose: None,
            unsettled: 0,
            panoramas: Vec::new(),
            distrust: 0,
            anchored_at: None,
            anchor: None,
        }
    }
    /// A watch job (see the `watch` field).
    pub fn watch(max_s: f64, now: Instant) -> Self {
        let mut job = Job::new(Vec::new(), max_s, false, now);
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
        // A journey runs its own loop (see `navigate.rs`), none of the
        // explorer's rules below.
        if self.goal.is_some() {
            return self.run_journey(handle, robot);
        }
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
            if let Some(min) = self.battery_min_pct
                && self.battery_checked.is_none_or(|t| (robot.now() - t).as_secs_f64() >= BATTERY_EVERY_S)
            {
                self.battery_checked = Some(robot.now());
                if let Some(pct) = robot.battery_percent()
                    && pct < min
                {
                    return (State::Done, format!("battery at {pct:.0} %: the session ends here — charged, the duck goes on from where it stopped"));
                }
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
                && self.fell.is_none()
                // Never beside a drop on the books: a pose the mapper does
                // not vouch for, 0.3 m off where the map itself is 0.3 m
                // off (casa_arredata's second-session map, 2026-09-25), is
                // the one thing a leg beside a stairwell must not rest on.
                // Stand there until the pose is confirmed.
                && !self.drop_within(UNTRUSTED_DROP_NEAR_M);
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
            if reanchor() {
                let due = self
                    .anchored_at
                    .is_none_or(|t| (robot.now() - t).as_secs_f64() >= ANCHOR_EVERY_S);
                // Not beside a drop: a revisit there turns the duck round in
                // the mouth of the passage it is entering. casa_arredata
                // (2026-09-26): aimed down the 0.49 m passage by the
                // stairwell, the timer sent it 116° back to an old place,
                // and it came back facing the end of the wall at 10 cm —
                // stuck there, the bathroom never reached in four sessions.
                // The revisit waits until the duck is clear of the rims.
                let beside_a_drop = self.drop_within(knob("QK_ANCHOR_DROP_M", ANCHOR_NOT_NEAR_DROP_DEFAULT_M));
                if self.anchor.is_none() && due && !beside_a_drop {
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
                        let deadline = robot.now() + travel_budget(path.len() as f64 * grid.cell_m);
                        match self.travel(handle, robot, anchor, deadline, false) {
                            (State::Stopped, why) => return (State::Stopped, why),
                            (State::Done, _) => {}
                            (_, why) => {
                                tracing::info!(why, "map explore: the way back to the old place failed; carrying on");
                                self.anchor = None;
                                self.anchored_at = Some(robot.now());
                            }
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
            let walls = self.planner_walls();
            self.walked((x, y), (x, y));
            let lanes = self.lanes();
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
                        // The frontiers are done: the walls seen from afar
                        // only get a look from near first (see
                        // `close_look.rs`).
                        match self.rim_look(handle, robot, &grid, (x, y), f64::INFINITY).or_else(|| self.close_look(handle, robot, &grid, (x, y), f64::INFINITY)) {
                            Some(Some(verdict)) => return verdict,
                            Some(None) => continue,
                            None => {}
                        }
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
                // The spots the job kept off (see `unseal`) may be what
                // seals it in — in a small room, the only way out (the
                // paper twin's bathroom, 2026-09-24): they go first.
                if cells_left >= MIN_FRONTIER_CELLS && !self.no_go.is_empty() {
                    tracing::info!(spots = self.no_go.len(), "map explore: sealed in, and the spots kept off may be the way out; forgetting them");
                    self.no_go.clear();
                    self.kept_route = None;
                    continue;
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
                // The frontiers are done (or out of reach): the walls seen
                // from afar only get a look from near first.
                match self.rim_look(handle, robot, &grid, (x, y), f64::INFINITY).or_else(|| self.close_look(handle, robot, &grid, (x, y), f64::INFINITY)) {
                    Some(Some(verdict)) => return verdict,
                    Some(None) => continue,
                    None => {}
                }
                // Frontier cells are always left on a furnished map — the
                // band of uncertain cells along every wall and round every
                // piece of furniture touches unknown — so what says whether
                // the house is mapped is the unknown itself: nothing but
                // small pieces, under the furniture and in the holes.
                // casa_grande's last two sessions ended "stuck" with 95 % of
                // the true floor known, their largest unknown piece 3.3 m²
                // (casa_arredata's 3.9, the apartment's 1.0; a first session
                // still exploring had 5.5–7.3) (x18, x19, 2026-09-30).
                let piece = largest_unknown_piece_m2(&grid);
                let mapped = format!("no frontier within reach, and what is left unknown is in pieces of {piece:.1} m² at most — under furniture and in holes: the house is mapped");
                let stuck = format!("stuck: frontiers remain but none is reachable from here, the largest unknown piece {piece:.1} m²");
                let why = if cells_left < MIN_FRONTIER_CELLS {
                    "no frontier left".to_string()
                } else if piece < DONE_PIECE_M2 {
                    mapped
                } else {
                    stuck
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

            // A hole on the books is gone round, a side at a time, before
            // the next frontier (see `rim_tour.rs`).
            if self.last_rim_look.is_none_or(|t| (robot.now() - t).as_secs_f64() >= RIM_LOOK_EVERY_S) {
                self.last_rim_look = Some(robot.now());
                match self.rim_look(handle, robot, &grid, (x, y), DETOUR_ROUTE_M) {
                    Some(Some(verdict)) => return verdict,
                    Some(None) => continue,
                    None => {}
                }
            }
            // Now and then, a wall seen from afar only gets a look from
            // near before the next frontier (see `close_look.rs`).
            if self.last_close_look.is_none_or(|t| (robot.now() - t).as_secs_f64() >= CLOSE_LOOK_EVERY_S) {
                self.last_close_look = Some(robot.now());
                match self.close_look(handle, robot, &grid, (x, y), DETOUR_ROUTE_M) {
                    Some(Some(verdict)) => return verdict,
                    Some(None) => continue,
                    None => {}
                }
            }
            // The way to the frontier: the navigation's (see `navigate.rs`).
            if let Some(verdict) = self.travel_to_frontier(handle, robot, f.stand, f.distance_m) {
                return verdict;
            }
        }
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

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::Cell;

    /// The progress share: floor known over floor known plus the unknown a
    /// frontier reaches inside the walls; a walled-in pocket is not left
    /// to explore.
    #[test]
    fn the_share_explored_leaves_out_walled_pockets() {
        let (rows, cols) = (20, 20);
        let mut cells = vec![Cell::Unknown; rows * cols];
        for r in 0..rows {
            for c in 0..cols {
                let border = r == 0 || c == 0 || r == rows - 1 || c == cols - 1;
                cells[r * cols + c] = if border { Cell::Wall } else if c < 10 { Cell::Free } else { Cell::Unknown };
            }
        }
        // A sofa's inside: unknown, walled in, in the known half.
        for r in 4..8 {
            for c in 3..7 {
                let edge = r == 4 || r == 7 || c == 3 || c == 6;
                cells[r * cols + c] = if edge { Cell::Wall } else { Cell::Unknown };
            }
        }
        let g = Grid { rows, cols, x_min: 0.0, y_min: 0.0, cell_m: 0.05, cells };
        let (share, free, open) = explored_share(&g);
        assert_eq!(open, 18 * 9, "the unknown half, not the pocket");
        assert!((share - free as f64 / (free + open) as f64).abs() < 1e-9 && share > 0.45 && share < 0.55, "{share}");
    }

    /// The largest unknown piece touching known floor: a bed's footprint in
    /// a mapped room is small, an unseen room is not.
    #[test]
    fn the_largest_unknown_piece_tells_a_bed_from_a_room() {
        let (rows, cols) = (100, 100);
        let mut cells = vec![Cell::Free; rows * cols];
        for r in 0..rows {
            for c in 0..cols {
                if r == 0 || c == 0 || r == rows - 1 || c == cols - 1 {
                    cells[r * cols + c] = Cell::Wall;
                }
            }
        }
        // A bed, 1.0 x 1.5 m, unknown under it and open on its sides.
        for r in 10..40 {
            for c in 10..30 {
                cells[r * cols + c] = Cell::Unknown;
            }
        }
        let g = Grid { rows, cols, x_min: 0.0, y_min: 0.0, cell_m: 0.05, cells: cells.clone() };
        let bed = largest_unknown_piece_m2(&g);
        assert!((bed - 1.5).abs() < 1e-9 && bed < DONE_PIECE_M2, "{bed}");
        // A room of 2.5 x 2.5 m not seen yet.
        for r in 40..90 {
            for c in 45..95 {
                cells[r * cols + c] = Cell::Unknown;
            }
        }
        let g = Grid { rows, cols, x_min: 0.0, y_min: 0.0, cell_m: 0.05, cells };
        assert!(largest_unknown_piece_m2(&g) > DONE_PIECE_M2);
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

/// The step back at house2's stairwell that fell (2026-09-26): the body at
/// the hole's north-east corner, nose to the rim, the rim ahead. The
/// model's path backs away from it and passed; the body turned the other
/// way than the model has it and the nose swung over the hole. With the
/// rim at hand, no blind step back — at the true pose and at the map's.
#[test]
fn no_blind_step_back_with_a_drop_at_hand() {
    // The stairwell's rim, x -0.4..0, y -1.4..-0.7, a point every 10 cm.
    let mut rim = Vec::new();
    for i in 0..=4 {
        let x = -0.4 + 0.1 * f64::from(i);
        rim.push((x, -0.7));
        rim.push((x, -1.4));
    }
    for i in 1..7 {
        let y = -1.4 + 0.1 * f64::from(i);
        rim.push((-0.4, y));
        rim.push((0.0, y));
    }
    let job = Job::to_goal((-3.44, -2.6), 300.0, std::time::Instant::now()).with_books(rim);
    assert!(job.drop_on_back((-0.01, -0.46, -1.30), 1.0, 1.5, DROP_PATH_MARGIN_M).is_none(), "the model's path is clear");
    assert!(job.drop_at_hand((-0.06, -0.56, -1.11)), "the truth");
    assert!(job.drop_at_hand((-0.01, -0.46, -1.30)), "the map");
    assert!(!job.drop_at_hand((0.3, -0.2, -1.30)));
}

/// The boot's search is no house's map: it has no name, its books are its
/// own, and it stays so until a saved map is adopted — the one thing that
/// ends it (casa_arredata, 2026-09-26: the search saved over the house).
#[test]
fn the_boots_search_is_no_houses_map_until_one_is_adopted() {
    let dir = std::env::temp_dir().join(format!("quack-nav-search-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let h = ExploreHandle::new().with_ground(dir.join("places.json").to_str().unwrap());
    h.map_named("house");
    h.update(|s| s.local = vec![((1.0, 1.0), DROP_RADIUS_M)]);
    assert!(!h.searching());
    h.map_searching();
    assert!(h.searching());
    assert_eq!(h.map_name(), None, "the search has no name to save under");
    assert!(h.status().local.is_empty(), "the house's books are not the search's");
    h.map_named("house");
    assert!(!h.searching());
    assert_eq!(h.map_name().as_deref(), Some("house"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_ground_book_keeps_the_drops_the_body_never_walked_over() {
    let dir = std::env::temp_dir().join(format!("quack-nav-ground-{}", std::process::id()));
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

/// Standing on a booked drop strikes it, and the reach booked behind
/// it; an obstacle edge and a far drop stay.
#[test]
    fn standing_on_a_booked_drop_strikes_it() {
        let mut job = Job::new(vec![], 60.0, false, Instant::now());
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

}
