//! The pilot: a learned policy for the stick's leg (docs/rl-pilot.md).
//!
//! The stick (`explore/stick.rs`) follows the route with three rules: turn
//! in place when the route is off the nose, step curving onto it, turn when
//! the steps do not move the body. The pilot is a small network that picks
//! the leg's move instead, from what the duck sees now — the route ahead,
//! the depth sensor's last second and the map around the body — trained on
//! thousands of simulated journeys (`quack-rl`) with what the map does not
//! know on the way: things left on the floor, people and pets crossing,
//! doorways and passages beside a hole, a map a little off.
//!
//! What stays out of it: the route (the planner), the books, the stands for
//! the mapper and the hole guard. The guard is a shield over the pilot: a
//! step it chooses into a true hole in the lane is not walked, as the
//! stick's is not (`stick.rs`).
//!
//! The observation is built here and only here, for the simulator and the
//! duck alike: the same function over the same types (`MapFrame`'s grid,
//! the books, `CliffStatus`), so what the network learned to read is what
//! it reads on the floor.

pub mod trace;

use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::cliff::CliffStatus;
use crate::frontier::ExtraWall;
use crate::map::{Cell, Grid};

/// The observation's layout; a pilot trained on another refuses to load.
pub const OBS_VERSION: u32 = 3;

/// The leg's moves. Steps are the stick's own (vx 0.3 for 0.6 s, a yaw
/// that curves it), five of them from hard right to hard left; turns are
/// the stick's turn in place, closed on odometry; the back-off is the one
/// backing the gait does from a standstill (a positive yaw); the wait is a
/// stand, for what moves to pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    /// `-2..=2`: vyaw `k * STEP_VYAW_UNIT`, positive to the left.
    Step(i8),
    TurnLeft,
    TurnRight,
    Back,
    Wait,
}

pub const N_ACTIONS: usize = 9;
pub const STEP_VX: f64 = 0.3;
pub const STEP_S: f64 = 0.6;
pub const STEP_VYAW_UNIT: f64 = 0.35;
/// A turn in place asks for this much.
pub const TURN_RAD: f64 = 0.4;
pub const BACK_VX: f64 = -0.3;
pub const BACK_VYAW: f64 = 0.7;
pub const BACK_S: f64 = 0.6;
pub const WAIT_S: f64 = 0.6;

impl Action {
    pub fn from_index(i: usize) -> Self {
        match i {
            0..=4 => Action::Step(i as i8 - 2),
            5 => Action::TurnLeft,
            6 => Action::TurnRight,
            7 => Action::Back,
            _ => Action::Wait,
        }
    }

    pub fn index(self) -> usize {
        match self {
            Action::Step(k) => (k.clamp(-2, 2) + 2) as usize,
            Action::TurnLeft => 5,
            Action::TurnRight => 6,
            Action::Back => 7,
            Action::Wait => 8,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Action::Step(-2) => "step_right_hard",
            Action::Step(-1) => "step_right",
            Action::Step(0) => "step",
            Action::Step(1) => "step_left",
            Action::Step(_) => "step_left_hard",
            Action::TurnLeft => "turn_left",
            Action::TurnRight => "turn_right",
            Action::Back => "back",
            Action::Wait => "wait",
        }
    }

    /// Whether the move walks forward (what the hole guard judges).
    pub fn forward(self) -> bool {
        matches!(self, Action::Step(_))
    }

    /// The timed move it is, as `(vx, vyaw, secs)`; `None` for the turns,
    /// which are closed on odometry by the caller, and for the wait.
    pub fn timed(self) -> Option<(f64, f64, f64)> {
        match self {
            Action::Step(k) => Some((STEP_VX, f64::from(k) * STEP_VYAW_UNIT, STEP_S)),
            Action::Back => Some((BACK_VX, BACK_VYAW, BACK_S)),
            _ => None,
        }
    }
}

// --- the observation ------------------------------------------------------

/// The route's points, this far ahead along it.
pub const ROUTE_AHEAD_M: [f64; 4] = [0.2, 0.4, 0.7, 1.0];
/// Bearing sectors of the depth sensor's memory, across this half-span:
/// the frames walking (head centred, ±0.39) and the stand's sweep (±1.19).
pub const SECTORS: usize = 12;
pub const SECTOR_HALF_SPAN: f64 = 1.2;
/// The two slices of the sensor's memory: what it saw in the last
/// `SLICE_S`, and in the `SLICE_S` before — what moves shows as a change.
pub const SLICE_S: f64 = 0.7;
pub const RANGE_NORM_M: f64 = 2.2;
/// The local map: `LOCAL_N` × `LOCAL_N` cells of `LOCAL_CELL_M`, from
/// `LOCAL_BEHIND_M` behind the body forward, centred across it.
pub const LOCAL_N: usize = 16;
pub const LOCAL_CELL_M: f64 = 0.1;
pub const LOCAL_BEHIND_M: f64 = 0.4;

pub const OBS_DIM: usize = ROUTE_AHEAD_M.len() * 2 + 3 + SECTORS * 3 * 2 + LOCAL_N * LOCAL_N + N_ACTIONS + 3;

/// What the pilot reads, borrowed from whoever drives: the map's pose, the
/// map, the books (drops and bumps, as the planner sees them), the route
/// from the pose, the goal, the sensor, and its own last move.
pub struct ObsInput<'a> {
    pub pose: (f64, f64, f64),
    pub grid: &'a Grid,
    pub walls: &'a [ExtraWall],
    pub route: &'a [(f64, f64)],
    pub goal: (f64, f64),
    pub cliff: Option<&'a CliffStatus>,
    pub now: Instant,
    /// Its last move as it chose it (walked or refused).
    pub last: Option<Action>,
    /// Its moves in a row the shields refused (see `explore/stick.rs`).
    pub refused: u32,
    /// Legs in a row that did not move the body.
    pub stalls: u32,
    /// How far the last leg moved the body, metres (odometry's or the map's).
    pub moved_m: f64,
}

/// World point → body frame.
fn ego((x, y, yaw): (f64, f64, f64), p: (f64, f64)) -> (f64, f64) {
    let (dx, dy) = (p.0 - x, p.1 - y);
    let (c, s) = (yaw.cos(), yaw.sin());
    (c * dx + s * dy, -s * dx + c * dy)
}

/// The point `ahead` metres along `route`, starting at its point nearest
/// the body; the route's end if it is shorter.
fn along(route: &[(f64, f64)], from: (f64, f64), ahead: f64) -> Option<(f64, f64)> {
    let start = route
        .iter()
        .enumerate()
        .min_by(|a, b| {
            let da = (a.1.0 - from.0).hypot(a.1.1 - from.1);
            let db = (b.1.0 - from.0).hypot(b.1.1 - from.1);
            da.total_cmp(&db)
        })?
        .0;
    let mut left = ahead;
    let mut prev = from;
    for p in &route[start..] {
        let d = (p.0 - prev.0).hypot(p.1 - prev.1);
        if d >= left && d > 1e-9 {
            let t = left / d;
            return Some((prev.0 + t * (p.0 - prev.0), prev.1 + t * (p.1 - prev.1)));
        }
        left -= d;
        prev = *p;
    }
    route.last().copied()
}

fn sector_of(bearing: f64) -> Option<usize> {
    let u = (bearing + SECTOR_HALF_SPAN) / (2.0 * SECTOR_HALF_SPAN);
    (0.0..1.0).contains(&u).then(|| (u * SECTORS as f64) as usize)
}

fn cell_value(grid: &Grid, x: f64, y: f64) -> f32 {
    match grid.at(x, y) {
        Some(Cell::Free) => 0.0,
        Some(Cell::Wall) => 1.0,
        Some(Cell::Unknown) | None => 0.5,
    }
}

/// The observation, `OBS_DIM` values (see the module and docs/rl-pilot.md
/// for the layout).
pub fn observe(input: &ObsInput) -> Vec<f32> {
    let mut obs = Vec::with_capacity(OBS_DIM);
    let pose = input.pose;
    let here = (pose.0, pose.1);
    // The route ahead, in the body frame.
    for a in ROUTE_AHEAD_M {
        let p = along(input.route, here, a).unwrap_or(input.goal);
        let (ex, ey) = ego(pose, p);
        obs.push(ex as f32);
        obs.push(ey as f32);
    }
    // The goal.
    let (gx, gy) = ego(pose, input.goal);
    let gd = gx.hypot(gy);
    let k = if gd > 2.0 { 2.0 / gd } else { 1.0 };
    obs.push((gx * k / 2.0) as f32);
    obs.push((gy * k / 2.0) as f32);
    obs.push((gd.min(4.0) / 4.0) as f32);
    // The sensor's memory, per sector and slice: the nearest obstacle,
    // the second nearest (a lone return is often nothing: the twin's
    // sensor returns a fifth of its zones short, a tenth of those within
    // 0.15 m — a thing seen is seen by several zones and frames), and the
    // nearest drop; 1 where nothing was seen.
    let mut first = [[1.0f32; SECTORS]; 2];
    let mut second = [[1.0f32; SECTORS]; 2];
    let mut drop = [[1.0f32; SECTORS]; 2];
    if let Some(cliff) = input.cliff {
        for f in &cliff.recent {
            let age = input.now.saturating_duration_since(f.at).as_secs_f64();
            let slice = if age < SLICE_S {
                0
            } else if age < 2.0 * SLICE_S {
                1
            } else {
                continue;
            };
            for o in &f.obstacles {
                if let Some(s) = sector_of(o.bearing) {
                    let v = (o.range_m / RANGE_NORM_M).clamp(0.0, 1.0) as f32;
                    if v < first[slice][s] {
                        second[slice][s] = first[slice][s];
                        first[slice][s] = v;
                    } else if v < second[slice][s] {
                        second[slice][s] = v;
                    }
                }
            }
            for d in &f.drops {
                if let Some(s) = sector_of(d.bearing) {
                    let r = if d.edge_min_m > 0.0 { d.edge_min_m } else { d.range_m * 0.5 };
                    let v = (r / RANGE_NORM_M).clamp(0.0, 1.0) as f32;
                    drop[slice][s] = drop[slice][s].min(v);
                }
            }
        }
    }
    for slice in 0..2 {
        obs.extend_from_slice(&first[slice]);
        obs.extend_from_slice(&second[slice]);
        obs.extend_from_slice(&drop[slice]);
    }
    // The map around the body, the books on it: each 0.1 m cell the worst
    // of the four 5 cm cells under it.
    let near: Vec<&ExtraWall> = input
        .walls
        .iter()
        .filter(|(p, r)| (p.0 - pose.0).hypot(p.1 - pose.1) < 2.0 + r)
        .collect();
    let (c, s) = (pose.2.cos(), pose.2.sin());
    let half = LOCAL_N as f64 * LOCAL_CELL_M / 2.0;
    for i in 0..LOCAL_N {
        for j in 0..LOCAL_N {
            let ex = -LOCAL_BEHIND_M + (i as f64 + 0.5) * LOCAL_CELL_M;
            let ey = -half + (j as f64 + 0.5) * LOCAL_CELL_M;
            let (wx, wy) = (pose.0 + c * ex - s * ey, pose.1 + s * ex + c * ey);
            let q = LOCAL_CELL_M / 4.0;
            let mut v = 0.0f32;
            for (ox, oy) in [(-q, -q), (-q, q), (q, -q), (q, q)] {
                v = v.max(cell_value(input.grid, wx + ox, wy + oy));
            }
            if v < 1.0 && near.iter().any(|(p, r)| (p.0 - wx).hypot(p.1 - wy) <= r + LOCAL_CELL_M / 2.0) {
                v = 1.0;
            }
            obs.push(v);
        }
    }
    // Its own last move, and whether the legs move the body.
    let mut onehot = [0.0f32; N_ACTIONS];
    if let Some(a) = input.last {
        onehot[a.index()] = 1.0;
    }
    obs.extend_from_slice(&onehot);
    obs.push((f64::from(input.stalls) / 3.0).min(1.0) as f32);
    obs.push((input.moved_m / 0.1).clamp(0.0, 1.5) as f32);
    obs.push((f64::from(input.refused) / 2.0).min(1.0) as f32);
    debug_assert_eq!(obs.len(), OBS_DIM);
    obs
}

// --- the network ----------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer {
    /// `out × in`, row-major.
    pub w: Vec<Vec<f32>>,
    pub b: Vec<f32>,
    /// `tanh`, `relu` or `none`.
    pub act: String,
}

/// A trained pilot as `scripts/rl/export.py` writes it: the network, the
/// observation's normalisation, and where it came from.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PilotFile {
    pub format: String,
    pub obs_version: u32,
    pub obs_dim: usize,
    pub n_actions: usize,
    pub obs_mean: Vec<f32>,
    pub obs_std: Vec<f32>,
    pub layers: Vec<Layer>,
    #[serde(default)]
    pub meta: serde_json::Value,
}

/// Whatever picks the leg's move from the observation: a trained
/// [`Pilot`] on the duck, the trainer itself in `quack-rl` (its brain
/// answers from the learner over a pipe, so the network learns on the very
/// loop it will fly).
pub trait Brain: Send + Sync {
    fn act(&self, obs: &[f32]) -> Action;
}

impl Brain for Pilot {
    fn act(&self, obs: &[f32]) -> Action {
        Pilot::act(self, obs)
    }
}

#[derive(Debug, Clone)]
pub struct Pilot {
    pub file: PilotFile,
    pub path: String,
}

impl Pilot {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path).map_err(|e| anyhow::anyhow!("cannot read the pilot {}: {e}", path.display()))?;
        let file: PilotFile = serde_json::from_str(&text).map_err(|e| anyhow::anyhow!("the pilot {} is not a pilot file: {e}", path.display()))?;
        anyhow::ensure!(file.format == "quack-pilot", "{}: format {:?}, not quack-pilot", path.display(), file.format);
        anyhow::ensure!(
            file.obs_version == OBS_VERSION && file.obs_dim == OBS_DIM && file.n_actions == N_ACTIONS,
            "{}: trained on observation v{} ({} values, {} actions), this build reads v{OBS_VERSION} ({OBS_DIM}, {N_ACTIONS})",
            path.display(),
            file.obs_version,
            file.obs_dim,
            file.n_actions
        );
        anyhow::ensure!(file.obs_mean.len() == OBS_DIM && file.obs_std.len() == OBS_DIM, "{}: normalisation of the wrong size", path.display());
        let mut width = OBS_DIM;
        for (i, l) in file.layers.iter().enumerate() {
            anyhow::ensure!(
                l.b.len() == l.w.len() && l.w.iter().all(|r| r.len() == width),
                "{}: layer {i} does not fit (expects {width} inputs)",
                path.display()
            );
            width = l.b.len();
        }
        anyhow::ensure!(width == N_ACTIONS, "{}: the network ends in {width} outputs, not {N_ACTIONS}", path.display());
        Ok(Self { file, path: path.display().to_string() })
    }

    /// The action scores (logits) for an observation.
    pub fn logits(&self, obs: &[f32]) -> Vec<f32> {
        let mut x: Vec<f32> = obs
            .iter()
            .zip(&self.file.obs_mean)
            .zip(&self.file.obs_std)
            .map(|((v, m), s)| ((v - m) / s.max(1e-6)).clamp(-10.0, 10.0))
            .collect();
        for l in &self.file.layers {
            let mut y: Vec<f32> = l.w.iter().zip(&l.b).map(|(row, b)| row.iter().zip(&x).map(|(w, v)| w * v).sum::<f32>() + b).collect();
            match l.act.as_str() {
                "tanh" => y.iter_mut().for_each(|v| *v = v.tanh()),
                "relu" => y.iter_mut().for_each(|v| *v = v.max(0.0)),
                _ => {}
            }
            x = y;
        }
        x
    }

    /// The move it picks: the best-scored.
    pub fn act(&self, obs: &[f32]) -> Action {
        let l = self.logits(obs);
        let best = l.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).map(|(i, _)| i).unwrap_or(Action::Step(0).index());
        Action::from_index(best)
    }
}

/// `QK_RL_POLICY`: the pilot file (docs/rl-pilot.md) that flies the stick's legs; unset, the stick.
/// Loaded once; a file that does not load is said in the log and the stick
/// drives.
pub fn from_env() -> Option<Arc<dyn Brain>> {
    static P: OnceLock<Option<Arc<Pilot>>> = OnceLock::new();
    P.get_or_init(|| {
        let path = std::env::var("QK_RL_POLICY").ok().filter(|p| !p.is_empty())?;
        match Pilot::load(Path::new(&path)) {
            Ok(p) => {
                tracing::info!(path, "rl pilot: loaded; the stick's legs are the pilot's");
                Some(Arc::new(p))
            }
            Err(e) => {
                tracing::warn!(error = %e, "rl pilot: not loaded; the stick drives");
                None
            }
        }
    })
    .clone()
    .map(|p| p as Arc<dyn Brain>)
}

/// How long a frame stays in the observation.
pub const OBS_MEMORY: Duration = Duration::from_millis((2.0 * SLICE_S * 1000.0) as u64);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cliff::{CliffFrame, Obstacle};

    fn grid() -> Grid {
        // 2 x 2 m of floor, a wall row at y = 0.50-0.55.
        let (rows, cols) = (40, 40);
        let mut cells = vec![Cell::Free; rows * cols];
        for c in 0..cols {
            cells[30 * cols + c] = Cell::Wall;
        }
        Grid { rows, cols, x_min: -1.0, y_min: -1.0, cell_m: 0.05, cells }
    }

    #[test]
    fn actions_round_trip() {
        for i in 0..N_ACTIONS {
            assert_eq!(Action::from_index(i).index(), i);
        }
        assert_eq!(Action::Step(-2).timed(), Some((STEP_VX, -0.7, STEP_S)));
    }

    #[test]
    fn observation_has_its_layout() {
        let g = grid();
        let now = Instant::now();
        let mut cliff = CliffStatus::default();
        cliff.recent.push(CliffFrame {
            seq: 1,
            at: now,
            head_yaw: 0.0,
            moving: true,
            drops: vec![],
            floors: vec![],
            obstacles: vec![Obstacle { bearing: 0.0, range_m: 0.55 }],
            floor_beams: 0,
            judged: 0,
        });
        let route = [(0.2, 0.0), (0.5, 0.0), (0.9, 0.0)];
        let obs = observe(&ObsInput {
            pose: (0.0, 0.0, std::f64::consts::FRAC_PI_2),
            grid: &g,
            walls: &[((0.0, -0.5), 0.1)],
            route: &route,
            goal: (0.9, 0.0),
            cliff: Some(&cliff),
            now,
            last: Some(Action::TurnLeft),
            refused: 0,
            stalls: 1,
            moved_m: 0.05,
        });
        assert_eq!(obs.len(), OBS_DIM);
        // Facing +y, the route along +x is to the right: negative ego y.
        assert!(obs[0] < 0.05 && obs[1] < -0.15);
        // The obstacle dead ahead, in the newest slice's middle sectors;
        // alone, so no second.
        let base = 8 + 3;
        assert!((obs[base + SECTORS / 2] - (0.55 / 2.2) as f32).abs() < 1e-4);
        assert_eq!(obs[base + SECTORS + SECTORS / 2], 1.0);
        // The wall row at y = 0.50-0.55 is 0.5 ahead: the local map's row
        // at 0.5 + 0.4.
        let local = base + SECTORS * 6;
        let i = ((0.5 + LOCAL_BEHIND_M) / LOCAL_CELL_M) as usize;
        assert_eq!(obs[local + i * LOCAL_N + LOCAL_N / 2], 1.0);
        // The book behind the body (y = -0.5, facing +y: 0.5 behind) is
        // out of the window (0.4 behind); a cell beside the body is free.
        assert_eq!(obs[local + 4 * LOCAL_N + LOCAL_N / 2], 0.0);
        assert_eq!(obs[local + LOCAL_N * LOCAL_N + Action::TurnLeft.index()], 1.0);
    }

    #[test]
    fn a_pilot_file_loads_and_picks() {
        // A one-layer net that always prefers the wait.
        let mut w = vec![vec![0.0f32; OBS_DIM]; N_ACTIONS];
        w[8][0] = 0.0;
        let mut b = vec![0.0f32; N_ACTIONS];
        b[8] = 1.0;
        let f = PilotFile {
            format: "quack-pilot".into(),
            obs_version: OBS_VERSION,
            obs_dim: OBS_DIM,
            n_actions: N_ACTIONS,
            obs_mean: vec![0.0; OBS_DIM],
            obs_std: vec![1.0; OBS_DIM],
            layers: vec![Layer { w, b, act: "none".into() }],
            meta: serde_json::Value::Null,
        };
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("pilot.json");
        std::fs::write(&p, serde_json::to_string(&f).unwrap()).unwrap();
        let pilot = Pilot::load(&p).unwrap();
        assert_eq!(pilot.act(&vec![0.3; OBS_DIM]), Action::Wait);
        let mut bad = f.clone();
        bad.obs_dim = 7;
        std::fs::write(&p, serde_json::to_string(&bad).unwrap()).unwrap();
        assert!(Pilot::load(&p).is_err());
    }
}
