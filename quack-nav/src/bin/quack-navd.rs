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
    // Which daemon robotd is (`daemon`, "auto" asks it) before the rest of
    // the file: `[gait]`'s defaults are that daemon's walk.
    let (choice, robotd_socket) = NavdConfig::daemon_head(&path)?;
    quack_duck::daemon::set_active(quack_duck::daemon::resolve(choice, &robotd_socket));
    let config = NavdConfig::load(&path)?;
    tracing::info!(config = %path, socket = %config.socket, robotd = %config.robotd_socket, "quack-navd");
    // The walk robotd runs: the explorer's gait model and `[gait]`'s
    // defaults follow it.
    quack_duck::gait::set_active(config.gait.profile);
    tracing::info!(profile = ?config.gait.profile, gait = ?config.gait, "quack-navd: the walk");
    // What this duck learned of its own walk, laid over the numbers.
    quack_nav::ledger::init(Some(config.ledger_path.clone().into()), config.gait.yaw_trim, (config.gait.yaw_gain_left, config.gait.yaw_gain_right));

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
        let driving = explore.clone();
        // ... and while it finds the pose a job asked for (see `relocate`),
        // which is also what lets an untrusted mapper search.
        host.set_driving(move || driving.busy());
        // ... and stands aside while a look at the floor ahead holds the
        // head (see `Body::look_down`).
        let lent = explore.clone();
        host.set_head_lent(move || lent.head_lent());
    }
    let shutdown = Shutdown { mapper: mapper.clone(), explore, sockets: vec![config.maploc.socket.clone(), config.socket.clone()] };
    if mapper.is_some() {
        save_on_signal(shutdown.clone())?;
    }
    // Events pushed to subscribers (`nav.subscribe`): a watcher reads the
    // job's status and the map's pose and publishes what changed.
    let events = quack_nav::events::Hub::default();
    {
        let (explore, map) = {
            let r = robot.lock().expect("robot poisoned");
            (r.places.explore.clone(), r.places.map.clone())
        };
        quack_nav::events::spawn(events.clone(), move || quack_nav::events::Facts {
            job: explore.status().to_json(),
            pose: map.as_ref().and_then(|m| m.snapshot().latest).map(|f| (f.tracking, f.untrusted, f.seated, f.pose())),
        });
    }
    let daemon = Arc::new(Daemon { robot: robot.clone(), knobs_env: config.knobs_env.clone(), shutdown, events, owner: Mutex::new(quack_nav::lease::Owner::default()), next_conn: std::sync::atomic::AtomicU64::new(1) });

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
    events: quack_nav::events::Hub,
    /// The lease on the duck's motion (`nav.claim`), if one is held.
    owner: Mutex<quack_nav::lease::Owner>,
    next_conn: std::sync::atomic::AtomicU64,
}

impl Daemon {
    /// Say a lease's end on the `lease` topic.
    fn lease_ended(&self, lease: &quack_nav::lease::Lease, why: &str) {
        tracing::info!(holder = %lease.holder, why, "lease ended");
        self.events.publish(quack_nav_proto::topic::LEASE, json!({"state": "ended", "holder": lease.holder, "why": why}));
    }
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
        quack_nav::ledger::flush();
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

/// One caller, one thread, NDJSON in and out — robotd's own shape. A
/// subscription adds a second writer on the same connection, the events'
/// (`nav.subscribe`), so the lane's writes go through one lock.
fn serve(stream: UnixStream, daemon: Arc<Daemon>) {
    let out = match stream.try_clone() {
        Ok(out) => Arc::new(Mutex::new(out)),
        Err(e) => {
            tracing::warn!(error = %e, "the caller's lane could not be split");
            return;
        }
    };
    let mut subscribed = false;
    let conn = daemon.next_conn.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match subscription(&line) {
            Some((id, params)) => subscribe(id, params, &daemon, &out, &mut subscribed),
            None => answer(&line, &daemon, conn),
        };
        if writeln!(out.lock().expect("lane poisoned"), "{reply}").is_err() {
            break;
        }
    }
    // The holder's connection closed: its lease ends with it.
    let gone = daemon.owner.lock().expect("owner poisoned").gone(conn);
    if let Some(l) = gone {
        daemon.lease_ended(&l, "its connection closed");
    }
}

/// The request, when it is a `nav.subscribe`.
fn subscription(line: &str) -> Option<(Value, Value)> {
    let request: Value = serde_json::from_str(line).ok()?;
    (request.get("method").and_then(Value::as_str) == Some(quack_nav_proto::method::SUBSCRIBE))
        .then(|| (request.get("id").cloned().unwrap_or(Value::Null), request.get("params").cloned().unwrap_or(json!({}))))
}

/// `nav.subscribe`: the topics granted, and from then on the events on
/// this connection, written by a thread of their own. Once per connection.
fn subscribe(id: Value, params: Value, daemon: &Daemon, out: &Arc<Mutex<UnixStream>>, subscribed: &mut bool) -> Value {
    use quack_nav_proto::code;
    let params = match serde_json::from_value::<quack_nav_proto::SubscribeParams>(params) {
        Ok(p) => p,
        Err(e) => return error(id, code::INVALID_PARAMS, &format!("nav.subscribe: {e}")),
    };
    let topics = match params.granted() {
        Ok(t) => t,
        Err(e) => return error(id, code::INVALID_PARAMS, &format!("nav.subscribe: {e}")),
    };
    if *subscribed {
        return error(id, code::INVALID_REQUEST, "nav.subscribe: this connection is already subscribed; open another for other topics");
    }
    *subscribed = true;
    let events = daemon.events.subscribe(&topics);
    let lane = out.clone();
    std::thread::Builder::new()
        .name("subscriber".into())
        .spawn(move || {
            for event in events {
                let note = json!({"jsonrpc": "2.0", "method": quack_nav_proto::method::EVENT, "params": event});
                if writeln!(lane.lock().expect("lane poisoned"), "{note}").is_err() {
                    return;
                }
            }
            // Dropped by the hub (it fell behind): the connection goes too.
            let _ = lane.lock().expect("lane poisoned").shutdown(std::net::Shutdown::Both);
        })
        .expect("the subscriber thread starts");
    tracing::info!(topics = ?topics, "nav.subscribe");
    json!({"jsonrpc": "2.0", "id": id, "result": quack_nav_proto::SubscribeResult { topics }})
}

fn answer(line: &str, daemon: &Daemon, conn: u64) -> Value {
    let robot = &daemon.robot;
    let request: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(e) => return error(Value::Null, quack_nav_proto::code::PARSE_ERROR, &format!("not JSON: {e}")),
    };
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let method = request.get("method").and_then(Value::as_str).unwrap_or_default();
    let params = request.get("params").cloned().unwrap_or(json!({}));
    use quack_nav_proto::{code, method as m};
    match method {
        m::HELLO => match serde_json::from_value::<quack_nav_proto::HelloParams>(params) {
            Ok(hello) => {
                let who = hello.client.as_deref().unwrap_or("a client");
                match hello.api_version.and_then(|v| quack_nav_proto::skew(v, quack_nav_proto::API_VERSION)) {
                    Some(skew) => tracing::info!(client = who, skew, "nav.hello"),
                    None => tracing::info!(client = who, api_version = quack_nav_proto::API_VERSION, "nav.hello"),
                }
                let result = quack_nav_proto::HelloResult::of_this_build(env!("CARGO_PKG_VERSION"), option_env!("QUACK_NAV_REVISION"));
                json!({"jsonrpc": "2.0", "id": id, "result": result})
            }
            Err(e) => error(id, code::INVALID_PARAMS, &format!("nav.hello: {e}")),
        },
        m::CATALOG => json!({"jsonrpc": "2.0", "id": id, "result": quack_nav::tools::catalog()}),
        m::CALL => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or_default().to_owned();
            let args = params.get("args").cloned().unwrap_or(json!({}));
            if !quack_nav::tools::handles(&name) {
                return error(id, code::METHOD_NOT_FOUND, &format!("this daemon has no tool `{name}`"));
            }
            // The lease: a tool that moves the duck from anyone but its
            // holder is refused; a stop never is, and ends the lease.
            let (verdict, ended) = daemon.owner.lock().expect("owner poisoned").check(conn, &name, &args, std::time::Instant::now());
            if let Some(l) = ended {
                daemon.lease_ended(&l, if quack_nav::lease::is_stop(&name, &args) { "a stop" } else { "its time ran out" });
            }
            if let quack_nav::lease::Verdict::Held { holder, left_s } = verdict {
                return error(id, code::LEASED, &format!("{holder} holds the duck (for {left_s:.0} s more, or until it lets go): `{name}` is refused; a stop is not"));
            }
            // A STOP, or a move by hand, reaches the duck's own motion
            // before the lock its step holds (see `tools::before_the_lock`).
            quack_nav::tools::before_the_lock(&daemon.shutdown.explore, &name, &args);
            let result = {
                let mut robot = robot.lock().expect("robot poisoned");
                quack_nav::tools::execute(&name, &args, &mut robot)
            };
            match result {
                Ok(value) => json!({"jsonrpc": "2.0", "id": id, "result": value}),
                Err(e) => error(id, code::TOOL_ERROR, &e),
            }
        }
        m::CLAIM => {
            let p = match serde_json::from_value::<quack_nav_proto::ClaimParams>(params) {
                Ok(p) => p,
                Err(e) => return error(id, code::INVALID_PARAMS, &format!("nav.claim: {e}")),
            };
            let now = std::time::Instant::now();
            let claimed = daemon.owner.lock().expect("owner poisoned").claim(conn, &p.holder, p.ttl_s, p.token.as_deref(), now);
            match claimed {
                Ok(l) => {
                    daemon.events.publish(quack_nav_proto::topic::LEASE, json!({"state": "held", "holder": l.holder, "why": "claimed"}));
                    let ttl_s = (l.until.saturating_duration_since(now).as_secs_f64() * 10.0).round() / 10.0;
                    json!({"jsonrpc": "2.0", "id": id, "result": quack_nav_proto::ClaimResult { token: l.token, holder: l.holder, ttl_s }})
                }
                Err(quack_nav::lease::Verdict::Held { holder, left_s }) => error(id, code::LEASED, &format!("{holder} holds the duck for {left_s:.0} s more")),
                Err(quack_nav::lease::Verdict::Free) => error(id, code::TOOL_ERROR, "nav.claim: no lease"),
            }
        }
        m::RELEASE => {
            let p = match serde_json::from_value::<quack_nav_proto::ReleaseParams>(params) {
                Ok(p) => p,
                Err(e) => return error(id, code::INVALID_PARAMS, &format!("nav.release: {e}")),
            };
            let released = daemon.owner.lock().expect("owner poisoned").release(conn, p.token.as_deref());
            match released {
                Some(l) => {
                    daemon.lease_ended(&l, "given back");
                    json!({"jsonrpc": "2.0", "id": id, "result": {"released": true, "holder": l.holder}})
                }
                None => json!({"jsonrpc": "2.0", "id": id, "result": {"released": false}}),
            }
        }
        m::KNOBS => match quack_nav::knobs::answer(&params, std::path::Path::new(&daemon.knobs_env)) {
            Ok(value) => json!({"jsonrpc": "2.0", "id": id, "result": value}),
            Err(e) => error(id, code::TOOL_ERROR, &e),
        },
        m::RESTART => json!({"jsonrpc": "2.0", "id": id, "result": restart(daemon)}),
        other => error(id, code::METHOD_NOT_FOUND, &format!("unknown method `{other}`")),
    }
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}
