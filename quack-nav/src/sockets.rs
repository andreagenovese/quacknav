//! The daemon's sockets, shared the way robotd and tofd share theirs.
//!
//! Connecting to a unix socket takes write permission on it. `quack-navd`
//! runs as `quacknav` and its callers do not (quacksat runs as `quacksat`),
//! so a socket left at the umask's 0755 would refuse every one of them.
//! robotd and tofd answer the same question with mode 0660 and the `robot`
//! group — "may talk to the robot" — and so does this: the navigation
//! drives the robot, and whoever may do that directly may do it here.

use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::Path;

use anyhow::Context;

/// Who may call: the group robotd's and tofd's sockets belong to.
pub const GROUP: &str = "robot";
const MODE: u32 = 0o660;

/// Bind the `what` socket at `path` (replacing a stale one) and [`share`]
/// it. A failure names the path, and the config key (`key`) that moves it:
/// by hand, outside the unit that creates `/run/quack-nav/`, a bare
/// "No such file or directory" was all the daemon said.
pub fn bind(path: &str, what: &str, key: &str) -> anyhow::Result<UnixListener> {
    let _ = std::fs::remove_file(path);
    let listener = UnixListener::bind(path).map_err(|e| anyhow::anyhow!(bind_failure(path, what, key, &e)))?;
    share(path).with_context(|| {
        format!("cannot hand the {what} {path} to the `{GROUP}` group at mode 0660 (is this user in `{GROUP}`?)")
    })?;
    Ok(listener)
}

/// What a failed bind says: the path, the error, and what to do about it.
fn bind_failure(path: &str, what: &str, key: &str, e: &std::io::Error) -> String {
    let dir = match Path::new(path).parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.display().to_string(),
        _ => ".".to_owned(),
    };
    let hint = match e.kind() {
        std::io::ErrorKind::NotFound => format!(
            "its directory {dir} does not exist (systemd's RuntimeDirectory= creates it; \
             by hand: mkdir -p {dir} or set {key})"
        ),
        std::io::ErrorKind::PermissionDenied => format!(
            "this user may not create files in {dir} (run it as the unit's user, \
             or set {key} to a directory this user owns)"
        ),
        _ => format!("set {key} to a path this user may create"),
    };
    format!("cannot bind the {what} {path}: {e} — {hint}")
}

/// Give a freshly bound socket to [`GROUP`], mode 0660. Without the group
/// (a laptop, the twin) the socket stays its owner's, which is everyone
/// who runs there; said once, like tofd says it.
pub fn share(path: &str) -> std::io::Result<()> {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(MODE))?;
    match group_id(Path::new("/etc/group"), GROUP) {
        Some(gid) => std::os::unix::fs::chown(path, None, Some(gid))?,
        None => tracing::warn!(
            socket = path,
            group = GROUP,
            "no such group on this system: the socket is for its owner only"
        ),
    }
    Ok(())
}

/// A group's id from a `group(5)` file: `name:password:gid:members`.
fn group_id(file: &Path, name: &str) -> Option<u32> {
    let text = std::fs::read_to_string(file).ok()?;
    text.lines().find_map(|line| {
        let mut fields = line.split(':');
        (fields.next()? == name).then_some(())?;
        fields.nth(1)?.parse().ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_group_is_found_by_name_and_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("group");
        std::fs::write(&file, "root:x:0:\nrobots:x:900:\nrobot:x:996:quacknav,quacksat\n").unwrap();
        assert_eq!(group_id(&file, "robot"), Some(996));
        assert_eq!(group_id(&file, "robo"), None);
        assert_eq!(group_id(&file, "nobody"), None);
    }

    #[test]
    fn a_socket_in_a_missing_directory_names_the_path_and_the_fix() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("absent").join("map.sock");
        let path = path.to_str().unwrap();
        let e = bind(path, "map socket", "[maploc] socket").unwrap_err().to_string();
        assert!(e.contains(&format!("cannot bind the map socket {path}")), "{e}");
        assert!(e.contains("mkdir -p"), "{e}");
        assert!(e.contains("[maploc] socket"), "{e}");
    }

    #[test]
    fn a_shared_socket_is_mode_0660() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nav.sock");
        let _listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        share(path.to_str().unwrap()).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o660);
    }
}
