//! guard_probe: a generated stairwell, the body walking straight at the
//! hole as the reckless brain did — per distance to the rim, what the
//! walking guard (`hole_in_lane_walking` with the pilot's reach) answers.
//!
//!     guard_probe --seed 300291 [--family stairwell] [--calib FILE]
use std::time::{Duration, Instant};

use quack_nav::cliff::{CliffStatus, StreamState};
use quack_rl::Rng;
use quack_rl::calib::Calib;
use quack_rl::scenarios::generate;

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let opt = |n: &str| a.iter().position(|x| x == n).and_then(|i| a.get(i + 1)).cloned();
    let seed: u64 = opt("--seed").and_then(|v| v.parse().ok()).unwrap_or(300291);
    let calib = match opt("--calib") {
        Some(p) => Calib::load(&p)?,
        None => Calib::default(),
    };
    let s = generate(seed, 3, opt("--family").as_deref().or(Some("stairwell")));
    let w = &s.world;
    let h = &w.holes[0];
    println!("hole x {:.2}..{:.2} y {:.2}..{:.2}; rects near it:", h.x0, h.x1, h.y0, h.y1);
    for r in &w.rects {
        if r.x1 > h.x0 - 0.5 && r.x0 < h.x1 + 0.5 && r.y1 > h.y0 - 0.5 && r.y0 < h.y1 + 0.5 {
            println!("  rect x {:.2}..{:.2} y {:.2}..{:.2} h {:.2}", r.x0, r.x1, r.y0, r.y1, r.height);
        }
    }
    // Straight at the hole's north rim, heading south, as 300291 did.
    let xm = 0.5 * (h.x0 + h.x1);
    let mut rng = Rng::new(1);
    let t0 = Instant::now();
    let mut d = 0.70;
    while d > 0.0 {
        let pose = (xm, h.y1 + d, -std::f64::consts::FRAC_PI_2);
        let mut c = CliffStatus { stream: StreamState::Serving, body_seen: true, ..CliffStatus::default() };
        let mut t = 0.0;
        let mut seq = 0;
        while t < 1.0 {
            seq += 1;
            let at = t0 + Duration::from_secs_f64(t);
            c.recent.push(w.sense(pose, 0.0, 0.0, true, &calib, &mut rng, seq, at));
            t += 1.0 / calib.tof_hz;
        }
        let now = t0 + Duration::from_secs_f64(1.0);
        let lane: Vec<String> = c.recent.last().unwrap().drops.iter().filter(|dr| dr.bearing.abs() < 0.3).map(|dr| format!("{:.2}..{:.2}@{:.0}°{:?}", dr.edge_min_m, dr.range_m, dr.bearing.to_degrees(), dr.kind)).collect();
        let obs: Vec<String> = c.recent.last().unwrap().obstacles.iter().filter(|o| o.bearing.abs() < 0.3).map(|o| format!("{:.2}@{:.0}°", o.range_m, o.bearing.to_degrees())).collect();
        let guard = c.hole_in_lane_walking(now, 0.0, 0.17, 0.06 + 0.15, Duration::from_millis(1200), 2);
        println!("  {d:.2} m: guard {} | drops {} | obstacles {}", guard.map(|g| format!("REFUSES (edge {:.2})", g.edge_min_m)).unwrap_or("silent".into()), lane.join(" "), obs.join(" "));
        d -= 0.10;
    }
    Ok(())
}
