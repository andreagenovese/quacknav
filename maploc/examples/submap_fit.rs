//! Each submap of a saved session against the house as it is: which of
//! them sit off, by how much, which way, and what holds them to the rest
//! (2026-09-29: the bathroom behind the passage beside the stairwell
//! drawn 15-20 cm off in both houses, the pose right while it was drawn).
//!
//!     cargo run -p maploc --release --example submap_fit -- <session> <truth.toml> [x0,x1,y0,y1]
//!
//! Per submap: its index (creation order), anchor, the wall cells it holds,
//! their mean distance to the true walls and the mean of the nearest-wall
//! offset (the way it would have to move), and the loop edges from its node
//! to others (the odometry chain is the edges between consecutive nodes).
//! The optional box keeps the submaps whose anchor lies inside it.

use maploc::pose_graph::{between, compose, wrap_pi};
use maploc::session::SessionState;

/// A cell holds a wall above this, as quack-navd's map classifies it.
const WALL_LOG: i16 = 150;

fn truth_points(path: &str) -> Vec<(f32, f32)> {
    let text = std::fs::read_to_string(path).expect("read the truth");
    let mut pts = Vec::new();
    for l in text.lines().map(|l| l.split('#').next().unwrap_or("").trim()) {
        if !(l.starts_with('[') && !l.starts_with("[[")) {
            continue;
        }
        let v: Vec<f32> = l.trim_start_matches('[').split(']').next().unwrap_or("").split(',').filter_map(|p| p.trim().parse().ok()).collect();
        if v.len() != 4 {
            continue;
        }
        let (x1, y1, x2, y2) = (v[0] / 100.0, v[1] / 100.0, v[2] / 100.0, v[3] / 100.0);
        let n = (((x2 - x1).hypot(y2 - y1)) / 0.02).ceil().max(1.0) as usize;
        for k in 0..=n {
            let t = k as f32 / n as f32;
            pts.push((x1 + t * (x2 - x1), y1 + t * (y2 - y1)));
        }
    }
    pts
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    assert!(a.len() >= 2, "usage: submap_fit <session> <truth.toml> [x0,x1,y0,y1]");
    let session = SessionState::load(std::path::Path::new(&a[0])).expect("read the session").expect("the session is empty");
    let truth = truth_points(&a[1]);
    let within: Option<[f32; 4]> = a.get(2).map(|b| {
        let v: Vec<f32> = b.split(',').filter_map(|p| p.parse().ok()).collect();
        [v[0], v[1], v[2], v[3]]
    });
    let graph = &session.graph;
    let node_submap: Vec<usize> = graph.nodes().iter().map(|n| n.submap_idx).collect();
    let submaps: Vec<_> = session.frozen.iter().chain(session.current.iter()).collect();
    println!("{:>4} {:>22} {:>6} {:>7} {:>16}  loops to", "sub", "anchor (x, y, yaw°)", "walls", "mean_m", "offset (dx, dy)");
    for (i, sm) in submaps.iter().enumerate() {
        let anchor = sm.anchor_pose();
        if let Some([x0, x1, y0, y1]) = within
            && !(anchor.0 >= x0 && anchor.0 <= x1 && anchor.1 >= y0 && anchor.1 <= y1)
        {
            continue;
        }
        let g = sm.grid();
        let cfg = g.cfg();
        let (w, h) = (g.width(), g.height());
        let log = g.log_raw();
        let mut sum_d = 0.0f32;
        let (mut sx, mut sy) = (0.0f32, 0.0f32);
        let mut n = 0usize;
        for r in 0..h {
            for c in 0..w {
                if log[r * w + c] <= WALL_LOG {
                    continue;
                }
                let local = (cfg.x_range.0 + (c as f32 + 0.5) * cfg.cell, cfg.y_range.0 + (r as f32 + 0.5) * cfg.cell, 0.0);
                let p = compose(anchor, local);
                let (mut best, mut bx, mut by) = (f32::INFINITY, 0.0, 0.0);
                for &(tx, ty) in &truth {
                    let d = (tx - p.0).hypot(ty - p.1);
                    if d < best {
                        (best, bx, by) = (d, tx - p.0, ty - p.1);
                    }
                }
                sum_d += best;
                sx += bx;
                sy += by;
                n += 1;
            }
        }
        let node = session.node_for_submap.get(i).copied();
        let loops: Vec<usize> = node
            .map(|nd| {
                graph
                    .edges()
                    .iter()
                    .filter(|e| (e.from == nd || e.to == nd) && e.from.abs_diff(e.to) > 1)
                    .map(|e| node_submap[if e.from == nd { e.to } else { e.from }])
                    .collect()
            })
            .unwrap_or_default();
        // `SLANT=x0,x1,y0,y1`: how many of this submap's wall cells fall in that box.
        if let Ok(b) = std::env::var("SLANT") {
            let v: Vec<f32> = b.split(',').filter_map(|p| p.parse().ok()).collect();
            let mut k = 0;
            for r in 0..h { for c in 0..w { if log[r * w + c] > WALL_LOG { let p = compose(anchor, (cfg.x_range.0 + (c as f32 + 0.5) * cfg.cell, cfg.y_range.0 + (r as f32 + 0.5) * cfg.cell, 0.0)); if p.0 >= v[0] && p.0 <= v[1] && p.1 >= v[2] && p.1 <= v[3] { k += 1; } } } }
            if k > 0 { println!("  submap {i} holds {k} wall cells in the box"); }
        }
        if n == 0 {
            println!("{i:>4} ({:+6.2}, {:+6.2}, {:+5.0}) {:>6}", anchor.0, anchor.1, anchor.2.to_degrees(), 0);
            continue;
        }
        let nf = n as f32;
        println!(
            "{i:>4} ({:+6.2}, {:+6.2}, {:+5.0}) {:>6} {:>7.3} ({:+6.3}, {:+6.3})  {:?}",
            anchor.0,
            anchor.1,
            anchor.2.to_degrees(),
            n,
            sum_d / nf,
            sx / nf,
            sy / nf,
            loops
        );
    }
    // Every edge's residual on the final graph: how far the two anchors
    // sit from what the edge measured. An optimizer that bent a chain to
    // meet a loop leaves it on the chain's odometry edges.
    println!("\nedges with a residual over 3 cm or 1.5° (from -> to submaps, kind, residual dx dy dyaw°):");
    let nodes = graph.nodes();
    let min_res_m: f32 = std::env::var("MIN_RES_M").ok().and_then(|v| v.parse().ok()).unwrap_or(0.03);
    let min_res_deg: f32 = std::env::var("MIN_RES_DEG").ok().and_then(|v| v.parse().ok()).unwrap_or(1.5);
    for e in graph.edges() {
        let now = between(nodes[e.from].pose, nodes[e.to].pose);
        let (dx, dy, dyaw) = (now.0 - e.measurement.0, now.1 - e.measurement.1, wrap_pi(now.2 - e.measurement.2));
        if dx.hypot(dy) < min_res_m && dyaw.abs().to_degrees() < min_res_deg {
            continue;
        }
        let (fs, ts) = (node_submap[e.from], node_submap[e.to]);
        if let Some([x0, x1, y0, y1]) = within {
            let inside = |s: usize| submaps.get(s).is_some_and(|sm| {
                let p = sm.anchor_pose();
                p.0 >= x0 && p.0 <= x1 && p.1 >= y0 && p.1 <= y1
            });
            if !inside(fs) && !inside(ts) {
                continue;
            }
        }
        let kind = if e.from.abs_diff(e.to) == 1 { "odometry" } else { "loop" };
        println!("  {fs:>4} -> {ts:>4} {kind:<8} ({dx:+.3}, {dy:+.3}, {:+.1}°)", dyaw.to_degrees());
    }
}
