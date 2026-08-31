# Study: padd as the model for quacksat's robotd client

Source: `pollen-robotics/microduck` @ clone of 2026-08-31. Blueprint for
`quacksat-core`'s client. Companion notes: `microduck-speaker-path.md`,
`microduck-mic-path.md`.

## padd in one paragraph

Unprivileged gamepad daemon: one thread, blocking `std` `UnixStream`, no
tokio. All wire types come from `duck-ipc-proto` (`proto::Call`,
`Request::{call,notify}`, `Response`); NDJSON JSON-RPC 2.0, one object per
line, `\n` framing, always `flush()`. Its crate doc states the thesis:
"no privileged access… the intent API is the path the app, the SDK and any
remote client will use".

## Wire contracts to internalize

- **Continuous → notification** (no id, no reply): `robot.move`,
  `robot.head`, `robot.pose`, `robot.mouth`, held `robot.do`/`robot.sound`.
  **Discrete → request** (incrementing `Id::Number`): `robot.enable`,
  one-shot `robot.do`, `robot.setMode`, `robot.shutdown`, `robot.stop`,
  `robot.init`, `robot.relax`, `robot.theremin`.
- `IntentResult { accepted: false, reason }` is a **normal answer**, not an
  error; only update local state when `accepted` (padd's setMode pattern).
  JSON-RPC `error` is a separate case. Params structs are
  `deny_unknown_fields`; result structs are not — tolerate unknown fields
  and unknown lines.
- robotd caps lines at `MAX_LINE = 64 KiB`.
- `hello` exists (`HelloResult { api_version: 16, … }`) but is a liveness
  check, **not a gate** — no daemon refuses on version skew; warn once.

## The deadman (safety contract)

- `deadman_ms = 500` (`proto::pad_link::DEADMAN_MS`). Only `robot.move`
  refreshes the twist clock; `robot.head` deliberately does not. Past the
  age, robotd zeroes the twist ("stop, not limp"); surfaced as
  `move.limited_by = "deadman"` in the state stream.
- Two valid postures: drive at ≥20 Hz (padd: 50 Hz) sending `robot.move`
  each tick, **or send nothing** when there's no command — the deadman
  stops the robot on its own. **Never send a fabricated zero `robot.move`
  as a keepalive**: it masks "no driver" as "driver said stop". When
  actively in a not-driving mode (posing the head), padd *does* send an
  explicit zero per tick — that's a decision, not a keepalive.
- There is no ping/keepalive method. Head/pose/mouth/sounds/skills are not
  deadmanned; the `wheee` hold decays (300 ms freshness) if not resent.

## Streams (robot.subscribe / robot.state)

- `robot.subscribe { hz: Option<u32> }` → `SubscribeResult` (policy
  filenames + `unavailable`), then `robot.state` notifications forever on
  the same connection. Decimation is server-side per subscriber; ask for
  the lowest hz you need. Re-subscribing replaces the rate.
- Backpressure never reaches the control loop: bounded broadcast (256),
  slow client gets gaps (`Lagged` skipped). Use `RobotState::t`, never
  assume continuity.
- A connection that both calls and subscribes needs **one long-lived
  BufReader**, correlating responses by `id` and skipping notifications
  (rule: a notification carries `method`, a response does not). padd's
  fresh-BufReader-per-request works only because padd never subscribes.
  Alternative: separate connections per lane (`Lane::Stream` vs `Prompt`),
  as `robotctl monitor` does (four connections, three daemons).
- Other streams: `pad.input`→`pad.report` (padd's own socket
  `/run/padd/pad.sock`), `tof.stream`→`tof.frame` (tofd).

## Connection lifecycle

- padd: connect once; on any error log + exit non-zero; systemd retries
  (`Restart=always`, `RestartSec=5s`, `After=robotd.service` +
  `Wants=robotd.service`, not `Requires`).
- In-process alternative (better for quacksat, which holds session state):
  robotd's theremin client — one function per connection lifetime, outer
  loop sleeps fixed 2 s, reconnects **and re-subscribes**. Bound connects
  and writes (btd: 3 s / 5 s timeouts), drop connection on write error.
- Socket path from `proto::socket::ROBOT` with a `--socket` override so
  `ssh -L /tmp/robotd.sock:/run/robotd.sock` bench work keeps working.

## Privileges and authorization

- robotd socket: `0o660`, owner `root:robot` (`Group=robot` on the unit is
  load-bearing). **Group membership is the entire authorization** — no
  SO_PEERCRED check, no handshake, no per-method ACL in robotd. quacksat in
  group `robot` gets the whole `robot.*` surface.
- Per-namespace restriction lives at the *transport*: btd's exhaustive
  `permits()` match (no wildcard, so new Call variants fail to compile
  until classified) refuses motor control, sounds, subscribe, shutdown etc.
  over BLE with `PERMISSION_DENIED` (14). updaterd adds a second tier:
  group = may talk, uid allowlist = may mutate.
- **Implication**: any narrower policy for LLM-originated calls is
  quacksat's own to write — imitate btd's exhaustive match if the agent
  backend forwards calls.

## Systemd/user pattern to copy (padd.service)

- `sysusers.d/quacksat.conf`: `u quacksat - "…" - -`.
- Unit: `User=quacksat`, `Group=quacksat`,
  `SupplementaryGroups=robot audio`, `Restart=always`, `RestartSec=5s`,
  `Environment=RUST_LOG=info`, journal output, tracing to stderr with
  EnvFilter.
- Hardening block verbatim from padd: `NoNewPrivileges`,
  `ProtectSystem=strict`, `ProtectHome`, `PrivateTmp`,
  `ProtectKernelTunables/Modules/ControlGroups`, `RestrictSUIDSGID`,
  `RestrictNamespaces`, `LockPersonality`, `MemoryDenyWriteExecute`,
  `SystemCallFilter=@system-service`, empty `CapabilityBoundingSet`.
  Adjust `RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6` (network
  backends); padd's `AF_NETLINK` is a gilrs need, not ours. **Not**
  `PrivateDevices` (ALSA needs /dev/snd).
- `RuntimeDirectory=quacksat` → `/run/quacksat/` is the only writable spot
  under `ProtectSystem=strict`; identity.json and any served socket live
  there; systemd cleans it on stop.
- First thing in main: `duck_ipc_proto::log_startup_identity!("quacksat")`
  (writes `/run/quacksat/identity.json`, logs build identity).

## If quacksat serves its own socket (bridge-facing)

Copy `padd/src/tap.rs`: bind under RuntimeDirectory, remove stale socket,
chmod 0660, then `getgrnam("robot")` + `chown(path, -1, gid)` in code
(supplementary groups aren't inherited by created files). Answer the
subscribe before the first notification; bounded per-subscriber queue
(256) with a dropped-counter, never block the producer;
`METHOD_NOT_FOUND` naming what you serve for foreign methods.

## Logging discipline

Nothing per tick. `warn` = greppable events (connection lost, backend
up/down), `info` = mode changes, `debug` = per-message detail. One line
per transition.
