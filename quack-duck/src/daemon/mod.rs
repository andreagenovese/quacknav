//! Which of Pollen's daemons robotd is, and what that changes for the
//! navigation. One parameter, `daemon` in quack-navd's config: `"auto"`
//! (the default) asks robotd at start; `"0.16"`, `"0.17"` or a release
//! (`"0.16.1"`, `"daemon-v0.17.6"`) names it.
//!
//! Daemons that walk alike form a [`Line`]; each line has a file here with
//! the numbers measured on it, so a daemon whose walk changes again is one
//! file more and the older lines keep working as they were.

mod detect;
mod v0_16;
mod v0_17;

use serde::Deserialize;

use crate::gait::Numbers;

pub use detect::{ask, resolve};

/// Daemons that walk alike: what robotd's policies do with a twist, and so
/// the gait's numbers under velstand. alpha's policies never changed, so
/// its numbers are the same on every line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Line {
    /// Up to daemon-v0.16.1: policy set v7 and before (validated on
    /// daemon-v0.16.1).
    V0_16,
    /// daemon-v0.16.2 on: policy set v8 (validated on daemon-v0.17.6).
    V0_17,
}

impl Line {
    /// The newest line: what an unknown, newer daemon is taken for.
    pub const LATEST: Line = Line::V0_17;

    /// As the config and a pilot's `meta.daemon` spell it.
    pub fn name(self) -> &'static str {
        match self {
            Line::V0_16 => "0.16",
            Line::V0_17 => "0.17",
        }
    }

    /// The release the line was validated on, for the log.
    pub fn validated_on(self) -> &'static str {
        match self {
            Line::V0_16 => "daemon-v0.16.1, policy set v7",
            Line::V0_17 => "daemon-v0.17.6, policy set v8",
        }
    }

    /// The line a release belongs to: v0.16.2 brought set v8.
    pub fn of(major: u64, minor: u64, patch: u64) -> Line {
        if (major, minor, patch) >= (0, 16, 2) { Line::V0_17 } else { Line::V0_16 }
    }

    /// `"0.16"` and `"0.17"` name a line; a release (`"0.16.1"`, `"v0.17.6"`,
    /// `"daemon-v0.17.6"`) names the line it belongs to.
    pub fn parse(text: &str) -> Option<Line> {
        let t = text.trim();
        let t = t.strip_prefix("daemon-").unwrap_or(t);
        let t = t.strip_prefix('v').unwrap_or(t);
        let parts: Vec<u64> = t.split('.').map(|p| p.parse().ok()).collect::<Option<_>>()?;
        match parts[..] {
            [0, 16] => Some(Line::V0_16),
            [0, 17] => Some(Line::V0_17),
            [major, minor, patch] => Some(Line::of(major, minor, patch)),
            _ => None,
        }
    }

    /// velstand's numbers on this line.
    pub(crate) fn velstand(self) -> Numbers {
        match self {
            Line::V0_16 => v0_16::VELSTAND,
            Line::V0_17 => v0_17::VELSTAND,
        }
    }
}

/// `daemon` as the config writes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(try_from = "String")]
pub enum Choice {
    /// Ask robotd at start (`hello`'s `daemon_version`).
    #[default]
    Auto,
    Line(Line),
}

impl TryFrom<String> for Choice {
    type Error = String;

    fn try_from(text: String) -> Result<Self, String> {
        if text.trim() == "auto" {
            return Ok(Choice::Auto);
        }
        Line::parse(&text).map(Choice::Line).ok_or_else(|| {
            format!("daemon {text:?} is \"auto\", \"0.16\", \"0.17\" or a release such as \"0.17.6\"")
        })
    }
}

/// The line this process drives: quack-navd's `daemon` once it has started;
/// 0.16 in a process that reads no config (the tests, the paper twin, the
/// quack-rl simulator: the numbers they were built on) unless it sets one.
pub fn active() -> Line {
    ACTIVE.get().copied().unwrap_or(Line::V0_16)
}

static ACTIVE: std::sync::OnceLock<Line> = std::sync::OnceLock::new();

/// Set the line this process drives, once, before the gait's numbers are read.
pub fn set_active(line: Line) {
    let _ = ACTIVE.set(line);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_release_names_its_line() {
        for (text, line) in [
            ("0.16", Line::V0_16),
            ("0.17", Line::V0_17),
            ("0.14.4", Line::V0_16),
            ("0.16.1", Line::V0_16),
            ("0.16.2", Line::V0_17),
            ("v0.17.6", Line::V0_17),
            ("daemon-v0.17.6", Line::V0_17),
            ("0.18.0", Line::V0_17),
        ] {
            assert_eq!(Line::parse(text), Some(line), "{text}");
        }
        for bad in ["", "0", "0.15", "seventeen", "0.17.x", "1.2.3.4"] {
            assert_eq!(Line::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_config_says_auto_a_line_or_a_release() {
        #[derive(Deserialize)]
        struct C {
            #[serde(default)]
            daemon: Choice,
        }
        let read = |t: &str| from_json(t).map(|c: C| c.daemon);
        assert_eq!(read(r#"{}"#).unwrap(), Choice::Auto);
        assert_eq!(read(r#"{"daemon": "auto"}"#).unwrap(), Choice::Auto);
        assert_eq!(read(r#"{"daemon": "0.16.1"}"#).unwrap(), Choice::Line(Line::V0_16));
        assert_eq!(read(r#"{"daemon": "0.17"}"#).unwrap(), Choice::Line(Line::V0_17));
        let e = read(r#"{"daemon": "latest"}"#).unwrap_err().to_string();
        assert!(e.contains("latest") && e.contains("auto"), "{e}");
    }

    fn from_json<T: for<'de> Deserialize<'de>>(json: &str) -> serde_json::Result<T> {
        serde_json::from_str(json)
    }

    #[test]
    fn each_line_has_its_own_velstand() {
        let (old, new) = (Line::V0_16.velstand(), Line::V0_17.velstand());
        // The numbers every result before 2026-10-19 came from.
        assert_eq!((old.m_per_s, old.yaw_rate_per_unit, old.turn_in_place_rad_s, old.yaw_trim), (0.115, 0.62, 1.5, 0.02));
        // Set v8: slower forward, turns about twice as fast, veers left.
        assert_eq!((new.m_per_s, new.yaw_rate_per_unit, new.turn_in_place_rad_s, new.yaw_trim), (0.095, 1.1, 1.0, -0.03));
        assert!(Line::V0_16 < Line::LATEST);
    }
}
