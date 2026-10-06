//! Brains that never think, for the shields: whatever they ask, the duck
//! must not fall (`rl_eval --reckless`, and the test below).

use std::sync::Mutex;

use quack_nav::rlnav::{Action, Brain, N_ACTIONS};

use crate::Rng;

pub struct Reckless {
    pub kind: String,
    rng: Mutex<Rng>,
}

impl Reckless {
    /// `random`, `back` (always backs off) or `straight` (always steps ahead).
    pub fn new(kind: &str, seed: u64) -> Self {
        Self { kind: kind.to_string(), rng: Mutex::new(Rng::new(seed)) }
    }
}

impl Brain for Reckless {
    fn act(&self, _obs: &[f32]) -> Action {
        match self.kind.as_str() {
            "back" => Action::Back,
            "straight" => Action::Step(0),
            _ => Action::from_index(self.rng.lock().unwrap().pick(N_ACTIONS)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::calib::Calib;
    use crate::episode::{Journey, Outcome};
    use crate::scenarios::generate;

    /// The shields hold: beside stairwells (booked or not, the map off or
    /// not) and in the mixed houses, no reckless brain falls into a hole
    /// (tipping over against something is the contact's risk, counted
    /// apart: `tipped`).
    #[test]
    fn no_reckless_brain_falls() {
        for fam in ["stairwell", "mixed"] {
            for seed in 0..12u64 {
                for kind in ["random", "back", "straight"] {
                    let s = generate(500_000 + seed, 3, Some(fam));
                    let mut j = Journey::new(s, Calib::default(), seed);
                    let r = j.run(Some(Arc::new(Reckless::new(kind, seed))));
                    assert!(r.outcome != Outcome::Fell || r.tipped, "{kind} fell into a hole: {fam} seed {}", 500_000 + seed);
                }
            }
        }
    }
}
