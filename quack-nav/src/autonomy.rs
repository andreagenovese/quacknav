//! Living beside Pollen's `autod`, the duck on its own (daemon 0.21, API v60).
//!
//! With `robot.autonomous` on, `autod` wanders, naps, chases a laser dot and
//! plays with the ball — sending `robot.move`, `robot.head` and the rest like
//! any client. `robotd` arbitrates nothing: every intent slot is
//! last-writer-wins. `autod` yields to one driver only, the pad (`padd`, seen
//! as `robot.state.move.source`), so a job of ours started while it drives
//! would fight it step by step.
//!
//! So this daemon names itself (`hello {client: "quack-navd"}`, see
//! `quack_duck::lane::set_client`), watches the switch and the driver on the
//! state stream, and before a tool that moves the duck does what
//! `QK_AUTONOMY` says:
//!
//! - `pause` (default): switch the duck on its own off (`robot.autonomous
//!   {active: false}`, which stops it where it stands), run the job, and switch
//!   it back on once nothing of ours has moved the duck for
//!   `QK_AUTONOMY_RESUME_S` (30 s, as `autod` waits for an idle pad) and no
//!   job runs. Switched back on only if we switched it off.
//! - `refuse`: the tool fails, saying why.
//! - `ignore`: as before 2026-11-13 — both drive.
//!
//! While the duck is on its own, nothing of ours starts by itself (the
//! homecoming, an exploration resumed) and the map's head sweep stands aside:
//! `autod` moves the head for its own surveys. A robotd before v60 never
//! reports the switch, so none of this ever fires there.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use duck_ipc_proto as proto;
use serde_json::Value;

/// What `QK_AUTONOMY` asks of a moving tool while the duck is on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Pause,
    Refuse,
    Ignore,
}

impl Mode {
    fn from_env() -> Self {
        match std::env::var("QK_AUTONOMY").as_deref() {
            Ok("refuse") => Mode::Refuse,
            Ok("ignore") => Mode::Ignore,
            _ => Mode::Pause,
        }
    }
}

/// What the state stream last said.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Seen {
    /// `robot.state.autonomous`: the duck on its own is switched on.
    pub active: bool,
    /// `robot.state.move.source`: whose `robot.move` fills the twist slot.
    pub driver: Option<String>,
}

/// `robot.state` read for the two fields this module needs. Absent fields
/// (a robotd before v60) read as off and nobody.
pub fn read(state: &Value) -> Seen {
    Seen {
        active: state.get("autonomous").and_then(Value::as_bool).unwrap_or(false),
        driver: state.get("move").and_then(|m| m.get("source")).and_then(Value::as_str).map(str::to_string),
    }
}

/// The resume rule, pure: switch it back on when we switched it off, no job
/// of ours runs, and nothing of ours moved the duck for `quiet`.
pub fn should_resume(paused_by_us: bool, busy: bool, since_last_move: Duration, quiet: Duration) -> bool {
    paused_by_us && !busy && since_last_move >= quiet
}

struct Inner {
    socket: String,
    mode: Mode,
    quiet: Duration,
    seen: Mutex<Seen>,
    paused_by_us: AtomicBool,
    last_move: Mutex<Option<Instant>>,
}

/// The handle the daemon keeps; [`global`] reaches it from the sweep and the
/// homecoming.
#[derive(Clone)]
pub struct Autonomy(Arc<Inner>);

static GLOBAL: OnceLock<Autonomy> = OnceLock::new();

/// The daemon's handle, once [`spawn`] ran.
pub fn global() -> Option<&'static Autonomy> {
    GLOBAL.get()
}

/// True while the duck is on its own and this daemon is not the one that
/// should be moving it: nothing of ours starts by itself, the sweep stands
/// aside.
pub fn on_its_own() -> bool {
    global().is_some_and(|a| a.active())
}

/// Start watching robotd. `busy` says whether a job of ours runs.
pub fn spawn(robotd_socket: &str, busy: impl Fn() -> bool + Send + Sync + 'static) -> Autonomy {
    let quiet = std::env::var("QK_AUTONOMY_RESUME_S")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| v.is_finite() && *v >= 0.0)
        .unwrap_or(30.0);
    let a = Autonomy(Arc::new(Inner {
        socket: robotd_socket.to_string(),
        mode: Mode::from_env(),
        quiet: Duration::from_secs_f64(quiet),
        seen: Mutex::new(Seen::default()),
        paused_by_us: AtomicBool::new(false),
        last_move: Mutex::new(None),
    }));
    let _ = GLOBAL.set(a.clone());
    let watcher = a.clone();
    std::thread::Builder::new()
        .name("autonomy".into())
        .spawn(move || loop {
            if let Err(e) = watcher.watch(&busy) {
                tracing::debug!(error = %e, "autonomy: robot.state lost; reconnecting");
            }
            std::thread::sleep(quack_duck::lane::RECONNECT_DELAY);
        })
        .expect("spawning the autonomy watcher cannot fail");
    a
}

impl Autonomy {
    /// The duck on its own is switched on (as last seen).
    pub fn active(&self) -> bool {
        self.0.seen.lock().expect("autonomy poisoned").active
    }

    /// Who drives the twist slot now (as last seen).
    pub fn driver(&self) -> Option<String> {
        self.0.seen.lock().expect("autonomy poisoned").driver.clone()
    }

    /// We switched it off and will switch it back on.
    pub fn paused_by_us(&self) -> bool {
        self.0.paused_by_us.load(Ordering::SeqCst)
    }

    /// Before a tool that moves the duck: `Err` refuses it.
    pub fn before_moving(&self, tool: &str) -> Result<(), String> {
        *self.0.last_move.lock().expect("autonomy poisoned") = Some(Instant::now());
        if !self.active() {
            return Ok(());
        }
        match self.0.mode {
            Mode::Ignore => Ok(()),
            Mode::Refuse => Err(format!(
                "the duck is on its own (Pollen's autonomous mode, autod): `{tool}` is refused; switch it off first (robotctl autonomous off), or set QK_AUTONOMY=pause"
            )),
            Mode::Pause => {
                self.set(false).map_err(|e| format!("the duck is on its own and could not be switched off for `{tool}`: {e}"))?;
                self.0.paused_by_us.store(true, Ordering::SeqCst);
                self.0.seen.lock().expect("autonomy poisoned").active = false;
                tracing::warn!(tool, "autonomy: the duck on its own switched off for a job; back on once the job is over and nothing moved it for a while");
                Ok(())
            }
        }
    }

    /// A line for `robot.map_status`'s hint, if there is something to say.
    pub fn hint(&self) -> Option<String> {
        if self.active() {
            Some(match self.0.mode {
                Mode::Pause => "the duck is on its own (Pollen's autonomous mode): a job asked for switches it off, and it is switched back on after".to_string(),
                Mode::Refuse => "the duck is on its own (Pollen's autonomous mode): moving tools are refused until it is switched off".to_string(),
                Mode::Ignore => "the duck is on its own (Pollen's autonomous mode), and this daemon drives beside it (QK_AUTONOMY=ignore)".to_string(),
            })
        } else if self.paused_by_us() {
            Some("the duck's own autonomous mode is paused for a job of ours; it comes back once the job is over".to_string())
        } else {
            None
        }
    }

    fn set(&self, active: bool) -> Result<(), String> {
        let mut lane = Some(quack_duck::Control::connect(&self.0.socket).map_err(|e| e.to_string())?);
        let call = proto::Call::RobotAutonomous(proto::AutonomyParams { active });
        let r = quack_duck::body::request(&mut lane, &call)?;
        match r.error {
            Some(e) => Err(format!("robot.autonomous: {}", e.message)),
            None => Ok(()),
        }
    }

    fn watch(&self, busy: &impl Fn() -> bool) -> anyhow::Result<()> {
        use std::io::{BufRead, BufReader, Write};
        let stream = std::os::unix::net::UnixStream::connect(&self.0.socket)?;
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        let mut writer = stream.try_clone()?;
        let sub = proto::Request::call(proto::Id::Number(0), &proto::Call::RobotSubscribe(proto::SubscribeParams { hz: Some(2) }));
        let mut line = serde_json::to_vec(&sub)?;
        line.push(b'\n');
        writer.write_all(&line)?;
        let reader = BufReader::new(stream);
        for line in reader.lines() {
            let msg: Value = serde_json::from_str(&line?)?;
            let Some(state) = msg.get("params").filter(|_| msg.get("method").is_some()) else { continue };
            let seen = read(state);
            {
                let mut last = self.0.seen.lock().expect("autonomy poisoned");
                if seen.active != last.active {
                    tracing::info!(active = seen.active, driver = ?seen.driver, "autonomy: the duck on its own switched");
                }
                *last = seen.clone();
            }
            if seen.active {
                // Switched on by somebody else (a tag, robotctl): it is theirs now.
                if self.0.paused_by_us.swap(false, Ordering::SeqCst) {
                    tracing::info!("autonomy: switched back on by somebody else");
                }
                continue;
            }
            let since = self.0.last_move.lock().expect("autonomy poisoned").map_or(Duration::MAX, |t| t.elapsed());
            if should_resume(self.paused_by_us(), busy(), since, self.0.quiet) {
                match self.set(true) {
                    Ok(()) => {
                        self.0.paused_by_us.store(false, Ordering::SeqCst);
                        tracing::warn!("autonomy: the job is over; the duck on its own switched back on");
                    }
                    Err(e) => tracing::warn!(error = %e, "autonomy: could not switch the duck on its own back on"),
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_state_before_v60_reads_as_off_and_nobody() {
        let s = json!({"t": 1.0, "move": {"requested": [0, 0, 0], "applied": [0, 0, 0]}});
        assert_eq!(read(&s), Seen { active: false, driver: None });
    }

    #[test]
    fn the_switch_and_the_driver_are_read() {
        let s = json!({"autonomous": true, "move": {"source": "autod", "requested": [0.2, 0, 0]}});
        assert_eq!(read(&s), Seen { active: true, driver: Some("autod".into()) });
    }

    #[test]
    fn it_resumes_only_what_we_paused_once_quiet() {
        let q = Duration::from_secs(30);
        assert!(should_resume(true, false, Duration::from_secs(31), q));
        assert!(!should_resume(false, false, Duration::from_secs(31), q), "not ours to switch on");
        assert!(!should_resume(true, true, Duration::from_secs(31), q), "a job runs");
        assert!(!should_resume(true, false, Duration::from_secs(5), q), "moved a moment ago");
    }
}
