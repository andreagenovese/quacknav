//! rl_env: the trainer's environment, over stdin/stdout.
//!
//! N journeys run at once, each on its own thread, each through
//! quack-navd's own journey loop with a brain on the stick's legs that asks
//! the learner: the observation goes out, the move comes back. When a
//! journey ends another starts (a new generated scenario, the simulator's
//! numbers drawn around the calibration), and the first observation of the
//! new one carries the end of the old: `done` and its last reward.
//!
//! The pipe, little-endian:
//!
//! - out, once: `u32 N, u32 OBS_DIM, u32 N_ACTIONS, u32 OBS_VERSION`;
//! - out, every step: `f32 obs[N][OBS_DIM]`, `f32 reward[N]`, `u8 done[N]`,
//!   `u8 expert[N]` (the teacher's move, for imitation), `u8 outcome[N]`
//!   (of the journey that just ended: 1 arrived, 2 arrived off, 3 fell,
//!   4 timed out, 5 failed; 0 running), `u8 family[N]` (of that journey);
//! - in: `b'A'` and `u8 action[N]`; `b'L'` and `u8 level` (the curriculum,
//!   for the journeys that start from now); `b'Q'` to quit.
//!
//!     rl_env --envs 64 [--seed 1] [--level 0] [--calib FILE] [--spread wide|calibrated|none]
//!            [--focus doorway,mixed]   (half the journeys from these families)

use std::io::{BufWriter, Read, Write};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use quack_nav::rlnav::{Action, Brain, N_ACTIONS, OBS_DIM};
use quack_rl::Rng;
use quack_rl::calib::{Calib, Spread};
use quack_rl::episode::{Journey, Reward};
use quack_rl::expert::expert;
use quack_rl::scenarios::{FAMILIES, generate};

struct Msg {
    env: usize,
    obs: Vec<f32>,
    reward: f32,
    done: bool,
    expert: u8,
    outcome: u8,
    family: u8,
}

/// What a journey leaves for the next one's first message.
#[derive(Default)]
struct Carry {
    reward: f64,
    done: bool,
    outcome: u8,
    family: u8,
}

struct Remote {
    env: usize,
    journey_body: quack_rl::body::SimBody,
    geo: Arc<quack_rl::field::Field>,
    teach: Arc<quack_rl::field::Field>,
    tx: Sender<Msg>,
    rx: Arc<Mutex<Receiver<u8>>>,
    state: Mutex<RemoteState>,
}

struct RemoteState {
    reward: Reward,
    last: Option<Action>,
    carry: Carry,
    acted: bool,
    stall_at: Option<(f64, f64)>,
    stalls: u32,
}

impl Brain for Remote {
    fn act(&self, obs: &[f32]) -> Action {
        let mut st = self.state.lock().unwrap();
        let (r, label) = {
            let mut sim = self.journey_body.lock();
            let since = sim.take_since();
            let last = st.last;
            let r = st.reward.step(&sim, &since, last, &self.geo);
            let here = (sim.x, sim.y);
            st.stalls = if st.stall_at.is_some_and(|p| quack_rl::dist(p, here) < 0.01) { st.stalls + 1 } else { 0 };
            st.stall_at = Some(here);
            let label = expert(&sim.world, &self.teach, sim.truth(), st.stalls);
            (r, label)
        };
        // The first move of a journey: the reward and the end of the last.
        let carry = std::mem::take(&mut st.carry);
        let (reward, done) = if st.acted { (r, false) } else { (carry.reward, carry.done) };
        st.acted = true;
        let _ = self.tx.send(Msg {
            env: self.env,
            obs: obs.to_vec(),
            reward: reward as f32,
            done,
            expert: label.index() as u8,
            outcome: carry.outcome,
            family: carry.family,
        });
        let a = self.rx.lock().unwrap().recv().unwrap_or(Action::Wait.index() as u8);
        let action = Action::from_index(a as usize);
        st.last = Some(action);
        action
    }
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opt = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let n: usize = opt("--envs").and_then(|v| v.parse().ok()).unwrap_or(64);
    let seed: u64 = opt("--seed").and_then(|v| v.parse().ok()).unwrap_or(1);
    let level = Arc::new(AtomicU32::new(opt("--level").and_then(|v| v.parse().ok()).unwrap_or(0)));
    let calib = match opt("--calib") {
        Some(p) => Calib::load(&p)?,
        None => Calib::default(),
    };
    let spread = match opt("--spread").as_deref() {
        Some("none") => Spread::none(),
        Some("calibrated") => Spread::calibrated(),
        _ => Spread::default(),
    };
    let focus: Vec<String> = opt("--focus").map(|f| f.split(',').map(str::to_string).collect()).unwrap_or_default();
    let (tx, rx) = channel::<Msg>();
    let mut action_tx: Vec<Sender<u8>> = Vec::new();
    for env in 0..n {
        let (atx, arx) = channel::<u8>();
        action_tx.push(atx);
        let (tx, level, calib, spread, focus) = (tx.clone(), level.clone(), calib.clone(), spread.clone(), focus.clone());
        std::thread::spawn(move || {
            let arx = Arc::new(Mutex::new(arx));
            let mut carry = Carry::default();
            let mut rng = Rng::new(seed.wrapping_mul(1_000_003) ^ env as u64);
            let mut k: u64 = 0;
            loop {
                let ep_seed = seed.wrapping_mul(0x1000_0000) + (k * n as u64 + env as u64);
                k += 1;
                let lv = level.load(Ordering::Relaxed);
                let fam = (!focus.is_empty() && rng.chance(0.5)).then(|| focus[rng.pick(focus.len())].clone());
                let s = generate(ep_seed, lv, fam.as_deref());
                let family = FAMILIES.iter().position(|f| *f == s.family).unwrap_or(0) as u8;
                let c = calib.sample(&spread, &mut rng);
                let mut j = Journey::new(s, c, ep_seed);
                let remote = Arc::new(Remote {
                    env,
                    journey_body: j.body.clone(),
                    geo: j.geo.clone(),
                    teach: j.teach.clone(),
                    tx: tx.clone(),
                    rx: arx.clone(),
                    state: Mutex::new(RemoteState {
                        reward: Reward::new(&j.body.lock(), &j.geo),
                        last: None,
                        carry: std::mem::take(&mut carry),
                        acted: false,
                        stall_at: None,
                        stalls: 0,
                    }),
                });
                let result = j.run(Some(remote.clone() as Arc<dyn Brain>));
                let st = remote.state.lock().unwrap();
                if st.acted {
                    // The last move's reward and the end's, for the next
                    // journey's first message.
                    let mut sim = j.body.lock();
                    let since = sim.take_since();
                    let mut rw = st.reward.clone();
                    let last_r = rw.step(&sim, &since, st.last, &j.geo);
                    carry = Carry { reward: last_r + Reward::terminal(result.outcome), done: true, outcome: result.outcome.code(), family };
                } else {
                    // A journey that ended before its first move: nothing
                    // to learn from, the carry passes on untouched.
                    carry = Carry { reward: st.carry.reward, done: st.carry.done, outcome: st.carry.outcome, family: st.carry.family };
                }
            }
        });
    }
    drop(tx);
    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    out.write_all(&(n as u32).to_le_bytes())?;
    out.write_all(&(OBS_DIM as u32).to_le_bytes())?;
    out.write_all(&(N_ACTIONS as u32).to_le_bytes())?;
    out.write_all(&quack_nav::rlnav::OBS_VERSION.to_le_bytes())?;
    out.flush()?;
    let mut stdin = std::io::stdin().lock();
    let mut slots: Vec<Option<Msg>> = (0..n).map(|_| None).collect();
    loop {
        let mut have = 0;
        while have < n {
            let m = rx.recv()?;
            let e = m.env;
            if slots[e].is_none() {
                have += 1;
            }
            slots[e] = Some(m);
        }
        let msgs: Vec<Msg> = slots.iter_mut().map(|s| s.take().unwrap()).collect();
        for m in &msgs {
            for v in &m.obs {
                out.write_all(&v.to_le_bytes())?;
            }
        }
        for m in &msgs {
            out.write_all(&m.reward.to_le_bytes())?;
        }
        out.write_all(&msgs.iter().map(|m| u8::from(m.done)).collect::<Vec<_>>())?;
        out.write_all(&msgs.iter().map(|m| m.expert).collect::<Vec<_>>())?;
        out.write_all(&msgs.iter().map(|m| m.outcome).collect::<Vec<_>>())?;
        out.write_all(&msgs.iter().map(|m| m.family).collect::<Vec<_>>())?;
        out.flush()?;
        loop {
            let mut op = [0u8; 1];
            stdin.read_exact(&mut op)?;
            match op[0] {
                b'A' => {
                    let mut acts = vec![0u8; n];
                    stdin.read_exact(&mut acts)?;
                    for (e, a) in acts.into_iter().enumerate() {
                        let _ = action_tx[e].send(a);
                    }
                    break;
                }
                b'L' => {
                    let mut l = [0u8; 1];
                    stdin.read_exact(&mut l)?;
                    level.store(u32::from(l[0]), Ordering::Relaxed);
                }
                _ => std::process::exit(0),
            }
        }
    }
}
