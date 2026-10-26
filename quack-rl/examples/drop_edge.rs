//! drop_edge: where the simulated sensor puts a hole's edge, by pitch — a
//! hole 0.5 m ahead (its rim at x 0.5) and one beside the lane, the body
//! walking and standing.
use std::time::Instant;

use quack_rl::Rng;
use quack_rl::calib::Calib;
use quack_rl::world::{Hole, World};

fn main() {
    for (name, hole) in [("ahead, rim at 0.50", Hole { x0: 0.5, x1: 0.9, y0: -0.5, y1: 0.5, booked: false }), ("beside, rim at y -0.15, x 0.2..0.9", Hole { x0: 0.2, x1: 0.9, y0: -0.5, y1: -0.15, booked: false })] {
        let mut w = World { bounds: [-2.0, 2.0, -2.0, 2.0], ..Default::default() };
        w.holes.push(hole);
        println!("== {name}");
        for deg in [-3.0, -6.0, -9.0, -11.0] {
            for walking in [true, false] {
                let c = Calib { tof_dropout: 0.0, phantom_any_p: 0.0, tof_range_sd: 0.0, tof_pitch_rad: Some(deg * std::f64::consts::PI / 180.0), ..Calib::default() };
                let mut rng = Rng::new(1);
                let mut near = f64::INFINITY;
                let mut n = 0;
                for head in if walking { vec![0.0] } else { vec![-0.8, -0.4, 0.0, 0.4, 0.8] } {
                    let f = w.sense((0.0, 0.0, 0.0), head, walking, &c, &mut rng, 1, Instant::now());
                    for d in &f.drops {
                        n += 1;
                        let (x, y) = (d.range_m * (head + d.bearing).cos(), d.range_m * (head + d.bearing).sin());
                        let _ = y;
                        near = near.min(x);
                    }
                }
                println!("  pitch {deg:5.1}° {:8}: drops {n:3}, nearest drop x {:.2} m", if walking { "walking" } else { "standing" }, near);
            }
        }
    }
}
