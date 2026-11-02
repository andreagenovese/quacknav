//! The wire between `quack-navd` and its clients.
//!
//! NDJSON, JSON-RPC 2.0, on the daemon's Unix socket — `robotd`'s own
//! shape. This crate holds what both ends must agree on: the version, the
//! method names, the params and results, the error codes. No I/O. The
//! daemon and every client build against it, so a member renamed is a
//! compile error on both sides rather than a silence on the wire.
//!
//! The design is `docs/design/service.md`; the history of the versions is
//! `docs/protocol.md`.
//!
//! # The version
//!
//! [`API_VERSION`] is bumped by every change a client could notice: a new
//! method, a new member, a new event. A skew is logged and served, never
//! refused: an older client does not call what it does not know, a newer
//! one calling a method this daemon lacks gets [`code::METHOD_NOT_FOUND`],
//! an unknown member [`code::INVALID_PARAMS`] naming it.
//!
//! # v1 — the hello
//!
//! [`method::HELLO`]: which version is speaking, which build, which methods
//! and events it serves. `nav.catalog` and `nav.call` are unchanged from
//! before the version existed, and stay.

use serde::{Deserialize, Serialize};

pub const API_VERSION: u32 = 1;

/// The methods `quack-navd` serves.
pub mod method {
    /// Which version is speaking ([`super::HelloResult`]). v1.
    pub const HELLO: &str = "nav.hello";
    /// The tools, each with its JSON schema: the agents' view.
    pub const CATALOG: &str = "nav.catalog";
    /// One tool: `{name, args}`.
    pub const CALL: &str = "nav.call";
    /// The knobs: read, set, reset (the knobs page owns them).
    pub const KNOBS: &str = "nav.knobs";
    /// Under systemd, answer, then restart with the knobs read anew.
    pub const RESTART: &str = "nav.restart";

    /// Every method this version serves, in the order `nav.hello` lists them.
    pub const ALL: [&str; 5] = [HELLO, CATALOG, CALL, KNOBS, RESTART];
}

/// JSON-RPC error codes. The first four are the specification's; the
/// daemon's own start at −32000.
pub mod code {
    pub const PARSE_ERROR: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
    /// A tool answered with an error: its message says why.
    pub const TOOL_ERROR: i64 = -32000;
}

/// `nav.hello` takes nothing, or what the client wants known about itself
/// for the daemon's log. Unknown members are refused, as everywhere.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HelloParams {
    /// Who is asking ("quacksat", "quack-control", an agent's name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
    /// The protocol version the client was built against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_version: Option<u32>,
}

/// What `nav.hello` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelloResult {
    pub api_version: u32,
    /// The daemon's package version.
    pub daemon_version: String,
    /// The source revision the running binary was built from, when the
    /// build knew it (`QUACK_NAV_REVISION` at build time); `null` otherwise.
    /// Always serialised, so the wire's shape does not depend on the value.
    pub revision: Option<String>,
    /// The methods served, [`method::ALL`] of this version.
    pub methods: Vec<String>,
    /// The event topics a subscription may ask for (none before v2).
    pub events: Vec<String>,
}

impl HelloResult {
    /// The hello of this build: the version, the methods and the events
    /// this crate names, with the daemon's own version and revision.
    pub fn of_this_build(daemon_version: &str, revision: Option<&str>) -> Self {
        Self {
            api_version: API_VERSION,
            daemon_version: daemon_version.to_string(),
            revision: revision.map(str::to_string),
            methods: method::ALL.iter().map(|m| m.to_string()).collect(),
            events: Vec::new(),
        }
    }
}

/// What a skew between the client's build and the daemon's means, for the
/// client's log: never a reason to stop.
pub fn skew(client: u32, daemon: u32) -> Option<String> {
    use std::cmp::Ordering::*;
    match client.cmp(&daemon) {
        Equal => None,
        Less => Some(format!("the daemon speaks v{daemon}, this client v{client}: what came after v{client} is not used")),
        Greater => Some(format!("the daemon speaks v{daemon}, this client v{client}: what came after v{daemon} answers METHOD_NOT_FOUND")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hello_names_this_version_and_its_methods() {
        let h = HelloResult::of_this_build("0.3.0", None);
        assert_eq!(h.api_version, API_VERSION);
        assert_eq!(h.methods, method::ALL);
        let v = serde_json::to_value(&h).unwrap();
        assert!(v.get("revision").is_some_and(|r| r.is_null()), "always serialised: {v}");
        let back: HelloResult = serde_json::from_value(v).unwrap();
        assert_eq!(back, h);
    }

    #[test]
    fn an_unknown_member_of_the_hello_is_refused_by_name() {
        let e = serde_json::from_value::<HelloParams>(serde_json::json!({"client": "x", "colour": "blue"})).unwrap_err();
        assert!(e.to_string().contains("colour"), "{e}");
        assert_eq!(serde_json::from_value::<HelloParams>(serde_json::json!({})).unwrap(), HelloParams::default());
    }

    #[test]
    fn a_skew_is_said_not_refused() {
        assert_eq!(skew(1, 1), None);
        assert!(skew(1, 3).unwrap().contains("not used"));
        assert!(skew(3, 1).unwrap().contains("METHOD_NOT_FOUND"));
    }
}
