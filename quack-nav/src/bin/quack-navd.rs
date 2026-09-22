//! `quack-navd`: the navigation daemon.
//!
//! It owns everything that knows where the duck is and drives it
//! somewhere — the map lane, the cliff guard, the places registry, the
//! explorer and the homecoming — and answers for them on a unix socket,
//! the way robotd answers for the body. A voice satellite, an agent, a
//! ROS bridge or a shell script all reach it the same way:
//!
//!     {"jsonrpc":"2.0","id":1,"method":"nav.catalog","params":{}}
//!     {"jsonrpc":"2.0","id":2,"method":"nav.call","params":{"name":"robot.where_am_i","args":{}}}
//!
//! Split from quacksat on 2026-09-22 (the user's: the satellite is a
//! voice assistant, the navigation is its own thing).

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{Arc, Mutex};

use quack_nav::config::NavdConfig;
use quack_nav::tools::Robot;
use serde_json::{Value, json};

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .init();
    let path = std::env::args().nth(1).unwrap_or_else(|| "/etc/robot/quack-nav.toml".into());
    let config = NavdConfig::load(&path)?;
    tracing::info!(config = %path, socket = %config.socket, robotd = %config.robotd_socket, "quack-navd");

    let robot = Arc::new(Mutex::new(Robot::connect(&config.map, &config.robotd_socket, config.gait.clone())));

    // Waking up in a house the duck has mapped before: the daemon's own
    // business now, not the satellite's.
    if config.homecoming.enabled {
        quack_nav::homecoming::spawn(robot.clone(), config.homecoming.clone());
    }

    let _ = std::fs::remove_file(&config.socket);
    let listener = UnixListener::bind(&config.socket)?;
    tracing::info!(socket = %config.socket, "listening");
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let robot = robot.clone();
                std::thread::spawn(move || serve(stream, robot));
            }
            Err(e) => tracing::warn!(error = %e, "a caller could not be accepted"),
        }
    }
    Ok(())
}

/// One caller, one thread, NDJSON in and out — robotd's own shape.
fn serve(stream: UnixStream, robot: Arc<Mutex<Robot>>) {
    let mut out = match stream.try_clone() {
        Ok(out) => out,
        Err(e) => {
            tracing::warn!(error = %e, "the caller's lane could not be split");
            return;
        }
    };
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = answer(&line, &robot);
        if writeln!(out, "{reply}").is_err() {
            break;
        }
    }
}

fn answer(line: &str, robot: &Arc<Mutex<Robot>>) -> Value {
    let request: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(e) => return error(Value::Null, -32700, &format!("not JSON: {e}")),
    };
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let method = request.get("method").and_then(Value::as_str).unwrap_or_default();
    let params = request.get("params").cloned().unwrap_or(json!({}));
    match method {
        "nav.catalog" => json!({"jsonrpc": "2.0", "id": id, "result": quack_nav::tools::catalog()}),
        "nav.call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or_default().to_owned();
            let args = params.get("args").cloned().unwrap_or(json!({}));
            if !quack_nav::tools::handles(&name) {
                return error(id, -32601, &format!("this daemon has no tool `{name}`"));
            }
            let result = {
                let mut robot = robot.lock().expect("robot poisoned");
                quack_nav::tools::execute(&name, &args, &mut robot)
            };
            match result {
                Ok(value) => json!({"jsonrpc": "2.0", "id": id, "result": value}),
                Err(e) => error(id, -32000, &e),
            }
        }
        other => error(id, -32601, &format!("unknown method `{other}`")),
    }
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}
