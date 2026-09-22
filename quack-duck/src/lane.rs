//! robotd client on the padd model (docs/study/microduck-client-pattern.md):
//! NDJSON JSON-RPC 2.0 over `/run/robotd.sock`, every message built from
//! `duck-ipc-proto` types, one connection per lane. quacksat holds session
//! state, so unlike padd it reconnects in-process instead of exiting.
//!
//! Deadman contract: quacksat does not drive, so it sends no `robot.move` at
//! all — a fabricated zero would masquerade as a driver saying "stop".

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::Context;
use duck_ipc_proto as proto;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
/// Reconnect cadence, same fixed delay robotd's own theremin client uses.
pub const RECONNECT_DELAY: Duration = Duration::from_secs(2);

fn connect(path: &str) -> std::io::Result<UnixStream> {
    // UnixStream has no connect_timeout; a missing socket fails fast and a
    // present-but-dead one is caught by the read/write timeouts below.
    let stream = UnixStream::connect(path)?;
    stream.set_write_timeout(Some(CONNECT_TIMEOUT))?;
    Ok(stream)
}

fn write_line(writer: &mut impl Write, message: &impl serde::Serialize) -> std::io::Result<()> {
    let mut line = serde_json::to_vec(message)?;
    line.push(b'\n');
    writer.write_all(&line)?;
    writer.flush()
}

/// The request lane: discrete intents and queries. Never subscribed, so
/// every incoming line is a response to something we asked.
pub struct Control {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    next_id: u64,
}

impl Control {
    pub fn connect(path: &str) -> anyhow::Result<Self> {
        let stream = connect(path).with_context(|| format!("connecting to robotd at {path}"))?;
        stream.set_read_timeout(Some(REQUEST_TIMEOUT))?;
        Ok(Self {
            reader: BufReader::new(stream.try_clone()?),
            writer: stream,
            next_id: 1,
        })
    }

    /// Send a continuous intent: no id, no reply, nothing to wait for.
    pub fn notify(&mut self, call: &proto::Call) -> std::io::Result<()> {
        write_line(&mut self.writer, &proto::Request::notify(call))
    }

    /// Send a discrete intent or query and wait for its response.
    pub fn request(&mut self, call: &proto::Call) -> anyhow::Result<proto::Response> {
        let id = proto::Id::Number(self.next_id);
        self.next_id += 1;
        write_line(&mut self.writer, &proto::Request::call(id.clone(), call))?;
        self.answer(id)
    }

    /// Send a request by method name, for a call the released proto crate
    /// does not know yet.
    ///
    /// The map library — `robot.map_save`, `robot.map_list`,
    /// `robot.map_load` — is prototyped on a local robotd branch and asked
    /// of upstream in docs/study/upstream-asks.md. Until it lands in a
    /// tagged `duck-ipc-proto`, quacksat spells those three on the wire
    /// rather than pinning itself to a fork of the protocol crate. A robotd
    /// that does not know them answers METHOD_NOT_FOUND, which is exactly
    /// the "this robot cannot do that yet" the caller needs.
    pub fn request_method(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> anyhow::Result<proto::Response> {
        let n = self.next_id;
        self.next_id += 1;
        let mut request = serde_json::json!({"jsonrpc": "2.0", "id": n, "method": method});
        if let Some(params) = params {
            request["params"] = params;
        }
        write_line(&mut self.writer, &request)?;
        self.answer(proto::Id::Number(n))
    }

    /// Read until the answer to `id` arrives.
    fn answer(&mut self, id: proto::Id) -> anyhow::Result<proto::Response> {
        loop {
            let mut line = String::new();
            if self.reader.read_line(&mut line)? == 0 {
                anyhow::bail!("robotd closed the connection");
            }
            // A notification carries `method`, a response does not, so the
            // two cannot be confused; anything unexpected is skipped rather
            // than treated as an error (robotctl's monitor rule).
            if serde_json::from_str::<proto::Request>(&line).is_ok() {
                continue;
            }
            let response: proto::Response = serde_json::from_str(&line)
                .with_context(|| format!("unparsable answer: {}", line.trim()))?;
            if response.id == Some(id.clone()) {
                return Ok(response);
            }
        }
    }

    /// A discrete intent whose answer is an [`proto::IntentResult`]. A soft
    /// refusal (`accepted: false`) is a normal outcome and is returned as
    /// such; a JSON-RPC error is an actual error.
    pub fn intent(&mut self, call: &proto::Call) -> anyhow::Result<proto::IntentResult> {
        let response = self.request(call)?;
        if let Some(error) = &response.error {
            anyhow::bail!("robotd refused {}: {error}", call.method());
        }
        Ok(response.result_as::<proto::IntentResult>()?)
    }
}

/// What one connection-lifetime of the state stream delivers.
pub enum StreamEvent {
    /// The subscribe ack: policy names, constant for the daemon's life.
    Subscribed(Box<proto::SubscribeResult>),
    State(Box<proto::RobotState>),
}

/// One connection-lifetime of the stream lane: subscribe, then forward
/// events until the peer goes away. Returns `Ok(())` when the receiver
/// hangs up, `Err` when the connection dies — the caller sleeps
/// [`RECONNECT_DELAY`] and calls again (re-subscribing is part of the call).
pub fn run_state_stream(
    path: &str,
    hz: Option<u32>,
    tx: &mpsc::Sender<StreamEvent>,
) -> anyhow::Result<()> {
    let stream = connect(path).with_context(|| format!("connecting to robotd at {path}"))?;
    // Generous: at the lowest useful rate (1 Hz) a 30 s silence means the
    // daemon is gone, not slow.
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    let mut writer = stream.try_clone()?;
    let mut reader = BufReader::new(stream);

    let subscribe = proto::Call::RobotSubscribe(proto::SubscribeParams { hz });
    write_line(
        &mut writer,
        &proto::Request::call(proto::Id::Number(0), &subscribe),
    )?;

    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            anyhow::bail!("robotd closed the state stream");
        }
        if let Ok(request) = serde_json::from_str::<proto::Request>(&line) {
            if let Some(state) = request.as_state()
                && tx.send(StreamEvent::State(Box::new(state))).is_err()
            {
                return Ok(());
            }
            continue;
        }
        if let Ok(response) = serde_json::from_str::<proto::Response>(&line)
            && let Ok(result) = response.result_as::<proto::SubscribeResult>()
            && tx.send(StreamEvent::Subscribed(Box::new(result))).is_err()
        {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    fn state_fixture() -> proto::RobotState {
        proto::RobotState {
            t: 1.5,
            movement: proto::MoveState {
                requested: [0.0; 3],
                applied: [0.0; 3],
                limited_by: vec!["deadman".to_string()],
            },
            head: [0.0; 4],
            policy: "held".to_string(),
            safety: proto::SafetyState {
                fallen: false,
                limp: true,
                gravity: [0.0, 0.0, -1.0],
                gain: None,
            },
            control_loop: proto::LoopState {
                hz: 50.0,
                missed: 0,
            },
            joints: vec![0.0; 15],
            targets: vec![0.0; 15],
            odom: proto::OdomState::default(),
            theremin: None,
            chorale: None,
        }
    }

    #[test]
    fn request_reads_its_own_response_and_skips_notifications() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("robotd.sock");
        let listener = UnixListener::bind(&socket).unwrap();

        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let request: proto::Request = serde_json::from_str(&line).unwrap();
            assert_eq!(request.method, "robot.sound");
            let id = request.id.unwrap();

            let mut writer = stream;
            // A stray notification first: the client must skip it.
            write_line(&mut writer, &proto::Request::notify_state(&state_fixture())).unwrap();
            write_line(
                &mut writer,
                &proto::Response::ok(Some(id), &proto::IntentResult::accepted()),
            )
            .unwrap();
        });

        let mut control = Control::connect(socket.to_str().unwrap()).unwrap();
        let result = control
            .intent(&proto::Call::RobotSound(proto::SoundParams {
                tag: proto::SoundTag::Chirp,
                hold: None,
            }))
            .unwrap();
        assert!(result.accepted);
        server.join().unwrap();
    }

    #[test]
    fn soft_refusal_is_an_outcome_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("robotd.sock");
        let listener = UnixListener::bind(&socket).unwrap();

        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let request: proto::Request = serde_json::from_str(&line).unwrap();
            let mut writer = stream;
            write_line(
                &mut writer,
                &proto::Response::ok(
                    request.id,
                    &proto::IntentResult::refused("this robot has no voice"),
                ),
            )
            .unwrap();
        });

        let mut control = Control::connect(socket.to_str().unwrap()).unwrap();
        let result = control.intent(&proto::Call::RobotStop).unwrap();
        assert!(!result.accepted);
        assert_eq!(result.reason.as_deref(), Some("this robot has no voice"));
        server.join().unwrap();
    }

    #[test]
    fn notify_writes_an_idless_line() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("robotd.sock");
        let listener = UnixListener::bind(&socket).unwrap();

        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let request: proto::Request = serde_json::from_str(&line).unwrap();
            assert!(request.is_notification());
            assert_eq!(request.method, "robot.mouth");
        });

        let mut control = Control::connect(socket.to_str().unwrap()).unwrap();
        control
            .notify(&proto::Call::RobotMouth(proto::MouthParams { open: 0.5 }))
            .unwrap();
        server.join().unwrap();
    }

    #[test]
    fn state_stream_subscribes_then_delivers_states() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("robotd.sock");
        let listener = UnixListener::bind(&socket).unwrap();

        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let request: proto::Request = serde_json::from_str(&line).unwrap();
            assert_eq!(request.method, "robot.subscribe");

            let mut writer = stream;
            let ack = proto::SubscribeResult {
                accepted: true,
                ..Default::default()
            };
            write_line(&mut writer, &proto::Response::ok(request.id, &ack)).unwrap();
            for _ in 0..2 {
                write_line(&mut writer, &proto::Request::notify_state(&state_fixture())).unwrap();
            }
            // Server closes; the client must report the stream as dead.
        });

        let (tx, rx) = mpsc::channel();
        let result = run_state_stream(socket.to_str().unwrap(), Some(5), &tx);
        assert!(result.is_err(), "server close must be an error");

        let events: Vec<StreamEvent> = rx.try_iter().collect();
        assert_eq!(events.len(), 3);
        assert!(matches!(&events[0], StreamEvent::Subscribed(r) if r.accepted));
        assert!(
            matches!(&events[1], StreamEvent::State(s) if s.policy == "held"
                && s.movement.limited_by == ["deadman"])
        );
        server.join().unwrap();
    }
}
