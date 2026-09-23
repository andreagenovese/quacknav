//! The map socket: robotd's `robot.map*` dialect, served by this daemon.
//!
//! What the robotd fork answered on its own socket, answered here with the
//! same shapes: `robot.map` (a [`MapStreamResult`], then a `map.frame` a
//! second), the library (`map_save`, `map_list`, `map_load`, `map_match`,
//! `map_adopt`) and `map_wipe`. NDJSON JSON-RPC 2.0, one thread per
//! caller. Anything else is `METHOD_NOT_FOUND`: this is the map, not the
//! robot.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{Arc, Mutex};

use duck_ipc_proto as proto;
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::Host;
use super::wire::{self, MapAdoptParams, MapMatchParams, MapMatches, MapNameParams, SavedMaps};
use crate::map::{METHOD_MAP_FRAME, METHOD_ROBOT_MAP, METHOD_ROBOT_MAP_WIPE, MapStreamResult};

const BAD_NAME: &str = "a map name is 1 to 64 letters, digits, '-' or '_'";

/// Bind `path` (replacing a stale socket) and serve it on a thread of its own.
pub fn serve(host: Host, path: &str) -> anyhow::Result<()> {
    let _ = std::fs::remove_file(path);
    let listener = UnixListener::bind(path)?;
    tracing::info!(socket = path, mode = host.mode().as_str(), "maploc: serving the map");
    std::thread::Builder::new()
        .name("maploc-serve".into())
        .spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        let host = host.clone();
                        std::thread::spawn(move || caller(stream, host));
                    }
                    Err(e) => tracing::warn!(error = %e, "maploc: a map caller could not be accepted"),
                }
            }
        })?;
    Ok(())
}

type Writer = Arc<Mutex<UnixStream>>;

fn write_line(out: &Writer, message: &impl serde::Serialize) -> std::io::Result<()> {
    let mut line = serde_json::to_vec(message)?;
    line.push(b'\n');
    let mut out = out.lock().expect("map caller writer poisoned");
    out.write_all(&line)?;
    out.flush()
}

fn caller(stream: UnixStream, host: Host) {
    let Ok(out) = stream.try_clone() else {
        return;
    };
    let out: Writer = Arc::new(Mutex::new(out));
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(request) = serde_json::from_str::<proto::Request>(&line) else {
            let _ = write_line(&out, &proto::Response::err(None, proto::Error::new(proto::code::PARSE_ERROR, "not JSON-RPC")));
            continue;
        };
        // A notification asks for nothing back.
        let Some(id) = request.id.clone() else {
            continue;
        };
        let params = request.params.clone().unwrap_or(Value::Object(Default::default()));
        let reply = match request.method.as_str() {
            METHOD_ROBOT_MAP => {
                let reply = proto::Response::ok(
                    Some(id),
                    &MapStreamResult { accepted: true, enabled: true, mode: Some(host.mode().as_str().to_owned()) },
                );
                if write_line(&out, &reply).is_err() {
                    break;
                }
                stream_frames(&host, out.clone());
                continue;
            }
            wire::METHOD_ROBOT_MAP_SAVE => match decode::<MapNameParams>(&params) {
                Some(p) if p.is_valid() => intent(host.save_as(&p.name)),
                _ => refused(BAD_NAME),
            },
            wire::METHOD_ROBOT_MAP_LIST => ok(&SavedMaps { maps: host.list() }),
            wire::METHOD_ROBOT_MAP_LOAD => match decode::<MapNameParams>(&params) {
                Some(p) if p.is_valid() => intent(host.load(&p.name)),
                _ => refused(BAD_NAME),
            },
            wire::METHOD_ROBOT_MAP_MATCH => match decode::<MapMatchParams>(&params) {
                Some(p) if p.name.as_deref().is_none_or(wire::valid_name) => {
                    ok(&host.matches(p.name.as_deref()).unwrap_or_else(MapMatches::none))
                }
                _ => ok(&MapMatches::none(BAD_NAME)),
            },
            wire::METHOD_ROBOT_MAP_ADOPT => match decode::<MapAdoptParams>(&params) {
                Some(p) if wire::valid_name(&p.name) => intent(host.adopt(p)),
                _ => refused(BAD_NAME),
            },
            METHOD_ROBOT_MAP_WIPE => {
                if host.wipe() {
                    ok(&proto::IntentResult::accepted())
                } else {
                    refused("the mapper is overloaded; try again")
                }
            }
            other => Err(proto::Error::new(proto::code::METHOD_NOT_FOUND, format!("the map socket has no `{other}`"))),
        };
        let response = match reply {
            Ok(result) => proto::Response::ok(Some(id), &result),
            Err(error) => proto::Response::err(Some(id), error),
        };
        if write_line(&out, &response).is_err() {
            break;
        }
    }
}

/// A `robot.map` subscription: its own thread pushing `map.frame`s until
/// the caller goes away.
fn stream_frames(host: &Host, out: Writer) {
    let frames = host.subscribers.add();
    std::thread::spawn(move || {
        for frame in frames {
            let note = serde_json::json!({"jsonrpc": "2.0", "method": METHOD_MAP_FRAME, "params": frame});
            if write_line(&out, &note).is_err() {
                break; // dropping `frames` unsubscribes at the next publish
            }
        }
    });
}

fn decode<T: DeserializeOwned>(params: &Value) -> Option<T> {
    serde_json::from_value(params.clone()).ok()
}

fn ok(result: &impl serde::Serialize) -> Result<Value, proto::Error> {
    Ok(serde_json::to_value(result).expect("results serialize"))
}

fn refused(reason: &str) -> Result<Value, proto::Error> {
    ok(&proto::IntentResult { accepted: false, reason: Some(reason.to_owned()) })
}

fn intent(outcome: Result<(), String>) -> Result<Value, proto::Error> {
    match outcome {
        Ok(()) => ok(&proto::IntentResult::accepted()),
        Err(e) => refused(&e),
    }
}
