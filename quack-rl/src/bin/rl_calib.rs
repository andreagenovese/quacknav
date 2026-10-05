//! rl_calib: the simulator fitted to the duck's own traces.
//!
//! quack-navd with `QK_RL_TRACE=<dir>` writes every leg of a journey — the
//! command, the map's pose and odometry before and after, the depth frames
//! meanwhile — and every stand (`quack_nav::rlnav::trace`). From those this
//! measures what the simulator assumes, number by number:
//!
//! - the gait: forward speed per step, the yaw a curving step turns, the
//!   veer of a straight one, the turn in place each way, the back-off;
//! - the pose: odometry's drift against the map's pose;
//! - the depth sensor: its rate, its range noise and bias against the map
//!   (a ray cast on the saved map from the pose at a stand), its dropouts,
//!   its phantom drops on known floor.
//!
//! What the traces cannot tell (too few samples, or not identifiable) keeps
//! its prior, and the report says so. Then it replays every step leg
//! through the simulator's gait with the prior and with the fit, and
//! reports how far each lands from what the duck did.
//!
//!     rl_calib [--prior calib.json] [--out DIR] TRACE.jsonl...
//!
//! Writes DIR/calib.json (`{"calib": {...}, "fit": {...}, "replay": {...}}`,
//! what `rl_env`/`rl_eval --calib` read) and DIR/calib.md.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use quack_rl::calib::Calib;
use quack_rl::world::{COL_FOV, COLS};
use serde_json::{Value, json};

/// Fewer samples than this and a number keeps its prior.
const MIN_N: usize = 8;

#[derive(Clone)]
struct MapGrid {
    x0: f64,
    y0: f64,
    cell: f64,
    rows: usize,
    cols: usize,
    codes: Vec<u8>,
}

impl MapGrid {
    fn at(&self, x: f64, y: f64) -> Option<u8> {
        let c = ((x - self.x0) / self.cell).floor();
        let r = ((y - self.y0) / self.cell).floor();
        (c >= 0.0 && r >= 0.0 && (c as usize) < self.cols && (r as usize) < self.rows).then(|| self.codes[r as usize * self.cols + c as usize])
    }

    /// The first wall along a ray, or `None` within `max` (also `None`
    /// when the ray leaves the known floor first: nothing to compare).
    fn ray(&self, x: f64, y: f64, a: f64, max: f64) -> Option<f64> {
        let mut d = 0.05;
        while d <= max {
            match self.at(x + d * a.cos(), y + d * a.sin()) {
                Some(2) => return Some(d),
                Some(1) => {}
                _ => return None,
            }
            d += 0.01;
        }
        None
    }

    /// Whether (x, y) is known floor with known floor all round within `r`.
    fn floor_around(&self, x: f64, y: f64, r: f64) -> bool {
        let k = (r / self.cell).ceil() as i64;
        for i in -k..=k {
            for j in -k..=k {
                if self.at(x + i as f64 * self.cell, y + j as f64 * self.cell) != Some(1) {
                    return false;
                }
            }
        }
        true
    }
}

fn b64_decode(s: &str) -> Vec<u8> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => u32::from(c - b'A'),
            b'a'..=b'z' => u32::from(c - b'a') + 26,
            b'0'..=b'9' => u32::from(c - b'0') + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        })
    };
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let bytes: Vec<u8> = s.bytes().filter(|c| *c != b'\n').collect();
    for chunk in bytes.chunks(4) {
        let mut n = 0u32;
        let mut k: usize = 0;
        for c in chunk {
            if let Some(v) = val(*c) {
                n = (n << 6) | v;
                k += 1;
            }
        }
        n <<= 6 * (4 - k);
        for i in 0..k.saturating_sub(1) {
            out.push((n >> (16 - 8 * i)) as u8);
        }
    }
    out
}

fn triple(v: &Value) -> Option<(f64, f64, f64)> {
    let a = v.as_array()?;
    Some((a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2)?.as_f64()?))
}

fn wrap(a: f64) -> f64 {
    quack_rl::wrap(a)
}

fn median(v: &mut [f64]) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(f64::total_cmp);
    let n = v.len();
    if n % 2 == 1 { v[n / 2] } else { 0.5 * (v[n / 2 - 1] + v[n / 2]) }
}

/// A robust spread: 1.4826 × the median absolute deviation.
fn mad_sd(v: &[f64]) -> f64 {
    let mut c = v.to_vec();
    let m = median(&mut c);
    let mut d: Vec<f64> = v.iter().map(|x| (x - m).abs()).collect();
    1.4826 * median(&mut d)
}

struct Leg {
    act: String,
    vx: f64,
    vyaw: f64,
    secs: f64,
    dur: f64,
    pose0: (f64, f64, f64),
    pose1: (f64, f64, f64),
    odom0: Option<(f64, f64, f64)>,
    odom1: Option<(f64, f64, f64)>,
    /// The nearest obstacle seen ahead during it (a bump's suspect).
    near_ahead: f64,
    /// Legs between two stands share a chain.
    chain: usize,
}

struct StandRec {
    pose1: (f64, f64, f64),
    frames: Vec<Value>,
    map: Option<usize>,
}

#[derive(Default)]
struct Fit {
    rows: BTreeMap<&'static str, Value>,
}

impl Fit {
    fn set(&mut self, name: &'static str, prior: f64, fitted: Option<(f64, usize, f64)>, why: &str) -> f64 {
        match fitted {
            Some((v, n, sd)) if n >= MIN_N && v.is_finite() => {
                self.rows.insert(name, json!({"prior": prior, "value": v, "n": n, "sd": sd, "fitted": true, "how": why}));
                v
            }
            Some((_, n, _)) => {
                self.rows.insert(name, json!({"prior": prior, "value": prior, "n": n, "fitted": false, "how": format!("{why} — too few samples ({n} < {MIN_N}): prior kept")}));
                prior
            }
            None => {
                self.rows.insert(name, json!({"prior": prior, "value": prior, "n": 0, "fitted": false, "how": why}));
                prior
            }
        }
    }
}

/// The gait model's answer to one leg, noise off (the simulator's
/// `Sim::walk`, its mean): displacement and turn.
fn predict(c: &Calib, act: &str, vx: f64, vyaw: f64, secs: f64) -> (f64, f64) {
    if act.starts_with("turn") {
        let rate = c.in_place_rate(vyaw).unwrap_or(0.0);
        return (0.0, rate * secs);
    }
    if vx > 0.05 {
        let gain = if secs <= 0.8 && vyaw.abs() > 0.3 { c.pulse_gain_mean } else { 1.0 };
        let w = c.yaw_per_unit * vyaw * gain + if vyaw.abs() < 0.1 { c.straight_veer } else { 0.0 };
        return (c.speed_at_03 * vx / 0.3 * secs, w * secs);
    }
    if vx < -0.05 {
        return (c.back_speed * secs, 0.6 * vyaw * secs);
    }
    (0.0, 0.0)
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut prior_path = None;
    let mut out_dir = std::path::PathBuf::from("calib-out");
    let mut files = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--prior" => {
                prior_path = args.get(i + 1).cloned();
                i += 1;
            }
            "--out" => {
                out_dir = args.get(i + 1).map(Into::into).unwrap_or(out_dir);
                i += 1;
            }
            f => files.push(f.to_string()),
        }
        i += 1;
    }
    anyhow::ensure!(!files.is_empty(), "usage: rl_calib [--prior calib.json] [--out DIR] TRACE.jsonl...");
    let prior = match &prior_path {
        Some(p) => Calib::load(p)?,
        None => Calib::default(),
    };
    let mut maps: Vec<MapGrid> = Vec::new();
    let mut legs: Vec<Leg> = Vec::new();
    let mut stands: Vec<StandRec> = Vec::new();
    let mut frame_dts: Vec<f64> = Vec::new();
    let mut events = 0usize;
    let mut falls = 0usize;
    let mut chain = 0usize;
    for f in &files {
        let text = std::fs::read_to_string(f)?;
        let mut cur_map: Option<usize> = None;
        chain += 1;
        for line in text.lines() {
            let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
            match v.get("k").and_then(Value::as_str) {
                Some("map") => {
                    let codes = b64_decode(v["cells"].as_str().unwrap_or(""));
                    let (rows, cols) = (v["rows"].as_u64().unwrap_or(0) as usize, v["cols"].as_u64().unwrap_or(0) as usize);
                    if codes.len() == rows * cols && rows > 0 {
                        maps.push(MapGrid { x0: v["x_min"].as_f64().unwrap_or(0.0), y0: v["y_min"].as_f64().unwrap_or(0.0), cell: v["cell_m"].as_f64().unwrap_or(0.05), rows, cols, codes });
                        cur_map = Some(maps.len() - 1);
                        chain += 1;
                    }
                }
                Some("fall") => {
                    falls += 1;
                    chain += 1;
                }
                Some(kind @ ("leg" | "stand")) => {
                    events += 1;
                    let frames: Vec<Value> = v["frames"].as_array().cloned().unwrap_or_default();
                    let ts: Vec<f64> = frames.iter().filter_map(|f| f["t"].as_f64()).collect();
                    frame_dts.extend(ts.windows(2).map(|w| w[1] - w[0]).filter(|d| *d > 0.0 && *d < 0.5));
                    let (Some(pose0), Some(pose1)) = (triple(&v["pose0"]), triple(&v["pose1"])) else { continue };
                    if kind == "stand" {
                        chain += 1;
                        if v["what"]["secs"].as_f64().unwrap_or(0.0) >= 1.5 {
                            stands.push(StandRec { pose1, frames, map: cur_map });
                        }
                        continue;
                    }
                    let w = &v["what"];
                    let act = w["act"].as_str().unwrap_or("").to_string();
                    let near_ahead = frames
                        .iter()
                        .flat_map(|f| f["obs"].as_array().cloned().unwrap_or_default())
                        .filter_map(|o| {
                            let a = o.as_array()?;
                            let (b, r) = (a.first()?.as_f64()?, a.get(1)?.as_f64()?);
                            (b.abs() < 0.3).then_some(r)
                        })
                        .fold(f64::INFINITY, f64::min);
                    legs.push(Leg {
                        act,
                        vx: w["vx"].as_f64().unwrap_or(0.0),
                        vyaw: w["vyaw"].as_f64().unwrap_or(0.0),
                        secs: w["secs"].as_f64().unwrap_or(0.0),
                        dur: v["t1"].as_f64().unwrap_or(0.0) - v["t0"].as_f64().unwrap_or(0.0),
                        pose0,
                        pose1,
                        odom0: triple(&v["odom0"]),
                        odom1: triple(&v["odom1"]),
                        near_ahead,
                        chain,
                    });
                }
                _ => {}
            }
        }
    }
    let mut fit = Fit::default();
    let mut c = prior.clone();
    // --- the gait, from the steps the body walked freely (nothing within
    // 0.35 m ahead, and it moved: a bump is not the gait).
    let free_steps: Vec<&Leg> = legs.iter().filter(|l| l.act.starts_with("step") && l.vx > 0.05 && l.secs > 0.0 && l.near_ahead > 0.35).collect();
    let speeds: Vec<f64> = free_steps
        .iter()
        .map(|l| (l.pose1.0 - l.pose0.0).hypot(l.pose1.1 - l.pose0.1) / l.secs * 0.3 / l.vx)
        .filter(|s| *s > 0.02)
        .collect();
    let sp = { let mut s = speeds.clone(); median(&mut s) };
    let sp_sd = mad_sd(&speeds);
    c.speed_at_03 = fit.set("speed_at_03", prior.speed_at_03, Some((sp, speeds.len(), sp_sd)), "median forward speed of the free steps, by the map's pose, scaled to vx 0.3");
    // The map's pose jitters at both ends of a step: its variance off.
    let step_m = sp * 0.6;
    let jit = (2.0 * prior.pose_jitter_m.powi(2)).sqrt() / step_m.max(1e-6);
    let rel = ((sp_sd / sp.max(1e-6)).powi(2) - jit.powi(2)).max(0.0).sqrt();
    c.speed_sd = fit.set("speed_sd", prior.speed_sd, Some((rel, speeds.len(), 0.0)), "their spread (MAD), relative, less the map pose's jitter at both ends");
    // The yaw, by odometry where it is (the map's lags a turn), else the map's.
    let dyaw = |l: &Leg| match (l.odom0, l.odom1) {
        (Some(a), Some(b)) => wrap(b.2 - a.2),
        _ => wrap(l.pose1.2 - l.pose0.2),
    };
    let straight: Vec<f64> = free_steps.iter().filter(|l| l.vyaw.abs() < 0.1).map(|l| dyaw(l) / l.secs).collect();
    let veer = { let mut s = straight.clone(); median(&mut s) };
    c.straight_veer = fit.set("straight_veer", prior.straight_veer, Some((veer, straight.len(), mad_sd(&straight))), "median yaw rate of the straight steps (|vyaw| < 0.1)");
    let pulses: Vec<f64> = free_steps.iter().filter(|l| l.vyaw.abs() > 0.3 && l.secs <= 0.8).map(|l| dyaw(l) / l.secs / (c.yaw_per_unit * l.vyaw)).collect();
    let g = { let mut s = pulses.clone(); median(&mut s) };
    c.pulse_gain_mean = fit.set("pulse_gain_mean", prior.pulse_gain_mean, Some((g, pulses.len(), mad_sd(&pulses))), "median of (yaw turned) / (yaw_per_unit × vyaw × secs) over the curving steps ≤ 0.8 s");
    c.pulse_gain_sd = fit.set("pulse_gain_sd", prior.pulse_gain_sd, Some((mad_sd(&pulses), pulses.len(), 0.0)), "their spread (MAD)");
    let long: Vec<f64> = free_steps.iter().filter(|l| l.vyaw.abs() > 0.3 && l.secs > 0.8).map(|l| dyaw(l) / l.secs / l.vyaw).collect();
    let ypu = { let mut s = long.clone(); median(&mut s) };
    c.yaw_per_unit = fit.set("yaw_per_unit", prior.yaw_per_unit, Some((ypu, long.len(), mad_sd(&long))), "median yaw rate per unit of vyaw over curving legs longer than 0.8 s (the pilot's and the stick's steps are 0.6 s: rarely seen)");
    // Turns in place: the rate over the move's own duration.
    let turns = |sign: f64| -> Vec<f64> {
        legs.iter().filter(|l| l.act.starts_with("turn") && l.vx.abs() < 0.05 && l.vyaw * sign >= 1.45 && l.dur > 0.1).map(|l| dyaw(l) * sign / l.dur).filter(|r| *r > 0.0).collect()
    };
    let (tl, tr) = (turns(1.0), turns(-1.0));
    let (ml, mr) = ({ let mut s = tl.clone(); median(&mut s) }, { let mut s = tr.clone(); median(&mut s) });
    c.turn_left_rad_s = fit.set("turn_left_rad_s", prior.turn_left_rad_s, Some((ml, tl.len(), mad_sd(&tl))), "median rate of the turns in place to the left (vyaw ≥ 1.45), by odometry over the turn's duration");
    c.turn_right_rad_s = fit.set("turn_right_rad_s", prior.turn_right_rad_s, Some((mr, tr.len(), mad_sd(&tr))), "the same to the right");
    let all_turns: Vec<f64> = tl.iter().map(|r| r / ml).chain(tr.iter().map(|r| r / mr)).collect();
    c.turn_sd = fit.set("turn_sd", prior.turn_sd, Some((mad_sd(&all_turns), all_turns.len(), 0.0)), "their spread, relative");
    let backs: Vec<f64> = legs.iter().filter(|l| l.vx < -0.05 && l.secs > 0.0).map(|l| (l.pose1.0 - l.pose0.0).hypot(l.pose1.1 - l.pose0.1) / l.secs).collect();
    let bk = { let mut s = backs.clone(); median(&mut s) };
    c.back_speed = fit.set("back_speed", prior.back_speed, Some((bk, backs.len(), mad_sd(&backs))), "median speed of the back-offs");
    // --- odometry against the map's pose over the walks between two
    // stands: the simulator's per-tick random walk (k × the tick's metres)
    // leaves an error of variance k² · D · d_tick over D walked, the
    // map pose's jitter adds 2 j² at the two ends.
    let mut chains: BTreeMap<usize, Vec<&Leg>> = BTreeMap::new();
    for l in legs.iter().filter(|l| l.odom0.is_some() && l.odom1.is_some()) {
        chains.entry(l.chain).or_default().push(l);
    }
    let d_tick = c.speed_at_03 * 0.1;
    let mut ks = Vec::new();
    for ch in chains.values() {
        let (Some(a), Some(b)) = (ch.first(), ch.last()) else { continue };
        let walked: f64 = ch.iter().map(|l| (l.pose1.0 - l.pose0.0).hypot(l.pose1.1 - l.pose0.1)).sum();
        if walked < 0.2 || ch.iter().any(|l| l.act.starts_with("turn")) {
            continue;
        }
        let (o0, o1) = (a.odom0.unwrap(), b.odom1.unwrap());
        // Odometry's displacement in the map's frame: rotated by the
        // frames' yaw difference at the start.
        let rot = wrap(a.pose0.2 - o0.2);
        let (dx, dy) = (o1.0 - o0.0, o1.1 - o0.1);
        let (ox, oy) = (rot.cos() * dx - rot.sin() * dy, rot.sin() * dx + rot.cos() * dy);
        let (mx, my) = (b.pose1.0 - a.pose0.0, b.pose1.1 - a.pose0.1);
        let e2 = ((ox - mx).powi(2) + (oy - my).powi(2)) / 2.0;
        ks.push((e2, walked));
    }
    // Both poses drift alike in the simulator (odometry, and the map's
    // between its corrections): the difference's variance is twice one.
    let noise = 2.0 * prior.pose_jitter_m.powi(2);
    let mut e2s: Vec<f64> = ks.iter().map(|(e, _)| *e).collect();
    let me2 = median(&mut e2s);
    if ks.len() >= MIN_N && me2 > 2.0 * noise {
        let mut k2: Vec<f64> = ks.iter().map(|(e, w)| ((e - noise) / (2.0 * w * d_tick)).max(0.0)).collect();
        let k = median(&mut k2).sqrt();
        c.odom_xy_per_m = fit.set("odom_xy_per_m", prior.odom_xy_per_m, Some((k, ks.len(), 0.0)), "odometry against the map's pose over the walks between stands (no turn): per-axis variance less the map pose's jitter, halved (both drift), per metre and tick");
    } else {
        fit.set("odom_xy_per_m", prior.odom_xy_per_m, None, &format!("odometry against the map's pose over {} walks between stands: the difference ({:.1} mm) is within the map pose's own noise — not identifiable, prior kept", ks.len(), me2.sqrt() * 1000.0));
    }
    fit.set("stand_keep", prior.stand_keep, None, "not identifiable from the traces (the map's own error is not seen): prior kept");
    fit.set("pose_jitter_m", prior.pose_jitter_m, None, "not identifiable from the traces: prior kept");
    // --- the sensor.
    let hz = { let mut d = frame_dts.clone(); 1.0 / median(&mut d) };
    c.tof_hz = fit.set("tof_hz", prior.tof_hz, Some((hz, frame_dts.len(), 0.0)), "1 / the median gap between frames");
    let mut residuals = Vec::new();
    let (mut expected_cols, mut seen_cols, mut phantoms, mut floor_beams) = (0usize, 0usize, 0usize, 0usize);
    for s in &stands {
        let Some(m) = s.map.and_then(|i| maps.get(i)) else { continue };
        let (x, y, yaw) = s.pose1;
        for f in &s.frames {
            let head = f["head"].as_f64().unwrap_or(0.0);
            let obs: Vec<(f64, f64)> = f["obs"].as_array().into_iter().flatten().filter_map(|o| Some((o.get(0)?.as_f64()?, o.get(1)?.as_f64()?))).collect();
            for col in 0..COLS {
                let b = head + COL_FOV * ((col as f64 + 0.5) / COLS as f64 - 0.5);
                // The face is inside the first wall cell: half a cell on.
                let Some(exp) = m.ray(x, y, yaw + b, 1.8).map(|d| d + m.cell / 2.0) else { continue };
                expected_cols += 1;
                if let Some((_, r)) = obs.iter().find(|(ob, _)| (ob - b).abs() < 0.03) {
                    seen_cols += 1;
                    let res = r - exp;
                    if res.abs() < 0.3 {
                        residuals.push(res);
                    }
                }
            }
            floor_beams += f["floors"].as_u64().unwrap_or(0) as usize;
            for d in f["drops"].as_array().into_iter().flatten() {
                let (Some(b), Some(r)) = (d.get(0).and_then(Value::as_f64), d.get(1).and_then(Value::as_f64)) else { continue };
                let (px, py) = (x + r * (yaw + b).cos(), y + r * (yaw + b).sin());
                if m.floor_around(px, py, 0.3) {
                    phantoms += 1;
                }
            }
        }
    }
    let bias = { let mut r = residuals.clone(); median(&mut r) };
    let sd = mad_sd(&residuals);
    c.tof_range_bias = fit.set("tof_range_bias", prior.tof_range_bias, Some((bias, residuals.len(), sd)), "median of (range − the map's ray) at the stands, inliers within 0.3 m");
    c.tof_range_sd = fit.set("tof_range_sd", prior.tof_range_sd, Some((sd, residuals.len(), 0.0)), "their spread (MAD)");
    let dropout = if expected_cols > 0 { 1.0 - seen_cols as f64 / expected_cols as f64 } else { f64::NAN };
    c.tof_dropout = fit.set("tof_dropout", prior.tof_dropout, Some((dropout, expected_cols, 0.0)), "columns whose ray meets a mapped wall within 1.8 m with no return at its bearing");
    let pany = if floor_beams + phantoms > 0 { phantoms as f64 / (floor_beams + phantoms) as f64 } else { f64::NAN };
    c.phantom_any_p = fit.set("phantom_any_p", prior.phantom_any_p, Some((pany, floor_beams + phantoms, 0.0)), "drops on known floor (0.3 m of floor all round) per floor row judged, at the stands");
    fit.set("phantom_p", prior.phantom_p, None, "low furniture's phantoms are not told apart from the others in a trace: prior kept");
    fit.set("tof_low_walk_m", prior.tof_low_walk_m, None, "not identifiable from the traces: prior kept");
    // --- falls per bump: a bump is a forward step that moved the body
    // less than 40 % of a step with something within 0.25 m ahead.
    let bumps = legs
        .iter()
        .filter(|l| l.act.starts_with("step") && l.vx > 0.05 && l.near_ahead < 0.25)
        .filter(|l| (l.pose1.0 - l.pose0.0).hypot(l.pose1.1 - l.pose0.1) < 0.4 * c.speed_at_03 * l.secs * l.vx / 0.3)
        .count();
    if bumps >= 20 {
        // No fall in n bumps: under 1/(2n) as the estimate (half the rule
        // of three's bound would be too sure).
        let p = if falls > 0 { falls as f64 / bumps as f64 } else { 0.5 / bumps as f64 };
        let ratio = prior.post_fall_p / prior.bump_fall_p.max(1e-9);
        c.bump_fall_p = fit.set("bump_fall_p", prior.bump_fall_p, Some((p, bumps, 0.0)), &format!("{falls} falls in {bumps} bumps (forward steps that barely moved with something within 0.25 m ahead)"));
        c.post_fall_p = (c.bump_fall_p * ratio).min(0.5);
        fit.rows.insert("post_fall_p", json!({"prior": prior.post_fall_p, "value": c.post_fall_p, "n": bumps, "fitted": true, "how": "bump_fall_p × the prior's ratio of posts to boxes (a trace does not tell what was met)"}));
    } else {
        fit.set("bump_fall_p", prior.bump_fall_p, Some((f64::NAN, bumps, 0.0)), &format!("{falls} falls in {bumps} bumps"));
    }
    // --- the replay: every step leg through the gait, prior against fit.
    let replay = |cal: &Calib| -> (f64, f64, usize) {
        let (mut ed, mut ey, mut n) = (0.0, 0.0, 0usize);
        for l in legs.iter().filter(|l| (l.act.starts_with("step") && l.near_ahead > 0.35) || l.act.starts_with("turn") || l.act == "back") {
            let secs = if l.act.starts_with("turn") { l.dur } else { l.secs };
            let (pd, py) = predict(cal, &l.act, l.vx, l.vyaw, secs);
            let md = (l.pose1.0 - l.pose0.0).hypot(l.pose1.1 - l.pose0.1);
            ed += (pd - md).powi(2);
            ey += wrap(py - dyaw(l)).powi(2);
            n += 1;
        }
        if n == 0 { (f64::NAN, f64::NAN, 0) } else { ((ed / n as f64).sqrt(), (ey / n as f64).sqrt(), n) }
    };
    let (pd, py, n) = replay(&prior);
    let (fd, fy, _) = replay(&c);
    std::fs::create_dir_all(&out_dir)?;
    let out = json!({
        "calib": c,
        "prior": prior,
        "fit": fit.rows,
        "replay": {"legs": n, "prior_rmse_m": pd, "fit_rmse_m": fd, "prior_rmse_yaw_rad": py, "fit_rmse_yaw_rad": fy},
        "traces": files, "events": events, "falls": falls, "legs": legs.len(), "stands": stands.len(), "maps": maps.len(),
    });
    std::fs::write(out_dir.join("calib.json"), serde_json::to_string_pretty(&out)?)?;
    let mut md = String::new();
    let _ = writeln!(md, "# Calibration\n\n{} traces, {} events: {} legs, {} stands of 1.5 s or more, {} maps.\n", files.len(), events, legs.len(), stands.len(), maps.len());
    let _ = writeln!(md, "| number | prior | fitted | n | how |\n|---|---|---|---|---|");
    for (k, v) in &fit.rows {
        let _ = writeln!(
            md,
            "| `{k}` | {:.4} | {} | {} | {} |",
            v["prior"].as_f64().unwrap_or(f64::NAN),
            if v["fitted"].as_bool().unwrap_or(false) { format!("**{:.4}**", v["value"].as_f64().unwrap_or(f64::NAN)) } else { "—".into() },
            v["n"],
            v["how"].as_str().unwrap_or("")
        );
    }
    let _ = writeln!(md, "\nReplay of {n} legs through the gait model (no noise): distance RMSE {pd:.4} m with the prior, {fd:.4} m with the fit; yaw RMSE {py:.4} rad with the prior, {fy:.4} rad with the fit.");
    std::fs::write(out_dir.join("calib.md"), &md)?;
    print!("{md}");
    Ok(())
}
