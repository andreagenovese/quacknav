//! The homecoming's map-to-map question, asked offline, with the answer
//! known: a recording replayed from `start_s` into a FRESH map — a duck
//! woken there, its map starting where it stands — and every `ASK_EVERY_S`
//! the fresh map matched against saved maps as `robot.map_match` does
//! (`align::match_maps`). The truth says where the fresh map's origin
//! really is: the twin's pose at `start_s`, the saved maps being drawn in
//! the world's frame (every session there starts at the origin).
//!
//!     cargo run -p maploc --release --features kinematics --example wake_match -- \
//!         <session.mdlg> <pose.tsv> <start_s> <map.session>...
//!
//! One line per ask and saved map, tab-separated: seconds since the wake,
//! the fresh map's wall cells, the saved map, the fit (x, y, yaw°), score,
//! margin (best over runner-up, lower is more decisive), overlap, and the
//! fit's error against the truth (m, °). A saved map of another house has
//! no truth: its error is printed as NaN, and every fit of it is wrong.
//! `ASK_EVERY_S` (60) and `ASK_FOR_S` (900) set the cadence and the span.

use std::path::PathBuf;

use maploc::align::{AlignConfig, match_maps, wall_cells};
use maploc::mapper::{Mapper, MapperConfig};
use maploc::pipeline::{Slam, SlamConfig};

/// (unix s, true x, true y, true yaw) from the tracked sampler's file.
fn truth(path: &PathBuf) -> Vec<(f64, f64, f64, f64)> {
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    let mut rows: Vec<_> = text
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            let num = |i: usize| f.get(i).and_then(|v| v.parse::<f64>().ok());
            Some((num(0)?, num(3)?, num(4)?, num(9)?))
        })
        .collect();
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    rows
}

fn main() {
    let usage = "usage: wake_match <session.mdlg> <pose.tsv> <start_s> <map.session>...";
    let mut args = std::env::args().skip(1);
    let session: PathBuf = args.next().expect(usage).into();
    let truth_path: PathBuf = args.next().expect(usage).into();
    let start_s: f32 = args.next().expect(usage).parse().expect("start_s");
    let maps: Vec<PathBuf> = args.map(PathBuf::from).collect();
    assert!(!maps.is_empty(), "{usage}");
    let envf = |k: &str, d: f32| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
    let (every, span) = (envf("ASK_EVERY_S", 60.0), envf("ASK_FOR_S", 900.0));
    let truth = truth(&truth_path);
    let own_house = |p: &PathBuf| std::env::var("OTHER_HOUSE").map_or(true, |o| !p.display().to_string().contains(&o));

    let mut saved: Vec<(String, maploc::grid::OccupancyGrid, bool)> = maps
        .iter()
        .map(|p| {
            let s = maploc::session::SessionState::load(p).expect("read").expect("empty session");
            let grid = Slam::from_session(SlamConfig::default(), s).render().expect("a saved map renders");
            (p.display().to_string(), grid, own_house(p))
        })
        .collect();

    let mut mapper = Mapper::new(MapperConfig::default(), Slam::new(SlamConfig::default()));
    let fresh: Box<dyn FnOnce(&Mapper) -> Mapper> = Box::new(|_| Mapper::new(MapperConfig::default(), Slam::new(SlamConfig::default())));
    let mut woke: Option<(f64, (f64, f64, f64))> = None;
    let mut next_ask = start_s + every;
    let cfg = AlignConfig::default();
    println!("since_s\tcells\tmap\tx\ty\tyaw_deg\tscore\tmargin\toverlap\terr_m\terr_deg");
    maploc::bench::replay_loading(&session, &mut mapper, start_s + span, Some((start_s, fresh)), |step| {
        if woke.is_none() && step.t_s >= start_s {
            let t = truth.iter().min_by(|a, b| (a.0 - step.unix_s).abs().total_cmp(&(b.0 - step.unix_s).abs())).copied();
            if let Some((tt, x, y, yaw)) = t {
                // The fresh map's frame is not the body's at the wake: the
                // mapper's pose there is its own (odometry's) — the truth
                // of the frame is the true pose composed with its inverse.
                let (fx, fy, fyaw) = step.mapper.slam().tracked();
                let f = maploc::pose_graph::inverse((fx, fy, fyaw));
                let (ox, oy, oyaw) = maploc::pose_graph::compose((x as f32, y as f32, yaw as f32), f);
                eprintln!(
                    "woke at {:.1} s: truth ({x:.2}, {y:.2}, {:.0}°), sampled {:.1} s away; mapper at ({fx:.2}, {fy:.2}, {:.0}°); the fresh frame's truth ({ox:.2}, {oy:.2}, {:.0}°)",
                    step.t_s,
                    yaw.to_degrees(),
                    (tt - step.unix_s).abs(),
                    fyaw.to_degrees(),
                    oyaw.to_degrees()
                );
                woke = Some((step.unix_s, (f64::from(ox), f64::from(oy), f64::from(oyaw))));
            }
        }
        if step.t_s < next_ask {
            return true;
        }
        next_ask += every;
        let Some(live) = step.mapper.slam().render() else { return true };
        let cells = wall_cells(&live, cfg.certain_log);
        let p0 = woke.map(|w| w.1);
        for (name, grid, own) in saved.iter_mut() {
            let found = match_maps(&live, grid, &cfg);
            let Some(best) = found.first() else {
                println!("{:.0}\t{cells}\t{name}\t-\t-\t-\t-\t-\t-\t-\t-", step.t_s - start_s);
                continue;
            };
            let margin = found.get(1).map_or(1.0, |n| best.score / n.score.max(1e-6));
            let (ex, ey) = match (p0, *own) {
                (Some(p), true) => {
                    let e = (f64::from(best.pose.0) - p.0).hypot(f64::from(best.pose.1) - p.1);
                    let ey = (f64::from(best.pose.2) - p.2 + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI;
                    (e, ey.to_degrees())
                }
                _ => (f64::NAN, f64::NAN),
            };
            println!(
                "{:.0}\t{cells}\t{name}\t{:.2}\t{:.2}\t{:.0}\t{:.3}\t{margin:.2}\t{:.2}\t{ex:.2}\t{ey:.1}",
                step.t_s - start_s,
                best.pose.0,
                best.pose.1,
                best.pose.2.to_degrees(),
                best.score,
                best.overlap,
            );
        }
        true
    })
    .expect("replay");
}
