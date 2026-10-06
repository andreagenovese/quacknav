//! rl_trace: one generated scenario walked three ways — the pilot, the
//! stick (a journey), and the exploration's travel — through quack-navd's
//! own loop, with what each did: the true track, the contacts, the guard's
//! and the shields' interventions, the books, the last route. JSON for a
//! page to draw.
//!
//!     rl_trace --seed N --family F [--level 3] --pilot FILE [--calib FILE] --out FILE
use std::io::Write;
use std::sync::{Arc, Mutex};

use quack_nav::rlnav::{Brain, Pilot};
use quack_rl::calib::Calib;
use quack_rl::episode::Journey;
use quack_rl::scenarios::generate;
use serde_json::json;

#[derive(Clone, Default)]
struct Buf(Arc<Mutex<Vec<u8>>>);
impl Write for Buf {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The interventions in the log: (kind, x, y) at the map's pose.
fn events(log: &str) -> Vec<serde_json::Value> {
    let pat = [
        ("turned from it", "guard"),
        ("what stops it goes on the books", "shield_book"),
        ("bumped; what the nose met", "bump_book"),
        ("the stick takes the leg", "stick_takes"),
        ("shield=true", "shield"),
    ];
    let mut out = Vec::new();
    for l in log.lines() {
        for (needle, kind) in pat {
            if l.contains(needle)
                && let Some(i) = l.find("at=(")
            {
                let nums: Vec<f64> = l[i + 4..].split(')').next().unwrap_or("").split(',').filter_map(|v| v.trim().parse().ok()).collect();
                if nums.len() >= 2 {
                    out.push(json!({"kind": kind, "x": nums[0], "y": nums[1]}));
                }
            }
        }
    }
    out
}

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let opt = |n: &str| a.iter().position(|x| x == n).and_then(|i| a.get(i + 1)).cloned();
    let seed: u64 = opt("--seed").and_then(|v| v.parse().ok()).unwrap_or(1);
    let level: u32 = opt("--level").and_then(|v| v.parse().ok()).unwrap_or(3);
    let family = opt("--family");
    let calib = match opt("--calib") {
        Some(p) => Calib::load(&p)?,
        None => Calib::default(),
    };
    let pilot: Arc<dyn Brain> = Arc::new(Pilot::load(std::path::Path::new(&opt("--pilot").expect("--pilot FILE")))?);
    let buf = Buf::default();
    let w = buf.clone();
    tracing_subscriber::fmt().with_env_filter("quack_nav=info").with_ansi(false).with_writer(move || w.clone()).init();
    let s = generate(seed, level, family.as_deref());
    let mut runs = Vec::new();
    for (name, brain, explore) in [("pilot", Some(pilot.clone()), false), ("stick", None, false), ("exploration", None, true)] {
        buf.0.lock().unwrap().clear();
        let mut j = Journey::new(s.clone(), calib.clone(), seed);
        j.exploration_travel = explore;
        let r = j.run(brain);
        let log = String::from_utf8_lossy(&buf.0.lock().unwrap()).to_string();
        let sim = j.body.lock();
        let b = s.bias;
        runs.push(json!({
            "name": name, "result": r,
            "track": sim.track, "contacts": sim.contacts,
            "events": events(&log).into_iter().map(|mut e| { e["x"] = json!(e["x"].as_f64().unwrap() - b.0); e["y"] = json!(e["y"].as_f64().unwrap() - b.1); e }).collect::<Vec<_>>(),
            "books": j.books.iter().map(|(p, r)| json!([p.0 - b.0, p.1 - b.1, r])).collect::<Vec<_>>(),
            "route": j.route.iter().map(|p| json!([p.0 - b.0, p.1 - b.1])).collect::<Vec<_>>(),
        }));
    }
    let out = json!({"scenario": s, "runs": runs});
    std::fs::write(opt("--out").unwrap_or_else(|| "trace.json".into()), serde_json::to_string(&out)?)?;
    for r in out["runs"].as_array().unwrap() {
        let res = &r["result"];
        println!("{:<13} {:?} {:.0} s, path {:.1} m, bumps {}, tipped {}", r["name"].as_str().unwrap(), res["outcome"], res["secs"].as_f64().unwrap(), res["path_m"].as_f64().unwrap(), res["bumps"], res["tipped"]);
    }
    Ok(())
}
