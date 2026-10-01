//! The knobs a client may set: `nav.knobs` on the nav socket.
//!
//! Most of the navigation's tuning is environment variables (docs/knobs.md),
//! read when the process starts or when a job begins — from the process's
//! own environment, which nothing outside can change. So a client does not
//! change them in the running daemon: it writes them to an env file
//! (`knobs_env`, `/var/lib/quack-nav/knobs.env`) that the unit reads with
//! `EnvironmentFile=-`, and `nav.restart` brings them in. Every knob needs
//! that restart, whether the code reads it once or at every call: the
//! environment it reads is fixed when the process starts.
//!
//! The list is `knobs.json`, generated from the code by `scripts/knobs.py`
//! (CI checks it is current): only the `QK_*` and `MAPLOC_*` knobs the
//! daemon's own sources read, not the benches'. A value is checked against
//! its knob's type before it is written, and the file keeps any line it
//! does not own (a `RUST_LOG`, a comment) as it was.

use std::path::Path;

use anyhow::Context;
use serde::Deserialize;
use serde_json::{Map, Value, json};

/// Written by `scripts/knobs.py`.
const LIST: &str = include_str!("knobs.json");

#[derive(Debug, Clone, Deserialize)]
pub struct Knob {
    pub name: String,
    /// `number`, `switch` (`0`/`1`), `choice` (one of `options`), `flag`
    /// (`1` is on, unset off) or `text`.
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub options: Vec<String>,
}

#[derive(Deserialize)]
struct List {
    knobs: Vec<Value>,
}

/// The list as generated, each entry whole (doc, default, where...).
pub fn list() -> Vec<Value> {
    serde_json::from_str::<List>(LIST).expect("knobs.json is generated valid").knobs
}

fn knobs() -> Vec<Knob> {
    list().into_iter().map(|k| serde_json::from_value(k).expect("knobs.json entries are knobs")).collect()
}

/// A value the env file can hold as it is, for systemd and for a shell
/// that sources it: no quotes, spaces or `$` to be read two ways.
fn plain(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.chars().all(|c| c.is_ascii_alphanumeric() || "._,:/+-".contains(c))
}

/// Whether `value` is one `knob` takes; `Err` says what it takes.
pub fn check(knob: &Knob, value: &str) -> Result<(), String> {
    let ok = match knob.kind.as_str() {
        "number" => value.parse::<f64>().is_ok_and(f64::is_finite) && plain(value),
        "switch" => matches!(value, "0" | "1"),
        "flag" => value == "1",
        "choice" => knob.options.iter().any(|o| o == value),
        _ => plain(value),
    };
    if ok {
        return Ok(());
    }
    Err(match knob.kind.as_str() {
        "number" => format!("{} is a number", knob.name),
        "switch" => format!("{} is 0 or 1", knob.name),
        "flag" => format!("{} is 1 (on) or unset (off)", knob.name),
        "choice" => format!("{} is one of {}", knob.name, knob.options.join(", ")),
        _ => format!("{} is up to 256 letters, digits and . _ , : / + -", knob.name),
    })
}

/// The `NAME=value` lines of an env file, in order; a missing file has none.
pub fn read_env(path: &Path) -> anyhow::Result<Vec<(String, String)>> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    Ok(text.lines().filter_map(assignment).map(|(k, v)| (k.to_owned(), v.to_owned())).collect())
}

fn assignment(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    if line.starts_with('#') {
        return None;
    }
    let (name, value) = line.split_once('=')?;
    let value = value.trim().trim_matches('"');
    Some((name.trim().trim_start_matches("export ").trim(), value))
}

/// Apply `changes` (a name to a value, or to null: back to the default) to
/// the env file's text: a knob's line replaced, added at the end or
/// dropped; every other line as it was.
fn edit(text: &str, changes: &[(String, Option<String>)]) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut done: Vec<&str> = Vec::new();
    for line in text.lines() {
        match assignment(line).and_then(|(name, _)| changes.iter().find(|(n, _)| n == name)) {
            Some((name, value)) => {
                if let Some(value) = value
                    && !done.contains(&name.as_str())
                {
                    out.push(format!("{name}={value}"));
                }
                done.push(name);
            }
            None => out.push(line.to_owned()),
        }
    }
    if text.is_empty() {
        out.push("# quack-navd's knobs, written by nav.knobs (a client such as quack-control);".into());
        out.push("# read at the daemon's start (EnvironmentFile=). docs/knobs.md says what each does.".into());
    }
    for (name, value) in changes {
        if let Some(value) = value
            && !done.contains(&name.as_str())
        {
            out.push(format!("{name}={value}"));
        }
    }
    let mut text = out.join("\n");
    text.push('\n');
    text
}

/// `nav.knobs`: `{}` lists; `{"set": {"QK_X": "0.3", "QK_Y": null}}` writes
/// (null drops the override); `{"reset_all": true}` drops every knob's.
/// Every answer is the list with each knob's `saved` value (the file's, or
/// null) and `running` one (this process's environment, or null), and
/// `restart_needed` when they differ anywhere.
pub fn answer(params: &Value, path: &Path) -> Result<Value, String> {
    let all = knobs();
    let mut changes: Vec<(String, Option<String>)> = Vec::new();
    if params.get("reset_all").and_then(Value::as_bool) == Some(true) {
        changes = all.iter().map(|k| (k.name.clone(), None)).collect();
    } else if let Some(set) = params.get("set") {
        let set = set.as_object().ok_or("`set` maps knob names to values (or null)")?;
        for (name, value) in set {
            let knob = all.iter().find(|k| &k.name == name).ok_or_else(|| format!("no knob `{name}` (see docs/knobs.md)"))?;
            let value = match value {
                Value::Null => None,
                Value::String(s) => Some(s.trim().to_owned()),
                Value::Number(n) => Some(n.to_string()),
                Value::Bool(b) => Some(if *b { "1" } else { "0" }.to_owned()),
                _ => return Err(format!("{name}: a string, a number, or null")),
            };
            if let Some(value) = &value {
                check(knob, value)?;
            }
            changes.push((name.clone(), value));
        }
    } else if let Some(other) = params.as_object().and_then(|m| m.keys().next()) {
        return Err(format!("nav.knobs takes `set` or `reset_all`, not `{other}`"));
    }
    if !changes.is_empty() {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
        };
        write_atomic(path, &edit(&text, &changes)).map_err(|e| format!("{e:#}"))?;
    }
    let saved = read_env(path).map_err(|e| format!("{e:#}"))?;
    let mut restart_needed = false;
    let listed: Vec<Value> = list()
        .into_iter()
        .map(|mut k| {
            let name = k["name"].as_str().unwrap_or_default().to_owned();
            let file = saved.iter().rev().find(|(n, _)| *n == name).map(|(_, v)| v.clone());
            let running = std::env::var(&name).ok();
            restart_needed |= file != running;
            if let Some(map) = k.as_object_mut() {
                map.insert("saved".into(), json!(file));
                map.insert("running".into(), json!(running));
            }
            k
        })
        .collect();
    let mut out = Map::new();
    out.insert("env_file".into(), json!(path.display().to_string()));
    out.insert("restart_needed".into(), json!(restart_needed));
    out.insert("knobs".into(), Value::Array(listed));
    Ok(Value::Object(out))
}

fn write_atomic(path: &Path, text: &str) -> anyhow::Result<()> {
    if let Some(dir) = path.parent()
        && !dir.as_os_str().is_empty()
    {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    let tmp = path.with_extension("env.tmp");
    std::fs::write(&tmp, text).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn knob(kind: &str) -> Knob {
        Knob { name: "QK_ENV_TEST_T".into(), kind: kind.into(), options: vec!["0".into(), "1".into(), "2".into()] }
    }

    #[test]
    fn the_list_is_the_daemon_s_and_parses() {
        let all = knobs();
        assert!(all.len() > 20, "{}", all.len());
        assert!(all.iter().all(|k| k.name.starts_with("QK_") || k.name.starts_with("MAPLOC_")));
        assert!(all.iter().any(|k| k.name == "QK_CLIFF_MARGIN_M" && k.kind == "number"));
        // The benches' knobs are not the daemon's to set.
        assert!(!all.iter().any(|k| k.name == "MAPLOC_VERBOSE"));
    }

    #[test]
    fn values_are_checked_against_their_knob() {
        assert!(check(&knob("number"), "0.25").is_ok());
        assert!(check(&knob("number"), "-3e-2").is_ok());
        for bad in ["", "abc", "NaN", "inf", "1 2"] {
            assert!(check(&knob("number"), bad).is_err(), "{bad}");
        }
        assert!(check(&knob("switch"), "1").is_ok());
        assert!(check(&knob("switch"), "true").is_err());
        assert!(check(&knob("flag"), "1").is_ok());
        assert!(check(&knob("flag"), "0").is_err(), "a flag is on when set at all");
        assert!(check(&knob("choice"), "2").is_ok());
        assert!(check(&knob("choice"), "3").is_err());
        assert!(check(&knob("text"), "/tmp/walls.json").is_ok());
        for bad in ["a b", "$(reboot)", "a\"b", "a\nb", "x;y"] {
            assert!(check(&knob("text"), bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn an_edit_keeps_what_it_does_not_own() {
        let text = "# mine\nRUST_LOG=debug\nQK_ENV_TEST_A=1\nQK_ENV_TEST_B=2\nQK_ENV_TEST_A=3\n";
        let out = edit(text, &[("QK_ENV_TEST_A".into(), Some("5".into())), ("QK_ENV_TEST_B".into(), None), ("QK_ENV_TEST_C".into(), Some("0".into()))]);
        assert_eq!(out, "# mine\nRUST_LOG=debug\nQK_ENV_TEST_A=5\nQK_ENV_TEST_C=0\n");
        assert!(edit("", &[("QK_ENV_TEST_A".into(), Some("1".into()))]).ends_with("\nQK_ENV_TEST_A=1\n"));
    }

    #[test]
    fn set_reset_and_list_go_through_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state").join("knobs.env");
        let listed = answer(&json!({}), &path).unwrap();
        assert!(!path.exists(), "listing writes nothing");
        assert!(listed["knobs"].as_array().unwrap().iter().all(|k| k["saved"].is_null()));

        let after = answer(&json!({"set": {"QK_CLIFF_MARGIN_M": "0.3", "QK_TRAIL": 0}}), &path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\nQK_CLIFF_MARGIN_M=0.3\n") && text.contains("\nQK_TRAIL=0\n"), "{text}");
        let margin = after["knobs"].as_array().unwrap().iter().find(|k| k["name"] == "QK_CLIFF_MARGIN_M").unwrap();
        assert_eq!(margin["saved"], "0.3");
        assert_eq!(after["restart_needed"], true);

        let e = answer(&json!({"set": {"QK_CLIFF_MARGIN_M": "far"}}), &path).unwrap_err();
        assert!(e.contains("is a number"), "{e}");
        assert!(answer(&json!({"set": {"PATH": "/"}}), &path).unwrap_err().contains("no knob"));
        assert!(answer(&json!({"wipe": true}), &path).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text, "a refused set writes nothing");

        answer(&json!({"set": {"QK_TRAIL": null}}), &path).unwrap();
        assert!(!std::fs::read_to_string(&path).unwrap().contains("QK_TRAIL"));
        answer(&json!({"reset_all": true}), &path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("QK_CLIFF_MARGIN_M="), "{text}");
    }
}
