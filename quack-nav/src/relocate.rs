//! Find the pose before a job walks on it.
//!
//! A duck resting in a room may be picked up and set down anywhere, and
//! odometry does not know (the user's, 2026-10-02). maploc then marks the
//! pose *untrusted* — a watch the map contradicted, a push past a nudge, a
//! sit or a fall while it rested (`maploc::mapper::Mapper::untrusted`) —
//! and searches the whole map for it as at a boot, but only while a job
//! asks: the duck does not wander on its own. A job asked for then (a
//! `go_to`, an exploration) comes here first: the homecoming's
//! walk-and-look search ([`crate::homecoming::find_pose`]) — stand, look,
//! step where the sensor sees room, the shadow map walking along — until
//! maploc confirms a pose under its boot rules (unique, travelled, no
//! valley), and only then is the job itself started, by calling its tool
//! again. A search that does not confirm within its budget ends the job
//! failed, with nothing walked toward the goal; a stop ends it stopped.
//!
//! The job's status says so meanwhile: `explore.state` "relocalizing",
//! [`REASON`], the goal beside it.

use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::explore::{ExploreHandle, State};
use crate::tools::{self, Robot};

/// What `explore.reason` says while the duck finds where it is.
pub const REASON: &str = "the duck may have been moved: finding where it is first";

/// A job to start once the pose is confirmed: the tool call that asked for
/// it, replayed then.
#[derive(Debug, Clone)]
pub struct Request {
    pub tool: String,
    pub args: Value,
    /// Where the job goes, for the status meanwhile.
    pub goal: Option<(f64, f64)>,
}

/// Start the relocator thread and hand its lane to the explore handle.
/// `budget_s` bounds each search.
pub fn spawn(robot: Arc<Mutex<Robot>>, budget_s: f64) {
    let (tx, rx) = mpsc::channel::<Request>();
    let explore = robot.lock().expect("robot poisoned").places.explore.clone();
    explore.set_relocator(tx);
    std::thread::Builder::new()
        .name("relocate".into())
        .spawn(move || {
            // The search is the duck's own motion (see `tools::mark_self_driven`).
            tools::mark_self_driven();
            for request in rx {
                let found = |stop: &dyn Fn() -> bool| crate::homecoming::find_pose(&robot, budget_s, stop);
                let start = || {
                    let mut robot = robot.lock().expect("robot poisoned");
                    tools::execute(&request.tool, &request.args, &mut robot)
                };
                run(&explore, budget_s, found, start);
            }
        })
        .map(|_| ())
        .unwrap_or_else(|e| tracing::warn!(error = %e, "relocate: cannot start the thread"));
}

/// One request: the search, then the job — or the job's end, failed or
/// stopped, with nothing started.
pub fn run(
    explore: &ExploreHandle,
    budget_s: f64,
    search: impl FnOnce(&dyn Fn() -> bool) -> bool,
    start: impl FnOnce() -> Result<Value, String>,
) {
    tracing::info!(budget_s, "relocate: the pose is untrusted; finding where the duck is before the job");
    let began = std::time::Instant::now();
    let stop = || explore.stop_requested();
    let found = search(&stop);
    let took_s = began.elapsed().as_secs_f64();
    if explore.stop_requested() {
        tracing::info!(took_s = format!("{took_s:.0}"), "relocate: stopped before the pose was found");
        explore.relocalized(Err((State::Stopped, "stopped while finding where the duck is; nothing walked toward the goal".into())));
        return;
    }
    if !found {
        tracing::warn!(took_s = format!("{took_s:.0}"), "relocate: the pose was not found; the job does not start");
        explore.relocalized(Err((
            State::Failed,
            format!("the duck may have been moved and could not find where it is within {budget_s:.0} s; it did not walk toward the goal"),
        )));
        return;
    }
    tracing::info!(took_s = format!("{took_s:.0}"), "relocate: the pose is confirmed; starting the job");
    explore.relocalized(Ok(()));
    if let Err(e) = start() {
        tracing::warn!(error = %e, "relocate: the job did not start after the pose was found");
        explore.relocalized(Err((State::Failed, format!("the pose was found, then the job refused: {e}"))));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn request() -> Request {
        Request { tool: "robot.go_to".into(), args: serde_json::json!({"x": 1.0, "y": 2.0}), goal: Some((1.0, 2.0)) }
    }

    /// The journey's first command comes only once the search has
    /// confirmed the pose, and never when it has not.
    #[test]
    fn the_job_starts_only_after_the_pose_is_confirmed() {
        let explore = ExploreHandle::new();
        let (tx, _rx) = mpsc::channel();
        explore.set_relocator(tx);
        explore.relocalize_then(request()).unwrap();
        let s = explore.status();
        assert_eq!(s.state, State::Relocalizing);
        assert_eq!(s.reason.as_deref(), Some(REASON));
        assert_eq!(s.goal, Some((1.0, 2.0)));
        assert_eq!(s.self_started.as_deref(), Some(REASON), "the walk-and-look is the duck's own motion");
        assert!(explore.busy() && !explore.running());
        assert!(explore.relocalize_then(request()).is_err(), "one at a time");

        let log = RefCell::new(Vec::new());
        run(
            &explore,
            60.0,
            |_| {
                log.borrow_mut().push("search: stand, look, step");
                assert_eq!(explore.status().state, State::Relocalizing);
                log.borrow_mut().push("confirmed");
                true
            },
            || {
                log.borrow_mut().push("journey: first move");
                Ok(serde_json::json!({"started": true}))
            },
        );
        assert_eq!(*log.borrow(), vec!["search: stand, look, step", "confirmed", "journey: first move"]);
        assert_eq!(explore.status().state, State::Idle);

        // Not found: the job fails, nothing moves toward the goal.
        explore.relocalize_then(request()).unwrap();
        let moved = RefCell::new(false);
        run(&explore, 60.0, |_| false, || {
            *moved.borrow_mut() = true;
            Ok(Value::Null)
        });
        assert!(!*moved.borrow());
        let s = explore.status();
        assert_eq!(s.state, State::Failed);
        assert!(s.reason.unwrap().contains("could not find where it is"));

        // Stopped while searching: the search sees the stop, nothing starts.
        explore.relocalize_then(request()).unwrap();
        run(
            &explore,
            60.0,
            |stop| {
                explore.request_stop();
                assert!(stop());
                false
            },
            || panic!("a stopped relocalization starts nothing"),
        );
        assert_eq!(explore.status().state, State::Stopped);
    }
}
