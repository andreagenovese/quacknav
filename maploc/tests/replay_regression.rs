//! A recorded walk, replayed through the mapper on every build.
//!
//! `fixtures/house2-10min.mdlg` is the first ten minutes of the release's
//! house2 exploration on the MuJoCo twin (2026-09-24): 9.5 m walked, a map
//! grown from nothing, loops closed. `house2-10min.truth.tsv` is where the
//! twin says the duck really was, every 5 s. The test replays the recording,
//! scores the tracked pose against the truth, and holds the build to two
//! things:
//!
//! - **Determinism.** Two replays in one process give the same trajectory,
//!   to the bit. A bench number stands for a change to the mapper only if
//!   the bench cannot move on its own.
//! - **No regression.** The trajectory error (ATE RMSE, and the worst
//!   sample) stays within 10 % of the golden figures, and the replay never
//!   loses its pose. A change that improves them is welcome and should
//!   update the golden file, so the next change is held to the new bar:
//!
//!       UPDATE_GOLDEN=1 cargo test -p maploc --features kinematics --release --test replay_regression
//!
//! Counts (windows, submaps, loops) are reported beside the golden ones but
//! do not fail the test: they move with any honest change to the mapper and
//! say nothing, alone, about whether the change was good.
#![cfg(feature = "kinematics")]

use std::path::PathBuf;

use maploc::mapper::{Mapper, MapperConfig};
use maploc::pipeline::{Slam, SlamConfig};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// (unix s, true x, true y)
fn truth() -> Vec<(f64, f64, f64)> {
    let text = std::fs::read_to_string(fixtures().join("house2-10min.truth.tsv")).expect("truth fixture");
    text.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            Some((f.first()?.parse().ok()?, f.get(3)?.parse().ok()?, f.get(4)?.parse().ok()?))
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
struct Run {
    /// The tracked pose at each truth sample (None while lost), bit for bit.
    poses: Vec<Option<(u32, u32, u32)>>,
    ate_rmse_m: f64,
    ate_max_m: f64,
    lost: usize,
    windows: u32,
    submaps: usize,
    loops: usize,
}

fn run() -> Run {
    let truth = truth();
    let mut mapper = Mapper::new(MapperConfig::default(), Slam::new(SlamConfig::default()));
    let (mut next, mut poses, mut errs) = (0usize, Vec::new(), Vec::new());
    maploc::bench::replay(&fixtures().join("house2-10min.mdlg"), &mut mapper, f32::INFINITY, |step| {
        while next < truth.len() && truth[next].0 <= step.unix_s {
            let (_, tx, ty) = truth[next];
            next += 1;
            if !step.mapper.tracking() {
                poses.push(None);
                continue;
            }
            let (x, y, yaw) = step.mapper.slam().tracked();
            poses.push(Some((x.to_bits(), y.to_bits(), yaw.to_bits())));
            errs.push((f64::from(x) - tx).hypot(f64::from(y) - ty));
        }
        true
    })
    .expect("replay the fixture");
    let n = errs.len().max(1) as f64;
    Run {
        lost: poses.iter().filter(|p| p.is_none()).count(),
        poses,
        ate_rmse_m: (errs.iter().map(|e| e * e).sum::<f64>() / n).sqrt(),
        ate_max_m: errs.iter().copied().fold(0.0, f64::max),
        windows: mapper.windows(),
        submaps: mapper.slam().n_submaps(),
        loops: mapper.slam().n_loops(),
    }
}

#[test]
fn the_recorded_walk_replays_the_same_and_no_worse() {
    let a = run();
    let b = run();
    assert_eq!(a, b, "two replays of one recording differ: the bench is not deterministic");
    assert!(a.poses.len() >= 100, "only {} truth samples scored", a.poses.len());

    let golden_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/replay_house2_10min.json");
    let got = serde_json::json!({
        "ate_rmse_m": (a.ate_rmse_m * 1e4).round() / 1e4,
        "ate_max_m": (a.ate_max_m * 1e4).round() / 1e4,
        "lost_samples": a.lost,
        "samples": a.poses.len(),
        "windows": a.windows,
        "submaps": a.submaps,
        "loops": a.loops,
    });
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(golden_path.parent().unwrap()).unwrap();
        std::fs::write(&golden_path, serde_json::to_string_pretty(&got).unwrap() + "\n").unwrap();
        return;
    }
    let want: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&golden_path).expect("golden file (UPDATE_GOLDEN=1 to create it)")).unwrap();
    let f = |v: &serde_json::Value, k: &str| v[k].as_f64().unwrap_or(f64::NAN);
    eprintln!("replay house2 10 min: now {got}, golden {want}");
    assert_eq!(a.lost, 0, "the replay lost its pose on {} samples", a.lost);
    let bar = |k: &str| f(&want, k) * 1.10 + 0.002;
    assert!(a.ate_rmse_m <= bar("ate_rmse_m"), "ATE RMSE {:.4} m, golden {:.4}: worse by more than 10 %", a.ate_rmse_m, f(&want, "ate_rmse_m"));
    assert!(a.ate_max_m <= bar("ate_max_m"), "worst sample {:.4} m, golden {:.4}: worse by more than 10 %", a.ate_max_m, f(&want, "ate_max_m"));
    if a.ate_rmse_m < f(&want, "ate_rmse_m") * 0.9 {
        eprintln!("ATE improved by more than 10 % — UPDATE_GOLDEN=1 to hold the next change to it");
    }
}
