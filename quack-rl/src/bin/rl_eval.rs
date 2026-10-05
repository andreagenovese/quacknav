//! rl_eval: the bench. The same generated scenarios, journey after
//! journey through quack-navd's own loop, with the stick, the expert (the
//! teacher that sees the truth: the bound) and any pilot files — and per
//! family: arrived, fell, arrived off, timed out, failed, the time, the
//! bumps, the nearest a hole's rim came.
//!
//!     cargo run --release -p quack-rl --bin rl_eval -- \
//!         [--seeds 40] [--seed0 100000] [--level 3] [--families a,b] \
//!         [--stick] [--expert] [--pilot FILE]... [--reckless random|back|straight]... \
//!         [--calib FILE] \
//!         [--threads 10] [--out results.json]

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use quack_nav::rlnav::{Brain, Pilot};
use quack_rl::calib::Calib;
use quack_rl::episode::{EpisodeResult, Journey, Outcome};
use quack_rl::expert::ExpertBrain;
use quack_rl::scenarios::{FAMILIES, generate};
use serde_json::json;

#[derive(Clone)]
enum Who {
    Stick,
    Expert,
    Pilot(String, Arc<Pilot>),
    /// A reckless brain, for the shields: `random`, `back`, `straight`.
    Reckless(String),
}

impl Who {
    fn name(&self) -> String {
        match self {
            Who::Stick => "stick".into(),
            Who::Expert => "expert".into(),
            Who::Pilot(n, _) => n.clone(),
            Who::Reckless(k) => format!("reckless-{k}"),
        }
    }
}

fn main() -> anyhow::Result<()> {
    if std::env::var_os("RUST_LOG").is_some() {
        let _ = tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).with_writer(std::io::stderr).try_init();
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opt = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let seeds: u64 = opt("--seeds").and_then(|v| v.parse().ok()).unwrap_or(40);
    let seed0: u64 = opt("--seed0").and_then(|v| v.parse().ok()).unwrap_or(100_000);
    let level: u32 = opt("--level").and_then(|v| v.parse().ok()).unwrap_or(3);
    let threads: usize = opt("--threads").and_then(|v| v.parse().ok()).unwrap_or(10);
    let families: Vec<String> = opt("--families").map(|f| f.split(',').map(str::to_string).collect()).unwrap_or_else(|| FAMILIES.iter().map(|s| s.to_string()).collect());
    let calib = match opt("--calib") {
        Some(p) => Calib::load(&p)?,
        None => Calib::default(),
    };
    let mut who = Vec::new();
    if args.iter().any(|a| a == "--stick") {
        who.push(Who::Stick);
    }
    if args.iter().any(|a| a == "--expert") {
        who.push(Who::Expert);
    }
    for (i, a) in args.iter().enumerate() {
        if a == "--pilot"
            && let Some(p) = args.get(i + 1)
        {
            let pilot = Pilot::load(std::path::Path::new(p))?;
            let path = std::path::Path::new(p);
            let mut name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| p.clone());
            // Two pilots of the same name: their directories tell them apart.
            if who.iter().any(|w: &Who| w.name() == name)
                && let Some(dir) = path.parent().and_then(|d| d.file_name())
            {
                name = format!("{}/{name}", dir.to_string_lossy());
            }
            who.push(Who::Pilot(name, Arc::new(pilot)));
        }
    }
    for (i, a) in args.iter().enumerate() {
        if a == "--reckless"
            && let Some(k) = args.get(i + 1)
        {
            who.push(Who::Reckless(k.clone()));
        }
    }
    if who.is_empty() {
        who.push(Who::Stick);
    }
    let mut jobs = Vec::new();
    for fam in &families {
        for k in 0..seeds {
            for w in &who {
                jobs.push((fam.clone(), seed0 + k, w.clone()));
            }
        }
    }
    let jobs = Arc::new(Mutex::new(jobs.into_iter().enumerate().collect::<Vec<_>>()));
    let results: Arc<Mutex<Vec<(String, EpisodeResult)>>> = Arc::new(Mutex::new(Vec::new()));
    std::thread::scope(|scope| {
        for _ in 0..threads {
            let (jobs, results, calib) = (jobs.clone(), results.clone(), calib.clone());
            scope.spawn(move || {
                loop {
                    let Some((_, (fam, seed, w))) = jobs.lock().unwrap().pop() else { break };
                    let s = generate(seed, level, Some(&fam));
                    let mut j = Journey::new(s, calib.clone(), seed);
                    let brain: Option<Arc<dyn Brain>> = match &w {
                        Who::Stick => None,
                        Who::Expert => Some(Arc::new(ExpertBrain::new(j.body.clone(), j.teach.clone()))),
                        Who::Pilot(_, p) => Some(p.clone() as Arc<dyn Brain>),
                        Who::Reckless(k) => Some(Arc::new(quack_rl::brains::Reckless::new(k, seed)) as Arc<dyn Brain>),
                    };
                    let r = j.run(brain);
                    results.lock().unwrap().push((w.name(), r));
                }
            });
        }
    });
    let results = Arc::try_unwrap(results).ok().unwrap().into_inner().unwrap();
    // Per family and brain.
    let mut table: BTreeMap<(String, String), Vec<&EpisodeResult>> = BTreeMap::new();
    for (w, r) in &results {
        table.entry((r.family.clone(), w.clone())).or_default().push(r);
        table.entry(("ALL".into(), w.clone())).or_default().push(r);
    }
    println!("{:<10} {:<14} {:>4} {:>7} {:>5} {:>5} {:>5} {:>5} {:>7} {:>6} {:>6} {:>7}", "family", "brain", "n", "arrive%", "fell", "off", "tout", "fail", "secs", "bumps", "mbump", "rim_m");
    let mut summary = Vec::new();
    for ((fam, w), rs) in &table {
        let n = rs.len() as f64;
        let count = |o: Outcome| rs.iter().filter(|r| r.outcome == o).count();
        let arrived: Vec<_> = rs.iter().filter(|r| r.outcome == Outcome::Arrived).collect();
        let secs = if arrived.is_empty() { 0.0 } else { arrived.iter().map(|r| r.secs).sum::<f64>() / arrived.len() as f64 };
        let bumps = rs.iter().map(|r| f64::from(r.bumps)).sum::<f64>() / n;
        let mbumps = rs.iter().map(|r| f64::from(r.mover_bumps)).sum::<f64>() / n;
        let rim = rs.iter().map(|r| r.min_hole_m).filter(|v| v.is_finite()).fold(f64::INFINITY, f64::min);
        println!(
            "{fam:<10} {w:<14} {:>4} {:>7.1} {:>5} {:>5} {:>5} {:>5} {:>7.1} {:>6.2} {:>6.2} {:>7.3}",
            rs.len(),
            100.0 * arrived.len() as f64 / n,
            count(Outcome::Fell),
            count(Outcome::ArrivedOff),
            count(Outcome::Timeout),
            count(Outcome::Failed),
            secs,
            bumps,
            mbumps,
            if rim.is_finite() { rim } else { -1.0 }
        );
        summary.push(json!({"family": fam, "brain": w, "n": rs.len(), "arrived": arrived.len(), "fell": count(Outcome::Fell),
            "arrived_off": count(Outcome::ArrivedOff), "timeout": count(Outcome::Timeout), "failed": count(Outcome::Failed),
            "mean_secs_arrived": secs, "bumps_per_ep": bumps, "mover_bumps_per_ep": mbumps, "min_rim_m": if rim.is_finite() { rim } else { -1.0 }}));
    }
    if let Some(out) = opt("--out") {
        let episodes: Vec<_> = results.iter().map(|(w, r)| json!({"brain": w, "result": r})).collect();
        std::fs::write(&out, serde_json::to_string_pretty(&json!({"level": level, "seeds": seeds, "seed0": seed0, "calib": calib, "summary": summary, "episodes": episodes}))?)?;
    }
    Ok(())
}
