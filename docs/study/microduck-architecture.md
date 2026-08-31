# Microduck — System architecture (first reading)

Source: `docs/design/architecture.md` (draft 2026-07-22) + README + roadmap.
Stack: Rust, one workspace, no framework. Supervisor: systemd.
SoC: Rockchip RK3566 · 1 GB RAM · 15 Dynamixel servos + IMU on a single UART bus.

## 1. Daemon grid

| Daemon | Owns | Socket / port | Talks to | Survives robotd being dead |
|---|---|---|---|---|
| **robotd** | motors, kinematics, odometry, ONNX policy, safety, `robot.health` | `/run/robotd.sock` | Dynamixel bus (UART) | — |
| **configd** | wifi, robot identity/name, PIN, gamepad bonding, reboot | `/run/configd.sock` | BlueZ + NetworkManager via D-Bus | ✅ (recovery path) |
| **updaterd** | updates: signature verification, atomic swap, health gate, rollback | `/run/updaterd.sock` | GitHub releases, systemctl, robotd | ✅ (recovery path) |
| **btd** | nothing — BLE transport (API subset) | BLE GATT service | robotd, configd, updaterd | ✅ (recovery path) |
| **padd** | nothing — gamepad transport (`pad.input`) | `/run/padd/pad.sock` | robotd | ❌ (depends on robotd) |
| **mediad** | **camera + microphone** pipeline, encoding, perception, WebRTC gateway | TCP `:8080` console, `:8443` signalling | robotd, configd, updaterd | ❌ |
| **tofd** | ToF sensor (8×8 depth matrix) — publishes, nobody reads it | `/run/tofd/tof.sock` | HAT I²C bus | ✅ |
| robotctl | nothing — the CLI, must work on a broken robot | — | all sockets | ✅ |

## 2. Core principles (invariants)

1. **Only robotd touches the robot's hardware.** Clients send *intents*
   ("go at this velocity", "look there", "stand up"); the safety layer inside
   robotd decides what is executable. No other process can command a motor.
2. **configd, updaterd, btd survive robotd being dead**: they are the recovery
   path (no systemd dependency on robotd, no ML runtime, no media stack).
3. **The 50 Hz loop never blocks on other services**: cross-service reads are
   always last-value-wins caches, never synchronous RPC.
4. **A single writer for every piece of state**; everyone else reads/subscribes.

## 3. Communication

- **Control plane**: JSON-RPC 2.0, one object per line (NDJSON), one unix socket
  per service. No broker/bus. Authorization via filesystem permissions
  (mode 0660 + group) and `SO_PEERCRED` (uid/gid for auditing and for mutating calls).
- **Data plane** (video/audio, ~27 MB/s): **never crosses a socket**.
  robotd receives derived *features* ("ball at (x,y)", "loud sound"), not frames.
  Perception next to the sensor, inside mediad.
- One API definition, multiple transports: BLE (subset) · unix socket · WebSocket
  (for server-side agents/LLMs) · WebRTC datachannel.

## 4. State: who owns what, what survives updates

| State | Owner | Notes |
|---|---|---|
| `/etc/robot/robotd.toml` | per-board | feature switches (policy, walk/roller, **audio**, pet detection, camera…); never overwritten by updates |
| `/var/lib/robot/config/config.json` | configd | robot name + PIN (file + flock + atomic rename) |
| Wifi credentials | NetworkManager | never stored by the daemons |
| `/opt/robot/daemon/releases/<ver>/` | updaterd | binaries + policies, atomic swap of the `current` symlink |
| Calibrations / learned state | owning service | outside the releases: survives updates AND rollbacks |

Update flow: push branch → CI builds and signs → `robotctl update apply` →
updaterd verifies signature → swap `current` → restart unit → asks `robot.health`
→ if not healthy, automatic rollback. Crash-loop covered by a boot counter.

## 5. Remote access and agents

- **mediad is the remote gateway**: it owns the WebRTC PeerConnection with
  a video track, a **bidirectional audio track (mic + speaker!)**, a "control"
  datachannel (reliable → robot API) and a "teleop" one (unreliable → high-frequency input).
- **For server-side agents/LLMs the recommended path is WebSocket**, not WebRTC:
  `get_frame` → JPEG on demand + state blob + intents. "A few dozen lines,
  no media stack".
- Remote safety: deadman/heartbeat (if commands stop, robotd stops the robot),
  intents and never motor writes, authority arbitration
  (physical > remote), 1 media session at a time.
- Privacy: explicit consent for remote sessions, visible indicator while
  streaming, end-to-end DTLS-SRTP.

## 6. Notes for the "HA voice satellite" project 🦆🎤

- **Audio lives in mediad** ("camera/mic, encode"), and the audio codec sits on
  the **I²C bus shared with the ToF**. Voice synthesis (quack/chorale/theremin) is
  currently inside robotd (the chorale is ~55 KB of robotd — flagged in the roadmap
  as a pattern to fix with the future "behaviour layer").
- **Path A (zero on-board changes)**: a server-side client that opens the
  WebRTC/WebSocket session towards mediad and uses the bidirectional audio track as
  remote mic/speaker → bridge towards Wyoming/HA on the server. The robot stays stock.
- **Path B (on-board daemon)**: an eighth daemon "voiced/satellited" alongside the
  others, same pattern (unix socket, JSON-RPC), speaking Wyoming/ESPHome towards
  HA. More integrated, but must be kept aligned with updaterd's signed releases.
- The roadmap already lists "ambient sound events" and "voice tags" as inputs of the
  future behaviour layer: possible upstream convergence.
- The `robotd.toml` file has an `audio` switch: figure out what it enables/disables.

## 7. Next readings (in order)

1. `docs/design/robotd-design.md` — the 50 Hz loop in detail: Dynamixel bus,
   observations, policy, safety ("what else hangs off the tick").
2. `mediad/` sources — the audio pipeline: GStreamer or webrtc-rs? Is the mic
   exclusive to the pipeline or shareable (ALSA dmix)?
3. `docs/design/remote-webrtc.md` + `webrtc-console.md` — sessions, signalling,
   control channel (for Path A).
4. `docs/design/updater-design.md` + `restart-order.md` — to understand how to
   package an extra daemon without getting overwritten.
5. `docs/project/roadmap.md` — milestones and open problems (autonomous.rs).
6. `microduck_rl` repo — MuJoCo + PPO, for simulation on the MacBook.
