# ADR 0007: The mapper lives in quack-navd, and robotd stays Pollen's

- Status: accepted
- Date: 2026-09-23
- Inputs: ADR 0005 (consume maploc), ADR 0006 (the navigation is its own
  daemon), `docs/study/upstream-asks.md`, the twin runs of 2026-09-22/23,
  the user's decision of 2026-09-23

## Context

ADR 0005 chose to consume `maploc` as robotd hosted it (upstream PR 127).
Three weeks on the twin added to it — a Localize mode, a map library
(`robot.map_save`, `map_list`, `map_load`, `map_match`, `map_adopt`), a
boot search, the corrections of `upstream-asks.md` — about 3,400 lines on
a branch of `pollen-robotics/microduck` that exists on one laptop. The
navigation uses all of it: without the library there is no homecoming,
without Localize no house mapped once and driven many times.

So quacknav, published on its own since 2026-09-22, could not be run by
anybody else. PR 127 is still open; the additions on top of it are not
even proposed; if they are refused, the only way to try the navigation
is a patched robotd, which on the duck means a binary updaterd does not
manage, rebased at every Pollen release.

What robotd actually gives `maploc` is small: one struct per control
tick (contact odometry, gravity, trunk height, the measured head joints,
a monotonic stamp, three verdicts) and tofd's depth frames. The released
robotd (daemon-v0.14.4, API 34) publishes every field of that struct on
`robot.state` since API v24, on the same clock as `tof.stream`.

## Decision

### 1. quack-navd hosts maploc; robotd is the release, unmodified

`maploc` is vendored as a workspace crate (the fork's `maploc-quacknav`,
16070fd, with `NOTICE` crediting Pollen's PR 127). `quack-nav::mapd` runs
the fork's worker (`robotd/src/maploc.rs`) line for line, fed from
outside:

- `robot.subscribe` with no rate gives every tick (50 Hz); `tof.stream`
  every frame (~14 Hz). The two stamps pair a frame with the head of its
  own instant, as the fork did (9 ms at worst on the twin).
- The released `kinematics` has `Reprojector::project` and not
  `flatten`; `maploc::flat` rebuilds it from the public API. A twin
  recording replays through `evaluate` to the same 200 lines and the
  same map, byte for byte, here and in the fork.
- `[maploc]` in `quack-nav.toml` holds robotd's old section, off by
  default. Off, the map comes from a robotd that hosts maploc (the fork
  still works); on, from this daemon.

### 2. The map keeps robotd's dialect, on a socket of its own

`quack-navd` serves `robot.map` and the library on
`/run/quack-nav/map.sock`, in the fork's methods and shapes. The rest
of the navigation, the homecoming and the twin's viewer read it
unchanged; `NavdConfig::map_socket()` says where to ask. Both of the
daemon's sockets live in the unit's `RuntimeDirectory` — the one place
under `/run` it may bind — at mode 0660, group `robot`, as robotd and
tofd share theirs (ADR 0006 §2, as amended).

### 3. Two verdicts are rebuilt, by the fork's rule

`robot.state` does not carry the loop's `moving` or `sitting`. Both come
from the step label: moving is every label but `stand`, `sit` and
`held`, sitting is `sit`. This is the fork's rule (`busy || label ==
"walk"`), not the release's (`twist_magnitude() > 0.0`): the smoothed
twist decays towards zero for a minute without reaching it, and with the
release's rule no stop reached the map (39 windows in three minutes).

### 4. The head sweep asks before it takes the head

robotd's loop panned the head at every stop. From outside that is
`robot.head` at 20 Hz — and the slot is shared, last writer wins. The
sweep runs only while the navigation drives (an explore job, a journey,
the homecoming exploring) or the mapper searches for its pose, and it
stands aside while the commanded head carries a pitch, a neck pitch or a
roll — values it never writes — and 5 s after. A duck chatting in the
living room keeps its head still; quacksat's thinking pose and a
`robot.look` are left alone.

### 5. The worker keeps the fork's priority

robotd runs at nice 0. The unit puts `quack-navd` at 5; the maploc
worker lowers its own thread to 10, where it ran inside robotd, so a
relocalize search weighs a tenth of the control loop under contention.

## Consequences

- Anybody can run the navigation with the robot Pollen ships: the
  released robotd, `quack-navd`, and (for the twin) `scripts/twin/`.
  Nothing in robotd is patched, nothing waits on an upstream merge.
- Measured on the twin against the fork, 2026-09-23: explore, recognise
  the saved house at boot (78 s against 95), walk to the kitchen (37 s
  against 32); walls against the true apartment 0.032 m against 0.031 on
  the same scripted route, twice — within the fork's own spread.
- The map library, Localize and the boot search are now this repo's to
  keep. If Pollen merges maploc with a library of its own, `[maploc]`
  off and the release's `robot.map` take over, and `mapd` is removed.
- The fork's maploc changes still have an upstream home:
  `upstream-asks.md`, and one line the release could take from the fork
  — the `moving` verdict — is worth asking for whatever happens here.
- Left open: a foreign head pose made of yaw alone is not told from the
  sweep's own; `setpriority` under the unit's `SystemCallFilter` and the
  sockets' group are to be checked on the physical duck.
- Measured the same evening against the fork, on the twin (`scripts/twin/`):
  - a 30-minute autonomous exploration: 82 legs, 157 submaps, no fall, the
    control loop at 50 Hz with no missed tick; its walls 0.043 m from the
    true ones (house2's recording, replayed through today's mapper: 0.061),
    and `map_match` finds it inside house2 at a near-identity transform with
    a 0.030 m wall residual;
  - the pose on the same map (house2, localize), at the same eight points
    of the flat: 0.050 m mean and 0.077 max here, 0.086 and 0.223 on the
    fork — no loss of accuracy;
  - the gait under the same commands is the fork's (from an open spot, the
    yaw rate within 0.06 rad/s at every command): the `[gait]` trims
    stand;
  - blind journeys round six goals: the fork 16/18 and 12/12 over five
    rounds, no fall; here 12/18 on the explored map and one fall, 4/6 on
    house2 and a fall in the round after — **both falls in the passage beside the stairwell**, one
    with the pose 17 cm off towards the hole. Not traced to the port (the
    pose is better here elsewhere, and the fork failed that goal too) and
    not ruled out on so few runs: the passage is the open risk.
- Found the same evening: the gait turns in place from a standstill above a
  dead zone the explorer never crossed (`yaw_max` 0.9): 30°/s at +1.2 rad/s,
  50–60°/s at ±1.5, the body within 4 cm (`scripts/twin/turnprobe.py`). The
  explorer's kick-then-yaw, and the backing manoeuvres beside a drop where
  both falls happened, may not be needed.
- Found on the way, not caused by it: the panorama's turn froze when its
  kick was refused (fixed, 495e8b7), and the example config's tofd
  socket and default socket paths were wrong for the board.
