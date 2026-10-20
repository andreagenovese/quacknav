//! `daemon = "auto"`: robotd says which release it is in `hello`'s
//! `daemon_version` (every robotd since the protocol had a hello).

use std::time::{Duration, Instant};

use duck_ipc_proto as proto;

use super::{Choice, Line};
use crate::lane::{Control, RECONNECT_DELAY};

/// How long a start waits for robotd to answer before it takes the newest
/// line: robotd comes up with the board, quack-navd right after it.
pub const AUTO_WAIT: Duration = Duration::from_secs(30);

/// The release robotd says it is, once: `None` when it does not answer or
/// does not say.
pub fn ask(robotd_socket: &str) -> Option<String> {
    let mut control = Control::connect(robotd_socket).ok()?;
    let call = proto::Call::Hello(proto::HelloParams { api_version: proto::API_VERSION });
    let hello: proto::HelloResult = control.request(&call).ok()?.result_as().ok()?;
    hello.daemon_version.map(|v| v.to_string())
}

/// The line to drive. `auto` asks robotd, for up to [`AUTO_WAIT`], and takes
/// the newest line when it never answers (Pollen's ducks update
/// themselves, so an unknown daemon is most likely a recent one). A line
/// the config names is taken as it is, robotd asked once only to say in the
/// log when it disagrees.
pub fn resolve(choice: Choice, robotd_socket: &str) -> Line {
    resolve_within(choice, robotd_socket, AUTO_WAIT)
}

fn resolve_within(choice: Choice, robotd_socket: &str, wait: Duration) -> Line {
    match choice {
        Choice::Line(line) => {
            match ask(robotd_socket).as_deref().and_then(|v| Line::parse(v).map(|l| (v.to_owned(), l))) {
                Some((version, said)) if said != line => tracing::warn!(
                    daemon = line.name(),
                    robotd = %version,
                    "daemon: the config names {} but robotd is {version}, of line {}; the config's is used",
                    line.name(),
                    said.name()
                ),
                _ => {}
            }
            tracing::info!(daemon = line.name(), validated_on = line.validated_on(), "daemon: as the config names it");
            line
        }
        Choice::Auto => {
            let started = Instant::now();
            loop {
                if let Some(version) = ask(robotd_socket) {
                    match Line::parse(&version) {
                        Some(line) => {
                            tracing::info!(robotd = %version, daemon = line.name(), validated_on = line.validated_on(), "daemon: auto, as robotd says");
                            return line;
                        }
                        None => {
                            tracing::warn!(robotd = %version, "daemon: auto, robotd's version does not parse; the newest line");
                            return Line::LATEST;
                        }
                    }
                }
                if started.elapsed() >= wait {
                    tracing::warn!(
                        socket = robotd_socket,
                        daemon = Line::LATEST.name(),
                        "daemon: auto, robotd did not say which it is; the newest line (set `daemon` in the config to choose)"
                    );
                    return Line::LATEST;
                }
                std::thread::sleep(RECONNECT_DELAY.min(wait));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;

    /// A robotd that answers `hello` as `version` (or with no version).
    fn robotd(version: Option<&'static str>) -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("robotd.sock").to_str().unwrap().to_owned();
        let listener = UnixListener::bind(&path).unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                let mut line = String::new();
                BufReader::new(stream.try_clone().unwrap()).read_line(&mut line).unwrap();
                let request: proto::Request = serde_json::from_str(&line).unwrap();
                let result = proto::HelloResult {
                    api_version: 41,
                    daemon_version: version.map(|v| proto::semver::Version::parse(v).unwrap()),
                    revision: None,
                };
                let mut out = serde_json::to_vec(&proto::Response::ok(request.id, &result)).unwrap();
                out.push(b'\n');
                stream.write_all(&out).unwrap();
            }
        });
        (dir, path)
    }

    #[test]
    fn auto_takes_what_robotd_says() {
        let (_old, path) = robotd(Some("0.16.1"));
        assert_eq!(ask(&path).as_deref(), Some("0.16.1"));
        assert_eq!(resolve_within(Choice::Auto, &path, Duration::ZERO), Line::V0_16);
        let (_new, path) = robotd(Some("0.17.6"));
        assert_eq!(resolve_within(Choice::Auto, &path, Duration::ZERO), Line::V0_17);
    }

    #[test]
    fn auto_without_an_answer_is_the_newest_line() {
        let (_quiet, path) = robotd(None);
        assert_eq!(resolve_within(Choice::Auto, &path, Duration::ZERO), Line::LATEST);
        assert_eq!(resolve_within(Choice::Auto, "/nonexistent/robotd.sock", Duration::ZERO), Line::LATEST);
    }

    #[test]
    fn a_named_line_wins_over_robotd() {
        let (_new, path) = robotd(Some("0.17.6"));
        assert_eq!(resolve_within(Choice::Line(Line::V0_16), &path, Duration::ZERO), Line::V0_16);
    }
}
