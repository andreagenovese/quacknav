//! `robot.map` client: the live occupancy map robotd's `maploc` worker
//! publishes (quacksat ADR 0005, docs/study/maploc-dataflow.md).
//!
//! Same padd model as quacksat's robotd client: one unprivileged NDJSON connection,
//! a `robot.map` subscription, then `map.frame` notifications at ~1 Hz for
//! as long as the peer lives. quacksat never touches the sensors; it reads
//! what robotd already decided.
//!
//! **Wire shape pinned to upstream PR 127 (API v17).** The `duck-ipc-proto`
//! release this workspace pins predates `robot.map`, so the two types live
//! here as a mirror of the PR's `MapStreamResult` and `MapFrame`, with the
//! PR's own `serde(default)` fields. When the release that carries them
//! ships, these become re-exports. A robotd older than that answers
//! `METHOD_NOT_FOUND`, which is reported as [`MapStreamEnd::Unsupported`]
//! rather than treated as a failure: the feature is simply off.
//!
//! Frames only flow while `[maploc]` is enabled on the robot; the
//! subscription is held either way (upstream's choice), so the read timeout
//! is generous.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use anyhow::Context;
use duck_ipc_proto as proto;
use serde::{Deserialize, Serialize};

/// The upstream API version whose `robot.map` shape this module mirrors.
pub const MAP_API_VERSION: u32 = 17;

/// Reconnect cadence on loss, the same fixed delay robotd's own clients
/// use.
pub const RECONNECT_DELAY: Duration = Duration::from_secs(2);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

fn connect(path: &str) -> std::io::Result<UnixStream> {
    // UnixStream has no connect_timeout; a missing socket fails fast and a
    // present-but-dead one is caught by the read timeout below.
    let stream = UnixStream::connect(path)?;
    stream.set_write_timeout(Some(CONNECT_TIMEOUT))?;
    Ok(stream)
}

fn write_line(writer: &mut impl Write, message: &impl serde::Serialize) -> std::io::Result<()> {
    let mut line = serde_json::to_vec(message)?;
    line.push(b'\n');
    writer.write_all(&line)?;
    writer.flush()
}

pub const METHOD_ROBOT_MAP: &str = "robot.map";
pub const METHOD_ROBOT_MAP_WIPE: &str = "robot.map_wipe";
pub const METHOD_MAP_FRAME: &str = "map.frame";

/// Frames come at 1 Hz while mapping runs and never while it is disabled;
/// a minute of silence on an enabled robot means the daemon is gone.
const FRAME_TIMEOUT: Duration = Duration::from_secs(60);
/// After `METHOD_NOT_FOUND`, how long before asking again. Only a daemon
/// upgrade changes the answer, and that restarts robotd anyway.
const UNSUPPORTED_RETRY: Duration = Duration::from_secs(300);

/// Answer to `robot.map` (upstream `MapStreamResult`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapStreamResult {
    /// The subscription is held either way; frames flow only if mapping is
    /// (or later becomes) enabled.
    pub accepted: bool,
    /// Whether `[maploc]` is enabled on this robot.
    pub enabled: bool,
    /// `stop_and_scan` or `continuous`, when enabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

/// One rendered occupancy map (upstream `MapFrame`), pushed at ~1 Hz.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapFrame {
    /// Renders since mapping started on this robotd.
    pub seq: u64,
    /// The robot's pose in the MAP frame — the odometry frame until a loop
    /// closure or a relocalization says otherwise.
    pub x: f64,
    pub y: f64,
    pub yaw: f64,
    /// False while the pose is not to be trusted (searching, or no map yet).
    pub tracking: bool,
    /// World coordinates of cell (0, 0)'s corner, and the cell pitch.
    pub x_min: f32,
    pub y_min: f32,
    pub cell_m: f32,
    /// `cells` decodes to `rows × cols` bytes, row-major, row 0 at `y_min`.
    pub rows: u32,
    pub cols: u32,
    /// Base64 of one byte per cell: 0 unknown, 1 free, 2 wall.
    pub cells: String,
    pub n_submaps: u32,
    pub n_loops: u32,
    /// Still-windows integrated so far — whether the robot's stops are
    /// actually reaching the map.
    #[serde(default)]
    pub windows: u32,
    /// The mapper currently believes the robot is standing still.
    #[serde(default)]
    pub still: bool,
    /// Seated or fallen: the mapper refuses to map or relocalize from the
    /// floor.
    #[serde(default)]
    pub seated: bool,
    /// The map is frozen (maploc `localize`, or frozen at runtime once the
    /// house is mapped, `quack.map_freeze`): nothing inks, the pose is
    /// corrected against the map as saved.
    #[serde(default)]
    pub frozen: bool,
}

impl MapFrame {
    /// `(x, y, yaw)` in the map frame.
    pub fn pose(&self) -> (f64, f64, f64) {
        (self.x, self.y, self.yaw)
    }

    /// Decode the grid. Cheap enough to call on demand; not cached because
    /// a frame is a value and most consumers only want the pose.
    pub fn grid(&self) -> anyhow::Result<Grid> {
        let bytes = b64_decode(&self.cells).context("map.frame cells are not base64")?;
        let expected = self.rows as usize * self.cols as usize;
        anyhow::ensure!(
            bytes.len() == expected,
            "map.frame cells: {} bytes for a {}×{} grid",
            bytes.len(),
            self.rows,
            self.cols
        );
        let cells = bytes
            .into_iter()
            .map(|b| match b {
                1 => Cell::Free,
                2 => Cell::Wall,
                _ => Cell::Unknown,
            })
            .collect();
        Ok(Grid {
            rows: self.rows as usize,
            cols: self.cols as usize,
            x_min: f64::from(self.x_min),
            y_min: f64::from(self.y_min),
            cell_m: f64::from(self.cell_m),
            cells,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cell {
    Unknown,
    Free,
    Wall,
}

/// A decoded trinary grid with its world placement.
#[derive(Debug, Clone, PartialEq)]
pub struct Grid {
    pub rows: usize,
    pub cols: usize,
    pub x_min: f64,
    pub y_min: f64,
    pub cell_m: f64,
    /// Row-major, row 0 at `y_min`.
    pub cells: Vec<Cell>,
}

/// How many rails [`Grid::lane_clear`] samples across the lane: 3 is the
/// old behaviour, anything more means every half cell. Read once.
fn lane_rails() -> usize {
    static V: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *V.get_or_init(|| {
        std::env::var("QK_LANE_RAILS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(usize::MAX)
    })
}

impl Grid {
    pub fn cell(&self, row: usize, col: usize) -> Option<Cell> {
        (row < self.rows && col < self.cols).then(|| self.cells[row * self.cols + col])
    }

    /// The cell under a world point, or `None` outside the grid.
    pub fn at(&self, x: f64, y: f64) -> Option<Cell> {
        let col = ((x - self.x_min) / self.cell_m).floor();
        let row = ((y - self.y_min) / self.cell_m).floor();
        if col < 0.0 || row < 0.0 {
            return None;
        }
        self.cell(row as usize, col as usize)
    }

    /// How far the known free space extends from `(x, y)` along `heading`
    /// (radians, map frame), up to `max_m`: a ray marched half a cell at a
    /// time until a wall, unknown territory or the grid's edge. The first
    /// cell is skipped — the robot's own cell is often unknown, because the
    /// sensor looks ahead, not down.
    pub fn clearance(&self, x: f64, y: f64, heading: f64, max_m: f64) -> Clearance {
        let step = self.cell_m / 2.0;
        let (dx, dy) = (heading.cos() * step, heading.sin() * step);
        let mut d = self.cell_m;
        while d <= max_m {
            let (px, py) = (x + d / step * dx, y + d / step * dy);
            match self.at(px, py) {
                Some(Cell::Free) => {}
                Some(Cell::Wall) => {
                    return Clearance {
                        free_m: (d - step).max(0.0),
                        by: Blocked::Wall,
                    };
                }
                Some(Cell::Unknown) => {
                    return Clearance {
                        free_m: (d - step).max(0.0),
                        by: Blocked::Unknown,
                    };
                }
                None => {
                    return Clearance {
                        free_m: (d - step).max(0.0),
                        by: Blocked::Edge,
                    };
                }
            }
            d += step;
        }
        Clearance {
            free_m: max_m,
            by: Blocked::Open,
        }
    }

    /// Whether a straight walk from `(x, y)` along `heading` for `len_m`
    /// meets no mapped wall within `half_w` of the line.
    ///
    /// The whole width is sampled, every half cell across as well as
    /// along. It used to be three rails — the centre and the two edges of
    /// the body — and with 5 cm cells and a 16 cm half-width that leaves an
    /// 8 cm gap on each side that no rail touches: a wall cell sitting at
    /// ±8 cm of the line, the width of a table leg or a door post, passed
    /// as clear. Measured on the twin (2026-09-13): three quarters of the
    /// stalls in a journey were within 35 cm of a piece of furniture and
    /// half of them on a leg going nearly straight — the duck walking into
    /// things the map had. `QK_LANE_RAILS=3` restores the three rails, for
    /// measuring what the full width is worth.
    pub fn lane_clear(&self, x: f64, y: f64, heading: f64, len_m: f64, half_w: f64) -> bool {
        self.lane_clear_with(lane_rails(), x, y, heading, len_m, half_w)
    }

    /// [`Grid::lane_clear`] with the rail count given, so a test can pin
    /// both samplings whatever `QK_LANE_RAILS` says in the shell.
    fn lane_clear_with(&self, rails: usize, x: f64, y: f64, heading: f64, len_m: f64, half_w: f64) -> bool {
        let (dx, dy) = (heading.cos(), heading.sin());
        let (nx, ny) = (-dy, dx);
        let step = self.cell_m / 2.0;
        // Offsets across the lane: three rails, or one every half cell.
        let across: Vec<f64> = if rails <= 3 {
            vec![-half_w, 0.0, half_w]
        } else {
            let n = ((2.0 * half_w) / step).ceil().max(2.0) as usize;
            (0..=n).map(|i| -half_w + i as f64 * (2.0 * half_w) / n as f64).collect()
        };
        let mut d = 0.0;
        while d <= len_m {
            for side in &across {
                let (px, py) = (x + d * dx + side * nx, y + d * dy + side * ny);
                if self.at(px, py) == Some(Cell::Wall) {
                    return false;
                }
            }
            d += step;
        }
        true
    }

    /// The share of cells within `radius_m` of `(x, y)` that are unknown;
    /// cells off the grid count as unknown.
    pub fn unknown_around(&self, x: f64, y: f64, radius_m: f64) -> f64 {
        self.unknown_share(x, y, radius_m, None)
    }

    /// As [`Grid::unknown_around`], but only the half-disc ahead of `yaw`.
    pub fn unknown_ahead(&self, x: f64, y: f64, yaw: f64, radius_m: f64) -> f64 {
        self.unknown_share(x, y, radius_m, Some(yaw))
    }

    fn unknown_share(&self, x: f64, y: f64, radius_m: f64, ahead_of: Option<f64>) -> f64 {
        let (mut unknown, mut total) = (0usize, 0usize);
        let r = (radius_m / self.cell_m).ceil() as i64;
        let (c0, r0) = (
            ((x - self.x_min) / self.cell_m).floor() as i64,
            ((y - self.y_min) / self.cell_m).floor() as i64,
        );
        for dr in -r..=r {
            for dc in -r..=r {
                if dr * dr + dc * dc > r * r {
                    continue;
                }
                if let Some(yaw) = ahead_of
                    && (dc as f64) * yaw.cos() + (dr as f64) * yaw.sin() < 0.0
                {
                    continue;
                }
                total += 1;
                let (rr, cc) = (r0 + dr, c0 + dc);
                let known = rr >= 0
                    && cc >= 0
                    && !matches!(self.cell(rr as usize, cc as usize), None | Some(Cell::Unknown));
                if !known {
                    unknown += 1;
                }
            }
        }
        if total == 0 { 1.0 } else { unknown as f64 / total as f64 }
    }

    pub fn counts(&self) -> (usize, usize, usize) {
        self.cells.iter().fold((0, 0, 0), |(u, f, w), c| match c {
            Cell::Unknown => (u + 1, f, w),
            Cell::Free => (u, f + 1, w),
            Cell::Wall => (u, f, w + 1),
        })
    }
}

/// What ended a [`Grid::clearance`] ray.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blocked {
    /// A mapped wall.
    Wall,
    /// The map does not know what is there (unexplored, or never in view).
    Unknown,
    /// The rendered grid ends here.
    Edge,
    /// Nothing within `max_m`.
    Open,
}

impl Blocked {
    pub fn as_str(self) -> &'static str {
        match self {
            Blocked::Wall => "wall",
            Blocked::Unknown => "unknown",
            Blocked::Edge => "edge",
            Blocked::Open => "open",
        }
    }
}

/// Free distance along a heading and what stops it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Clearance {
    pub free_m: f64,
    pub by: Blocked,
}

/// Standard base64 (RFC 4648, padded), as upstream spells it. Owned here so
/// reading a map costs no dependency.
pub fn b64_decode(input: &str) -> Option<Vec<u8>> {
    fn value(c: u8) -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => u32::from(c - b'A'),
            b'a'..=b'z' => u32::from(c - b'a') + 26,
            b'0'..=b'9' => u32::from(c - b'0') + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        })
    }
    let bytes = input.trim_end_matches('=').as_bytes();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        if chunk.len() == 1 {
            return None;
        }
        let mut acc = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            acc |= value(c)? << (18 - 6 * i);
        }
        out.push((acc >> 16) as u8);
        if chunk.len() > 2 {
            out.push((acc >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(acc as u8);
        }
    }
    Some(out)
}

/// What one connection-lifetime of the map stream delivers.
#[derive(Debug, Clone, PartialEq)]
pub enum MapEvent {
    /// The subscribe ack: whether this robot maps at all, and how.
    Subscribed(MapStreamResult),
    Frame(Box<MapFrame>),
}

/// Why [`run_map_stream`] returned without an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapStreamEnd {
    /// The receiver hung up.
    ReceiverGone,
    /// robotd answered `METHOD_NOT_FOUND`: it predates `robot.map`. Not a
    /// failure — the caller should stop retrying for a good while.
    Unsupported,
}

/// One connection-lifetime of the map lane: subscribe, then forward events
/// until the peer goes away. `Err` means the connection died and the caller
/// should sleep [`RECONNECT_DELAY`] and call again.
pub fn run_map_stream(path: &str, tx: &mpsc::Sender<MapEvent>) -> anyhow::Result<MapStreamEnd> {
    let stream = connect(path).with_context(|| format!("connecting to robotd at {path}"))?;
    stream.set_read_timeout(Some(FRAME_TIMEOUT))?;
    let mut writer = stream.try_clone()?;
    let mut reader = BufReader::new(stream);

    // Built by hand: the pinned proto release has no `Call::RobotMap`.
    let subscribe = proto::Request {
        jsonrpc: "2.0".to_owned(),
        id: Some(proto::Id::Number(0)),
        method: METHOD_ROBOT_MAP.to_owned(),
        params: Some(serde_json::Value::Object(serde_json::Map::new())),
    };
    write_line(&mut writer, &subscribe)?;

    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            anyhow::bail!("robotd closed the map stream");
        }
        if let Ok(request) = serde_json::from_str::<proto::Request>(&line) {
            if request.method == METHOD_MAP_FRAME
                && let Some(params) = request.params
                && let Ok(frame) = serde_json::from_value::<MapFrame>(params)
                && tx.send(MapEvent::Frame(Box::new(frame))).is_err()
            {
                return Ok(MapStreamEnd::ReceiverGone);
            }
            continue;
        }
        let Ok(response) = serde_json::from_str::<proto::Response>(&line) else {
            continue;
        };
        if let Some(error) = &response.error {
            if error.code == proto::code::METHOD_NOT_FOUND {
                return Ok(MapStreamEnd::Unsupported);
            }
            anyhow::bail!("robotd refused {METHOD_ROBOT_MAP}: {error}");
        }
        let ack: MapStreamResult = response
            .result_as()
            .with_context(|| format!("unparsable {METHOD_ROBOT_MAP} answer: {}", line.trim()))?;
        if tx.send(MapEvent::Subscribed(ack)).is_err() {
            return Ok(MapStreamEnd::ReceiverGone);
        }
    }
}

/// What the robot said about mapping, as far as we know.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum MapSupport {
    /// No answer yet (not connected, or waiting for the ack).
    #[default]
    Unknown,
    /// robotd predates `robot.map`.
    Unsupported,
    /// `robot.map` exists; `enabled` says whether frames will come.
    Supported { enabled: bool, mode: Option<String> },
}

/// The newest word from the map lane, shared with whoever needs a pose.
#[derive(Debug, Clone, Default)]
pub struct MapStatus {
    pub support: MapSupport,
    pub latest: Option<MapFrame>,
    /// When `latest` arrived.
    pub received_at: Option<Instant>,
    /// Frames received since quacksat started.
    pub frames: u64,
    /// Bumped whenever the map frame may no longer be the one earlier
    /// poses were expressed in — a wipe, or a boot that started a fresh
    /// map. Two things must hold at once: a **new mapper** (`windows` or
    /// `seq` went backwards: `windows` resets with every `Mapper`, `seq`
    /// with every robotd) **and fewer submaps** than before. A robotd
    /// restart that restores its saved session is a new mapper with the
    /// same submaps, and keeps the epoch: the frame is the one it was.
    /// (A wipe alone does not reset `seq`, and a frame never carries zero
    /// submaps, so neither is a signal by itself.) There is no session id
    /// on the wire; this is the evidence we have. Anything anchored to the
    /// map (the places registry) records the epoch it was taught in and
    /// double-checks with its own persisted submap high-water mark.
    pub epoch: u64,
}

impl MapStatus {
    /// Fold one frame in; returns true when the epoch changed.
    pub fn absorb(&mut self, frame: MapFrame, now: Instant) -> bool {
        let reset = match &self.latest {
            Some(prev) => {
                let new_mapper = frame.windows < prev.windows || frame.seq < prev.seq;
                new_mapper && frame.n_submaps < prev.n_submaps
            }
            None => false,
        };
        if reset {
            self.epoch += 1;
        }
        self.latest = Some(frame);
        self.received_at = Some(now);
        self.frames += 1;
        reset
    }

    /// The pose, only while the mapper vouches for it.
    pub fn trusted_pose(&self) -> Option<(f64, f64, f64)> {
        self.latest
            .as_ref()
            .filter(|f| f.tracking && !f.seated)
            .map(MapFrame::pose)
    }
}

/// A background subscriber that keeps [`MapStatus`] current, reconnecting
/// on loss like the other lanes. Cheap to clone and hand to a tool.
#[derive(Clone)]
pub struct MapWatch {
    inner: Arc<Mutex<MapStatus>>,
}

impl MapWatch {
    /// Start the lane against `path`. The thread runs for the life of the
    /// process; dropping every handle does not stop it (the map is
    /// advisory, and a detached reader costs one idle socket).
    pub fn spawn(path: String) -> Self {
        let inner = Arc::new(Mutex::new(MapStatus::default()));
        let shared = inner.clone();
        std::thread::Builder::new()
            .name("robot-map".into())
            .spawn(move || watch_loop(&path, &shared))
            .expect("spawning the map watcher cannot fail");
        Self { inner }
    }

    /// A watcher fed by tests or by a replay, never connecting anywhere.
    pub fn detached() -> Self {
        Self {
            inner: Arc::new(Mutex::new(MapStatus::default())),
        }
    }

    pub fn snapshot(&self) -> MapStatus {
        self.inner.lock().expect("map status poisoned").clone()
    }

    pub fn latest(&self) -> Option<MapFrame> {
        self.inner
            .lock()
            .expect("map status poisoned")
            .latest
            .clone()
    }

    /// Feed one event by hand (replays, tests).
    pub fn push(&self, event: MapEvent) {
        apply(&mut self.inner.lock().expect("map status poisoned"), event);
    }
}

fn apply(status: &mut MapStatus, event: MapEvent) {
    match event {
        MapEvent::Subscribed(ack) => {
            status.support = MapSupport::Supported {
                enabled: ack.enabled,
                mode: ack.mode,
            };
        }
        MapEvent::Frame(frame) => {
            let seq = frame.seq;
            let first = status.frames == 0;
            if status.absorb(*frame, Instant::now()) {
                tracing::info!(epoch = status.epoch, seq, "robot.map: map frame reset");
            } else if first {
                tracing::info!(seq, "robot.map: first frame");
            }
        }
    }
}

fn watch_loop(path: &str, shared: &Arc<Mutex<MapStatus>>) {
    loop {
        let (tx, rx) = mpsc::channel();
        let shared_rx = shared.clone();
        let pump = std::thread::spawn(move || {
            for event in rx {
                if let MapEvent::Subscribed(ack) = &event {
                    tracing::info!(
                        enabled = ack.enabled,
                        mode = ack.mode.as_deref().unwrap_or("-"),
                        "robot.map: subscribed"
                    );
                }
                apply(&mut shared_rx.lock().expect("map status poisoned"), event);
            }
        });
        let outcome = run_map_stream(path, &tx);
        drop(tx);
        let _ = pump.join();
        match outcome {
            Ok(MapStreamEnd::Unsupported) => {
                shared.lock().expect("map status poisoned").support = MapSupport::Unsupported;
                tracing::info!(
                    "robot.map: not on this robotd (predates API v{MAP_API_VERSION}); mapping off"
                );
                std::thread::sleep(UNSUPPORTED_RETRY);
            }
            Ok(MapStreamEnd::ReceiverGone) => return,
            Err(e) => {
                tracing::debug!(error = %e, "robot.map: stream lost; reconnecting");
                shared.lock().expect("map status poisoned").support = MapSupport::Unknown;
                std::thread::sleep(RECONNECT_DELAY);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    fn b64_encode(bytes: &[u8]) -> String {
        const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let mut acc = 0u32;
            for (i, &b) in chunk.iter().enumerate() {
                acc |= u32::from(b) << (16 - 8 * i);
            }
            for i in 0..4 {
                if i <= chunk.len() {
                    out.push(A[((acc >> (18 - 6 * i)) & 63) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    fn frame(seq: u64, n_submaps: u32, cells: &[u8], rows: u32, cols: u32) -> MapFrame {
        MapFrame {
            seq,
            x: 0.1,
            y: 0.2,
            yaw: 0.3,
            tracking: true,
            x_min: -1.0,
            y_min: -1.0,
            cell_m: 0.5,
            rows,
            cols,
            cells: b64_encode(cells),
            n_submaps,
            n_loops: 0,
            windows: 3,
            still: true,
            seated: false,
            frozen: false,
        }
    }

    #[test]
    fn base64_matches_the_rfc_vectors() {
        assert_eq!(b64_decode(""), Some(vec![]));
        assert_eq!(b64_decode("Zg=="), Some(b"f".to_vec()));
        assert_eq!(b64_decode("Zm8="), Some(b"fo".to_vec()));
        assert_eq!(b64_decode("Zm9v"), Some(b"foo".to_vec()));
        assert_eq!(b64_decode("Zm9vYmFy"), Some(b"foobar".to_vec()));
        assert_eq!(b64_decode("Z"), None);
        assert_eq!(b64_decode("Zm9v!"), None);
        let all: Vec<u8> = (0..=255).collect();
        assert_eq!(b64_decode(&b64_encode(&all)), Some(all));
    }

    #[test]
    fn grid_decodes_and_places_cells_in_the_world() {
        // 2 rows × 3 cols, row 0 at y_min: unknown free wall / wall wall free
        let f = frame(1, 1, &[0, 1, 2, 2, 2, 1], 2, 3);
        let g = f.grid().unwrap();
        assert_eq!(g.counts(), (1, 2, 3));
        assert_eq!(g.cell(0, 0), Some(Cell::Unknown));
        assert_eq!(g.cell(0, 2), Some(Cell::Wall));
        assert_eq!(g.cell(1, 2), Some(Cell::Free));
        assert_eq!(g.cell(2, 0), None);
        // World lookup: cell (row 1, col 2) spans x ∈ [0, 0.5), y ∈ [-0.5, 0).
        assert_eq!(g.at(0.25, -0.25), Some(Cell::Free));
        assert_eq!(g.at(-0.75, -0.75), Some(Cell::Unknown));
        assert_eq!(g.at(-1.5, 0.0), None);
        assert_eq!(g.at(0.0, 5.0), None);
    }

    #[test]
    fn clearance_marches_to_the_first_wall_or_unknown() {
        // 1 row × 8 cols at 0.5 m: free free free wall free unknown free free
        let f = frame(1, 1, &[1, 1, 1, 2, 1, 0, 1, 1], 1, 8);
        let g = f.grid().unwrap();
        // From the middle of cell 0 heading +x: cells 1, 2 free, wall at 3.
        let c = g.clearance(-0.75, -0.75, 0.0, 5.0);
        assert_eq!(c.by, Blocked::Wall);
        assert!((c.free_m - 1.25).abs() < 0.3, "{c:?}");
        // From cell 4 heading +x: cell 5 is unknown.
        let c = g.clearance(1.25, -0.75, 0.0, 5.0);
        assert_eq!(c.by, Blocked::Unknown);
        assert!(c.free_m < 0.6, "{c:?}");
        // Heading -x from cell 2: cells 1, 0 free, then the edge.
        let c = g.clearance(0.25, -0.75, std::f64::consts::PI, 5.0);
        assert_eq!(c.by, Blocked::Edge);
        // Capped by max_m.
        let c = g.clearance(-0.75, -0.75, 0.0, 0.6);
        assert_eq!(c.by, Blocked::Open);
        assert_eq!(c.free_m, 0.6);
    }

    #[test]
    fn a_short_grid_is_an_error_not_a_panic() {
        let f = frame(1, 1, &[0, 1], 2, 3);
        assert!(f.grid().is_err());
    }

    #[test]
    fn frame_parses_without_the_newer_fields() {
        let json = serde_json::json!({
            "seq": 7, "x": 1.0, "y": 2.0, "yaw": 0.5, "tracking": false,
            "x_min": -3.0, "y_min": -3.0, "cell_m": 0.05, "rows": 1, "cols": 1,
            "cells": "AA==", "n_submaps": 2, "n_loops": 0
        });
        let f: MapFrame = serde_json::from_value(json).unwrap();
        assert_eq!(f.windows, 0);
        assert!(!f.still && !f.seated);
        assert_eq!(f.grid().unwrap().counts(), (1, 0, 0));
    }

    #[test]
    fn status_tracks_epoch_and_trust() {
        let mut s = MapStatus::default();
        let now = Instant::now();
        let mut f1 = frame(1, 1, &[0], 1, 1);
        f1.windows = 2;
        assert!(!s.absorb(f1, now), "the first frame is never a reset");
        let mut f2 = frame(2, 3, &[0], 1, 1);
        f2.windows = 9;
        assert!(!s.absorb(f2, now));
        assert_eq!(s.epoch, 0);
        assert_eq!(s.trusted_pose(), Some((0.1, 0.2, 0.3)));

        // A wipe: a new mapper (windows back to zero; seq goes on) with the
        // map shrunk to its bootstrap submap.
        let mut wiped = frame(3, 1, &[0], 1, 1);
        wiped.windows = 0;
        assert!(s.absorb(wiped, now));
        assert_eq!(s.epoch, 1);

        let mut grown = frame(4, 5, &[0], 1, 1);
        grown.windows = 4;
        assert!(!s.absorb(grown, now));
        // A robotd restart that restored its session: new mapper, same
        // submaps — the frame is the one it was, no bump.
        let mut restored = frame(1, 5, &[0], 1, 1);
        restored.windows = 0;
        assert!(!s.absorb(restored, now));
        assert_eq!(s.epoch, 1);
        let mut later = frame(2, 5, &[0], 1, 1);
        later.windows = 3;
        assert!(!s.absorb(later, now));
        // A restart on a fresh map (wipe_on_boot, or no session file):
        // seq and windows both start over, and the map is one submap.
        let mut fresh = frame(1, 1, &[0], 1, 1);
        fresh.windows = 0;
        assert!(s.absorb(fresh, now));
        assert_eq!(s.epoch, 2);
        assert_eq!(s.frames, 7);

        let mut seated = frame(2, 1, &[0], 1, 1);
        seated.seated = true;
        s.absorb(seated, now);
        assert_eq!(s.trusted_pose(), None);
        let mut lost = frame(3, 1, &[0], 1, 1);
        lost.tracking = false;
        s.absorb(lost, now);
        assert_eq!(s.trusted_pose(), None);
    }

    #[test]
    fn map_stream_subscribes_then_delivers_frames() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("robotd.sock");
        let listener = UnixListener::bind(&socket).unwrap();

        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let request: proto::Request = serde_json::from_str(&line).unwrap();
            assert_eq!(request.method, METHOD_ROBOT_MAP);
            assert!(!request.is_notification());

            let mut writer = stream;
            let ack = MapStreamResult {
                accepted: true,
                enabled: true,
                mode: Some("stop_and_scan".into()),
            };
            write_line(&mut writer, &proto::Response::ok(request.id, &ack)).unwrap();
            for seq in 1..=2 {
                let notify = proto::Request {
                    jsonrpc: "2.0".into(),
                    id: None,
                    method: METHOD_MAP_FRAME.into(),
                    params: Some(serde_json::to_value(frame(seq, 1, &[2], 1, 1)).unwrap()),
                };
                write_line(&mut writer, &notify).unwrap();
            }
            // Something else on the wire must be ignored, not fatal.
            let other = proto::Request {
                jsonrpc: "2.0".into(),
                id: None,
                method: "robot.state".into(),
                params: Some(serde_json::json!({"unrelated": true})),
            };
            write_line(&mut writer, &other).unwrap();
        });

        let (tx, rx) = mpsc::channel();
        let result = run_map_stream(socket.to_str().unwrap(), &tx);
        assert!(result.is_err(), "server close must be an error");
        let events: Vec<MapEvent> = rx.try_iter().collect();
        assert_eq!(events.len(), 3);
        assert!(matches!(&events[0], MapEvent::Subscribed(a) if a.enabled));
        assert!(matches!(&events[2], MapEvent::Frame(f) if f.seq == 2));

        let watch = MapWatch::detached();
        for event in events {
            watch.push(event);
        }
        let status = watch.snapshot();
        assert_eq!(
            status.support,
            MapSupport::Supported {
                enabled: true,
                mode: Some("stop_and_scan".into())
            }
        );
        assert_eq!(status.frames, 2);
        assert_eq!(watch.latest().unwrap().grid().unwrap().counts(), (0, 0, 1));
        server.join().unwrap();
    }

    #[test]
    fn an_old_robotd_is_unsupported_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("robotd.sock");
        let listener = UnixListener::bind(&socket).unwrap();

        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let request: proto::Request = serde_json::from_str(&line).unwrap();
            let mut writer = stream;
            write_line(
                &mut writer,
                &proto::Response::err(
                    request.id,
                    proto::Error::new(
                        proto::code::METHOD_NOT_FOUND,
                        "unknown method \"robot.map\"",
                    ),
                ),
            )
            .unwrap();
        });

        let (tx, _rx) = mpsc::channel();
        let result = run_map_stream(socket.to_str().unwrap(), &tx).unwrap();
        assert_eq!(result, MapStreamEnd::Unsupported);
        server.join().unwrap();
    }

    /// The hole the three rails left: a single wall cell inside a 16 cm
    /// half-width lane, between the centre rail and the edge rail — row 21
    /// spans y ∈ [1.05, 1.10), the rails of a lane at y = 1.00 sit at rows
    /// 16, 20 and 23. A table leg is that wide. The full width has to see
    /// it, and the three rails are shown missing it, which is the whole
    /// point. Both samplings are called by name, so the shell's
    /// `QK_LANE_RAILS` cannot turn this test red.
    #[test]
    fn a_table_leg_between_the_rails_blocks_the_lane() {
        let (rows, cols, cell_m) = (40, 40, 0.05);
        let mut cells = vec![Cell::Free; rows * cols];
        cells[21 * cols + 20] = Cell::Wall; // x ∈ [1.00, 1.05), y ∈ [1.05, 1.10)
        let grid = Grid { rows, cols, x_min: 0.0, y_min: 0.0, cell_m, cells };
        let (full, three) = (usize::MAX, 3);
        // Walking along +x at y = 1.00, the leg sits 5–10 cm to the left.
        assert!(!grid.lane_clear_with(full, 0.5, 1.0, 0.0, 1.0, 0.16), "the full width must see the leg");
        assert!(grid.lane_clear_with(three, 0.5, 1.0, 0.0, 1.0, 0.16), "the three rails miss it: that is the hole");
        // The width stops at half_w, no further: a lane whose edge ends one
        // cell short of the leg is clear, one whose edge reaches it is not.
        assert!(grid.lane_clear_with(full, 0.5, 0.88, 0.0, 1.0, 0.16), "edge at y = 1.04, row 20: clear");
        assert!(!grid.lane_clear_with(full, 0.5, 0.90, 0.0, 1.0, 0.16), "edge at y = 1.06, row 21: blocked");
    }
}
