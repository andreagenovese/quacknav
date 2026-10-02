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
//! Two methods more are for a control plane (quack-control), not for an
//! agent, and are not in the catalog: `nav.knobs` lists the knobs and
//! writes their env file, `nav.restart` saves the session and exits for
//! systemd to start the daemon again with them.
//!
//! Split from quacksat on 2026-09-22 (the user's: the satellite is a
//! voice assistant, the navigation is its own thing).

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
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

    // The mapper, when this daemon hosts it (`[maploc]`): the released robotd
    // publishes what it needs, and the rest of the navigation reads the map
    // on the socket it serves, in robotd's own dialect.
    let mapper = config.maploc.enabled.then(|| {
        quack_nav::mapd::spawn(&config.maploc, &config.robotd_socket, &config.map.tof_socket)
    });
    if let Some(host) = &mapper {
        quack_nav::mapd::server::serve(host.clone(), &config.maploc.socket)?;
    }
    let robot = Arc::new(Mutex::new(Robot::connect(
        &config.map,
        &config.robotd_socket,
        config.map_socket(),
        config.gait.clone(),
    )));
    let explore = robot.lock().expect("robot poisoned").places.explore.clone();
    // The head sweep is the navigation's only while the navigation drives.
    if let Some(host) = &mapper {
        let explore = explore.clone();
        // ... and while it finds the pose a job asked for (see `relocate`),
        // which is also what lets an untrusted mapper search.
        host.set_driving(move || explore.busy());
    }
    let shutdown = Shutdown { mapper: mapper.clone(), explore, sockets: vec![config.maploc.socket.clone(), config.socket.clone()] };
    if mapper.is_some() {
        save_on_signal(shutdown.clone())?;
    }
    let daemon = Arc::new(Daemon { robot: robot.clone(), knobs_env: config.knobs_env.clone(), shutdown });

    // A job asked for on a pose the mapper no longer trusts (the duck may
    // have been moved while it rested) finds the pose first, with the
    // homecoming's search and its budget on a frozen map.
    quack_nav::relocate::spawn(robot.clone(), config.homecoming.boot_search_s.max(60.0) * 4.0);

    // Waking up in a house the duck has mapped before: the daemon's own
    // business now, not the satellite's.
    if config.homecoming.enabled {
        quack_nav::homecoming::spawn(robot.clone(), config.homecoming.clone());
    }

    let listener = quack_nav::sockets::bind(&config.socket, "nav socket", "`socket`")?;
    tracing::info!(socket = %config.socket, "listening");
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let daemon = daemon.clone();
                std::thread::spawn(move || serve(stream, daemon));
            }
            Err(e) => tracing::warn!(error = %e, "a caller could not be accepted"),
        }
    }
    Ok(())
}

/// What the daemon's callers reach beyond the tools.
struct Daemon {
    robot: Arc<Mutex<Robot>>,
    knobs_env: String,
    shutdown: Shutdown,
}

/// How the daemon goes: the running job stopped, the mapping session
/// saved, the sockets removed. A signal and `nav.restart` both end here.
#[derive(Clone)]
struct Shutdown {
    mapper: Option<quack_nav::mapd::Host>,
    explore: quack_nav::explore::ExploreHandle,
    sockets: Vec<String>,
}

impl Shutdown {
    fn now(&self) -> ! {
        self.explore.request_stop();
        if let Some(host) = &self.mapper {
            host.shutdown();
        }
        for socket in &self.sockets {
            let _ = std::fs::remove_file(socket);
        }
        std::process::exit(0);
    }
}

/// SIGTERM and SIGINT save the mapping session before the process goes:
/// the autosave runs once a minute, and a `systemctl restart` should not
/// cost up to a minute of walking (robotd's shutdown path did the same).
fn save_on_signal(shutdown: Shutdown) -> anyhow::Result<()> {
    use signal_hook::consts::{SIGINT, SIGTERM};
    let mut signals = signal_hook::iterator::Signals::new([SIGTERM, SIGINT])?;
    std::thread::Builder::new().name("signals".into()).spawn(move || {
        if let Some(signal) = signals.forever().next() {
            tracing::info!(signal, "shutting down; saving the map");
            shutdown.now();
        }
    })?;
    Ok(())
}

/// `nav.restart`: under systemd, answer, then go the way a SIGTERM goes —
/// the unit's `Restart=always` starts the daemon again, with the knobs'
/// env file read anew, and the homecoming runs as at boot. Anywhere else
/// nobody would start it again, so it stays and says so.
fn restart(daemon: &Daemon) -> Value {
    // systemd (248 and later; Debian 13 has 257) sets it to the pid it
    // started. Inherited from a shell it names that shell, not this
    // process: a twin started from a terminal must not exit for nobody.
    let by_systemd = std::env::var("SYSTEMD_EXEC_PID").ok().and_then(|p| p.parse::<u32>().ok()) == Some(std::process::id());
    if !by_systemd {
        return json!({
            "restarting": false,
            "reason": "quack-navd is not running under systemd: restart it by hand to apply the knobs \
                       (the twin: scripts/twin/twin.sh restart-navd)",
        });
    }
    let shutdown = daemon.shutdown.clone();
    std::thread::spawn(move || {
        // The answer goes out first.
        std::thread::sleep(std::time::Duration::from_millis(300));
        tracing::info!("nav.restart: saving the map and exiting; systemd starts quack-navd again");
        shutdown.now();
    });
    json!({
        "restarting": true,
        "hint": "systemd starts quack-navd again in about 5 s (RestartSec); the running job stops, the map is saved, and the homecoming runs as at boot",
    })
}

/// One caller, one thread, NDJSON in and out — robotd's own shape.
fn serve(stream: UnixStream, daemon: Arc<Daemon>) {
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
        let reply = answer(&line, &daemon);
        if writeln!(out, "{reply}").is_err() {
            break;
        }
    }
}

fn answer(line: &str, daemon: &Daemon) -> Value {
    let robot = &daemon.robot;
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
        "nav.knobs" => match quack_nav::knobs::answer(&params, std::path::Path::new(&daemon.knobs_env)) {
            Ok(value) => json!({"jsonrpc": "2.0", "id": id, "result": value}),
            Err(e) => error(id, -32000, &e),
        },
        "nav.restart" => json!({"jsonrpc": "2.0", "id": id, "result": restart(daemon)}),
        other => error(id, -32601, &format!("unknown method `{other}`")),
    }
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}
