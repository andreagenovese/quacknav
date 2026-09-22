//! Replan on a saved `map.frame` from a given pose, with the explore
//! job's books and its walked trail, with and without the lanes.
//!
//!     cargo run -p quack-nav --example replan -- <frame.json> <x> <y> <local.json> <trail.json> [inflate_m]
//!
//! `local.json` is `[[[x, y], r], ...]` (the books, drops with r ≥ 0.10
//! mapped to the planner radius 0.12 as the job does), `trail.json` is
//! `[[x, y], ...]` (the truth path of a run, interpolated every 5 cm).

use quack_nav::frontier::{ExtraWall, INFLATE_M, frontiers_with};
use quack_nav::map::MapFrame;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    anyhow::ensure!(args.len() >= 5, "usage: replan <frame.json> <x> <y> <local.json> <trail.json> [inflate_m]");
    let value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&args[0])?)?;
    let frame: MapFrame = serde_json::from_value(value.get("frame").cloned().unwrap_or(value))?;
    let grid = frame.grid()?;
    let (x, y): (f64, f64) = (args[1].parse()?, args[2].parse()?);
    let local: Vec<((f64, f64), f64)> = serde_json::from_str(&std::fs::read_to_string(&args[3])?)?;
    let walls: Vec<ExtraWall> = local
        .iter()
        .map(|(p, r)| if *r >= 0.10 { (*p, 0.12) } else { (*p, *r) })
        .collect();
    let raw: Vec<(f64, f64)> = serde_json::from_str(&std::fs::read_to_string(&args[4])?)?;
    let inflate: f64 = args.get(5).map(|s| s.parse()).transpose()?.unwrap_or(INFLATE_M);
    // Interpolate the trail every 5 cm, as the job's `walked` does.
    let mut trail = Vec::new();
    for w in raw.windows(2) {
        let (a, b) = (w[0], w[1]);
        let d = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        let n = (d / 0.05).ceil().max(1.0) as usize;
        for k in 0..=n {
            let t = k as f64 / n as f64;
            trail.push((a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1)));
        }
    }
    let (unknown, free, wall) = grid.counts();
    println!("grid {}x{}: unknown {unknown} free {free} wall {wall}; pose ({x:.2}, {y:.2}); books {} ; trail {} points", grid.rows, grid.cols, local.len(), trail.len());
    for (label, w, lanes) in [
        ("no books, no lanes", Vec::new(), Vec::new()),
        ("books, no lanes", walls.clone(), Vec::new()),
        ("books + lanes", walls.clone(), trail.clone()),
    ] {
        let fs = frontiers_with(&grid, x, y, &[], &w, inflate, &lanes);
        let best = fs
            .iter()
            .map(|f| (f.target, f.cells, f.distance_m))
            .take(4)
            .collect::<Vec<_>>();
        println!("{label:20} inflate {inflate:.2}: {} reachable frontiers; first {:?}", fs.len(), best);
    }
    Ok(())
}
