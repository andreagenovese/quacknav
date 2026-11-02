//! A blocking client of `quack-navd`.
//!
//! The wire is the daemon's (NDJSON, JSON-RPC 2.0 on its Unix socket) and
//! its types are `quack-nav-proto`'s, so a client built here and the daemon
//! agree by construction. What a satellite, a control page or a script
//! needs and used to write by hand:
//!
//! ```no_run
//! let mut nav = quack_nav_client::Client::connect("/run/quack-nav/nav.sock")?;
//! let hello = nav.hello("my-tool")?;           // the version, logged if it differs
//! let tools = nav.catalog()?;                  // for an agent's tool list
//! let status = nav.call("robot.map_status", serde_json::json!({}))?;
//! nav.call("robot.go_to", serde_json::json!({"place": "kitchen"}))?;
//! for event in quack_nav_client::subscribe("/run/quack-nav/nav.sock", &["job"])? {
//!     let event = event?;
//!     if event.data["state"] == "done" { break }
//! }
//! # Ok::<(), quack_nav_client::Error>(())
//! ```
//!
//! A subscription takes a connection of its own: the events arrive on it
//! between the answers, and a lane that only listens is the simple one.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

pub use quack_nav_proto::{self as proto, API_VERSION, Event, HelloResult};
use serde_json::{Value, json};

/// How long one answer may take. A journey does not block it: `robot.go_to`
/// starts a job and answers at once.
pub const ANSWER_TIMEOUT: Duration = Duration::from_secs(15);

/// What went wrong.
#[derive(Debug)]
pub enum Error {
    /// The socket: nothing listening, or the daemon went away.
    Io(std::io::Error),
    /// The daemon answered with an error: its code and its message (a
    /// tool's own error is [`proto::code::TOOL_ERROR`]).
    Daemon { code: i64, message: String },
    /// The answer was not what the protocol says.
    Protocol(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "quack-navd: {e}"),
            Error::Daemon { code, message } => write!(f, "quack-navd ({code}): {message}"),
            Error::Protocol(m) => write!(f, "quack-navd: {m}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

/// One connection, one request at a time.
pub struct Client {
    writer: UnixStream,
    reader: BufReader<UnixStream>,
    next_id: u64,
}

impl Client {
    pub fn connect(socket: impl AsRef<Path>) -> Result<Self, Error> {
        let stream = UnixStream::connect(socket)?;
        stream.set_read_timeout(Some(ANSWER_TIMEOUT))?;
        Ok(Self { reader: BufReader::new(stream.try_clone()?), writer: stream, next_id: 1 })
    }

    /// One JSON-RPC request and its answer's `result`.
    pub fn request(&mut self, method: &str, params: Value) -> Result<Value, Error> {
        let id = self.next_id;
        self.next_id += 1;
        writeln!(self.writer, "{}", json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))?;
        loop {
            let mut line = String::new();
            if self.reader.read_line(&mut line)? == 0 {
                return Err(Error::Io(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "the daemon closed the connection")));
            }
            let reply: Value = serde_json::from_str(&line).map_err(|e| Error::Protocol(format!("not JSON: {e}")))?;
            // A notification on this lane (it subscribed) is not the answer.
            if reply.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(e) = reply.get("error") {
                return Err(Error::Daemon {
                    code: e.get("code").and_then(Value::as_i64).unwrap_or(0),
                    message: e.get("message").and_then(Value::as_str).unwrap_or_default().to_string(),
                });
            }
            return Ok(reply.get("result").cloned().unwrap_or(Value::Null));
        }
    }

    /// `nav.hello`, saying who asks and the version this client was built
    /// against. A daemon from before the hello (no `nav.hello`) answers as a
    /// v0: its tools still work.
    pub fn hello(&mut self, client: &str) -> Result<HelloResult, Error> {
        match self.request(proto::method::HELLO, json!({"client": client, "api_version": API_VERSION})) {
            Ok(v) => serde_json::from_value(v).map_err(|e| Error::Protocol(format!("nav.hello: {e}"))),
            Err(Error::Daemon { code: proto::code::METHOD_NOT_FOUND, .. }) => Ok(HelloResult {
                api_version: 0,
                daemon_version: "before the hello".into(),
                revision: None,
                methods: vec![proto::method::CATALOG.into(), proto::method::CALL.into()],
                events: Vec::new(),
            }),
            Err(e) => Err(e),
        }
    }

    /// The tools, each `{name, description, parameters}`.
    pub fn catalog(&mut self) -> Result<Vec<Value>, Error> {
        match self.request(proto::method::CATALOG, json!({}))? {
            Value::Array(tools) => Ok(tools),
            other => Err(Error::Protocol(format!("nav.catalog answered {other}"))),
        }
    }

    /// One tool.
    pub fn call(&mut self, tool: &str, args: Value) -> Result<Value, Error> {
        self.request(proto::method::CALL, json!({"name": tool, "args": args}))
    }

    /// `nav.claim`: the duck's motion is this connection's for `ttl_s`
    /// (at most 300), renewed by claiming again — with the token from
    /// another connection. Refused with [`proto::code::LEASED`] when
    /// another connection holds it. v3.
    pub fn claim(&mut self, holder: &str, ttl_s: f64, token: Option<&str>) -> Result<proto::ClaimResult, Error> {
        let mut p = json!({"holder": holder, "ttl_s": ttl_s});
        if let Some(t) = token {
            p["token"] = json!(t);
        }
        let v = self.request(proto::method::CLAIM, p)?;
        serde_json::from_value(v).map_err(|e| Error::Protocol(format!("nav.claim: {e}")))
    }

    /// `nav.jobs`: run these steps (`(tool, args)`) in order on the board,
    /// each to its end; `go_on` keeps going past a failed step. The steps'
    /// ends arrive on the `queue` topic. v4.
    pub fn jobs(&mut self, steps: &[(&str, Value)], go_on: bool) -> Result<Value, Error> {
        let steps: Vec<Value> = steps.iter().map(|(t, a)| json!({"tool": t, "args": a})).collect();
        self.request(proto::method::JOBS, json!({"steps": steps, "on_failure": if go_on { "continue" } else { "stop" }}))
    }

    /// The queue's state: `{running: false}`, or the steps done so far. v4.
    pub fn jobs_state(&mut self) -> Result<Value, Error> {
        self.request(proto::method::JOBS, json!({}))
    }

    /// End the queue and the step under way. v4.
    pub fn jobs_stop(&mut self) -> Result<Value, Error> {
        self.request(proto::method::JOBS, json!({"stop": true}))
    }

    /// `nav.act`: one of the duck's skills (as `robot.skills` names them),
    /// at a place's name or `{x, y}` when given (the duck walks there
    /// first). Starts a queue and answers at once; its end comes on the
    /// `queue` topic. v5.
    pub fn act(&mut self, skill: &str, at: Option<Value>) -> Result<Value, Error> {
        let mut p = json!({"skill": skill});
        if let Some(at) = at {
            p["at"] = at;
        }
        self.request(proto::method::ACT, p)
    }

    /// `nav.release`: give the lease back. Whether one was released. v3.
    pub fn release(&mut self, token: Option<&str>) -> Result<bool, Error> {
        let p = token.map_or_else(|| json!({}), |t| json!({"token": t}));
        Ok(self.request(proto::method::RELEASE, p)?.get("released").and_then(Value::as_bool).unwrap_or(false))
    }
}

/// The events on these topics (all when empty), on a connection of their
/// own. The iterator ends when the daemon closes the connection.
pub fn subscribe(socket: impl AsRef<Path>, topics: &[&str]) -> Result<Events, Error> {
    let mut client = Client::connect(socket)?;
    client.writer.set_read_timeout(None)?;
    let params = if topics.is_empty() { json!({}) } else { json!({"topics": topics}) };
    let granted = client.request(proto::method::SUBSCRIBE, params)?;
    let granted: proto::SubscribeResult = serde_json::from_value(granted).map_err(|e| Error::Protocol(format!("nav.subscribe: {e}")))?;
    Ok(Events { reader: client.reader, topics: granted.topics, _writer: client.writer })
}

/// A subscription's events.
pub struct Events {
    reader: BufReader<UnixStream>,
    /// The topics the daemon granted.
    pub topics: Vec<String>,
    _writer: UnixStream,
}

impl Iterator for Events {
    type Item = Result<Event, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let mut line = String::new();
            match self.reader.read_line(&mut line) {
                Ok(0) => return None,
                Err(e) => return Some(Err(Error::Io(e))),
                Ok(_) => {}
            }
            let Ok(m) = serde_json::from_str::<Value>(&line) else {
                return Some(Err(Error::Protocol(format!("not JSON: {line}"))));
            };
            if m.get("method").and_then(Value::as_str) != Some(proto::method::EVENT) {
                continue;
            }
            return Some(m.get("params").cloned().ok_or_else(|| Error::Protocol("an event without params".into())).and_then(|p| serde_json::from_value(p).map_err(|e| Error::Protocol(format!("nav.event: {e}")))));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    /// A daemon that answers the hello, the catalogue, one tool, an error,
    /// and a subscription that pushes two events then closes.
    fn fake(dir: &tempfile::TempDir, v1_only: bool) -> std::path::PathBuf {
        let path = dir.path().join("nav.sock");
        let listener = UnixListener::bind(&path).unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                std::thread::spawn(move || {
                    let mut out = stream.try_clone().unwrap();
                    for line in BufReader::new(stream).lines().map_while(Result::ok) {
                        let r: Value = serde_json::from_str(&line).unwrap();
                        let id = r["id"].clone();
                        let reply = match r["method"].as_str().unwrap() {
                            "nav.hello" if v1_only => json!({"id": id, "error": {"code": -32601, "message": "unknown method"}}),
                            "nav.hello" => json!({"id": id, "result": HelloResult::of_this_build("test", None)}),
                            "nav.catalog" => json!({"id": id, "result": [{"name": "robot.map_status"}]}),
                            "nav.call" if r["params"]["name"] == "robot.go_to" => json!({"id": id, "error": {"code": -32000, "message": "no way there"}}),
                            "nav.call" => json!({"id": id, "result": {"tracking": true}}),
                            "nav.subscribe" => {
                                writeln!(out, "{}", json!({"id": id, "result": {"topics": ["job"]}})).unwrap();
                                for (seq, state) in [(1, "running"), (2, "done")] {
                                    writeln!(out, "{}", json!({"method": "nav.event", "params": {"topic": "job", "seq": seq, "t": 1.0, "data": {"state": state}}})).unwrap();
                                }
                                return;
                            }
                            _ => json!({"id": id, "error": {"code": -32601, "message": "?"}}),
                        };
                        writeln!(out, "{reply}").unwrap();
                    }
                });
            }
        });
        path
    }

    #[test]
    fn hello_catalogue_a_call_and_a_tools_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = fake(&dir, false);
        let mut nav = Client::connect(&path).unwrap();
        assert_eq!(nav.hello("test").unwrap().api_version, API_VERSION);
        assert_eq!(nav.catalog().unwrap()[0]["name"], "robot.map_status");
        assert_eq!(nav.call("robot.map_status", json!({})).unwrap()["tracking"], true);
        match nav.call("robot.go_to", json!({})) {
            Err(Error::Daemon { code, message }) => assert_eq!((code, message.as_str()), (proto::code::TOOL_ERROR, "no way there")),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_daemon_from_before_the_hello_is_a_v0_and_still_works() {
        let dir = tempfile::tempdir().unwrap();
        let path = fake(&dir, true);
        let mut nav = Client::connect(&path).unwrap();
        assert_eq!(nav.hello("test").unwrap().api_version, 0);
        assert!(nav.call("robot.map_status", json!({})).is_ok());
    }

    #[test]
    fn a_subscription_yields_its_events_and_ends_with_the_connection() {
        let dir = tempfile::tempdir().unwrap();
        let path = fake(&dir, false);
        let events = subscribe(&path, &["job"]).unwrap();
        assert_eq!(events.topics, vec!["job"]);
        let got: Vec<Event> = events.map(Result::unwrap).collect();
        assert_eq!(got.iter().map(|e| (e.seq, e.data["state"].as_str().unwrap())).collect::<Vec<_>>(), vec![(1, "running"), (2, "done")]);
    }
}
