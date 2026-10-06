//! One journey: a scenario, the simulated body, quack-navd's own journey
//! loop (`Job::to_goal`, the books of the booked holes, the stick or a
//! brain on its legs) — and how it went, against the truth.

use std::sync::Arc;

use quack_nav::explore::{Body, ExploreHandle, Job, State};
use quack_nav::rlnav::{Action, Brain};
use serde::Serialize;

use crate::body::{Since, Sim, SimBody};
use crate::calib::Calib;
use crate::field::Field;
use crate::scenarios::Scenario;
use crate::dist;

/// Arrived means within this of the goal, on the truth.
pub const ARRIVED_TRUTH_M: f64 = 0.4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Outcome {
    Arrived,
    /// The journey said arrived; the truth is further off.
    ArrivedOff,
    Fell,
    Timeout,
    Failed,
}

impl Outcome {
    pub fn code(self) -> u8 {
        match self {
            Outcome::Arrived => 1,
            Outcome::ArrivedOff => 2,
            Outcome::Fell => 3,
            Outcome::Timeout => 4,
            Outcome::Failed => 5,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct EpisodeResult {
    pub family: String,
    pub level: u32,
    pub seed: u64,
    pub outcome: Outcome,
    pub reason: String,
    pub secs: f64,
    pub route_m: f64,
    pub path_m: f64,
    pub bumps: u32,
    pub mover_bumps: u32,
    pub min_hole_m: f64,
    pub min_static_m: f64,
    /// A fall against something (not into a hole).
    pub tipped: bool,
    pub truth_err_m: f64,
    pub legs: u32,
}

pub struct Journey {
    pub scenario: Scenario,
    pub body: SimBody,
    /// The plain geodesic (the reward) and the expert's field.
    pub geo: Arc<Field>,
    pub teach: Arc<Field>,
    pub budget_s: f64,
    pub route_m: f64,
}

impl Journey {
    pub fn new(scenario: Scenario, calib: Calib, seed: u64) -> Self {
        let geo = Arc::new(Field::build(&scenario.world, scenario.goal, calib.body_r, false));
        let teach = Arc::new(Field::build(&scenario.world, scenario.goal, calib.body_r, true));
        let route_m = geo.at(scenario.start.0, scenario.start.1);
        let route_m = if route_m.is_finite() { route_m } else { dist((scenario.start.0, scenario.start.1), scenario.goal) * 1.5 };
        // A go_to's budget: a minute and 40 s a metre of the true way
        // (the stick makes ~0.1 m/s with its stands), 400 s at most.
        let budget_s = (60.0 + 40.0 * route_m).min(400.0);
        let sim = Sim::new(&scenario, calib, seed);
        Self { scenario, body: SimBody::new(sim), geo, teach, budget_s, route_m }
    }

    /// The journey, start to end, with `brain` on the stick's legs (`None`:
    /// the stick).
    pub fn run(&mut self, brain: Option<Arc<dyn Brain>>) -> EpisodeResult {
        let s = &self.scenario;
        let handle = ExploreHandle::new();
        {
            let mut sim = self.body.lock();
            sim.handle = Some(handle.clone());
            sim.deadline_s = self.budget_s + 30.0;
        }
        let goal_map = (s.goal.0 + s.bias.0, s.goal.1 + s.bias.1);
        let books = s.world.books(s.bias);
        let mut job = Job::to_goal(goal_map, self.budget_s, self.body.now()).with_books(books).with_pilot(brain);
        let mut body = self.body.clone();
        let (state, reason) = job.run(&handle, &mut body);
        let sim = self.body.lock();
        let truth_err_m = dist((sim.x, sim.y), s.goal);
        let outcome = if sim.fell {
            Outcome::Fell
        } else {
            match state {
                State::Done if reason.starts_with("arrived") => {
                    if truth_err_m <= ARRIVED_TRUTH_M {
                        Outcome::Arrived
                    } else {
                        Outcome::ArrivedOff
                    }
                }
                State::Done | State::Stopped => Outcome::Timeout,
                _ => Outcome::Failed,
            }
        };
        EpisodeResult {
            family: s.family.clone(),
            level: s.level,
            seed: s.seed,
            outcome,
            reason,
            secs: sim.t,
            route_m: self.route_m,
            path_m: sim.path_m,
            bumps: sim.bumps,
            mover_bumps: sim.mover_bumps,
            min_hole_m: sim.min_hole_m,
            min_static_m: sim.min_static_m,
            tipped: sim.tipped,
            truth_err_m,
            legs: handle.status().legs,
        }
    }
}

/// The reward, decision to decision: progress down the true way, time,
/// bumps, the shield's refusals, the rim's nearness; and the end's.
#[derive(Debug, Clone)]
pub struct Reward {
    prev_d: f64,
    prev_t: f64,
}

pub const R_PROGRESS: f64 = 1.0;
pub const R_TIME_PER_S: f64 = 0.02;
pub const R_BUMP: f64 = 0.3;
pub const R_MOVER_BUMP: f64 = 0.5;
pub const R_SHIELD: f64 = 0.3;
pub const R_RIM: f64 = 0.5;
/// A turn in place costs a little beyond its time: turning is never
/// "safe" for nothing (a pilot settled in turning one way and back).
pub const R_TURN: f64 = 0.03;
pub const RIM_NEAR_M: f64 = 0.15;

impl Reward {
    pub fn new(sim: &Sim, geo: &Field) -> Self {
        Self { prev_d: geo.at(sim.x, sim.y), prev_t: sim.t }
    }

    pub fn step(&mut self, sim: &Sim, since: &Since, last: Option<Action>, geo: &Field) -> f64 {
        let d = geo.at(sim.x, sim.y);
        let progress = if d.is_finite() && self.prev_d.is_finite() { (self.prev_d - d).clamp(-0.5, 0.5) } else { 0.0 };
        if d.is_finite() {
            self.prev_d = d;
        }
        let mut r = R_PROGRESS * progress - R_TIME_PER_S * (sim.t - self.prev_t);
        self.prev_t = sim.t;
        r -= R_BUMP * f64::from(since.bumps.min(3)) + R_MOVER_BUMP * f64::from(since.mover_bumps.min(3));
        if last.is_some_and(Action::forward) && since.walked_forward_s == 0.0 && since.bumps == 0 && since.mover_bumps == 0 {
            r -= R_SHIELD;
        }
        if matches!(last, Some(Action::TurnLeft) | Some(Action::TurnRight)) {
            r -= R_TURN;
        }
        // A back-off the shield refused (nothing known behind): it stood.
        if last == Some(Action::Back) && since.walked_back_s == 0.0 {
            r -= R_SHIELD;
        }
        let hole = sim.world.hole_clearance(sim.x, sim.y);
        if hole < RIM_NEAR_M {
            r -= R_RIM * (RIM_NEAR_M - hole) / RIM_NEAR_M;
        }
        r
    }

    pub fn terminal(outcome: Outcome) -> f64 {
        match outcome {
            Outcome::Arrived => 3.0,
            Outcome::ArrivedOff => 0.5,
            Outcome::Fell => -10.0,
            Outcome::Timeout | Outcome::Failed => -1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenarios::generate;

    #[test]
    fn the_stick_arrives_in_a_plain_room() {
        let mut ok = 0;
        for seed in 0..6 {
            let s = generate(seed, 0, Some("clutter"));
            let mut j = Journey::new(s, Calib::default(), seed);
            let r = j.run(None);
            assert_ne!(r.outcome, Outcome::Fell, "{r:?}");
            if r.outcome == Outcome::Arrived {
                ok += 1;
            }
        }
        assert!(ok >= 4, "{ok}/6");
    }
}
