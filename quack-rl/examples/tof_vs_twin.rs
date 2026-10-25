//! The simulator's depth frames beside the twin's: every frame of a twin
//! trace sensed again by `World::sense` at the trace's pose, in the twin
//! house's world (unmapped things included), one JSON line per frame.
//!
//!     tof_vs_twin --house H --calib FILE [--truth TRUTH.jsonl] TRACE.jsonl > pairs.jsonl
//!
//! `--truth`: the twin body's `TOF_TRUTH_LOG`, the true pose and sensor
//! axis per frame by wall time; each frame is then sensed at the true pose,
//! once with the calibration's pitch and once with the sensor's own.
use quack_rl::calib::Calib;
use quack_rl::Rng;
use serde_json::{json, Value};
use std::io::BufRead;

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let opt = |k: &str| a.iter().position(|x| x == k).and_then(|i| a.get(i + 1)).cloned();
    let house = opt("--house").expect("--house H");
    let mut calib = Calib::load(&opt("--calib").expect("--calib FILE"))?;
    // `--pitch-deg`: the sensor's pitch for the simulator, instead of the calibration's.
    if let Some(p) = opt("--pitch-deg").and_then(|v| v.parse::<f64>().ok()) {
        calib.tof_pitch_rad = Some(p.to_radians());
    }
    let trace = a.last().expect("TRACE.jsonl");
    // (unix_s, trunk x, y, yaw, sensor z, pitch)
    let truth: Vec<(f64, f64, f64, f64, f64, f64)> = match opt("--truth") {
        Some(f) => std::fs::read_to_string(f)?
            .lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .map(|v| {
                let ax = &v["axis"];
                let pitch = ax[2].as_f64().unwrap_or(0.0).clamp(-1.0, 1.0).asin();
                (v["unix_s"].as_f64().unwrap_or(0.0), v["trunk"][0].as_f64().unwrap_or(0.0), v["trunk"][1].as_f64().unwrap_or(0.0), v["trunk"][2].as_f64().unwrap_or(0.0), v["sensor"][2].as_f64().unwrap_or(0.0), pitch)
            })
            .collect(),
        None => Vec::new(),
    };
    let mut start_unix = 0.0;
    // The trace's start is whole seconds: the rest, found by `--offset`.
    let offset: f64 = opt("--offset").and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let s = quack_rl::scenarios::twin_house(&house, 1)?;
    let mut rng = Rng(7);
    let at = std::time::Instant::now();
    let p3 = |v: &Value| (v[0].as_f64().unwrap_or(0.0), v[1].as_f64().unwrap_or(0.0), v[2].as_f64().unwrap_or(0.0));
    for line in std::io::BufReader::new(std::fs::File::open(trace)?).lines() {
        let d: Value = serde_json::from_str(&line?)?;
        if d["k"] == "start" {
            start_unix = d["unix_s"].as_str().and_then(|v| v.parse().ok()).or_else(|| d["unix_s"].as_f64()).unwrap_or(0.0);
        }
        if d["k"] != "leg" && d["k"] != "stand" {
            continue;
        }
        let (p0, p1) = (p3(&d["pose0"]), p3(&d["pose1"]));
        let (t0, t1) = (d["t0"].as_f64().unwrap_or(0.0), d["t1"].as_f64().unwrap_or(0.0));
        for f in d["frames"].as_array().into_iter().flatten() {
            let t = f["t"].as_f64().unwrap_or(t0);
            let u = if t1 > t0 { ((t - t0) / (t1 - t0)).clamp(0.0, 1.0) } else { 0.0 };
            let dyaw = (p1.2 - p0.2 + std::f64::consts::PI).rem_euclid(2.0 * std::f64::consts::PI) - std::f64::consts::PI;
            let pose = (p0.0 + u * (p1.0 - p0.0), p0.1 + u * (p1.1 - p0.1), p0.2 + u * dyaw);
            let head = f["head"].as_f64().unwrap_or(0.0);
            let moving = f["moving"].as_bool().unwrap_or(false);
            let mut row = json!({"k": d["k"], "t": t, "pose": [pose.0, pose.1, pose.2], "head": head, "moving": moving, "twin": f["obs"], "twin_floors": f["floors"]});
            let mut sensed = |pose: (f64, f64, f64), calib: &Calib| {
                let f = s.world.sense(pose, head, moving, calib, &mut rng, 0, at);
                (f.obstacles.iter().map(|o| [o.bearing, o.range_m]).collect::<Vec<_>>(), f.floors.len())
            };
            let (sim, floors) = sensed(pose, &calib);
            row["sim"] = json!(sim);
            row["sim_floors"] = json!(floors);
            if !truth.is_empty() {
                let when = start_unix + t + offset;
                let i = truth.partition_point(|r| r.0 < when).min(truth.len() - 1);
                let r = truth[i];
                if (r.0 - when).abs() < 0.1 {
                    let tp = (r.1, r.2, r.3);
                    let mut own = calib.clone();
                    own.tof_pitch_rad = Some(r.5);
                    row["truth"] = json!([r.1, r.2, r.3, r.4, r.5]);
                    let ((sim, floors), (own, own_floors)) = (sensed(tp, &calib), sensed(tp, &own));
                    row["sim_truth"] = json!(sim);
                    row["sim_truth_floors"] = json!(floors);
                    row["sim_truth_pitch"] = json!(own);
                    row["sim_truth_pitch_floors"] = json!(own_floors);
                }
            }
            println!("{row}");
        }
    }
    Ok(())
}
