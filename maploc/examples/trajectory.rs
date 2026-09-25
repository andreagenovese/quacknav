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
//! run. `MAP_SESSION=<file>` replays into a saved map, starting lost, as
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

    let mut mapper = match std::env::var_os("MAP_SESSION") {
        Some(p) => {
            let saved = maploc::session::SessionState::load(std::path::Path::new(&p))
                .expect("read the saved session")
                .expect("the saved session is empty");
            Mapper::resumed_lost(MapperConfig::default(), Slam::from_session(SlamConfig::default(), saved))
        }
        None => Mapper::new(MapperConfig::default(), Slam::new(SlamConfig::default())),
    };
    let mut out = std::io::BufWriter::new(std::fs::File::create(&out_path).expect("create out.tsv"));
    let (mut next, mut written, mut untracked) = (0usize, 0u32, 0u32);
    let replayed = maploc::bench::replay(&session, &mut mapper, f32::INFINITY, |step| {
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
            writeln!(
                out,
                "{t:.1}\t{x:.3}\t{y:.3}\t{tx:.3}\t{ty:.3}\t{:.3}\treplay\tNone\t{yaw:.3}\t{tyaw:.3}",
                (x - tx).hypot(y - ty)
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
