# robotd — in-depth analysis (robotd-design.md, draft 2026-08-20)

Hardware v1: alpha variant only, Radxa board (RK3566) only, `imu_to_dxl` v2
board only. The "roller" mode (wheels) is not a hardware variant but a
preset: `policy.mode = "roller"` selects policy and tuning.

## 1. The bus — one UART, one owner

- `/dev/ttyS2` at 1 Mbps, Dynamixel v2 protocol, **16 devices**: 15 servos
  (ids 20–24 left leg, 30–34 neck/head/mouth, 10–14 right leg) + the IMU board
  as **id 200 on the same bus** — the SFLP quaternion comes out of the same
  registers as the servos, a single code path, zero IMU abstraction.
- Port exclusion is *organized, not enforced*: `TIOCEXCL` only blocks
  unprivileged opens, but robotd runs as root. Known trap: Armbian puts
  a `serial-getty` on UART2 — `setup-board.sh` masks the unit;
  `fuser -v /dev/ttyS2` answers "who has the bus".
- Two transactions per tick (one `sync_read` of registers 124–136 for all,
  one `sync_write` of the goals) + one per second for voltage/temperature (144–146).
  `return_delay_time=0` is load-bearing: at the default 250 there would be ~8 ms/tick
  of turnaround, 40% of the budget. The `sync_read` is all-or-nothing: one mute
  servo fails the whole transaction and the previous sample is kept.
- Voltage averaged over the pack (discarding anyone that reports 0); temperature
  NEVER averaged — it reports the hottest joint by name. Third source: SoC
  temperature from sysfs, outside RobotIo, so it answers even with a dead bus.
- IMU staleness tracked forever: consecutive identical blocks counted and
  logged (0.5 s threshold), rate-limited so it doesn't flood the journal.

## 2. The tick (50 Hz, tokio task on a dedicated runtime)

```
sync_read (IMU+15 servo) → safety.observe (fallen? debounce 0,2 s)
  → Observation::build [f32;61] ← Command ← gate(deadman) ← intent snapshot
  → Policy::infer (ONNX) [f32;14]  (bocca esclusa, slot 9 = 0)
  → home pose + action_scale × azione → low-pass su testa e gambe
  → safety.apply (UNICO RobotIo: rifiuta non-finiti, clampa al range attuatore)
  → sync_write goal positions
  → publish: atomics sempre; frame di stato solo se qualcuno è iscritto
```

- 61-D observation: `gyro(3) | projected gravity(3) | joint pos(14) |
  vel(14) | last action(14) | command(13)` with command = twist(3) + head(4)
  + body(6). Three documented pitfalls: all-zero body is the nominal encoding;
  head targets travel IN the command (they don't add to the output → double
  bend); body order is `z, roll, pitch` (swapping them tilts sideways).
- Skill priority chain: roulade > kick > ground-pick > sit/rise > stand
  (by |twist| or forced) > walk. Quirks preserved on purpose: kick and rise run
  with the "standing" tuning because that's how they were trained. "Don't regress
  what already works" is the rule.
- `MissedTickBehavior::Skip`: Burst accumulates motor commands, Delay makes the
  rate drift; Skip keeps the schedule and drops the missed ticks.
- `driving = enabled ∧ policy loaded ∧ sensors this tick ∧ ¬limp-fall`.
  Edges: driving starts → controller.reset() (otherwise a lurch from stale
  state); driving ends → hold = current pose captured once (re-reading it every
  tick would let the robot sag under gravity).
- Policy validation at LOAD, not at inference: shape obs[1,61]→act[1,14],
  warm-up before the loop. Workaround for `ort` PANICKING (not erroring) when
  ONNX Runtime is missing: `ensure_runtime` probes the dylib before touching ort.
  `policy.enabled=false` ≠ broken policy: the former is healthy (bench), the
  latter triggers a rollback. ONNX Runtime is a board prerequisite, not in the release.

## 3. Safety — structure, not convention

- `safety` owns the only `RobotIo` write handle: policy and clients
  *propose* targets, the borrow checker prevents them from writing. "Make the
  broken state unrepresentable" instead of remembering a rule.
- Unconditional rules: NaN rejection (not clamped: rejected) + clamping to the
  ACTUATOR range (not per-joint anatomical limits — open point §9.3).
- **Deadman**: stale intents → velocity to zero. Stop ≠ limp: losing comms
  makes the biped STAND STILL (safe state), it doesn't go limp.
- **The fall verdict reports and gates nothing**: a robot on the ground remains
  enable-able, init-able, drivable — it's exactly when those need to work.
  The versions with `fall_limp`/`fall_recover` gates in safety were REMOVED:
  "a safety rule the recovery has to bypass is not a rule".
- **limp_fall is a third event** (on by default): a predictive detector on
  `ġ = −ω × g` from the gyro (exact, without the SFLP filter lag), fires at ~26°
  of predicted tip-over beyond threshold, 3-tick debounce. Sequence: limp at
  gain_limp following the joints down → wait for a quiet gyro → ~1 s ramp to the
  standing pose → handover (zero twist selects the standing network by itself).
  Tuning deliberately "late": a false positive IS a fall caused.
- Battery shutdown: ~10 s EMA, at 6.6 V it sits down and powers off the board
  (a load-induced sag cannot trip it).

## 4. API and concurrency

- Two vocabularies: **incoming intents** (JSON-RPC notifications without id,
  last-writer-wins, 50 Hz: `robot.move`, `robot.head`) and **discrete requests**
  with a response (`robot.stop`, `robot.enable`). Over WebRTC the notifications
  will go on the unreliable "teleop" channel, the requests on the reliable
  "control" one — the distinction falls out of the message family, not from a
  rule to remember.
- Twist and head in separate slots on purpose: de facto single-writer, a gamepad
  on the body and another client on the head don't step on each other. Every
  slot is timestamped: the loop cares about "how old it is", not "what it's worth".
- **Outgoing state**: a `robot.subscribe` stream, decimated per-subscriber,
  bounded broadcast, drop-on-lag (never backpressure on the loop). It reports
  the REJECTED as well as the applied: `requested` vs `applied` + `limited_by` —
  without that, a UI with the joystick forward and the robot standing still is
  unusable. Battery in volts AND percent (6.6–8.2 V NP-F550 mapping computed on
  the robot side, so two screens never show two different numbers). No frame
  assembled if nobody is subscribed.
- **Stateful bring-up**: Limp → (enable: policy loaded + fresh sample) →
  Homing (torque on, 2 s ramp) → Ready. robotd NEVER moves the robot because
  a process started: on restart it adopts the current pose and doesn't touch
  torque — a standing robot stays standing through an update (asserted by a
  test on the ABSENCE of writes). `robot.init`/`robot.relax` served by the
  daemon from the loop itself (no second writer on the bus); not reachable
  via BLE (a phone button that makes the robot go limp is not to be offered).
- **Health published, never asked**: computed on the IPC side from atomics
  (last-tick timestamp + counters) — a stuck loop declares itself unhealthy
  instead of hanging the caller. Only what is attributable to a RELEASE may
  touch the verdict: battery and temperatures are description (a gate on
  battery = robot not updatable while the pack is drained).
- **Separate maintenance namespace** (§3.5): init, torque-off, calibration,
  raw writes outside the intents, so the relay's per-transport allow-list
  keeps them off the remote transports.

## 5. §4.5 — The audio gems (decisive for the voice satellite 🎤)

| Module | What it does | Implication for us |
|---|---|---|
| `sound.rs` | runtime voice: ONE `aplay` child, a new sound kills the old one, **"the codec's PCM is exclusive"** | the speaker is NOT shareable (no dmix): the satellite's TTS must go through sound.rs (via intent/RPC) or coordinate, not open ALSA in parallel |
| `pet-detect/` | ~20 KB CNN over a 40-band log-mel window **from the onboard mic**, in a dedicated worker | the microphone is already read in continuous capture by a process: to verify whether via shareable ALSA (dsnoop) or exclusive — item #1 to clarify in the code |
| `theremin.rs` | depth from tofd at 15 Hz → note + mouth opening | confirms: audio out drivable by external events |
| `chorale.rs` | several ducks sing together, the lowest id conducts, btd carries the beacons | a multi-robot audio coordination pattern already exists |

The audio codec sits on the I²C bus shared with the ToF (from architecture.md).
The "camera/mic → mediad" pipeline is roadmap M5: today the mic is consumed by
pet-detect inside robotd. So the audio picture is STILL IN MOTION towards
mediad — worth following the mediad/ commits before choosing Path A or Path B.

## 6. The model for our daemon: padd

`padd` is the exact template for the voice satellite: its own crate, an
**unprivileged client** (only the `input` and `robot` groups), talks to robotd's
socket, "safe with no pad" because if it goes silent the deadman holds the
robot. An identical `voiced` daemon: minimal groups, intents/RPC towards robotd
(e.g. quack a confirmation, turn the head towards whoever is speaking), audio
stream towards HA. Huge bonus for remote development:
`ssh -L /tmp/robotd.sock:/run/robotd.sock` — the satellite can run on the
MacBook against the real robot without touching the board.

## 7. Their open points / our risks

- The 50 Hz rate is inherited from the Pi Zero 2W, not yet measured on the Radxa
  → there is probably CPU headroom, good for us.
- No per-joint anatomical limits (actuator range only).
- `gilrs` pulls in `libudev-sys`: prefer pure-Rust crates for anything that goes
  on the board — also applies to our daemon (choose the audio crate carefully).
- Cross-building from a Mac: the aarch64 sysroot is missing → build the shipped
  set with `-p updater -p robotd -p robotctl`, or use Linux/Docker (`--docker`).
- robotd does not reload (no SIGHUP, no hot ort swap) — restart to change the
  policy, for now.
