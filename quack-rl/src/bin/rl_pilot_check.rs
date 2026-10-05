//! rl_pilot_check: a pilot file read as quack-navd reads it, its probe
//! observations through this build's arithmetic, against the logits the
//! trainer wrote beside them (`meta.probe`): the network that flies is the
//! one that was trained.
//!
//!     rl_pilot_check PILOT.json
fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).ok_or_else(|| anyhow::anyhow!("usage: rl_pilot_check PILOT.json"))?;
    let pilot = quack_nav::rlnav::Pilot::load(std::path::Path::new(&path))?;
    let probe = &pilot.file.meta["probe"];
    let (Some(obs), Some(logits)) = (probe["obs"].as_array(), probe["logits"].as_array()) else {
        anyhow::bail!("{path}: no probe in its meta (export with one)");
    };
    let mut worst = 0.0f64;
    let mut agree = 0;
    for (o, l) in obs.iter().zip(logits) {
        let o: Vec<f32> = o.as_array().into_iter().flatten().filter_map(|v| v.as_f64()).map(|v| v as f32).collect();
        let l: Vec<f64> = l.as_array().into_iter().flatten().filter_map(|v| v.as_f64()).collect();
        let mine = pilot.logits(&o);
        worst = mine.iter().zip(&l).map(|(a, b)| (f64::from(*a) - b).abs()).fold(worst, f64::max);
        let best = |v: &[f64]| v.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).map(|x| x.0);
        if best(&mine.iter().map(|v| f64::from(*v)).collect::<Vec<_>>()) == best(&l) {
            agree += 1;
        }
    }
    println!("{path}: {} probes, the same move {agree}/{}, largest logit difference {worst:.2e}", obs.len(), obs.len());
    anyhow::ensure!(worst < 1e-3 && agree == obs.len(), "the pilot does not compute here what it computed in training");
    Ok(())
}
