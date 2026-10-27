//! hole_walk: what the sensor sees of a narrow hole as the body walks up
//! to it — per distance to the near rim, the nearest drop's bounds in the
//! middle columns, level and with the head looking down.
use std::time::Instant;

use quack_rl::Rng;
use quack_rl::calib::Calib;
use quack_rl::world::{Hole, World};

fn main() {
    for width in [0.33, 0.50] {
        for deg in [-3.0, -6.0, -11.0] {
            println!("== hole {width:.2} m wide, pitch {deg}°  (distance to the rim: level walking | level standing | looking down)");
            let c = Calib { tof_dropout: 0.0, phantom_any_p: 0.0, tof_range_sd: 0.0, tof_pitch_rad: Some(deg * std::f64::consts::PI / 180.0), ..Calib::default() };
            let mut w = World { bounds: [-2.0, 3.0, -2.0, 2.0], ..Default::default() };
            w.holes.push(Hole { x0: 1.0, x1: 1.0 + width, y0: -0.5, y1: 0.5, booked: false });
            let mut d = 0.90;
            while d > 0.0 {
                let x = 1.0 - d;
                let mut cols = Vec::new();
                for (look, walking) in [(0.0, true), (0.0, false), (-0.69, false)] {
                    let f = w.sense((x, 0.0, 0.0), 0.0, look, walking, &c, &mut Rng::new(1), 1, Instant::now());
                    let near = f.drops.iter().filter(|dr| dr.bearing.abs() < 0.2).map(|dr| (dr.edge_min_m, dr.range_m)).min_by(|a, b| a.0.total_cmp(&b.0));
                    cols.push(match near { Some((e, r)) => format!("edge {e:.2}..{r:.2}"), None => "   nothing   ".into() });
                }
                println!("  {d:.2} m: {} | {} | {}", cols[0], cols[1], cols[2]);
                d -= 0.10;
            }
        }
    }
}
