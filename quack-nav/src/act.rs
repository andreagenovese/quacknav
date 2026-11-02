//! Doing, not only walking (`nav.act`, protocol v5; the roadmap's N5).
//!
//! The skills are `robotd`'s — Pollen's policies, run by the daemon that
//! owns the motors: a pick, the kicks, sit and stand, whatever a policy set
//! adds. quack-navd never trains, ships or replaces one; it reads the list
//! from `robot.skills` at run time, asks `robot.do`, and watches the state
//! stream's policy label to see the skill begin (the label leaves
//! `walk`/`stand`) and end (it comes back). A skill that never begins, or
//! does not end in its time, or a body that falls, is a failed act, said
//! with the reason. The design is `docs/design/service.md` §6.

use std::time::Duration;

use serde_json::Value;

/// The labels of a body that is not running a skill.
pub const RESTING: [&str; 2] = ["walk", "stand"];
/// A skill that has not left the resting labels within this never ran.
pub const BEGIN_WAIT: Duration = Duration::from_secs(3);
/// A skill whose table gives no duration is given this to end.
pub const DEFAULT_LENGTH: Duration = Duration::from_secs(12);
/// Beyond its own length a skill is given this much more to hand back.
pub const END_MARGIN: Duration = Duration::from_secs(6);
const POLL: Duration = Duration::from_millis(100);

/// The skills `robot.skills` names: the table's entries, then the
/// built-ins, each with its length when the table gives one.
pub fn skills(result: &Value) -> Vec<(String, Option<f64>)> {
    let mut out: Vec<(String, Option<f64>)> = result
        .get("skills")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|s| Some((s.get("name")?.as_str()?.to_string(), s.get("duration").and_then(Value::as_f64))))
        .collect();
    for b in result.get("built_in").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
        if !out.iter().any(|(n, _)| n == b) {
            out.push((b.to_string(), None));
        }
    }
    out
}

/// How a skill went, as the policy label told it.
#[derive(Debug, Clone, PartialEq)]
pub enum Seen {
    /// It began and ended: the seconds it ran, and the label it ran under.
    Ran { secs: f64, label: String },
    /// The label never left the resting ones.
    NeverBegan,
    /// It began and had not ended in its time.
    DidNotEnd { label: String },
    /// The body fell or was held while it ran.
    Fell { label: String },
}

/// Watch a skill through the policy label: `label` reads it (`None` while
/// unknown), `fallen` whether the body is down, `sleep` waits. `length` is
/// the skill's own, when known.
pub fn watch(mut label: impl FnMut() -> Option<String>, mut fallen: impl FnMut() -> bool, mut sleep: impl FnMut(Duration), length: Option<f64>) -> Seen {
    let mut waited = Duration::ZERO;
    let mut began: Option<(Duration, String)> = None;
    let limit = length.map(Duration::from_secs_f64).unwrap_or(DEFAULT_LENGTH) + END_MARGIN;
    loop {
        sleep(POLL);
        waited += POLL;
        let now = label();
        let resting = now.as_deref().is_some_and(|l| RESTING.contains(&l));
        match &began {
            None => {
                if let Some(l) = now.filter(|l| !RESTING.contains(&l.as_str()) && l != "held") {
                    began = Some((waited, l));
                } else if waited >= BEGIN_WAIT {
                    return Seen::NeverBegan;
                }
            }
            Some((at, l)) => {
                if fallen() || now.as_deref() == Some("held") {
                    return Seen::Fell { label: l.clone() };
                }
                if resting {
                    return Seen::Ran { secs: (waited - *at).as_secs_f64(), label: l.clone() };
                }
                if waited - *at >= limit {
                    return Seen::DidNotEnd { label: l.clone() };
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::cell::Cell;

    fn script(labels: &[(u32, &str)]) -> impl FnMut() -> Option<String> {
        // (ticks, label): the label for that many polls, then the next.
        let mut plan: Vec<String> = Vec::new();
        for (n, l) in labels {
            for _ in 0..*n {
                plan.push(l.to_string());
            }
        }
        let i = Cell::new(0usize);
        move || {
            let k = i.get();
            i.set(k + 1);
            plan.get(k).or(plan.last()).cloned()
        }
    }

    #[test]
    fn the_skills_are_the_tables_and_the_built_ins() {
        let r = json!({"skills": [{"name": "ball_kick_left", "duration": 1.2}, {"name": "roulade"}], "built_in": ["ground_pick", "sit_toggle", "roulade"]});
        assert_eq!(skills(&r), vec![("ball_kick_left".into(), Some(1.2)), ("roulade".into(), None), ("ground_pick".into(), None), ("sit_toggle".into(), None)]);
        assert!(skills(&json!({})).is_empty());
    }

    #[test]
    fn a_skill_is_seen_to_begin_and_end_by_its_label() {
        let seen = watch(script(&[(3, "walk"), (12, "ball_kick_left"), (5, "walk")]), || false, |_| {}, Some(1.2));
        match seen {
            Seen::Ran { secs, label } => assert!(label == "ball_kick_left" && (secs - 1.2).abs() < 1e-6, "{secs} {label}"),
            s => panic!("{s:?}"),
        }
    }

    #[test]
    fn a_skill_that_never_began_did_not_end_or_fell_is_said_so() {
        assert_eq!(watch(script(&[(100, "walk")]), || false, |_| {}, None), Seen::NeverBegan);
        assert!(matches!(watch(script(&[(2, "walk"), (1000, "ground_pick")]), || false, |_| {}, Some(2.0)), Seen::DidNotEnd { .. }));
        let n = Cell::new(0);
        let fell = watch(script(&[(2, "walk"), (100, "ground_pick")]), || { n.set(n.get() + 1); n.get() > 5 }, |_| {}, None);
        assert!(matches!(fell, Seen::Fell { .. }));
        assert!(matches!(watch(script(&[(2, "walk"), (5, "roulade"), (5, "held")]), || false, |_| {}, None), Seen::Fell { .. }));
    }
}
