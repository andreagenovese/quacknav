# Installing quack-nav on the duck

This package installs `quack-navd`, the Microduck's navigation daemon, on
the duck's board (Radxa Zero 3, aarch64 Linux, Debian 13) from your own
computer, over ssh. No checkout of the repository and no build are needed.
Italian copy: [README-install.it.md](README-install.it.md).

What you need:

- a duck provisioned by microduck: robotd and tofd running, and the
  `robot` group their sockets belong to;
- ssh access to the duck with an account that has `sudo` (`microduck` on
  the board image; older images had `radxa`);
- on your computer: `bash`, `ssh`, `scp`, `tar`, and `shasum` or
  `sha256sum` (macOS and Linux have them all).

## 1. Download and verify

From <https://github.com/andreagenovese/quacknav/releases>, the package
and its checksum. This package is `@VERSION@`; for a newer one set `V` to
its version (the tag without its `v`):

```sh
V=@VERSION@
gh release download "v$V" --repo andreagenovese/quacknav \
    --pattern "quack-nav-$V-aarch64-linux.tar.gz*"
# or, without gh:
curl -LO "https://github.com/andreagenovese/quacknav/releases/download/v$V/quack-nav-$V-aarch64-linux.tar.gz"
curl -LO "https://github.com/andreagenovese/quacknav/releases/download/v$V/quack-nav-$V-aarch64-linux.tar.gz.sha256"

shasum -a 256 -c "quack-nav-$V-aarch64-linux.tar.gz.sha256"   # or sha256sum -c
```

It must print `OK`. Then unpack:

```sh
tar xzf "quack-nav-$V-aarch64-linux.tar.gz"
cd "quack-nav-$V"
```

## 2. Install

```sh
./install-on-duck.sh --dry-run microduck@192.168.1.42   # optional: print what it would do
./install-on-duck.sh microduck@192.168.1.42
```

It copies the files to the duck, then with `sudo` there:

| on the duck | from this package |
|---|---|
| `/usr/local/bin/quack-navd` | `bin/quack-navd` |
| `/etc/systemd/system/quack-navd.service` | `systemd/quack-navd.service` |
| `/etc/sysusers.d/quack-nav.conf` (user `quacknav`) | `systemd/sysusers.d/quack-nav.conf` |
| `/etc/robot/quack-nav.toml` — **only when there is none** | `quack-nav.example.toml` |

An old `/var/lib/quacksat/places.json` (quack-nav 0.1.0) is copied to
`/var/lib/quack-nav/` when that has none. Then it enables and restarts the
service, printing every command. `SSH_OPTS="-p 2222"` passes options to ssh
and scp.

## 3. The config: /etc/robot/quack-nav.toml

The example works as it is on a standard duck. Edit it on the duck
(`sudo nano /etc/robot/quack-nav.toml`) when something differs:

| key | default | what it is |
|---|---|---|
| `socket` | `/run/quack-nav/nav.sock` | where quack-navd listens (quacksat, quack-control) |
| `robotd_socket` | `/run/robotd.sock` | robotd's socket |
| `[map] tof_socket` | `/run/tofd/tof.sock` | tofd's depth stream (cliff guard, mapper) |
| `[map] places_path` | `/var/lib/quack-nav/places.json` | the named places |
| `[map] cliff_guard` | `true` | refuse steps toward a drop the depth sensor sees |
| `[maploc] enabled` | `true` in the example | quack-navd builds the map itself (off: robotd's own map) |
| `[maploc] mode` | `"stop_and_scan"` | **mapping**: the map grows at every stop. Set `"localize"` once the house is mapped: the map stays as saved, the pose is corrected against it |
| `[maploc] map_path` | `/var/lib/quack-nav/maploc.session` | the working session; the named maps live in `maps/` beside it |
| `[homecoming] enabled` | `true` in the example | at boot, recognise a house mapped before and take its map back |
| `[homecoming] resume_explore` | `true` in the example | after each charge, explore on until the house is done |

Paths must stay under `/var/lib/quack-nav/` (the only directory the
service may write) or `/run/quack-nav/`. Every key left out takes its
default; an unknown key stops the daemon with a message naming it. After an
edit:

```sh
sudo systemctl restart quack-navd
```

## 4. Check

On the duck:

```sh
systemctl status quack-navd
journalctl -u quack-navd -f
# from a user in the `robot` group (or with sudo)
printf '{"jsonrpc":"2.0","id":1,"method":"nav.call","params":{"name":"robot.where_am_i","args":{}}}\n' \
    | nc -U -q1 /run/quack-nav/nav.sock
```

`/run/quack-nav/nav.sock` and `map.sock` exist, mode 0660, group `robot`.
Without robotd or tofd the daemon still starts and waits for them.

## 5. Upgrade

Download the newer package, verify it, unpack it, and run its
`./install-on-duck.sh` the same way. The binary, the unit and the account
are replaced; your `/etc/robot/quack-nav.toml` and the maps and places in
`/var/lib/quack-nav/` are kept (stopping the service saves the mapping
session first).

## 6. Uninstall

On the duck:

```sh
sudo systemctl disable --now quack-navd
sudo rm /usr/local/bin/quack-navd /etc/systemd/system/quack-navd.service /etc/sysusers.d/quack-nav.conf
sudo systemctl daemon-reload
# kept on purpose: /etc/robot/quack-nav.toml and /var/lib/quack-nav/ (the
# maps and places). Remove them, and `sudo userdel quacknav`, only to
# forget the house.
```

More: the project's README, <https://github.com/andreagenovese/quacknav>.
