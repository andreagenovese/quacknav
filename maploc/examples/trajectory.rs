//! The mapper's trajectory on a recorded session, against the twin's truth:
//! the input for ATE and RPE (`scripts/twin/houses/traj_metrics.py`) with
//! every run of it giving the same numbers — a change to the mapper is
//! measured on the same walk, not on a new one.
//!
//!     cargo run -p maploc --release --features kinematics --example trajectory -- \
//!         <session.mdlg> <pose.tsv> <out.tsv>
//!
//! `pose.tsv` is the pose sampler's file from the live run (`poseerr.py`):
//! its truth columns — and those of `pose.tsv.untracked` beside it, sampled
//! while the live duck was lost — give where the duck really was, on the
//! Unix clock. The replay is joined to them by the recording's own epoch.
//! `out.tsv` is in the sampler's format, with the replayed pose where the
//! live one was, written only while the replayed mapper tracks: so
//! `traj_metrics.py out.tsv` scores the replay exactly as it scores a live
//! run. Four more columns carry the mapper's own covariance (xx, xy, yy,
//! yaw·yaw), which `traj_metrics.py` checks against the error (NEES).
//! `MAP_SESSION=<file>` replays into a saved map, starting lost, as
//! `evaluate` does — the session saved before the recorded one.

use std::io::Write;
use std::path::PathBuf;

use maploc::mapper::{Mapper, MapperConfig};
use maploc::pipeline::{Slam, SlamConfig};

/// (unix s, true x, true y, true yaw or NaN)
fn truth(path: &PathBuf) -> Vec<(f64, f64, f64, f64)> {
    let mut rows = Vec::new();
    for p in [path.clone(), PathBuf::from(format!("{}.untracked", path.display()))] {
        let Ok(text) = std::fs::read_to_string(&p) else { continue };
        for line in text.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() < 5 {
                continue;
            }
            let num = |i: usize| f.get(i).and_then(|v| v.parse::<f64>().ok());
            if let (Some(t), Some(x), Some(y)) = (num(0), num(3), num(4)) {
                rows.push((t, x, y, num(9).unwrap_or(f64::NAN)));
            }
        }
    }
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    rows.dedup_by(|a, b| a.0 == b.0);
    rows
}

fn main() {
    let usage = "usage: trajectory <session.mdlg> <pose.tsv> <out.tsv>";
    let mut args = std::env::args().skip(1);
    let session: PathBuf = args.next().expect(usage).into();
    let truth_path: PathBuf = args.next().expect(usage).into();
    let out_path: PathBuf = args.next().expect(usage).into();
    let truth = truth(&truth_path);
    assert!(!truth.is_empty(), "no truth rows in {}", truth_path.display());

    // Bench knobs for the covariance's calibration (see `uncertainty.rs`).
    let envf = |k: &str, d: f64| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
    let mut cfg = MapperConfig::default();
    let u = &mut cfg.uncertainty;
    u.independent_beams = envf("UNC_BEAMS", u.independent_beams);
    u.match_floor_m = envf("UNC_FLOOR", u.match_floor_m);
    u.xy_var_per_m = envf("UNC_XY", u.xy_var_per_m.sqrt()).powi(2);
    u.yaw_var_per_rad = envf("UNC_YAW_RAD", u.yaw_var_per_rad.sqrt()).powi(2);
    u.yaw_var_per_m = envf("UNC_YAW_M", u.yaw_var_per_m.sqrt()).powi(2);
    u.skip_recent_submaps = envf("UNC_SKIP", u.skip_recent_submaps as f64) as usize;
    let mut mapper = match std::env::var_os("MAP_SESSION") {
        Some(p) => {
            let saved = maploc::session::SessionState::load(std::path::Path::new(&p))
                .expect("read the saved session")
                .expect("the saved session is empty");
            Mapper::resumed_lost(cfg, Slam::from_session(SlamConfig::default(), saved))
        }
        None => Mapper::new(cfg, Slam::new(SlamConfig::default())),
    };
    let mut out = std::io::BufWriter::new(std::fs::File::create(&out_path).expect("create out.tsv"));
    let (mut next, mut written, mut untracked) = (0usize, 0u32, 0u32);
    // `DEGEN_LOG=<file>`: every relocalization the search confirmed or the
    // valley test refused, with the pose's error against the truth and the
    // scan's conditioning there — the valley test and the Hessian's
    // eigenvalues, side by side on the same decisions.
    let mut degen = std::env::var_os("DEGEN_LOG").map(|p| std::io::BufWriter::new(std::fs::File::create(p).expect("DEGEN_LOG")));
    let truth_at = |unix: f64| truth.iter().min_by(|a, b| (a.0 - unix).abs().total_cmp(&(b.0 - unix).abs())).copied();
    let replayed = maploc::bench::replay(&session, &mut mapper, f32::INFINITY, |step| {
        if let Some(w) = degen.as_mut() {
            for note in step.notes {
                let (kind, pose) = match note {
                    maploc::mapper::Note::Relocalized { pose, .. } => ("confirmed", *pose),
                    maploc::mapper::Note::RelocalizeAmbiguous { pose, .. } => ("valley", *pose),
                    _ => continue,
                };
                let (Some((_, composite)), Some(mut grid), Some((tt, tx, ty, _))) =
                    (step.mapper.last_window(), step.mapper.slam().render(), truth_at(step.unix_s))
                else {
                    continue;
                };
                let probe = composite.decimated(512);
                let c = maploc::scan_matcher::conditioning_at(&mut grid, &probe, pose);
                let err = (f64::from(pose.0) - tx).hypot(f64::from(pose.1) - ty);
                writeln!(w, "{:.1}\t{kind}\t{err:.3}\t{:.4}\t{:.1}\t{:.1}\t{}\t{:.1}", step.t_s, c.ratio(), c.l_max, c.l_min, c.n_beams, (step.unix_s - tt).abs()).expect("write");
            }
        }
        while next < truth.len() && truth[next].0 <= step.unix_s {
            let (t, tx, ty, tyaw) = truth[next];
            next += 1;
            if step.unix_s - t > 1.0 {
                // A truth row from before the recording began.
                continue;
            }
            if !step.mapper.tracking() {
                untracked += 1;
                continue;
            }
            let (x, y, yaw) = step.mapper.slam().tracked();
            let (x, y, yaw) = (f64::from(x), f64::from(y), f64::from(yaw));
            let c = step.mapper.pose_covariance().unwrap_or([[f64::NAN; 3]; 3]);
            writeln!(
                out,
                "{t:.1}\t{x:.3}\t{y:.3}\t{tx:.3}\t{ty:.3}\t{:.3}\treplay\tNone\t{yaw:.3}\t{tyaw:.3}\t{:.3e}\t{:.3e}\t{:.3e}\t{:.3e}",
                (x - tx).hypot(y - ty),
                c[0][0],
                c[0][1],
                c[1][1],
                c[2][2]
            )
            .expect("write");
            written += 1;
        }
        true
    })
    .expect("replay the session");
    eprintln!(
        "{}: {:.0} s replayed, {} truth samples scored, {} while the replay was lost; {} windows, {} submaps, {} loops",
        session.display(),
        replayed.t_end_s,
        written,
        untracked,
        mapper.windows(),
        mapper.slam().n_submaps(),
        mapper.slam().n_loops()
    );
}
