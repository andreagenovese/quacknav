//! `QK_RL_TRACE`: what the duck did and saw, leg by leg, for the
//! calibration (`quack-rl`'s `rl_calib`, docs/rl-pilot.md).
//!
//! One JSON line per event in `<dir>/trace-<unix seconds>.jsonl`: every
//! move of the stick or the pilot with the command, the map's pose and
//! odometry before and after, and the depth frames judged meanwhile; every
//! stand the same way (a stand is where the mapper corrects the pose, and
//! where the sensor looks at still walls); the map once, and again when
//! it changed and a minute has passed. Nothing is read back here: the file
//! is what the simulator's numbers are fitted to.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::cliff::{CliffFrame, DropKind};
use crate::explore::Body;

/// The map again no more often than this (unless it is another map).
const MAP_EVERY: Duration = Duration::from_secs(60);

/// Which map: its frame's sequence, its shape and placement.
type MapKey = (u64, u32, u32, i64, i64);

pub struct Tracer {
    out: Mutex<BufWriter<File>>,
    origin: Mutex<Option<Instant>>,
    last_map: Mutex<Option<(MapKey, Instant)>>,
    pub path: PathBuf,
}

/// The tracer, when `QK_RL_TRACE` names a directory it can write to.
pub fn tracer() -> Option<&'static Tracer> {
    static T: OnceLock<Option<Tracer>> = OnceLock::new();
    T.get_or_init(|| {
        let dir = std::env::var("QK_RL_TRACE").ok().filter(|d| !d.is_empty())?;
        let dir = PathBuf::from(dir);
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let path = dir.join(format!("trace-{stamp}.jsonl"));
        let file = std::fs::create_dir_all(&dir).and_then(|_| File::create(&path));
        match file {
            Ok(f) => {
                tracing::info!(path = %path.display(), "rl trace: recording the legs for the calibration");
                let t = Tracer { out: Mutex::new(BufWriter::new(f)), origin: Mutex::new(None), last_map: Mutex::new(None), path };
                t.line(&json!({"k": "start", "unix_s": stamp, "obs_version": super::OBS_VERSION, "build": env!("CARGO_PKG_VERSION")}));
                Some(t)
            }
            Err(e) => {
                tracing::warn!(error = %e, dir = %dir.display(), "rl trace: cannot write; not recording");
                None
            }
        }
    })
    .as_ref()
}

/// The body's state at an instant: the map's pose, odometry's, the clock.
#[derive(Debug, Clone, Copy)]
pub struct Snapshot {
    pub at: Instant,
    pub pose: Option<(f64, f64, f64)>,
    pub odom: Option<(f64, f64, f64)>,
}

impl Snapshot {
    pub fn take(robot: &dyn Body) -> Self {
        let odom = robot.cliff().and_then(|c| Some((c.odom_xy?.0, c.odom_xy?.1, c.odom_yaw?)));
        Self { at: robot.now(), pose: robot.frame().map(|f| f.pose()), odom }
    }
}

fn triple(p: Option<(f64, f64, f64)>) -> Value {
    p.map_or(Value::Null, |(x, y, a)| json!([round(x), round(y), round(a)]))
}

fn round(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

impl Tracer {
    fn line(&self, v: &Value) {
        if let Ok(mut out) = self.out.lock() {
            let _ = writeln!(out, "{v}");
            let _ = out.flush();
        }
    }

    fn t(&self, at: Instant) -> f64 {
        let mut o = self.origin.lock().unwrap_or_else(|e| e.into_inner());
        let origin = *o.get_or_insert(at);
        round(at.saturating_duration_since(origin).as_secs_f64())
    }

    fn frame_json(&self, f: &CliffFrame) -> Value {
        json!({
            "t": self.t(f.at),
            "head": round(f.head_yaw),
            "moving": f.moving,
            "obs": f.obstacles.iter().map(|o| [round(o.bearing), round(o.range_m)]).collect::<Vec<_>>(),
            "drops": f.drops.iter().map(|d| json!([round(d.bearing), round(d.range_m), round(d.edge_min_m), if d.kind == DropKind::Deep { "deep" } else { "missing" }])).collect::<Vec<_>>(),
            "floors": f.floors.len(),
        })
    }

    /// The map, when it is new to the trace (see [`MAP_EVERY`]).
    fn map_if_due(&self, robot: &dyn Body) {
        let Some(frame) = robot.frame() else { return };
        let now = robot.now();
        let mut last = self.last_map.lock().unwrap_or_else(|e| e.into_inner());
        let key: MapKey = (frame.seq, frame.rows, frame.cols, (frame.x_min * 1000.0) as i64, (frame.y_min * 1000.0) as i64);
        let due = match *last {
            None => true,
            Some((k, at)) => (k.1, k.2, k.3, k.4) != (key.1, key.2, key.3, key.4) || (k.0 != key.0 && now.saturating_duration_since(at) >= MAP_EVERY),
        };
        if due {
            *last = Some((key, now));
            drop(last);
            self.line(&json!({
                "k": "map", "t": self.t(now), "seq": frame.seq, "frozen": robot.frozen_map(),
                "x_min": frame.x_min, "y_min": frame.y_min, "cell_m": frame.cell_m,
                "rows": frame.rows, "cols": frame.cols, "cells": frame.cells,
            }));
        }
    }

    /// The duck down (seated or fallen) during a journey: once per fall,
    /// for the calibration's falls per bump.
    pub fn fall(&self, robot: &dyn Body) {
        let now = robot.now();
        self.line(&json!({"k": "fall", "t": self.t(now), "pose": triple(robot.frame().map(|f| f.pose()))}));
    }

    /// One event from `before` to now: `kind` is `leg` or `stand`, `what`
    /// the command and who chose it.
    pub fn event(&self, robot: &dyn Body, kind: &str, before: Snapshot, what: Value) {
        self.map_if_due(robot);
        let after = Snapshot::take(robot);
        let frames: Vec<Value> = robot
            .cliff()
            .map(|c| c.recent.iter().filter(|f| f.at >= before.at && f.at <= after.at).map(|f| self.frame_json(f)).collect())
            .unwrap_or_default();
        self.line(&json!({
            "k": kind,
            "t0": self.t(before.at),
            "t1": self.t(after.at),
            "pose0": triple(before.pose),
            "pose1": triple(after.pose),
            "odom0": triple(before.odom),
            "odom1": triple(after.odom),
            "what": what,
            "frames": frames,
        }));
    }
}
