//! The gait ledger on the simulator: a walk the priors get wrong on
//! purpose, and whether the ledger finds it from the journeys alone.
//!
//!     ledger_check [--gait velstand] [--daemon 0.17] [--speed 1.25] [--veer 0.12] [--ypu 0.8] \
//!                  [--journeys 12] [--seed0 100000] [--family F] [--base calib.json] [--write-calib FILE]
//!
//! `--base` perturbs a fitted calibration (the twin's) instead of the
//! walk's prior.
//! `--write-calib` writes the changed calibration for `rl_eval --calib`.
//! The calibration is the walk's (`Calib::for_walk`) with its forward
//! speed, its straight veer and its yaw per unit changed by the factors
//! (the veer absolute, rad/s); the stick walks the journeys one after the
//! other with the process's ledger open in memory, and after each the
//! ledger's numbers are printed beside the simulator's truth. One thread:
//! the ledger is the process's, as on the duck.
use quack_rl::calib::Calib;
use quack_rl::episode::{Journey, Outcome};
use quack_rl::scenarios::generate;
use serde_json::Value;

fn main() -> anyhow::Result<()> {
    if std::env::var_os("RUST_LOG").is_some() {
        let _ = tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).with_writer(std::io::stderr).try_init();
    }
    let a: Vec<String> = std::env::args().skip(1).collect();
    let opt = |k: &str| a.iter().position(|x| x == k).and_then(|i| a.get(i + 1)).cloned();
    let num = |k: &str, d: f64| opt(k).and_then(|v| v.parse::<f64>().ok()).unwrap_or(d);
    let gait = opt("--gait").unwrap_or_else(|| "velstand".into());
    let daemon = opt("--daemon").unwrap_or_else(|| "0.17".into());
    quack_duck::gait::set_active(quack_duck::gait::Profile::from_name(&gait).expect("--gait alpha|velstand"));
    quack_duck::daemon::set_active(quack_duck::daemon::Line::parse(&daemon).expect("--daemon 0.16|0.17"));
    let mut calib = match opt("--base") {
        Some(p) => Calib::load(&p)?,
        None => Calib::for_walk(),
    };
    calib.speed_at_03 *= num("--speed", 1.25);
    calib.straight_veer = num("--veer", 0.12);
    calib.yaw_per_unit *= num("--ypu", 0.8);
    calib.gait = Some(gait.clone());
    calib.daemon = Some(daemon.clone());
    if let Some(p) = opt("--write-calib") {
        std::fs::write(&p, serde_json::to_string_pretty(&serde_json::json!({"calib": calib}))?)?;
        println!("wrote {p}");
    }
    let journeys = num("--journeys", 12.0) as u64;
    let seed0 = num("--seed0", 100_000.0) as u64;
    let family = opt("--family");
    quack_nav::ledger::init(None, 0.0, (1.0, 1.0));
    let prior = quack_duck::gait::priors();
    println!("truth: speed_at_03 {:.4} (prior {:.4})  veer {:+.3} rad/s → trim {:+.3} (prior 0)  yaw_per_unit {:.3} (prior {:.3})  turn_coast {:.3} (prior {:.3})",
        calib.speed_at_03, prior.m_per_s, calib.straight_veer, -calib.straight_veer / calib.yaw_per_unit, calib.yaw_per_unit, prior.yaw_rate_per_unit, calib.turn_coast_rad, prior.coast_prior_rad);
    println!("{:>3} {:>8} {:>7} | {:>8} {:>3} | {:>8} {:>3} | {:>8} {:>3} | {:>8} {:>3} | {:>8} {:>3}", "n", "outcome", "secs", "m_per_s", "k", "trim", "k", "ypu", "k", "coast", "k", "turn", "k");
    for k in 0..journeys {
        let s = generate(seed0 + k, 3, family.as_deref());
        let mut j = Journey::new(s, calib.clone(), seed0 + k);
        let r = j.run(None);
        let rep = quack_nav::ledger::report();
        let cell = |name: &str| -> (String, String) {
            let e = &rep["numbers"][name];
            let v = match &e["learned"] { Value::Null => "-".to_string(), v => format!("{:.4}", v.as_f64().unwrap_or(0.0)) };
            (v, e["samples"].as_u64().unwrap_or(0).to_string())
        };
        let (m, mk) = cell("m_per_s");
        let (t, tk) = cell("yaw_trim");
        let (y, yk) = cell("yaw_rate_per_unit");
        let (c, ck) = cell("coast_rad");
        let (u, uk) = cell("turn_rate_rad_s");
        let outcome = match r.outcome { Outcome::Arrived => "arrived", _ => "other" };
        println!("{:>3} {:>8} {:>7.1} | {:>8} {:>3} | {:>8} {:>3} | {:>8} {:>3} | {:>8} {:>3} | {:>8} {:>3}", k + 1, outcome, r.secs, m, mk, t, tk, y, yk, c, ck, u, uk);
    }
    let n = quack_duck::gait::numbers();
    println!("in force now: m_per_s {:.4} yaw_rate_per_unit {:.3} trim {:+.3} coast {:.3}", n.m_per_s, n.yaw_rate_per_unit, quack_duck::gait::learned().yaw_trim.unwrap_or(0.0), n.coast_prior_rad);
    Ok(())
}
