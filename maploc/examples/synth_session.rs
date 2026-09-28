//! A saved session drawn from the house's truth, not walked (branch
//! `oracle`, step 2 of the user's plan of 2026-09-28): the map maploc would
//! have if it mapped perfectly, for the homecoming and the tracking to be
//! measured on with nothing of the exploration's in it.
//!
//!     cargo run -p maploc --example synth_session -- <walls.toml> <out.session>
//!
//! One frozen submap anchored at the origin (the truth's frame is the
//! twin's world frame, which a map explored from the start pose shares),
//! its grid the truth's extent and a margin: every cell inside the walls'
//! extent at the free clamp, every wall segment — sampled at a quarter
//! cell — at the occupied clamp. Holes stay floor, as the depth sensor
//! leaves them in a real map (a hole reads as floor missing, not as a
//! wall); the navigation's drop book carries them.

use maploc::grid::{GridConfig, LO_MAX, LO_MIN};
use maploc::pose_graph::PoseGraph;
use maploc::session::save_session;
use maploc::submap::Submap;

fn walls(path: &str) -> Vec<(f32, f32, f32, f32)> {
    let text = std::fs::read_to_string(path).expect("read the walls");
    text.lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter(|l| l.starts_with('[') && !l.starts_with("[["))
        .filter_map(|l| {
            let v: Vec<f32> = l.trim_start_matches('[').split(']').next()?.split(',').filter_map(|p| p.trim().parse().ok()).collect();
            (v.len() == 4).then(|| (v[0] / 100.0, v[1] / 100.0, v[2] / 100.0, v[3] / 100.0))
        })
        .collect()
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    assert!(a.len() == 2, "usage: synth_session <walls.toml> <out.session>");
    let segs = walls(&a[0]);
    assert!(!segs.is_empty(), "no wall segments in {}", a[0]);
    let xs = segs.iter().flat_map(|s| [s.0, s.2]);
    let ys = segs.iter().flat_map(|s| [s.1, s.3]);
    let (x_lo, x_hi) = xs.fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), v| (lo.min(v), hi.max(v)));
    let (y_lo, y_hi) = ys.fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), v| (lo.min(v), hi.max(v)));
    let cell = 0.05f32;
    let cfg = GridConfig { x_range: (x_lo - 0.5, x_hi + 0.5), y_range: (y_lo - 0.5, y_hi + 0.5), cell };
    let mut sub = Submap::new_at((0.0, 0.0, 0.0), cfg);
    let grid = sub.grid_mut();
    // Free inside the extent (twice the clamp: the sum saturates at it).
    let mut y = y_lo + cell / 2.0;
    while y < y_hi {
        let mut x = x_lo + cell / 2.0;
        while x < x_hi {
            grid.add_log_odds_at_world(x, y, LO_MIN);
            grid.add_log_odds_at_world(x, y, LO_MIN);
            x += cell;
        }
        y += cell;
    }
    // Walls: from the free clamp up to the occupied one.
    let mut marked = 0usize;
    for &(x1, y1, x2, y2) in &segs {
        let n = (((x2 - x1).hypot(y2 - y1)) / (cell / 4.0)).ceil().max(1.0) as usize;
        for i in 0..=n {
            let t = i as f32 / n as f32;
            let (x, y) = (x1 + t * (x2 - x1), y1 + t * (y2 - y1));
            for _ in 0..4 {
                grid.add_log_odds_at_world(x, y, LO_MAX);
            }
            marked += 1;
        }
    }
    let mut graph = PoseGraph::new();
    graph.add_node((0.0, 0.0, 0.0), 0);
    save_session(&a[1], std::slice::from_ref(&sub), None, &graph, &[0], (0.0, 0.0, 0.0)).expect("save the session");
    eprintln!(
        "{}: {} segments, {} wall samples, grid {}x{} at {} m, x {:.2}..{:.2}, y {:.2}..{:.2}",
        a[1],
        segs.len(),
        marked,
        sub.grid().width(),
        sub.grid().height(),
        cell,
        cfg.x_range.0,
        cfg.x_range.1,
        cfg.y_range.0,
        cfg.y_range.1
    );
}
