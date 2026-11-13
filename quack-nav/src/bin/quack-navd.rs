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
    // Every lane to robotd says who it is (`hello {client}`, API v60): the
    // duck on its own (`autod`) and a pad see whose twist it is.
    quack_duck::lane::set_client("quack-navd");
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
    // Pollen's duck on its own (`autod`, daemon 0.21): a job asked for
    // pauses it, nothing of ours starts by itself while it runs (see
    // `quack_nav::autonomy`).
    {
        let busy = explore.clone();
        quack_nav::autonomy::spawn(&config.robotd_socket, move || busy.busy());
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
    let daemon = Arc::new(Daemon { robot: robot.clone(), knobs_env: config.knobs_env.clone(), shutdown, events, owner: Mutex::new(quack_nav::lease::Owner::default()), next_conn: std::sync::atomic::AtomicU64::new(1), queue: Mutex::new(None) });

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

/// The acting tools the daemon serves beside the navigation's (v5).
const ACT_SKILLS: &str = "robot.skills";
const ACT_TOOL: &str = "robot.act";
/// A queue's skill step, internal: `nav.act`'s second step.
const ACT_STEP: &str = "nav.act.skill";

/// The catalogue: the navigation's tools and the acting ones.
fn catalog() -> Value {
    let mut tools = quack_nav::tools::catalog();
    tools.push(json!({
        "name": ACT_SKILLS,
        "description": "What the duck can do besides walking: its skills, as robotd has them now (a pick from the floor, the kicks, sit and stand, whatever its policy set adds). Nothing moves.",
        "parameters": {"type": "object", "properties": {}}
    }));
    tools.push(json!({
        "name": ACT_TOOL,
        "description": "Do one of the duck's skills (robot.skills names them): \"pick it up\", \"kick the ball\", \"sit down\". With `at` (a place's name, or {x, y} in map metres) the duck walks there first, then does it. Starts in the background and returns at once: answer the user now and end your turn. Its end — done, or why not — comes on the events (queue) and in nav.jobs. Call robot.go_to with stop=true to stop it.",
        "parameters": {"type": "object", "properties": {
            "skill": {"type": "string", "description": "the skill's name, from robot.skills"},
            "at": {"description": "\"here\" (the default), a place's name, or {\"x\": …, \"y\": …}"}
        }, "required": ["skill"]}
    }));
    Value::Array(tools)
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
    /// The queue of jobs under way (`nav.jobs`): its stop and what it has
    /// said so far.
    queue: Mutex<Option<(quack_nav::queue::Stop, Arc<Mutex<Value>>)>>,
}

impl Daemon {
    /// One tool, as `nav.call` runs it for connection `conn`: the lease
    /// checked, a stop reaching the duck's own motion first, then the tool
    /// under the robot's lock. `Err((code, message))`.
    fn call_tool(self: &Arc<Self>, conn: u64, name: &str, args: &Value) -> Result<Value, (i64, String)> {
        use quack_nav_proto::code;
        match name {
            ACT_SKILLS => return self.skills().map(|s| json!({"skills": s.iter().map(|(n, d)| json!({"name": n, "duration_s": d})).collect::<Vec<_>>()})).map_err(|e| (code::TOOL_ERROR, e)),
            ACT_TOOL => {
                let p: quack_nav_proto::ActParams = serde_json::from_value(args.clone()).map_err(|e| (code::INVALID_PARAMS, format!("robot.act: {e}")))?;
                return self.act(conn, p);
            }
            ACT_STEP => {
                let skill = args.get("skill").and_then(Value::as_str).unwrap_or_default();
                return self.do_skill(skill).map_err(|e| (code::TOOL_ERROR, e));
            }
            _ => {}
        }
        if !quack_nav::tools::handles(name) {
            return Err((code::METHOD_NOT_FOUND, format!("this daemon has no tool `{name}`")));
        }
        let (verdict, ended) = self.owner.lock().expect("owner poisoned").check(conn, name, args, std::time::Instant::now());
        if let Some(l) = ended {
            self.lease_ended(&l, if quack_nav::lease::is_stop(name, args) { "a stop" } else { "its time ran out" });
        }
        if let quack_nav::lease::Verdict::Held { holder, left_s } = verdict {
            return Err((code::LEASED, format!("{holder} holds the duck (for {left_s:.0} s more, or until it lets go): `{name}` is refused; a stop is not")));
        }
        if quack_nav::lease::MOVING.contains(&name)
            && !quack_nav::lease::is_stop(name, args)
            && let Some(a) = quack_nav::autonomy::global()
        {
            a.before_moving(name).map_err(|e| (code::TOOL_ERROR, e))?;
        }
        quack_nav::tools::before_the_lock(&self.shutdown.explore, name, args);
        let mut robot = self.robot.lock().expect("robot poisoned");
        quack_nav::tools::execute(name, args, &mut robot).map_err(|e| (code::TOOL_ERROR, e))
    }

    /// robotd's skills now (`robot.skills`): the table's and the built-ins.
    fn skills(&self) -> Result<Vec<(String, Option<f64>)>, String> {
        let mut robot = self.robot.lock().expect("robot poisoned");
        let r = quack_duck::body::request(&mut robot.control, &duck_ipc_proto::Call::RobotSkills)?;
        if let Some(e) = r.error {
            return Err(format!("robot.skills: {}", e.message));
        }
        Ok(quack_nav::act::skills(&r.result.unwrap_or(Value::Null)))
    }

    /// One skill, asked of robotd and watched to its end (the policy label
    /// leaving `walk`/`stand` and coming back), the robot's lock held only
    /// for the asking.
    fn do_skill(&self, skill: &str) -> Result<Value, String> {
        let skills = self.skills()?;
        let Some((_, length)) = skills.iter().find(|(n, _)| n == skill).cloned() else {
            return Err(format!("no skill `{skill}` on this duck (it has: {})", skills.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ")));
        };
        let (cliff, map) = {
            let mut robot = self.robot.lock().expect("robot poisoned");
            let r = quack_duck::body::request(&mut robot.control, &duck_ipc_proto::Call::RobotDo(duck_ipc_proto::DoParams { skill: skill.to_string() }))?;
            if let Some(e) = r.error {
                return Err(format!("robotd refused `{skill}`: {}", e.message));
            }
            (robot.places.cliff.clone(), robot.places.map.clone())
        };
        let Some(cliff) = cliff else { return Ok(json!({"skill": skill, "seen": "no state stream to watch it on"})) };
        let seen = quack_nav::act::watch(
            || cliff.snapshot().policy,
            || map.as_ref().and_then(|m| m.snapshot().latest).is_some_and(|f| f.seated),
            std::thread::sleep,
            length,
            quack_nav::act::parks(skill),
        );
        let carrying = quack_nav::ledger::report().get("state").and_then(Value::as_str).map(str::to_string);
        match seen {
            quack_nav::act::Seen::Ran { secs, label } => Ok(json!({"skill": skill, "ran_s": (secs * 10.0).round() / 10.0, "label": label, "body": carrying})),
            quack_nav::act::Seen::NeverBegan => Err(format!("`{skill}` never began: the body stayed on its walk")),
            quack_nav::act::Seen::DidNotEnd { label } => Err(format!("`{skill}` ({label}) did not end in its time")),
            quack_nav::act::Seen::Fell { label } => Err(format!("the duck fell or was held during `{skill}` ({label})")),
        }
    }

    /// `nav.act` / `robot.act`: a queue of its own — a walk to `at` first
    /// when given, then the skill.
    fn act(self: &Arc<Self>, conn: u64, p: quack_nav_proto::ActParams) -> Result<Value, (i64, String)> {
        use quack_nav_proto::code;
        let skills = self.skills().map_err(|e| (code::TOOL_ERROR, e))?;
        if let Some(a) = quack_nav::autonomy::global() {
            a.before_moving("robot.act").map_err(|e| (code::TOOL_ERROR, e))?;
        }
        if !skills.iter().any(|(n, _)| *n == p.skill) {
            return Err((code::INVALID_PARAMS, format!("no skill `{}` on this duck (it has: {})", p.skill, skills.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", "))));
        }
        let mut steps = Vec::new();
        match &p.at {
            None => {}
            Some(Value::String(here)) if here == "here" => {}
            Some(Value::String(place)) => steps.push(quack_nav::queue::Step { tool: "robot.go_to".into(), args: json!({"place": place}) }),
            Some(v) if v.get("x").is_some() && v.get("y").is_some() => steps.push(quack_nav::queue::Step { tool: "robot.go_to".into(), args: json!({"x": v["x"], "y": v["y"]}) }),
            Some(other) => return Err((code::INVALID_PARAMS, format!("robot.act: `at` is \"here\", a place's name or {{x, y}}, not {other}"))),
        }
        steps.push(quack_nav::queue::Step { tool: ACT_STEP.into(), args: json!({"skill": p.skill}) });
        self.start_queue(conn, steps, quack_nav::queue::OnFailure::Stop)
    }

    /// Start a queue for connection `conn`, refused when one runs or another
    /// connection holds the lease.
    fn start_queue(self: &Arc<Self>, conn: u64, steps: Vec<quack_nav::queue::Step>, on_failure: quack_nav::queue::OnFailure) -> Result<Value, (i64, String)> {
        use quack_nav_proto::{code, topic};
        let (verdict, _) = self.owner.lock().expect("owner poisoned").check(conn, "nav.jobs", &json!({}), std::time::Instant::now());
        if let quack_nav::lease::Verdict::Held { holder, .. } = verdict {
            return Err((code::LEASED, format!("{holder} holds the duck: a queue is refused")));
        }
        let mut slot = self.queue.lock().expect("queue poisoned");
        if slot.is_some() {
            return Err((code::TOOL_ERROR, "a queue is already running; stop it first (nav.jobs {\"stop\": true})".into()));
        }
        let stop = quack_nav::queue::Stop::default();
        let n = steps.len();
        let said = Arc::new(Mutex::new(json!({"running": true, "steps": n, "done": []})));
        *slot = Some((stop.clone(), said.clone()));
        drop(slot);
        let d = self.clone();
        std::thread::Builder::new()
            .name("queue".into())
            .spawn(move || {
                d.events.publish(topic::QUEUE, json!({"state": "started", "steps": n, "tools": steps.iter().map(|s| s.tool.clone()).collect::<Vec<_>>()}));
                let report = quack_nav::queue::run(
                    &steps,
                    on_failure,
                    &stop,
                    |tool, args| {
                        let first = d.call_tool(conn, tool, args).map_err(|(_, e)| e);
                        // A walk refused because the pose is not trusted yet
                        // (just after a sit, a rise, a carry): the queue waits
                        // for the map to vouch for it again, then tries once more.
                        match first {
                            Err(e) if quack_nav::queue::STARTS_A_JOB.contains(&tool) && e.contains("not sure of its position") && !stop.asked() => {
                                let map = d.robot.lock().expect("robot poisoned").places.map.clone();
                                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
                                while std::time::Instant::now() < deadline && !stop.asked() {
                                    if map.as_ref().and_then(|m| m.snapshot().latest).is_some_and(|f| f.tracking && !f.untrusted) {
                                        return d.call_tool(conn, tool, args).map_err(|(_, e)| e);
                                    }
                                    std::thread::sleep(std::time::Duration::from_millis(500));
                                }
                                Err(e)
                            }
                            other => other,
                        }
                    },
                    || {
                        let st = d.shutdown.explore.status();
                        (st.state.as_str().to_string(), st.reason.clone())
                    },
                    |o| {
                        d.events.publish(topic::QUEUE, json!({"state": "step", "index": o.index, "tool": o.tool, "ok": o.ok, "reason": o.reason, "result": o.result}));
                        if let Some(done) = said.lock().expect("said poisoned").get_mut("done").and_then(Value::as_array_mut) {
                            done.push(json!({"index": o.index, "tool": o.tool, "ok": o.ok, "reason": o.reason}));
                        }
                    },
                    std::thread::sleep,
                );
                tracing::info!(state = report.state, steps = report.steps.len(), "queue ended");
                d.events.publish(topic::QUEUE, json!({"state": report.state, "steps": report.steps.iter().map(|o| json!({"index": o.index, "tool": o.tool, "ok": o.ok, "reason": o.reason})).collect::<Vec<_>>()}));
                *d.queue.lock().expect("queue poisoned") = None;
            })
            .expect("the queue thread starts");
        Ok(json!({"queued": n, "started": true, "on_failure": if on_failure == quack_nav::queue::OnFailure::Stop { "stop" } else { "continue" }}))
    }

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

fn answer(line: &str, daemon: &Arc<Daemon>, conn: u64) -> Value {
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
        m::CATALOG => json!({"jsonrpc": "2.0", "id": id, "result": catalog()}),
        m::ACT => match serde_json::from_value::<quack_nav_proto::ActParams>(params) {
            Ok(p) => match daemon.act(conn, p) {
                Ok(v) => json!({"jsonrpc": "2.0", "id": id, "result": v}),
                Err((c, e)) => error(id, c, &e),
            },
            Err(e) => error(id, code::INVALID_PARAMS, &format!("nav.act: {e}")),
        },
        m::CALL => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or_default().to_owned();
            let args = params.get("args").cloned().unwrap_or(json!({}));
            match daemon.call_tool(conn, &name, &args) {
                Ok(value) => json!({"jsonrpc": "2.0", "id": id, "result": value}),
                Err((c, e)) => error(id, c, &e),
            }
        }
        m::JOBS => jobs(id, params, daemon, conn),
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

/// `nav.jobs`: start a queue, answer its state, or stop it.
fn jobs(id: Value, params: Value, daemon: &Arc<Daemon>, conn: u64) -> Value {
    use quack_nav_proto::code;
    let p = match serde_json::from_value::<quack_nav_proto::JobsParams>(params) {
        Ok(p) => p,
        Err(e) => return error(id, code::INVALID_PARAMS, &format!("nav.jobs: {e}")),
    };
    let ok = |v: Value| json!({"jsonrpc": "2.0", "id": id, "result": v});
    if p.stop == Some(true) {
        let running = daemon.queue.lock().expect("queue poisoned").as_ref().map(|(stop, _)| stop.clone());
        return match running {
            Some(stop) => {
                stop.now();
                let _ = daemon.call_tool(conn, "robot.go_to", &json!({"stop": true}));
                ok(json!({"stopping": true}))
            }
            None => ok(json!({"stopping": false, "reason": "no queue is running"})),
        };
    }
    let Some(steps) = p.steps else {
        let state = daemon.queue.lock().expect("queue poisoned").as_ref().map(|(_, said)| said.lock().expect("said poisoned").clone());
        return ok(state.unwrap_or_else(|| json!({"running": false})));
    };
    if steps.is_empty() || steps.len() > quack_nav::queue::MAX_STEPS {
        return error(id, code::INVALID_PARAMS, &format!("nav.jobs: 1 to {} steps", quack_nav::queue::MAX_STEPS));
    }
    if let Some(bad) = steps.iter().find(|s| !quack_nav::tools::handles(&s.tool) && s.tool != ACT_TOOL && s.tool != ACT_SKILLS) {
        return error(id, code::INVALID_PARAMS, &format!("nav.jobs: no tool `{}`", bad.tool));
    }
    let on_failure = match p.on_failure.as_deref() {
        None | Some("stop") => quack_nav::queue::OnFailure::Stop,
        Some("continue") => quack_nav::queue::OnFailure::Continue,
        Some(other) => return error(id, code::INVALID_PARAMS, &format!("nav.jobs: on_failure is `stop` or `continue`, not `{other}`")),
    };
    let mut expanded = Vec::new();
    for st in steps {
        if st.tool == ACT_TOOL {
            let p: quack_nav_proto::ActParams = match serde_json::from_value(st.args.clone()) {
                Ok(p) => p,
                Err(e) => return error(id, code::INVALID_PARAMS, &format!("nav.jobs: robot.act: {e}")),
            };
            if let Some(at) = &p.at
                && at.as_str() != Some("here")
            {
                let args = if let Some(place) = at.as_str() { json!({"place": place}) } else { json!({"x": at["x"], "y": at["y"]}) };
                expanded.push(quack_nav::queue::Step { tool: "robot.go_to".into(), args });
            }
            expanded.push(quack_nav::queue::Step { tool: ACT_STEP.into(), args: json!({"skill": p.skill}) });
        } else {
            expanded.push(quack_nav::queue::Step { tool: st.tool, args: st.args });
        }
    }
    match daemon.start_queue(conn, expanded, on_failure) {
        Ok(v) => ok(v),
        Err((c, e)) => error(id, c, &format!("nav.jobs: {e}")),
    }
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}
