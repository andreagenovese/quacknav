//! The mapper, hosted here: `maploc` on its own worker thread, fed from
//! outside robotd.
//!
//! In the robotd fork this was `robotd/src/maploc.rs`, and the control
//! loop handed it one struct per tick over a channel. The released robotd
//! (daemon-v0.14.4, API 34) publishes everything that struct held on
//! `robot.state` — contact odometry, gravity, trunk height, the measured
//! head joints, `CLOCK_MONOTONIC` — so the same worker runs in this daemon
//! instead, with no change to robotd at all:
//!
//!   - [`feed`] subscribes to `robot.state` (every tick) and to tofd's
//!     `tof.stream`, and turns them into the fork's two events;
//!   - the worker below is the fork's, line for line: stillness, window
//!     vetting, the tracking watchdog, relocalization, the library;
//!   - [`server`] serves the result on a socket of its own in robotd's
//!     `robot.map*` dialect, so every caller of the fork's robotd — this
//!     crate's map lane, the homecoming, the twin's viewer — reads it
//!     unchanged;
//!   - [`sweep`] pans the head at stops, which robotd's loop used to do.
//!
//! Frames are reprojected through the head FK with the IMU-levelled floor
//! filter and handed to [`maploc::mapper::Mapper`], which owns every
//! mapping decision. This file only moves bytes: channel in, log lines and
//! map frames out, the session to disk, and (when `record_dir` is set) a
//! `.mdlg` recording of everything the mapper consumed.

pub mod feed;
pub mod server;
pub mod sweep;
pub mod wire;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use duck_ipc_proto as proto;
use kinematics::tof::{Posture, Reprojector};
use maploc::mapper::{Mapper, MapperConfig, MapperSample, Note};
use maploc::pipeline::{Slam, SlamConfig};
use maploc::record::SessionRecorder;
use maploc::session::SessionState;
use maploc::submap::Scan;

use crate::config::{MaplocConfig, MaplocMode};
use crate::map::MapFrame;
use wire::{MapAdoptParams, MapMatch, MapMatches, SavedMap};

/// The feed-side channel depth. Sized for ~2 s of ticks plus frames: if the
/// worker stalls longer than that (a relocalize search, say), dropping
/// samples is the correct behaviour — odometry deltas re-fold on the next
/// accepted sample; a dropped depth frame is one of fifteen a second.
const EVENT_BUFFER: usize = 128;

/// ST's status codes for a usable range — the same wire contract
/// `robotctl`'s monitor applies.
const TOF_STATUS_VALID: [u8; 2] = [5, 9];

/// Autosave cadence. Sessions are small (a few hundred KB) and the write is
/// atomic, but flash on the board is not free — once a minute is plenty for
/// a map that took minutes to walk.
const AUTOSAVE_EVERY: Duration = Duration::from_secs(60);

/// Map publish cadence when someone is subscribed.
const PUBLISH_EVERY: Duration = Duration::from_secs(1);

/// One tick's worth of the robot's own state, as `robot.state` carries it.
#[derive(Debug, Clone, Copy)]
pub struct OdomSample {
    /// Contact odometry x, y, yaw.
    pub odom: (f32, f32, f32),
    /// Projected gravity in the trunk frame.
    pub gravity: [f64; 3],
    /// Odometry's trunk height above the floor, metres.
    pub trunk_z: f64,
    /// `[neck_pitch, head_pitch, head_yaw, head_roll]`, measured.
    pub head: [f64; 4],
    /// `CLOCK_MONOTONIC` when the sensors were read, nanoseconds — the
    /// clock `TofFrame::t_ns` shares, so a depth frame can be paired with
    /// the head as it was when the frame was taken.
    pub t_ns: u64,
    /// The loop's own "the robot is doing something" verdict, rebuilt from
    /// what it publishes (see [`feed::moving`]).
    pub moving: bool,
    /// Seated. The mapper never maps from sitting height.
    pub sitting: bool,
    /// Fallen over (the safety layer's verdict).
    pub fallen: bool,
}

enum Event {
    Odom(OdomSample),
    Frame(Box<proto::TofFrame>),
    /// Reset everything: map, graph, tracked pose, suspicion — and delete
    /// the saved session.
    Wipe,
    /// Copy the live map into the library under a name, keeping it live.
    SaveAs(String, mpsc::SyncSender<Result<(), String>>),
    /// Replace the live map with a named one from the library, and start
    /// lost: a resumed map is a map to search, never a pose to trust.
    Load(String, mpsc::SyncSender<Result<(), String>>),
    /// Does the live map fit inside a saved one? Candidates, not a verdict.
    Match(Option<String>, mpsc::SyncSender<Result<MapMatches, String>>),
    /// Trade the live map for a saved one at a transform, keeping the
    /// robot's place in it.
    Adopt(MapAdoptParams, mpsc::SyncSender<Result<(), String>>),
    /// Save and stop; the ack says the session is on disk.
    Shutdown(mpsc::SyncSender<()>),
}

/// Whoever wants `map.frame`s: one bounded sender per `robot.map`
/// subscription. A subscriber that stops reading loses frames, not the
/// worker's time; one that hung up is dropped at the next publish.
#[derive(Clone, Default)]
pub struct Subscribers(Arc<Mutex<Vec<mpsc::SyncSender<MapFrame>>>>);

impl Subscribers {
    pub fn add(&self) -> mpsc::Receiver<MapFrame> {
        let (tx, rx) = mpsc::sync_channel(4);
        self.0.lock().expect("subscribers poisoned").push(tx);
        rx
    }

    fn any(&self) -> bool {
        !self.0.lock().expect("subscribers poisoned").is_empty()
    }

    fn send(&self, frame: &MapFrame) {
        self.0.lock().expect("subscribers poisoned").retain(|tx| {
            !matches!(tx.try_send(frame.clone()), Err(mpsc::TrySendError::Disconnected(_)))
        });
    }
}

/// Handle to the worker. Cheap to clone; the feeds, the server and the
/// sweep each hold one.
#[derive(Clone)]
pub struct Host {
    tx: mpsc::SyncSender<Event>,
    searching: Arc<AtomicBool>,
    /// The map library: a `maps/` directory beside the working session.
    maps: Arc<PathBuf>,
    mode: MaplocMode,
    /// Where rendered maps go.
    pub subscribers: Subscribers,
}

impl Host {
    /// Feed one `robot.state` tick. Never blocks: a full channel drops the
    /// sample, and the next one carries the newer truth anyway.
    pub fn observe(&self, sample: OdomSample) {
        let _ = self.tx.try_send(Event::Odom(sample));
    }

    /// Feed one depth frame.
    pub fn frame(&self, frame: proto::TofFrame) {
        let _ = self.tx.try_send(Event::Frame(Box::new(frame)));
    }

    /// Ask the worker to reset the mapping session. False when the channel
    /// is jammed.
    pub fn wipe(&self) -> bool {
        self.tx.try_send(Event::Wipe).is_ok()
    }

    /// Copy the live map into the library under `name`. Blocks until the
    /// worker has written it.
    pub fn save_as(&self, name: &str) -> Result<(), String> {
        self.ask(|ack| Event::SaveAs(name.to_string(), ack))
    }

    /// Adopt a named map from the library, lost inside it.
    pub fn load(&self, name: &str) -> Result<(), String> {
        self.ask(|ack| Event::Load(name.to_string(), ack))
    }

    /// Ask whether the live map sits inside a saved one. Runs on the
    /// mapper thread, so mapping pauses for the length of the search.
    pub fn matches(&self, name: Option<&str>) -> Result<MapMatches, String> {
        let (ack_tx, ack_rx) = mpsc::sync_channel(1);
        if self.tx.send(Event::Match(name.map(str::to_string), ack_tx)).is_err() {
            return Err("the mapper is not running".into());
        }
        // Generous: the search is a coarse-to-fine sweep of every saved
        // map, and on the robot's own processor that is seconds each.
        match ack_rx.recv_timeout(Duration::from_secs(120)) {
            Ok(r) => r,
            Err(_) => Err("the mapper did not answer in time".into()),
        }
    }

    /// Adopt a saved map at a transform, keeping the robot's place.
    pub fn adopt(&self, params: MapAdoptParams) -> Result<(), String> {
        self.ask(|ack| Event::Adopt(params.clone(), ack))
    }

    fn ask<F>(&self, event: F) -> Result<(), String>
    where
        F: FnOnce(mpsc::SyncSender<Result<(), String>>) -> Event,
    {
        let (ack_tx, ack_rx) = mpsc::sync_channel(1);
        if self.tx.send(event(ack_tx)).is_err() {
            return Err("the mapper is not running".into());
        }
        match ack_rx.recv_timeout(Duration::from_secs(10)) {
            Ok(r) => r,
            Err(_) => Err("the mapper did not answer in time".into()),
        }
    }

    /// What the library holds, read straight off the disk: the worker owns
    /// the map, not the directory, and a listing must answer while the
    /// worker is busy searching.
    pub fn list(&self) -> Vec<SavedMap> {
        let mut out = Vec::new();
        let Ok(dir) = std::fs::read_dir(self.maps.as_path()) else {
            return out; // no library yet is an empty library
        };
        for entry in dir.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("session") {
                continue;
            }
            let Some(name) = path.file_stem().and_then(|n| n.to_str()) else {
                continue;
            };
            let meta = entry.metadata().ok();
            out.push(SavedMap {
                name: name.to_string(),
                bytes: meta.as_ref().map(|m| m.len()).unwrap_or(0),
                saved_at: meta
                    .as_ref()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
            });
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// Save the session and stop the worker, waiting briefly for the disk.
    pub fn shutdown(&self) {
        let (ack_tx, ack_rx) = mpsc::sync_channel(1);
        if self.tx.send(Event::Shutdown(ack_tx)).is_ok() {
            let _ = ack_rx.recv_timeout(Duration::from_secs(3));
        }
    }

    /// Is the mapper's pose suspect or lost right now? The sweep reads it.
    pub fn searching(&self) -> bool {
        self.searching.load(Ordering::Relaxed)
    }

    pub fn mode(&self) -> MaplocMode {
        self.mode
    }
}

/// Start the worker, its two feeds and the head sweep. The server is the
/// caller's to start ([`server::serve`]), on whatever socket it chooses.
pub fn spawn(config: &MaplocConfig, robotd_socket: &str, tof_socket: &str) -> Host {
    let (tx, rx) = mpsc::sync_channel(EVENT_BUFFER);
    let map_path = PathBuf::from(&config.map_path);
    let maps = Arc::new(maps_dir(&map_path));
    let searching = Arc::new(AtomicBool::new(false));
    let subscribers = Subscribers::default();
    let host = Host { tx, searching: searching.clone(), maps, mode: config.mode, subscribers: subscribers.clone() };

    let worker_config = config.clone();
    std::thread::Builder::new()
        .name("maploc".into())
        .spawn(move || worker(&worker_config, rx, &subscribers, &searching))
        .expect("spawning the maploc thread cannot fail");

    let body = feed::spawn(host.clone(), robotd_socket.to_owned(), tof_socket.to_owned());
    if config.search_sweep {
        sweep::spawn(host.clone(), body, robotd_socket.to_owned());
    }
    host
}

/// The map library lives beside the working session, so an installation
/// that moves `map_path` moves its maps with it.
fn maps_dir(map_path: &Path) -> PathBuf {
    map_path.parent().unwrap_or_else(|| Path::new(".")).join("maps")
}

/// Every name the library holds.
fn saved_names(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("session"))
        .filter_map(|p| p.file_stem().and_then(|n| n.to_str()).map(str::to_string))
        .collect()
}

/// Where one named map is kept. The name has already been checked
/// ([`wire::valid_name`]), so it cannot climb out of the directory.
fn map_file(map_path: &Path, name: &str) -> PathBuf {
    maps_dir(map_path).join(format!("{name}.session"))
}

fn worker(config: &MaplocConfig, rx: mpsc::Receiver<Event>, map_tx: &Subscribers, searching: &AtomicBool) {
    let map_path = PathBuf::from(&config.map_path);
    if let Some(dir) = map_path.parent()
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        tracing::warn!(error = %e, dir = %dir.display(), "maploc: cannot create the session's directory");
    }
    let slam = if config.wipe_on_boot {
        tracing::info!("maploc: starting fresh (wipe_on_boot)");
        Slam::new(SlamConfig::default())
    } else {
        match SessionState::load(&map_path) {
            Ok(Some(session)) => {
                tracing::info!(path = %map_path.display(), "maploc: resumed saved session");
                Slam::from_session(SlamConfig::default(), session)
            }
            Ok(None) => Slam::new(SlamConfig::default()),
            Err(e) => {
                tracing::warn!(error = %e, "maploc: saved session unreadable; starting fresh");
                Slam::new(SlamConfig::default())
            }
        }
    };
    let mut mapper = Mapper::new(mapper_config(config), slam);

    if !mapper.tracking() {
        tracing::info!("maploc: resumed map with a suspect pose — confirming before anything inks");
    }

    let mut recorder = config.record_dir.as_ref().map(PathBuf::from).and_then(|dir| {
        if let Err(e) = std::fs::create_dir_all(&dir) {
            tracing::warn!(error = %e, dir = %dir.display(), "maploc: cannot create record_dir");
            return None;
        }
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let path = dir.join(format!("{stamp}.mdlg"));
        match SessionRecorder::create(&path) {
            Ok(rec) => {
                tracing::info!(path = %path.display(), "maploc: recording session");
                Some(rec)
            }
            Err(e) => {
                tracing::warn!(error = %e, "maploc: cannot open recording; not recording");
                None
            }
        }
    });

    let reprojector = Reprojector::alpha();
    let started = Instant::now();
    let mut latest: Option<OdomSample> = None;
    // The last two seconds of samples, for pairing a depth frame with the
    // head as it was at the frame's own time (see `head_at`).
    let mut recent: std::collections::VecDeque<OdomSample> = std::collections::VecDeque::with_capacity(128);
    let mut lag_sum_us: i64 = 0;
    let mut lag_n: u64 = 0;
    let mut pending: std::collections::VecDeque<Box<proto::TofFrame>> = std::collections::VecDeque::new();
    let mut notes: Vec<Note> = Vec::new();
    let mut last_publish = Instant::now();
    let mut last_save = Instant::now();
    let mut seq = 0u64;
    let mut unsaved = false;
    let mut rendered: Option<RenderedGrid> = None;
    let mut render_stale = true;
    // One line every 5 s says what mapping is actually doing.
    let mut last_status = Instant::now();
    let (mut n_odom, mut n_frames, mut n_frames_kept) = (0u64, 0u64, 0u64);

    searching.store(!mapper.tracking(), Ordering::Relaxed);

    // Blocking recv; the feeds hold senders for the life of the process,
    // so the loop ends on `Shutdown`.
    while let Ok(event) = rx.recv() {
        match event {
            Event::Odom(sample) => {
                if let Some(rec) = recorder.as_mut()
                    && rec
                        .odom(
                            sample.odom,
                            sample.gravity.map(|g| g as f32),
                            sample.trunk_z as f32,
                            sample.head.map(|h| h as f32),
                            sample.moving,
                            sample.sitting,
                            sample.fallen,
                        )
                        .is_err()
                {
                    tracing::warn!("maploc: recording write failed; recording stopped");
                    recorder = None;
                }
                mapper.observe(
                    started.elapsed().as_secs_f32(),
                    MapperSample {
                        odom: sample.odom,
                        moving: sample.moving,
                        sitting: sample.sitting,
                        fallen: sample.fallen,
                    },
                    &mut notes,
                );
                latest = Some(sample);
                recent.push_back(sample);
                while recent.len() > 128 {
                    recent.pop_front();
                }
                n_odom += 1;
                while let Some(frame) = pending.front() {
                    let bracketed = frame.t_ns.saturating_add(head_lead_ns()) <= sample.t_ns;
                    let stale = sample.t_ns.saturating_sub(frame.t_ns) > 200_000_000;
                    if !bracketed && !stale {
                        break;
                    }
                    let frame = pending.pop_front().expect("front seen");
                    ingest(
                        &frame,
                        &sample,
                        &recent,
                        &mut lag_sum_us,
                        &mut lag_n,
                        &reprojector,
                        &mut mapper,
                        started.elapsed().as_secs_f32(),
                        &mut n_frames_kept,
                    );
                }
            }
            Event::Shutdown(ack) => {
                if unsaved {
                    match mapper.slam().save(&map_path) {
                        Ok(()) => tracing::info!(path = %map_path.display(), "maploc: session saved"),
                        Err(e) => tracing::warn!(error = %e, "maploc: final save failed"),
                    }
                }
                if let Some(rec) = recorder.as_mut() {
                    let _ = rec.flush();
                }
                let _ = ack.send(());
                return;
            }
            Event::Wipe => {
                mapper = Mapper::new(mapper_config(config), Slam::new(SlamConfig::default()));
                if let Err(e) = std::fs::remove_file(&map_path)
                    && e.kind() != std::io::ErrorKind::NotFound
                {
                    tracing::warn!(error = %e, "maploc: wipe could not delete the session file");
                }
                unsaved = false;
                rendered = None;
                render_stale = true;
                tracing::info!("maploc: session wiped by request");
            }
            Event::SaveAs(name, ack) => {
                let path = map_file(&map_path, &name);
                let result = std::fs::create_dir_all(maps_dir(&map_path))
                    .map_err(|e| format!("cannot create the map library: {e}"))
                    .and_then(|()| mapper.slam().save(&path).map_err(|e| format!("cannot write the map: {e}")));
                match &result {
                    Ok(()) => tracing::info!(name, path = %path.display(), "maploc: map saved to the library"),
                    Err(e) => tracing::warn!(name, error = %e, "maploc: map not saved"),
                }
                let _ = ack.send(result);
            }
            Event::Load(name, ack) => {
                let path = map_file(&map_path, &name);
                let loaded = match SessionState::load(&path) {
                    Ok(Some(session)) => Ok(session),
                    // A missing file and an empty one read the same here,
                    // and the caller has to tell them apart.
                    Ok(None) if !path.exists() => Err(format!("no map is saved as \"{name}\"")),
                    Ok(None) => Err(format!("the map \"{name}\" is empty")),
                    Err(e) => Err(format!("cannot read the map \"{name}\": {e}")),
                };
                let result = match loaded {
                    Err(e) => Err(e),
                    Ok(session) => {
                        mapper =
                            Mapper::resumed_lost(mapper_config(config), Slam::from_session(SlamConfig::default(), session));
                        // The loaded map is now the live one: autosave must
                        // write it to the working path.
                        unsaved = true;
                        rendered = None;
                        render_stale = true;
                        notes.clear();
                        tracing::info!(
                            name,
                            submaps = mapper.slam().n_submaps(),
                            "maploc: map adopted from the library; searching for the pose"
                        );
                        Ok(())
                    }
                };
                if let Err(e) = &result {
                    tracing::warn!(name, error = %e, "maploc: map not loaded");
                }
                let _ = ack.send(result);
            }
            Event::Match(only, ack) => {
                let started = Instant::now();
                let result = match mapper.slam().render() {
                    None => Err("the robot has no map yet to compare".to_string()),
                    Some(live) => {
                        let cfg = maploc::align::AlignConfig::default();
                        let live_cells = maploc::align::wall_cells(&live, cfg.certain_log) as u32;
                        let names: Vec<String> = match &only {
                            Some(name) => vec![name.clone()],
                            None => saved_names(&maps_dir(&map_path)),
                        };
                        let mut matches = Vec::new();
                        for name in names {
                            let path = map_file(&map_path, &name);
                            let session = match SessionState::load(&path) {
                                Ok(Some(session)) => session,
                                Ok(None) => continue,
                                Err(e) => {
                                    tracing::warn!(name, error = %e, "maploc: unreadable saved map");
                                    continue;
                                }
                            };
                            let Some(mut saved) = Slam::from_session(SlamConfig::default(), session).render() else {
                                continue;
                            };
                            let found = maploc::align::match_maps(&live, &mut saved, &cfg);
                            if let Some(best) = found.first() {
                                matches.push(MapMatch {
                                    name,
                                    x: best.pose.0,
                                    y: best.pose.1,
                                    yaw: best.pose.2,
                                    wall_residual_m: best.wall_residual_m,
                                    overlap: best.overlap,
                                    floor_on_wall: best.floor_on_wall,
                                    score: best.score,
                                    margin: found.get(1).map(|next| best.score / next.score.max(1e-6)).unwrap_or(1.0),
                                });
                            }
                        }
                        matches.sort_by(|a, b| a.score.total_cmp(&b.score));
                        Ok(MapMatches { live_cells, matches, reason: None })
                    }
                };
                if let Ok(answer) = &result {
                    tracing::info!(
                        live_cells = answer.live_cells,
                        candidates = answer.matches.len(),
                        best = answer.matches.first().map(|m| m.name.as_str()).unwrap_or("-"),
                        took_s = format!("{:.1}", started.elapsed().as_secs_f32()),
                        "maploc: asked whether this map sits inside a saved one"
                    );
                }
                let _ = ack.send(result);
            }
            Event::Adopt(p, ack) => {
                let path = map_file(&map_path, &p.name);
                let result = match SessionState::load(&path) {
                    Ok(Some(session)) => {
                        // The transform carries a point on the live map to
                        // its twin on the saved one, so where the robot
                        // stands now is that transform applied to where it
                        // stands on the live map — composed here, so the
                        // robot may keep walking while the client decides.
                        let here = mapper.slam().tracked();
                        let pose = maploc::pose_graph::compose((p.x, p.y, p.yaw), here);
                        let mut slam = Slam::from_session(SlamConfig::default(), session);
                        // Before the mapper is built: a mapper made from a
                        // session arms its suspicion on the pose the slam
                        // holds at that moment.
                        slam.set_tracked(pose);
                        mapper = Mapper::new(mapper_config(config), slam);
                        if config.mode == MaplocMode::Localize {
                            tracing::info!("maploc: localize mode — the loaded map is frozen once the pose is confirmed on it");
                        }
                        unsaved = true;
                        rendered = None;
                        render_stale = true;
                        notes.clear();
                        tracing::info!(
                            name = p.name,
                            x = format!("{:.2}", pose.0),
                            y = format!("{:.2}", pose.1),
                            "maploc: saved map adopted; the robot keeps its place, unconfirmed"
                        );
                        Ok(())
                    }
                    Ok(None) if !path.exists() => Err(format!("no map is saved as \"{}\"", p.name)),
                    Ok(None) => Err(format!("the map \"{}\" is empty", p.name)),
                    Err(e) => Err(format!("cannot read the map \"{}\": {e}", p.name)),
                };
                if let Err(e) = &result {
                    tracing::warn!(name = p.name, error = %e, "maploc: map not adopted");
                }
                let _ = ack.send(result);
            }
            Event::Frame(frame) => {
                n_frames += 1;
                if let Some(rec) = recorder.as_mut()
                    && rec
                        .tof(frame.at_us as f64 / 1e6, frame.rows, frame.cols, &frame.distance_mm, &frame.status)
                        .is_err()
                {
                    tracing::warn!("maploc: recording write failed; recording stopped");
                    recorder = None;
                }
                if latest.is_none() {
                    continue;
                }
                // A frame newer than every odometry sample waits for the
                // next tick, so the head can be read at the frame's own
                // instant (see `head_at`).
                if frame.t_ns > 0 && recent.back().is_some_and(|s| s.t_ns < frame.t_ns.saturating_add(head_lead_ns())) {
                    pending.push_back(frame);
                    while pending.len() > 8 {
                        pending.pop_front();
                    }
                    continue;
                }
                let sample = latest.expect("checked above");
                ingest(
                    &frame,
                    &sample,
                    &recent,
                    &mut lag_sum_us,
                    &mut lag_n,
                    &reprojector,
                    &mut mapper,
                    started.elapsed().as_secs_f32(),
                    &mut n_frames_kept,
                );
            }
        }

        for note in notes.drain(..) {
            log_note(note);
        }
        if mapper.slam_mut().take_dirty() {
            unsaved = true;
            render_stale = true;
        }
        searching.store(!mapper.tracking(), Ordering::Relaxed);

        if last_status.elapsed() >= Duration::from_secs(5) {
            last_status = Instant::now();
            tracing::info!(
                boot = ?mapper.boot_search(),
                odom = n_odom,
                frames = n_frames,
                kept = n_frames_kept,
                windows = mapper.windows(),
                still = mapper.still(),
                tracking = mapper.tracking(),
                moving = latest.as_ref().is_some_and(|s| s.moving),
                sitting = latest.as_ref().is_some_and(|s| s.sitting),
                fallen = latest.as_ref().is_some_and(|s| s.fallen),
                window_frames = mapper.window_frames(),
                submaps = mapper.slam().n_submaps(),
                "maploc: status"
            );
            if let Some(rec) = recorder.as_mut()
                && rec.flush().is_err()
            {
                tracing::warn!("maploc: recording flush failed; recording stopped");
                recorder = None;
            }
        }

        if map_tx.any() && last_publish.elapsed() >= PUBLISH_EVERY {
            last_publish = Instant::now();
            // The grid re-renders only when the map changed; the pose,
            // tracking and posture fields ride every frame.
            if render_stale || rendered.is_none() {
                rendered = render_grid(&mapper);
                render_stale = false;
            }
            if let Some(grid) = &rendered {
                seq += 1;
                let seated = latest.as_ref().is_some_and(|s| s.sitting || s.fallen);
                map_tx.send(&frame_from(&mapper, grid, seq, seated));
            }
        }

        if unsaved && last_save.elapsed() >= AUTOSAVE_EVERY {
            last_save = Instant::now();
            match mapper.slam().save(&map_path) {
                Ok(()) => unsaved = false,
                Err(e) => tracing::warn!(error = %e, "maploc: autosave failed"),
            }
        }
    }
}

fn log_note(note: Note) {
    match note {
        Note::WindowIntegrated { beams, windows, mean_residual_m, n_observed, .. } => {
            tracing::info!(
                beams,
                windows,
                agree = format!("{mean_residual_m:.3}/{n_observed}"),
                "maploc: still window integrated"
            );
        }
        Note::WindowDiscarded { beams } => {
            tracing::debug!(beams, "maploc: window too thin to ink; discarded");
        }
        Note::WindowQuarantined { mean_residual_m, n_observed } => {
            tracing::info!(
                residual = format!("{mean_residual_m:.3}"),
                n_observed,
                "maploc: window contradicts the map; quarantined"
            );
        }
        Note::SuspectAfterSit => tracing::info!("maploc: robot sat — pose suspect until a window confirms it"),
        Note::SuspectAfterFall => tracing::info!("maploc: robot fell — pose suspect until a window confirms it"),
        Note::ResumedUnverified { pose } => {
            tracing::warn!(
                x = format!("{:.2}", pose.0),
                y = format!("{:.2}", pose.1),
                "maploc: nothing could judge the pose; resumed unverified"
            );
        }
        Note::RelocalizeCandidate { pose, mean_residual_m } => {
            tracing::info!(
                x = format!("{:.2}", pose.0),
                y = format!("{:.2}", pose.1),
                yaw = format!("{:.2}", pose.2),
                residual = format!("{mean_residual_m:.3}"),
                "maploc: relocalize candidate; awaiting confirmation"
            );
        }
        Note::LostTracking { mean_residual_m, n_observed } => {
            tracing::warn!(
                residual = format!("{mean_residual_m:.3}"),
                n_observed,
                "maploc: scans contradict the map here — tracking lost, searching"
            );
        }
        Note::Relocalized { pose, mean_residual_m } => {
            tracing::info!(
                x = format!("{:.2}", pose.0),
                y = format!("{:.2}", pose.1),
                yaw = format!("{:.2}", pose.2),
                residual = format!("{mean_residual_m:.3}"),
                "maploc: relocalized"
            );
        }
        Note::RelocalizeRejected { best_pose, mean_residual_m } => {
            tracing::debug!(
                x = format!("{:.2}", best_pose.0),
                y = format!("{:.2}", best_pose.1),
                residual = format!("{mean_residual_m:.3}"),
                "maploc: relocalize attempt rejected"
            );
        }
        Note::TrackingCorrected { dx, dy, dyaw, residual_before_m, residual_after_m, n_beams_used } => {
            tracing::info!(
                dx = format!("{dx:.3}"),
                dy = format!("{dy:.3}"),
                dyaw = format!("{dyaw:.3}"),
                residual = format!("{residual_before_m:.3}->{residual_after_m:.3}"),
                beams = n_beams_used,
                "maploc: tracked pose corrected against the map"
            );
        }
        Note::LoopClosed { n_loops, dx, dy, dyaw } => {
            tracing::info!(
                loops = n_loops,
                dx = format!("{dx:.3}"),
                dy = format!("{dy:.3}"),
                dyaw = format!("{dyaw:.3}"),
                "maploc: loop closed; tracked pose corrected"
            );
        }
    }
}

/// The wire frame's 64 zones as metres, `None` where the sensor said the
/// measurement is not to be trusted.
fn decode_ranges(frame: &proto::TofFrame) -> Option<[Option<f64>; maploc::flat::N_ZONES]> {
    const N: usize = maploc::flat::N_ZONES;
    if frame.distance_mm.len() != N || frame.status.len() != N {
        return None;
    }
    let mut out = [None; N];
    for (slot, (&mm, &status)) in out.iter_mut().zip(frame.distance_mm.iter().zip(frame.status.iter())) {
        if TOF_STATUS_VALID.contains(&status) && mm > 0 {
            *slot = Some(f64::from(mm) / 1000.0);
        }
    }
    Some(out)
}

/// A rendered composite, already trinarized and base64'd — everything in a
/// [`MapFrame`] that only changes when the map's ink does.
struct RenderedGrid {
    x_min: f32,
    y_min: f32,
    cell_m: f32,
    rows: u32,
    cols: u32,
    cells: String,
}

fn render_grid(mapper: &Mapper) -> Option<RenderedGrid> {
    let grid = mapper.slam().render()?;
    let mut cells = Vec::with_capacity(grid.width() * grid.height());
    for i in 0..grid.height() {
        for j in 0..grid.width() {
            let lo = grid.log_at(i, j);
            cells.push(if lo > 150 {
                2u8
            } else if lo < -50 {
                1
            } else {
                0
            });
        }
    }
    Some(RenderedGrid {
        x_min: grid.cfg().x_range.0,
        y_min: grid.cfg().y_range.0,
        cell_m: grid.cell(),
        rows: grid.height() as u32,
        cols: grid.width() as u32,
        cells: wire::b64_encode(&cells),
    })
}

/// The wire frame: the cached grid plus everything that moves every second.
fn frame_from(mapper: &Mapper, grid: &RenderedGrid, seq: u64, seated: bool) -> MapFrame {
    let (x, y, yaw) = mapper.slam().tracked();
    MapFrame {
        seq,
        x: f64::from(x),
        y: f64::from(y),
        yaw: f64::from(yaw),
        tracking: mapper.tracking() && mapper.slam().n_submaps() > 0,
        x_min: grid.x_min,
        y_min: grid.y_min,
        cell_m: grid.cell_m,
        rows: grid.rows,
        cols: grid.cols,
        cells: grid.cells.clone(),
        n_submaps: mapper.slam().n_submaps() as u32,
        n_loops: mapper.slam().n_loops() as u32,
        windows: mapper.windows(),
        still: mapper.still(),
        seated,
    }
}

/// The odometry sample as it was at `t_ns`: the two samples around that
/// instant, the head angles, gravity and height interpolated between
/// them, the rest from the later one. `None` when the frame carries no
/// time or the ring does not bracket it — the caller then falls back to
/// the latest sample. Also returns how far behind the frame the latest
/// sample was, microseconds, for the log.
fn head_at(recent: &std::collections::VecDeque<OdomSample>, t_ns: u64) -> Option<(OdomSample, i64)> {
    if t_ns == 0 || recent.len() < 2 {
        return None;
    }
    let last = recent.back()?;
    if last.t_ns < t_ns {
        return None;
    }
    let after = recent.iter().position(|s| s.t_ns >= t_ns)?;
    if after == 0 {
        return None;
    }
    let (a, b) = (&recent[after - 1], &recent[after]);
    let span = (b.t_ns - a.t_ns) as f64;
    let f = if span > 0.0 { (t_ns - a.t_ns) as f64 / span } else { 1.0 };
    let mix = |x: f64, y: f64| x + (y - x) * f;
    let mut paired = *b;
    for k in 0..4 {
        paired.head[k] = mix(a.head[k], b.head[k]);
    }
    for k in 0..3 {
        paired.gravity[k] = mix(a.gravity[k], b.gravity[k]);
    }
    paired.trunk_z = mix(a.trunk_z, b.trunk_z);
    Some((paired, (last.t_ns as i64 - t_ns as i64) / 1000))
}

/// One depth frame into the mapper, projected with the odometry sample of
/// its own instant when the ring brackets it, else with `sample`.
#[allow(clippy::too_many_arguments)]
fn ingest(
    frame: &proto::TofFrame,
    sample: &OdomSample,
    recent: &std::collections::VecDeque<OdomSample>,
    lag_sum_us: &mut i64,
    lag_n: &mut u64,
    reprojector: &Reprojector,
    mapper: &mut Mapper,
    t_s: f32,
    n_frames_kept: &mut u64,
) {
    let Some(ranges) = decode_ranges(frame) else {
        return;
    };
    // Pair the frame with the head this much later than its stamp — the
    // fixed latency between the depth being taken and `t_ns`.
    let lead_ns = head_lead_ns();
    let at = if frame.t_ns > 0 { frame.t_ns.saturating_add(lead_ns) } else { 0 };
    let sample = match head_at(recent, at) {
        Some((paired, lag_us)) => {
            *lag_sum_us += lag_us;
            *lag_n += 1;
            if *lag_n == 200 {
                tracing::info!(
                    mean_lag_ms = format!("{:.1}", *lag_sum_us as f64 / 200.0 / 1000.0),
                    "maploc: depth frames paired with the head at their own time"
                );
            }
            paired
        }
        None => *sample,
    };
    let posture = Posture { gravity: sample.gravity, trunk_height_m: (sample.trunk_z > 0.02).then_some(sample.trunk_z) };
    let flat = maploc::flat::flatten(reprojector, &ranges, sample.head, &posture);
    if flat.angles_body.is_empty() {
        return;
    }
    let scan = Scan::from_polar(&flat.angles_body, &flat.ranges, flat.sensor_xy, 1e-3);
    if mapper.frame(t_s, scan) {
        *n_frames_kept += 1;
    }
}

/// `MAPLOC_HEAD_LEAD_MS`, as robotd read it: 5 ms by default — with 0 the
/// standing drift was +0.17/+0.22°/min, with 20 and 40 it turned negative
/// (twin, 2026-09-16).
fn head_lead_ns() -> u64 {
    static V: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *V.get_or_init(|| {
        std::env::var("MAPLOC_HEAD_LEAD_MS")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .map(|ms| (ms.max(0.0) * 1e6) as u64)
            .unwrap_or(5_000_000)
    })
}

/// The mapper's config from `[maploc]` — one place, so a mode is not
/// forgotten by one of the four constructions (Localize was, 2026-09-16).
fn mapper_config(config: &MaplocConfig) -> MapperConfig {
    MapperConfig {
        continuous: config.mode == MaplocMode::Continuous,
        frozen: config.mode == MaplocMode::Localize,
        ..MapperConfig::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(t_ns: u64, yaw: f64) -> OdomSample {
        OdomSample {
            odom: (0.0, 0.0, 0.0),
            gravity: [0.0, 0.0, -1.0],
            trunk_z: 0.12,
            head: [0.0, 0.0, yaw, 0.0],
            t_ns,
            moving: false,
            sitting: false,
            fallen: false,
        }
    }

    #[test]
    fn a_frame_gets_the_head_of_its_own_instant() {
        let ring: std::collections::VecDeque<OdomSample> =
            [sample(1_000_000, 0.0), sample(21_000_000, 0.2), sample(41_000_000, 0.4)].into_iter().collect();
        let (paired, lag_us) = head_at(&ring, 11_000_000).expect("bracketed");
        assert!((paired.head[2] - 0.1).abs() < 1e-9, "{}", paired.head[2]);
        assert_eq!(lag_us, 30_000);
        let (paired, _) = head_at(&ring, 21_000_000).unwrap();
        assert!((paired.head[2] - 0.2).abs() < 1e-9);
        assert!(head_at(&ring, 0).is_none());
        assert!(head_at(&ring, 50_000_000).is_none());
        assert!(head_at(&ring, 500_000).is_none());
    }
}
