# Study: duck-ipc-proto surface, updaterd, restart order

Source: `pollen-robotics/microduck` @ clone of 2026-08-31 (workspace
v0.10.0, edition 2024, rust-version 1.89). Companion notes:
`microduck-client-pattern.md`, `microduck-speaker-path.md`,
`microduck-mic-path.md`.

## duck-ipc-proto

- One file (`src/lib.rs`, ~4800 lines). Deps deliberately just three:
  serde, serde_json, semver ("no http, no tar, no crypto, no tokio" — btd
  is part of the recovery path). Feature `test-support` adds
  `every_call()` for exhaustiveness tests, zero extra deps.
- **Not on crates.io**; consumers use path deps. Repo is public by
  decision (updater-design.md: "publish pollen-robotics/microduck",
  2026-08-26). External crates can depend via git.
- **How quacksat should depend on it**:
  ```toml
  duck-ipc-proto = { git = "https://github.com/pollen-robotics/microduck.git", tag = "daemon-v0.10.0" }
  ```
  Pin to a `daemon-v*` release tag (immutable), matching the release on
  the board; bump deliberately. `API_VERSION` (16) is informational — no
  daemon refuses on skew; what breaks is a moved param shape, which
  refuses itself by name. Set `rust-version = "1.89"`. Use the crate's
  `semver` re-export. Enable `test-support` in dev-deps only. Don't
  vendor types.
- Constants: `socket::ROBOT = /run/robotd.sock`, `socket::PAD`,
  `socket::TOF`, `socket::CONFIG`, `socket::UPDATER`; `JOINT_NAMES` (15);
  `identity_path()` → `/run/<service>/identity.json`.
- Envelope: `Request { id: Option<Id>, method, params }` (id absent =
  notification), `Response { id, result, error }`. Public path is
  `Request::call/notify/as_call/as_state`, `Response::ok/err/result_as`.
  Params types are `deny_unknown_fields`; results are not.
- Routing metadata worth reusing: `Call::method()`, `is_mutating()`,
  `destination() -> Option<(Service, Lane)>`. `Lane` =
  `Prompt | Slow | Operation | Stream`; every daemon serves one
  connection one request at a time → **open a separate connection per
  lane** (a `robot.subscribe` stream connection never carries requests).
- Identity helpers quacksat should use: `build_info!()`,
  `publish_identity` / `log_startup_identity!("quacksat")` → writes
  `/run/quacksat/identity.json` (harmless: reconciliation ignores units
  it didn't ship).
- Error codes: JSON-RPC spec codes plus app codes 1–14
  (`BUSY=1 … PERMISSION_DENIED=14`).

## robotd's served surface (dispatch, main.rs:2675–3010)

- Intents accepted as notification *or* request: `robot.move`, `.head`,
  `.pose`, `.mouth`, `.do` (held), `.sound` (wheee hold).
- Request-only: `robot.look` (head IK), `.stop`, `.enable` (toggle),
  `.init`, `.relax`, `.setMode`, `.mode`, `.shutdown`, `.theremin`,
  `.chorale`, `.subscribe`, `.health`, `.safeToRestart`, `.modelApi`,
  `.remoteSessionActive`, `hello`. Anything else →
  `METHOD_NOT_FOUND "<m> is not served by robotd"`.
- Refusal reasons are strings in `IntentResult.reason` (e.g. sound: "this
  robot has no voice…"; theremin: "no depth frames — is tofd running?").
- Notifications never get a reply; unparsable notifications silently
  dropped. `MAX_LINE = 64 KiB`. Many concurrent clients fine (task per
  connection). Lagging subscribers get gaps, never backpressure.
- No `maintenance.*` namespace exists; init/relax etc. sit in `robot.*`
  and are kept off remote transports by btd's routing table only.
- Design invariants (architecture.md / robotd-design.md): no broker, one
  socket per service; async with timeouts everywhere — "a closed or
  silent socket is a normal, expected answer"; robotd authoritative on
  safety; continuous intents = expiring last-writer-wins slots; bring-up
  FSM `Limp → Homing → Ready` — **robotd never moves the robot on its
  own**; health computed from atomics, never by asking the loop.

## updaterd — how quacksat survives updates

- Layout: `/opt/robot/daemon/releases/<ver>/`, `current` symlink
  (atomic rename), `golden` never pruned, `keep_previous = 1`. **No A/B**,
  no rollback outside `install_dir`. Signed (minisign) artifacts; hooks
  ship inside the signed tarball.
- `hooks/postinstall` writes (never deletes): rescue scripts, sound bank,
  the release's own sysusers and unit files (overwriting them — customize
  shipped units only via drop-ins `/etc/systemd/system/<unit>.d/`).
  Nothing enumerates or removes files it didn't ship → a hand-installed
  `quacksat.service` is never touched by apply/rollback/select/revert.
- **The one trap — orphan check** (`updater/src/orphan.rs`): any unit in
  `/etc/systemd/system` whose `Exec*=` resolves under
  `/opt/robot/daemon/current/` joins the managed set; a candidate release
  missing that binary refuses to apply (`WouldOrphanUnit`).
  → **quacksat's ExecStart must NOT live under current/**: use
  `/usr/local/bin/quacksat`.
- Don'ts: don't add quacksat to `on_apply.units` (a failed restart would
  roll back *their* update); don't put files in `releases/`; don't bake
  config into the unit (config file in `/etc/robot/`, state in
  `/var/lib/quacksat/`, both survive update and rollback).
- Optional future coupling: a separate `[component.quacksat]` in
  `/etc/robot/updater.toml` with its own `install_dir = /opt/quacksat`,
  source, and signing key reuses the whole signed-update engine without
  entangling with the daemon component. Not needed for v0.
- Install: idempotent `install-quacksat.sh` writing binary, unit,
  sysusers (`/etc/sysusers.d/quacksat.conf`, not /usr/lib), config,
  state dir; `daemon-reload && enable --now`.

## Restart order / boot

- Update restart set = units the release ships ∪ `on_apply.units`, minus
  updaterd/btd. quacksat is in neither → **an update restarts robotd out
  from under quacksat and never restarts quacksat**. The client must
  treat a dropped `/run/robotd.sock` as normal: reconnect (or exit;
  `Restart=always RestartSec=5s`), **re-subscribe**, and assume latched
  state stale — a restarted robotd is `Limp`, `enabled = false`; the
  robot stays standing (Dynamixels hold last goal).
- Boot: no ordering between daemons beyond padd-after-robotd (advisory)
  and btd-after-bluetooth. quacksat: `After=robotd.service
  local-fs.target network-online.target`, `Wants=robotd.service
  network-online.target` (After, not Requires — a down robotd must not
  block quacksat from starting and retrying).
- Reconciliation (`updater/src/reconcile.rs`) iterates only shipped
  units — quacksat is invisible to it. Good.

## Resource reality (no formal budgets)

- No MemoryMax/CPUQuota/Nice anywhere; the real constraints: journald
  capped at 200 MB (per-tick logging evicts useful logs — log
  transitions only); `/var/log` is zram (lost on power cut); mediad's
  WebRTC path ≈ 25–40% of a core; software encode overheats the SoC.
  Keep quacksat's steady-state CPU modest; wake word inference budget
  matters.
- Hardening baseline: copy padd/tofd blocks; add `AF_INET AF_INET6`;
  **drop `MemoryDenyWriteExecute` if loading an ML runtime (ONNX/JIT)**.

## Alternative worth noting

Instead of playing audio itself, quacksat could drive `robot.sound` +
`robot.mouth` and let robotd be the voice — works only for duck noises
(closed enum), not TTS; see `microduck-speaker-path.md`.
