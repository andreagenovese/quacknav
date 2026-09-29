//! Where a recorded session's drops land: every depth frame of a `.mdlg`
//! through the cliff guard's own `analyze`, placed in the map by the
//! replayed mapper's pose and by the twin's truth, against given points —
//! the phantoms a drop book was scored with — to tell a phantom of the pose
//! from one of the sensor.
//!
//!     cargo run -p quack-nav --release --example drop_replay -- \
//!         <session.mdlg> <pose.tsv> <truth.json> "x,y;x,y;..."
//!
//! `MAP_SESSION` / `MAP_LOAD_AT_S` as `maploc`'s `trajectory` example: the
//! saved map the session resumed on, loaded when the daemon loaded it.
//! Each frame is judged against the latest robot-state sample before it,
//! as the cliff guard does live (it holds the newest 20 Hz sample). A
//! frame judged while the body walks is skipped: the books take only
//! standing frames. One line per drop within `NEAR_M` of a point; then,
//! per point, how many frames put a drop there and where the same beams
//! land with the true pose.

use std::path::PathBuf;
use std::time::Instant;

use duck_ipc_proto as proto;
use kinematics::tof::Reprojector;
use maploc::mapper::{Mapper, MapperConfig};
use maploc::pipeline::{Slam, SlamConfig};
use maploc::replay::{OdomRecord, Record};
use quack_nav::cliff::{BodyPose, DropKind, analyze};

const NEAR_M: f64 = 0.12;

/// (unix s, true x, true y, true yaw)
fn truth(path: &PathBuf) -> Vec<(f64, f64, f64, f64)> {
    let mut rows = Vec::new();
    for p in [path.clone(), PathBuf::from(format!("{}.untracked", path.display()))] {
        let Ok(text) = std::fs::read_to_string(&p) else { continue };
        for line in text.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            let num = |i: usize| f.get(i).and_then(|v| v.parse::<f64>().ok());
            if let (Some(t), Some(x), Some(y), Some(yaw)) = (num(0), num(3), num(4), num(9)) {
                rows.push((t, x, y, yaw));
            }
        }
    }
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    rows
}

/// Distance from `p` to the nearest true hole (0 inside one).
fn to_hole(holes: &[[f64; 4]], p: (f64, f64)) -> f64 {
    holes
        .iter()
        .map(|h| {
            let dx = (h[0] - p.0).max(p.0 - h[1]).max(0.0);
            let dy = (h[2] - p.1).max(p.1 - h[3]).max(0.0);
            dx.hypot(dy)
        })
        .fold(f64::INFINITY, f64::min)
}

fn main() {
    let usage = "usage: drop_replay <session.mdlg> <pose.tsv> <truth.json> \"x,y;x,y\"";
    let mut args = std::env::args().skip(1);
    let session: PathBuf = args.next().expect(usage).into();
    let truth_path: PathBuf = args.next().expect(usage).into();
    let truth_json: PathBuf = args.next().expect(usage).into();
    let points: Vec<(f64, f64)> = args
        .next()
        .expect(usage)
        .split(';')
        .filter(|s| !s.is_empty())
        .map(|s| {
            let v: Vec<f64> = s.split(',').map(|x| x.trim().parse().expect("x,y")).collect();
            (v[0], v[1])
        })
        .collect();
    let truth = truth(&truth_path);
    let holes: Vec<[f64; 4]> = {
        let j: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&truth_json).expect("truth.json")).expect("json");
        j["holes"].as_array().expect("holes").iter().map(|h| {
            let v: Vec<f64> = h.as_array().expect("hole").iter().map(|x| x.as_f64().expect("num")).collect();
            [v[0], v[1], v[2], v[3]]
        }).collect()
    };
    let truth_at = |unix: f64| truth.iter().min_by(|a, b| (a.0 - unix).abs().total_cmp(&(b.0 - unix).abs())).copied();

    let cfg = MapperConfig::default();
    let slam_cfg = SlamConfig::default();
    let load_at: Option<f32> = std::env::var("MAP_LOAD_AT_S").ok().and_then(|v| v.parse().ok());
    let saved = std::env::var_os("MAP_SESSION").map(|p| {
        maploc::session::SessionState::load(std::path::Path::new(&p)).expect("read the saved session").expect("the saved session is empty")
    });
    type Load = Box<dyn FnOnce(&Mapper) -> Mapper>;
    let (mut mapper, load): (Mapper, Option<(f32, Load)>) = match (saved, load_at) {
        (Some(mut saved), Some(at)) => {
            let (c, s) = (cfg.clone(), slam_cfg.clone());
            let build: Load = Box::new(move |fresh: &Mapper| {
                saved.tracked = maploc::pose_graph::compose(saved.tracked, fresh.slam().tracked());
                Mapper::resumed_lost(c, Slam::from_session(s, saved))
            });
            (Mapper::new(cfg, Slam::new(slam_cfg)), Some((at, build)))
        }
        (Some(saved), None) => (Mapper::resumed_lost(cfg, Slam::from_session(slam_cfg, saved)), None),
        (None, _) => (Mapper::new(cfg, Slam::new(slam_cfg)), None),
    };

    // `BOOK=<ground.json>:<map>`: the book's holes too, each checked
    // against the floor strike (explore::books' rule, replayed): which
    // phantoms it would take off, and whether it ever takes a true rim.
    let book: Vec<(f64, f64)> = std::env::var("BOOK").ok().map(|v| {
        let (path, map) = v.split_once(':').expect("BOOK=<file>:<map>");
        let j: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).expect("book")).expect("json");
        j[map].as_array().expect("drops").iter().map(|d| (d[0].as_f64().unwrap(), d[1].as_f64().unwrap())).collect()
    }).unwrap_or_default();
    let mut struck_at: Vec<Option<f32>> = vec![None; book.len()];
    // The fresh standing frames: (t, floor points, hole points) in the map.
    let mut window: std::collections::VecDeque<(f32, Vec<(f64, f64)>, Vec<(f64, f64)>)> = Default::default();
    let rp = Reprojector::alpha();
    let mut latest: Option<OdomRecord> = None;
    // Per point: frames with a drop near it, and the true-pose landings' distance to a hole.
    let mut hits: Vec<(u32, Vec<f64>)> = vec![(0, Vec::new()); points.len()];
    println!("t_s\tpoint\tmap_x\tmap_y\tmap_yaw\ttrue_x\ttrue_y\ttrue_yaw\tkind\trange\tbear_deg\thead_yaw_deg\tdrop_map\tdrop_true\ttrue_to_hole");
    maploc::bench::replay_loading(&session, &mut mapper, f32::INFINITY, load, |step| {
        match step.record {
            Record::Odom(o) => latest = Some(*o),
            Record::Tof(f) => {
                let Some(o) = latest else { return true };
                if o.moving || !step.mapper.tracking() {
                    return true;
                }
                let frame = proto::TofFrame {
                    seq: 0,
                    at_us: (f.sender_ts_s * 1e6) as u64,
                    t_ns: f.t_ns,
                    rows: 8,
                    cols: 8,
                    distance_mm: f.ranges_m.iter().flatten().map(|r| if r.is_finite() { (r * 1000.0).round() as i16 } else { 0 }).collect(),
                    status: f.status.iter().flatten().copied().collect(),
                };
                let body = BodyPose {
                    head: o.head.map(f64::from),
                    gravity: o.gravity.map(f64::from),
                    trunk_z: f64::from(o.trunk_z),
                    moving: o.moving,
                };
                let Some(judged) = analyze(&rp, &frame, &body, Instant::now()) else { return true };
                let (mx, my, myaw) = step.mapper.slam().tracked();
                let (mx, my, myaw) = (f64::from(mx), f64::from(my), f64::from(myaw));
                let t = truth_at(step.unix_s);
                {
                    let fl: Vec<(f64, f64)> = judged.floors.iter().map(|(b, r)| (mx + r * (myaw + b).cos(), my + r * (myaw + b).sin())).collect();
                    let dr: Vec<(f64, f64)> = judged.drops.iter().map(|d| { let r = d.range_m.max(0.2); (mx + r * (myaw + d.bearing).cos(), my + r * (myaw + d.bearing).sin()) }).collect();
                    window.push_back((step.t_s, fl, dr));
                    while window.front().is_some_and(|w| step.t_s - w.0 > 2.0) {
                        window.pop_front();
                    }
                    let d2 = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).hypot(a.1 - b.1);
                    for (i, p) in book.iter().enumerate() {
                        let rim = window.iter().flat_map(|w| w.2.iter()).map(|h| d2(*h, *p)).fold(f64::INFINITY, f64::min);
                        if struck_at[i].is_some() || !(0.20..0.45).contains(&rim) {
                            continue;
                        }
                        let frames = window.iter().filter(|w| w.1.iter().any(|q| d2(*q, *p) < 0.07)).count();
                        if frames < 2 {
                            continue;
                        }
                        let ring = (0..8).all(|k| {
                            let a = k as f64 * std::f64::consts::FRAC_PI_4;
                            let c = (p.0 + 0.15 * a.cos(), p.1 + 0.15 * a.sin());
                            window.iter().any(|w| w.1.iter().any(|q| d2(*q, c) < 0.07))
                        });
                        if ring {
                            struck_at[i] = Some(step.t_s);
                        }
                    }
                }
                let mut seen = vec![false; points.len()];
                for d in &judged.drops {
                    let r = d.range_m.max(0.2);
                    let pm = (mx + r * (myaw + d.bearing).cos(), my + r * (myaw + d.bearing).sin());
                    for (k, p) in points.iter().enumerate() {
                        if (pm.0 - p.0).hypot(pm.1 - p.1) >= NEAR_M {
                            continue;
                        }
                        let (tx, ty, tyaw) = t.map_or((f64::NAN, f64::NAN, f64::NAN), |t| (t.1, t.2, t.3));
                        let pt = (tx + r * (tyaw + d.bearing).cos(), ty + r * (tyaw + d.bearing).sin());
                        let th = to_hole(&holes, pt);
                        if !seen[k] {
                            seen[k] = true;
                            hits[k].0 += 1;
                        }
                        hits[k].1.push(th);
                        println!(
                            "{:.1}\t{k}\t{mx:.3}\t{my:.3}\t{:.1}\t{tx:.3}\t{ty:.3}\t{:.1}\t{}\t{:.2}\t{:.1}\t{:.1}\t({:.2},{:.2})\t({:.2},{:.2})\t{th:.2}",
                            step.t_s,
                            myaw.to_degrees(),
                            tyaw.to_degrees(),
                            if d.kind == DropKind::Missing { "missing" } else { "deep" },
                            d.range_m,
                            d.bearing.to_degrees(),
                            judged.head_yaw.to_degrees(),
                            pm.0,
                            pm.1,
                            pt.0,
                            pt.1,
                        );
                    }
                }
            }
            Record::Twin(_) => {}
        }
        true
    })
    .expect("replay");
    for (p, t) in book.iter().zip(&struck_at) {
        if let Some(t) = t {
            eprintln!("book hole ({:.2}, {:.2}), {:.2} m from a true hole: struck at {t:.0} s", p.0, p.1, to_hole(&holes, *p));
        }
    }
    eprintln!("book: {} holes, {} struck", book.len(), struck_at.iter().filter(|t| t.is_some()).count());
    for (k, (p, (n, th))) in points.iter().zip(&hits).enumerate() {
        let mut th = th.clone();
        th.sort_by(f64::total_cmp);
        let med = th.get(th.len() / 2).copied().unwrap_or(f64::NAN);
        eprintln!("point {k} ({:.2}, {:.2}): {n} frames, true-pose landing to a hole median {med:.2} m, own distance to a hole {:.2} m", p.0, p.1, to_hole(&holes, *p));
    }
}
