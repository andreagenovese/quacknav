//! A queue of jobs (`nav.jobs`; protocol v4).
//!
//! The agent plans "go to the bath, pick up the socks, take them to the
//! laundry, come back to me"; the duck runs it on the board, step by step,
//! each to its end — a journey to its arrival, an exploration to its end —
//! and says how each ended on the `queue` topic. On the first step that
//! fails or is refused the queue stops, unless it was asked to go on. A
//! stop — the user's, `robot.go_to {"stop": true}`, or `nav.jobs {"stop":
//! true}` — ends the step under way and the queue with it.
//!
//! The runner is pure: the daemon hands it the way to call a tool and the
//! way to read the job's state, so it is tested without one. The design is
//! `docs/design/service.md` §5.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The longest queue taken.
pub const MAX_STEPS: usize = 32;
/// How often a running step's job is read.
const POLL: Duration = Duration::from_millis(500);
/// A step that started a job but whose job never showed as running is
/// taken as ended after this.
const START_WAIT: Duration = Duration::from_secs(5);

/// The tools whose answer only starts a job: their step ends when the job does.
pub const STARTS_A_JOB: [&str; 2] = ["robot.go_to", "robot.map_explore"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub tool: String,
    #[serde(default)]
    pub args: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OnFailure {
    #[default]
    Stop,
    Continue,
}

/// How a step ended.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Outcome {
    pub index: usize,
    pub tool: String,
    pub ok: bool,
    pub reason: String,
    pub result: Value,
}

/// What the queue came to.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Report {
    /// `done` (every step run), `failed` (stopped at a step), `stopped` (by a stop).
    pub state: &'static str,
    pub steps: Vec<Outcome>,
}

/// The queue's stop: set by `nav.jobs {"stop": true}`.
#[derive(Clone, Default)]
pub struct Stop(Arc<AtomicBool>);

impl Stop {
    pub fn now(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn asked(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Run the steps in order. `call` runs one tool; `job` reads the job's
/// state and reason (`"running"`, `"done"`, `"failed"`, `"stopped"`, …);
/// `said` hears each step's end as it comes; `sleep` waits (a virtual clock
/// in the tests).
pub fn run(
    steps: &[Step],
    on_failure: OnFailure,
    stop: &Stop,
    mut call: impl FnMut(&str, &Value) -> Result<Value, String>,
    mut job: impl FnMut() -> (String, Option<String>),
    mut said: impl FnMut(&Outcome),
    mut sleep: impl FnMut(Duration),
) -> Report {
    let mut out = Vec::new();
    for (index, step) in steps.iter().enumerate() {
        if stop.asked() {
            return Report { state: "stopped", steps: out };
        }
        let outcome = match call(&step.tool, &step.args) {
            Err(reason) => Outcome { index, tool: step.tool.clone(), ok: false, reason, result: Value::Null },
            Ok(result) if STARTS_A_JOB.contains(&step.tool.as_str()) && result.get("started").and_then(Value::as_bool) == Some(true) => {
                // The job's end is the step's: wait for it.
                let mut seen_running = false;
                let mut waited = Duration::ZERO;
                loop {
                    sleep(POLL);
                    waited += POLL;
                    if stop.asked() {
                        let _ = call(&step.tool, &json!({"stop": true}));
                    }
                    let (state, reason) = job();
                    match state.as_str() {
                        "running" | "relocalizing" | "searching" => seen_running = true,
                        "done" => break Outcome { index, tool: step.tool.clone(), ok: true, reason: reason.unwrap_or_else(|| "done".into()), result },
                        "failed" | "stopped" if seen_running || waited >= START_WAIT => {
                            break Outcome { index, tool: step.tool.clone(), ok: false, reason: reason.unwrap_or(state), result };
                        }
                        _ if !seen_running && waited >= START_WAIT => {
                            break Outcome { index, tool: step.tool.clone(), ok: false, reason: format!("the job never ran ({state})"), result };
                        }
                        _ => {}
                    }
                }
            }
            Ok(result) => Outcome { index, tool: step.tool.clone(), ok: true, reason: "done".into(), result },
        };
        said(&outcome);
        let failed = !outcome.ok;
        out.push(outcome);
        if stop.asked() {
            return Report { state: "stopped", steps: out };
        }
        if failed && on_failure == OnFailure::Stop {
            return Report { state: "failed", steps: out };
        }
    }
    Report { state: "done", steps: out }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn step(tool: &str, args: Value) -> Step {
        Step { tool: tool.into(), args }
    }

    /// A fake duck: a journey runs three polls then arrives, except to "far".
    struct Duck {
        job: RefCell<(String, Option<String>, u32)>,
        calls: RefCell<Vec<String>>,
    }

    impl Duck {
        fn new() -> Self {
            Self { job: RefCell::new(("idle".into(), None, 0)), calls: RefCell::new(Vec::new()) }
        }
        fn call(&self, tool: &str, args: &Value) -> Result<Value, String> {
            self.calls.borrow_mut().push(format!("{tool} {args}"));
            if args.get("stop").and_then(Value::as_bool) == Some(true) {
                *self.job.borrow_mut() = ("stopped".into(), Some("stopped by the user".into()), 0);
                return Ok(json!({"stopped": true}));
            }
            match tool {
                "robot.go_to" if args["place"] == "nowhere" => Err("no place `nowhere`".into()),
                "robot.go_to" => {
                    *self.job.borrow_mut() = ("running".into(), None, if args["place"] == "far" { 99 } else { 3 });
                    Ok(json!({"started": true}))
                }
                _ => Ok(json!({"ok": true})),
            }
        }
        fn read(&self) -> (String, Option<String>) {
            let mut j = self.job.borrow_mut();
            if j.0 == "running" {
                if j.2 == 0 {
                    *j = ("done".into(), Some("arrived".into()), 0);
                } else if j.2 == 99 {
                    *j = ("failed".into(), Some("no way there".into()), 0);
                } else {
                    j.2 -= 1;
                }
            }
            (j.0.clone(), j.1.clone())
        }
    }

    #[test]
    fn the_steps_run_in_order_each_to_its_end() {
        let duck = Duck::new();
        let mut heard = Vec::new();
        let r = run(&[step("robot.go_to", json!({"place": "bath"})), step("robot.remember_place", json!({"name": "x"})), step("robot.go_to", json!({"place": "home"}))], OnFailure::Stop, &Stop::default(), |t, a| duck.call(t, a), || duck.read(), |o| heard.push(o.index), |_| {});
        assert_eq!(r.state, "done");
        assert!(r.steps.iter().all(|s| s.ok));
        assert_eq!(r.steps[0].reason, "arrived");
        assert_eq!(heard, vec![0, 1, 2]);
    }

    #[test]
    fn a_failed_step_stops_the_queue_unless_it_goes_on() {
        let duck = Duck::new();
        let steps = [step("robot.go_to", json!({"place": "far"})), step("robot.go_to", json!({"place": "home"}))];
        let r = run(&steps, OnFailure::Stop, &Stop::default(), |t, a| duck.call(t, a), || duck.read(), |_| {}, |_| {});
        assert_eq!((r.state, r.steps.len(), r.steps[0].reason.as_str()), ("failed", 1, "no way there"));
        let duck = Duck::new();
        let r = run(&steps, OnFailure::Continue, &Stop::default(), |t, a| duck.call(t, a), || duck.read(), |_| {}, |_| {});
        assert_eq!((r.state, r.steps.len()), ("done", 2));
        assert!(!r.steps[0].ok && r.steps[1].ok);
        // A refused call is a failed step.
        let duck = Duck::new();
        let r = run(&[step("robot.go_to", json!({"place": "nowhere"}))], OnFailure::Stop, &Stop::default(), |t, a| duck.call(t, a), || duck.read(), |_| {}, |_| {});
        assert_eq!((r.state, r.steps[0].reason.as_str()), ("failed", "no place `nowhere`"));
    }

    #[test]
    fn a_stop_ends_the_step_under_way_and_the_queue() {
        let duck = Duck::new();
        let stop = Stop::default();
        let polls = RefCell::new(0);
        let r = run(
            &[step("robot.go_to", json!({"place": "far"})), step("robot.go_to", json!({"place": "home"}))],
            OnFailure::Continue,
            &stop,
            |t, a| duck.call(t, a),
            || {
                *polls.borrow_mut() += 1;
                if *polls.borrow() == 1 {
                    stop.now();
                }
                let mut j = duck.job.borrow_mut();
                (j.0.clone(), j.1.take().or(Some("x".into())))
            },
            |_| {},
            |_| {},
        );
        assert_eq!(r.state, "stopped");
        assert!(duck.calls.borrow().iter().any(|c| c.contains("\"stop\":true")), "{:?}", duck.calls.borrow());
    }
}
