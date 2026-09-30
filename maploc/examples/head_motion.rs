//! What a head in motion costs the map: every depth frame of a recording
//! with the head's measured velocities (robotd API 36), placed with the
//! replayed pose and scored against the map the whole replay drew, binned
//! by how fast the head yaw was turning when the frame was taken.
//!
//!     cargo run -p maploc --release --features kinematics --example head_motion -- <session.mdlg>
//!
//! Two passes of the same deterministic replay: the first draws the final
//! map, the second scores each frame on it at the pose the replay held
//! then (`relocalize::score_pose`: mean distance of the frame's beams to
//! the nearest wall, over the beams landing where the map has an opinion).
//! A frame is flattened with the head of the robot-state sample before it,
//! as the old pairing did: the velocity is what the pairing's error scales
//! with. `MAP_SESSION` / `MAP_LOAD_AT_S` as in `trajectory`.
//!
//! x16's eight sessions on the twin (2026-09-30), 104 000 frames: 3.2-3.3 cm
//! from 0.05 to 1 rad/s, where 95 % of the frames are; 3.46 cm from 1 to
//! 2 rad/s (1 % of them); 2.4 cm with the head still, at the stands the
//! tracking corrects. A head in motion costs the map next to nothing: the
//! velocities of API 36 have no mapping gain to give.

use std::path::PathBuf;

use kinematics::tof::{Posture, Reprojector};
use maploc::mapper::{Mapper, MapperConfig};
use maploc::pipeline::{Slam, SlamConfig};
use maploc::replay::{OdomRecord, Record};
use maploc::submap::Scan;

const BINS: [f32; 6] = [0.05, 0.2, 0.5, 1.0, 2.0, f32::INFINITY];

fn fresh(load: &mut Option<(f32, Box<dyn FnOnce(&Mapper) -> Mapper>)>) -> Mapper {
    let cfg = MapperConfig::default();
    let slam_cfg = SlamConfig::default();
    let load_at: Option<f32> = std::env::var("MAP_LOAD_AT_S").ok().and_then(|v| v.parse().ok());
    let saved = std::env::var_os("MAP_SESSION").map(|p| {
        maploc::session::SessionState::load(std::path::Path::new(&p)).expect("read the saved session").expect("the saved session is empty")
    });
    match (saved, load_at) {
        (Some(mut saved), Some(at)) => {
            let (c, s) = (cfg.clone(), slam_cfg.clone());
            *load = Some((
                at,
                Box::new(move |f: &Mapper| {
                    saved.tracked = maploc::pose_graph::compose(saved.tracked, f.slam().tracked());
                    Mapper::resumed_lost(c, Slam::from_session(s, saved))
                }),
            ));
            Mapper::new(cfg, Slam::new(slam_cfg))
        }
        (Some(saved), None) => Mapper::resumed_lost(cfg, Slam::from_session(slam_cfg, saved)),
        (None, _) => Mapper::new(cfg, Slam::new(slam_cfg)),
    }
}

fn main() {
    let session: PathBuf = std::env::args().nth(1).expect("usage: head_motion <session.mdlg>").into();
    let mut load = None;
    let mut mapper = fresh(&mut load);
    maploc::bench::replay_loading(&session, &mut mapper, f32::INFINITY, load, |_| true).expect("replay");
    let mut map = mapper.slam().render().expect("the replay drew a map");

    let rp = Reprojector::alpha();
    let mut load = None;
    let mut mapper = fresh(&mut load);
    let mut latest: Option<OdomRecord> = None;
    // Per bin: frames, sum of residuals, sum of observed beams.
    let mut bins = [(0u32, 0.0f64, 0u64); BINS.len()];
    let mut no_vel = 0u32;
    maploc::bench::replay_loading(&session, &mut mapper, f32::INFINITY, load, |step| {
        match step.record {
            Record::Odom(o) => latest = Some(*o),
            Record::Tof(f) => {
                let Some(o) = latest else { return true };
                if o.moving || !step.mapper.tracking() {
                    return true;
                }
                let Some(v) = o.head_vel else {
                    no_vel += 1;
                    return true;
                };
                let mut ranges = [None; kinematics::tof::ROWS * kinematics::tof::COLS];
                for (k, (r, s)) in f.ranges_m.iter().flatten().zip(f.status.iter().flatten()).enumerate() {
                    if (*s == 5 || *s == 9) && r.is_finite() && *r > 0.0 {
                        ranges[k] = Some(f64::from(*r));
                    }
                }
                let posture = Posture { gravity: o.gravity.map(f64::from), trunk_height_m: (o.trunk_z > 0.02).then_some(f64::from(o.trunk_z)) };
                let flat = maploc::flat::flatten(&rp, &ranges, o.head.map(f64::from), &posture);
                if flat.angles_body.len() < 8 {
                    return true;
                }
                let scan = Scan::from_polar(&flat.angles_body, &flat.ranges, flat.sensor_xy, 1e-3);
                let a = maploc::relocalize::score_pose(&mut map, &scan, step.mapper.slam().tracked(), 0.3, 150, 50);
                if a.n_observed < 8 {
                    return true;
                }
                let b = BINS.iter().position(|edge| v[2].abs() < *edge).expect("the last bin is open");
                bins[b].0 += 1;
                bins[b].1 += f64::from(a.mean_residual_m);
                bins[b].2 += u64::from(a.n_observed);
            }
            Record::Twin(_) => {}
        }
        true
    })
    .expect("replay");
    println!("head yaw rate (rad/s)\tframes\tmean residual (cm)\tbeams per frame");
    let mut lo = 0.0;
    for (edge, (n, sum, beams)) in BINS.iter().zip(bins) {
        if n > 0 {
            println!("{lo:.2}-{edge:.2}\t{n}\t{:.2}\t{:.1}", 100.0 * sum / f64::from(n), beams as f64 / f64::from(n));
        }
        lo = *edge;
    }
    if no_vel > 0 {
        eprintln!("{no_vel} frames had no head velocity (a recording from before API 36)");
    }
}
