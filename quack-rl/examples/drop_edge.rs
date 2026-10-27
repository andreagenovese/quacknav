//! drop_edge: where the simulated sensor puts a hole's edge, by pitch — a
//! hole 0.5 m ahead (its rim at x 0.5) and one beside the lane, the body
//! walking and standing.
use std::time::Instant;

use quack_rl::Rng;
use quack_rl::calib::Calib;
use quack_rl::world::{Hole, World};

/// Signed distance of a point to the hole: positive inside, negative outside (metres).
fn inside(h: &Hole, x: f64, y: f64) -> f64 {
    let dx = (h.x0 - x).max(x - h.x1).max(0.0);
    let dy = (h.y0 - y).max(y - h.y1).max(0.0);
    if dx == 0.0 && dy == 0.0 {
        (x - h.x0).min(h.x1 - x).min(y - h.y0).min(h.y1 - y)
    } else {
        -(dx * dx + dy * dy).sqrt()
    }
}

fn main() {
    for (name, hole) in [("ahead, rim at 0.50", Hole { x0: 0.5, x1: 0.9, y0: -0.5, y1: 0.5, booked: false }), ("beside, rim at y -0.15, x 0.2..0.9", Hole { x0: 0.2, x1: 0.9, y0: -0.5, y1: -0.15, booked: false })] {
        let mut w = World { bounds: [-2.0, 2.0, -2.0, 2.0], ..Default::default() };
        w.holes.push(hole.clone());
        println!("== {name}");
        for deg in [-3.0, -6.0, -9.0, -11.0] {
            for walking in [true, false] {
                let c = Calib { tof_dropout: 0.0, phantom_any_p: 0.0, tof_range_sd: 0.0, tof_pitch_rad: Some(deg * std::f64::consts::PI / 180.0), ..Calib::default() };
                let mut rng = Rng::new(1);
                // Per candidate booking point: (n, n inside, deepest inside, farthest outside).
                let mut st = [(0usize, 0usize, f64::NEG_INFINITY, f64::NEG_INFINITY); 3];
                for head in if walking { vec![0.0] } else { vec![-0.8, -0.4, 0.0, 0.4, 0.8] } {
                    let f = w.sense((0.0, 0.0, 0.0), head, 0.0, walking, &c, &mut rng, 1, Instant::now());
                    for d in &f.drops {
                        // The frame is in the body frame: its bearings already carry the head's yaw.
                        let b = d.bearing;
                        let emin = if d.edge_min_m > 0.0 { d.edge_min_m } else { d.range_m - 0.15 };
                        for (k, r) in [d.range_m, emin, 0.5 * (emin + d.range_m)].into_iter().enumerate() {
                            let s = inside(&hole, r * b.cos(), r * b.sin());
                            st[k].0 += 1;
                            if s > 0.0 { st[k].1 += 1; st[k].2 = st[k].2.max(s); } else { st[k].3 = st[k].3.max(-s); }
                        }
                    }
                }
                let show = |s: &(usize, usize, f64, f64)| format!("{:2}/{:2} in, {:.2} deep, {:.2} out", s.1, s.0, s.2.max(0.0), s.3.max(0.0));
                println!("  pitch {deg:5.1}° {:8}: range_m [{}]  edge_min [{}]  mid [{}]", if walking { "walking" } else { "standing" }, show(&st[0]), show(&st[1]), show(&st[2]));
            }
        }
    }
}
