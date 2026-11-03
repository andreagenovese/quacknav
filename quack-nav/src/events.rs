//! Events pushed to subscribers (`nav.subscribe`, protocol v2).
//!
//! The facts are the ones the daemon already keeps — the job's status
//! (`ExploreHandle`), the map's pose (`MapWatch`) — read by one watcher a
//! few times a second and turned into an event where they change. Nothing
//! in the navigation calls in here: an event is an observation, and a slow
//! or absent subscriber cannot hold a job up. Each subscriber has a bounded
//! queue; one that does not read is dropped and its connection closed.
//!
//! The design is `docs/design/service.md` §3.

use std::collections::HashSet;
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use quack_nav_proto::{Event, topic};
use serde_json::{Value, json};

/// Events a subscriber may fall behind by before it is dropped.
pub const QUEUE: usize = 256;
/// How often the watcher reads the facts.
const WATCH_EVERY: Duration = Duration::from_millis(200);
/// How long a job that started without a goal is given to show one
/// before it is said to be an exploration.
const GOAL_WAIT: Duration = Duration::from_secs(3);
// (A journey's goal came 1.7 s after its start on the twin, 2026-11-02.)
/// `job.progress` at most this often.
const PROGRESS_EVERY: Duration = Duration::from_secs(1);

struct Subscriber {
    topics: HashSet<String>,
    tx: SyncSender<Event>,
    seq: u64,
}

/// The daemon's subscribers.
#[derive(Clone)]
pub struct Hub {
    subs: Arc<Mutex<Vec<Subscriber>>>,
    origin: Instant,
}

impl Default for Hub {
    fn default() -> Self {
        Self { subs: Arc::new(Mutex::new(Vec::new())), origin: Instant::now() }
    }
}

impl Hub {
    /// A new subscriber on these topics; its events arrive on the receiver.
    pub fn subscribe(&self, topics: &[String]) -> Receiver<Event> {
        let (tx, rx) = sync_channel(QUEUE);
        self.subs.lock().unwrap_or_else(|e| e.into_inner()).push(Subscriber { topics: topics.iter().cloned().collect(), tx, seq: 0 });
        rx
    }

    /// One event to every subscriber of its topic. A subscriber whose queue
    /// is full, or whose receiver is gone, is dropped.
    pub fn publish(&self, topic: &str, data: Value) {
        let t = (self.origin.elapsed().as_secs_f64() * 1000.0).round() / 1000.0;
        let mut subs = self.subs.lock().unwrap_or_else(|e| e.into_inner());
        subs.retain_mut(|s| {
            if !s.topics.contains(topic) {
                return true;
            }
            s.seq += 1;
            match s.tx.try_send(Event { topic: topic.to_string(), seq: s.seq, t, data: data.clone() }) {
                Ok(()) => true,
                Err(TrySendError::Full(_)) => {
                    tracing::warn!(topic, queue = QUEUE, "events: a subscriber fell behind; dropped");
                    false
                }
                Err(TrySendError::Disconnected(_)) => false,
            }
        });
    }

    /// How many subscribers there are now.
    pub fn len(&self) -> usize {
        self.subs.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// What the watcher saw last, to tell a change.
#[derive(Default)]
struct Seen {
    state: Option<String>,
    /// The job's goal as last said: a journey's start says its state a
    /// moment before its goal is set, so the goal's arrival is said too.
    goal: Option<Value>,
    /// A job running without a goal yet, since when: said as an
    /// exploration only if no goal comes within [`GOAL_WAIT`].
    goalless_since: Option<Instant>,
    tracking: Option<bool>,
    untrusted: Option<bool>,
    seated: bool,
    question: bool,
    progress_at: Option<Instant>,
}

fn kind_of(status: &Value) -> &'static str {
    if status.get("self_started").and_then(Value::as_bool) == Some(true) {
        "own"
    } else if !status.get("goal").is_none_or(Value::is_null) {
        "journey"
    } else {
        "explore"
    }
}

fn route_m(status: &Value) -> Option<f64> {
    let pts: Vec<(f64, f64)> = status.get("route")?.as_array()?.iter().filter_map(|p| Some((p.get(0)?.as_f64()?, p.get(1)?.as_f64()?))).collect();
    Some((pts.windows(2).map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1)).sum::<f64>() * 100.0).round() / 100.0 + 0.0)
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// The facts read once: the job's status, and the map's pose when there is
/// a map — `(tracking, untrusted, seated, pose)`.
pub struct Facts {
    pub job: Value,
    pub pose: Option<(bool, bool, bool, (f64, f64, f64))>,
}

/// The events one reading of the facts makes, against what was seen
/// before. Pure, so it is tested without a daemon.
fn changes(seen: &mut Seen, facts: &Facts, now: Instant) -> Vec<(&'static str, Value)> {
    let mut out = Vec::new();
    let job = &facts.job;
    let state = job.get("state").and_then(Value::as_str).unwrap_or("idle").to_string();
    let goal = job.get("goal").cloned().unwrap_or(Value::Null);
    let state_changed = seen.state.as_deref() != Some(state.as_str());
    let goal_came = !state_changed && state == "running" && seen.goal.as_ref() != Some(&goal);
    let say = |out: &mut Vec<(&'static str, Value)>| {
        out.push((topic::JOB, json!({"state": state, "kind": kind_of(job), "goal": goal, "reason": job.get("reason"), "self_started": job.get("self_started")})));
    };
    if state_changed || goal_came {
        if state_changed {
            seen.progress_at = None;
        }
        // A journey shows its state a moment before its goal: a start with
        // no goal waits for one, and is an exploration only past GOAL_WAIT.
        if seen.state.is_some() {
            if state == "running" && goal.is_null() && kind_of(job) != "own" {
                seen.goalless_since.get_or_insert(now);
            } else {
                seen.goalless_since = None;
                say(&mut out);
            }
        }
        seen.state = Some(state.clone());
    } else if seen.goalless_since.is_some_and(|t| now.duration_since(t) >= GOAL_WAIT) {
        seen.goalless_since = None;
        say(&mut out);
    }
    if state != "running" {
        seen.goalless_since = None;
    }
    seen.goal = Some(goal.clone());
    if state == "running" && seen.progress_at.is_none_or(|t| now.duration_since(t) >= PROGRESS_EVERY) {
        if seen.progress_at.is_some() {
            out.push((
                topic::JOB_PROGRESS,
                json!({"legs": job.get("legs"), "refusals": job.get("refusals"), "target_distance_m": job.get("target_distance_m"), "route_m": route_m(job)}),
            ));
        }
        seen.progress_at = Some(now);
    }
    let question = job.get("question_pending").and_then(Value::as_bool).unwrap_or(false);
    if let Some((tracking, untrusted, seated, (x, y, yaw))) = facts.pose {
        let pose = json!([round2(x), round2(y), round2(yaw)]);
        if (seen.tracking.is_some() && seen.tracking != Some(tracking)) || (seen.untrusted.is_some() && seen.untrusted != Some(untrusted)) {
            out.push((topic::POSE, json!({"tracking": tracking, "untrusted": untrusted, "pose": pose})));
        }
        seen.tracking = Some(tracking);
        seen.untrusted = Some(untrusted);
        let busy = matches!(state.as_str(), "running" | "relocalizing" | "searching");
        if seated && !seen.seated && busy {
            out.push((topic::FALL, json!({"pose": pose})));
        }
        seen.seated = seated;
        if question && !seen.question {
            out.push((topic::QUESTION, json!({"pose": pose})));
        }
    } else if question && !seen.question {
        out.push((topic::QUESTION, json!({"pose": Value::Null})));
    }
    seen.question = question;
    out
}

/// The watcher: reads the facts every [`WATCH_EVERY`] with `read`, and
/// publishes what changed. Reads nothing while nobody listens.
pub fn spawn(hub: Hub, read: impl Fn() -> Facts + Send + 'static) {
    std::thread::Builder::new()
        .name("events".into())
        .spawn(move || {
            let mut seen = Seen::default();
            loop {
                std::thread::sleep(WATCH_EVERY);
                if hub.is_empty() {
                    seen = Seen::default();
                    continue;
                }
                for (topic, data) in changes(&mut seen, &read(), Instant::now()) {
                    hub.publish(topic, data);
                }
            }
        })
        .expect("the events thread starts");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(state: &str, goal: Option<(f64, f64)>, tracking: bool, seated: bool, question: bool) -> Facts {
        Facts {
            job: json!({"state": state, "goal": goal.map(|(x, y)| json!([x, y])), "reason": null, "self_started": false, "legs": 3, "refusals": 0, "route": [[0.0, 0.0], [3.0, 4.0]], "question_pending": question}),
            pose: Some((tracking, false, seated, (1.0, 2.0, 0.5))),
        }
    }

    #[test]
    fn a_journey_is_heard_from_its_start_to_its_end() {
        let mut seen = Seen::default();
        let t0 = Instant::now();
        // The first reading is the baseline: nothing to say yet.
        assert!(changes(&mut seen, &facts("idle", None, true, false, false), t0).is_empty());
        // The journey starts a moment before its goal is set: nothing said
        // until the goal comes, then a journey.
        assert!(changes(&mut seen, &facts("running", None, true, false, false), t0).is_empty());
        let ev = changes(&mut seen, &facts("running", Some((3.0, 4.0)), true, false, false), t0);
        assert_eq!(ev.len(), 1);
        assert_eq!((ev[0].0, ev[0].1["state"].as_str(), ev[0].1["kind"].as_str()), (topic::JOB, Some("running"), Some("journey")));
        // Progress a second on, not before.
        assert!(changes(&mut seen, &facts("running", Some((3.0, 4.0)), true, false, false), t0 + Duration::from_millis(500)).is_empty());
        let ev = changes(&mut seen, &facts("running", Some((3.0, 4.0)), true, false, false), t0 + Duration::from_millis(1100));
        assert_eq!(ev[0].0, topic::JOB_PROGRESS);
        assert_eq!(ev[0].1["route_m"], 5.0);
        // A fall during the job, the pose lost with it.
        let ev = changes(&mut seen, &facts("running", Some((3.0, 4.0)), false, true, false), t0 + Duration::from_millis(1300));
        let topics: Vec<_> = ev.iter().map(|e| e.0).collect();
        assert!(topics.contains(&topic::POSE) && topics.contains(&topic::FALL), "{topics:?}");
        // Done.
        let ev = changes(&mut seen, &facts("done", Some((3.0, 4.0)), true, false, false), t0 + Duration::from_millis(1500));
        assert!(ev.iter().any(|e| e.0 == topic::JOB && e.1["state"] == "done"), "{ev:?}");
        // An exploration: no goal, said as one past GOAL_WAIT; a question asked.
        assert!(changes(&mut seen, &facts("running", None, true, false, false), t0 + Duration::from_millis(1700)).iter().all(|e| e.0 != topic::JOB));
        let ev = changes(&mut seen, &facts("running", None, true, false, true), t0 + Duration::from_millis(1700) + GOAL_WAIT);
        assert!(ev.iter().any(|e| e.0 == topic::QUESTION), "{ev:?}");
        assert!(ev.iter().any(|e| e.0 == topic::JOB && e.1["kind"] == "explore"), "{ev:?}");
    }

    #[test]
    fn a_subscriber_gets_its_topics_in_order_and_one_that_does_not_read_is_dropped() {
        let hub = Hub::default();
        let jobs = hub.subscribe(&["job".to_string()]);
        let all = hub.subscribe(&topic::ALL.iter().map(|t| t.to_string()).collect::<Vec<_>>());
        hub.publish(topic::JOB, json!({"state": "running"}));
        hub.publish(topic::POSE, json!({"tracking": true}));
        hub.publish(topic::JOB, json!({"state": "done"}));
        let got: Vec<Event> = jobs.try_iter().collect();
        assert_eq!(got.iter().map(|e| (e.topic.as_str(), e.seq)).collect::<Vec<_>>(), vec![("job", 1), ("job", 2)]);
        assert_eq!(all.try_iter().map(|e| e.seq).collect::<Vec<_>>(), vec![1, 2, 3]);
        // `jobs` stops reading: past the queue it is dropped, `all` stays.
        drop(all);
        for _ in 0..QUEUE + 1 {
            hub.publish(topic::JOB, json!({}));
        }
        assert_eq!(hub.len(), 0, "the full one dropped, the gone one too");
        drop(jobs);
    }
}
