//! Does a map the duck has just built fit inside one it saved before?
//!
//!     cargo run -p maploc --example align_maps -- <new.session> <known.session> [truth_x truth_y truth_deg]
//!
//! The bench face of [`maploc::align`]: it renders two saved sessions and
//! prints what the comparison found, scored against a truth when one is
//! given. `ALIGN_BEAMS` and `ALIGN_RESID` override the two knobs.

use maploc::align::{AlignConfig, match_maps};
use maploc::pipeline::{Slam, SlamConfig};
use maploc::session::SessionState;

fn render(path: &str) -> maploc::grid::OccupancyGrid {
    let session = SessionState::load(std::path::Path::new(path))
        .expect("read the session")
        .expect("the session is empty");
    Slam::from_session(SlamConfig::default(), session)
        .render()
        .expect("the session has no map")
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    assert!(
        a.len() >= 2,
        "usage: align_maps <new.session> <known.session> [truth_x truth_y truth_deg]"
    );
    let fresh = render(&a[0]);
    let mut known = render(&a[1]);

    let mut cfg = AlignConfig::default();
    if let Some(v) = std::env::var("ALIGN_BEAMS").ok().and_then(|v| v.parse().ok()) {
        cfg.beams = v;
    }
    if let Some(v) = std::env::var("ALIGN_RESID").ok().and_then(|v| v.parse().ok()) {
        cfg.max_mean_residual_m = v;
    }

    let started = std::time::Instant::now();
    let found = match_maps(&fresh, &mut known, &cfg);
    let took = started.elapsed().as_secs_f32();
    if found.is_empty() {
        println!("no alignment at all ({took:.1} s)");
        return;
    }
    for (i, c) in found.iter().enumerate().take(5) {
        println!(
            "  basin {i}: ({:.2}, {:.2}, {:.1}°) walls {:.3} on {:.0}% overlap  floor-on-wall {:.1}%  score {:.3}",
            c.pose.0,
            c.pose.1,
            c.pose.2.to_degrees(),
            c.wall_residual_m,
            100.0 * c.overlap,
            100.0 * c.floor_on_wall,
            c.score,
        );
    }
    let best = found[0];
    println!(
        "best ({:.2}, {:.2}, {:.1}°), score {:.3}, walls {:.3} on {:.0}% overlap, floor-on-wall {:.1}%{} ({took:.1} s)",
        best.pose.0,
        best.pose.1,
        best.pose.2.to_degrees(),
        best.score,
        best.wall_residual_m,
        100.0 * best.overlap,
        100.0 * best.floor_on_wall,
        match found.get(1) {
            Some(next) => format!(
                " — the next scores {:.3}, a margin of {:.2}×",
                next.score,
                best.score / next.score.max(1e-6)
            ),
            None => String::new(),
        }
    );
    if a.len() >= 5 {
        let (tx, ty, tdeg): (f32, f32, f32) =
            (a[2].parse().unwrap(), a[3].parse().unwrap(), a[4].parse().unwrap());
        let d = (best.pose.0 - tx).hypot(best.pose.1 - ty);
        let dyaw = (best.pose.2.to_degrees() - tdeg + 540.0) % 360.0 - 180.0;
        println!("  against the truth: off by {d:.2} m and {dyaw:.1}°");
    }
}
