# Changelog

All notable changes to quack-nav. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[Semantic Versioning](https://semver.org/) (pre-1.0: a minor version may
break things). Italian copy: [CHANGELOG.it.md](CHANGELOG.it.md).

Every number below was measured on the MuJoCo or paper twins, never on a
physical duck; the details are in [docs/results.md](docs/results.md) and
[docs/todo-map.md](docs/todo-map.md).

## [0.2.0-rc1] - 2026-10-01

A release candidate: the whole stack validated on the twins. Release notes:
[docs/release-notes-v0.2.0-rc1.md](docs/release-notes-v0.2.0-rc1.md).

### Added

- **The mapper hosted in quack-navd** (ADR 0007, 2d495a9). `maploc`,
  derived from Pollen's upstream PR 127, is a workspace crate; with
  `[maploc] enabled` `quack-navd` runs it against Pollen's *released*
  robotd (pinned to daemon-v0.14.4, API 34), pans the head at stops and
  serves the map on `/run/quack-nav/map.sock` in robotd's `robot.map*`
  dialect. The worker thread runs at a lower priority than the daemon.
- **The pose at 20 Hz** (e92aa14): a light `map.pose` every 50 ms between
  the 1 Hz `map.frame`s; the live heading error as sampled went from 17°
  RMS to about 2°.
- **The pose's uncertainty** (735b564): a 3×3 covariance (x, y, yaw),
  reported by `robot.map_status` as `pose_uncertainty`. It changes no
  decision yet.
- **Homecoming and relocalization**: the saved pose carried by the
  wake-up's own motion (5b4a041); the boot search asks for confirmation at
  every quarter turn of its scan (708a791); a loss searches near the
  odometry-carried pose (baf1124); the watchdog tries a local match before
  calling a window a contradiction (89c5abe).
- **The adoption rule** (3b5d4df): a saved map is adopted on overlap ≥ 0.50
  and margin ≤ 0.50, asking every 60 s, three agreeing asks. On 27
  replayed wakes: 626 right answers pass (565 before) and none wrong (9
  before).
- **The shadow map** (10f5a22, 2247634, 9c37473): a duck resumed lost on a
  saved map keeps a fresh map of its walk and asks every 30 s where it fits.
  On the twin's wake bench 23 of 24 wakes confirmed right, none wrong,
  medians 87–123 s (126–192 s before). `MAPLOC_SHADOW=0` turns it off.
- **Exploration a charge at a time** (ADR 0008, 2f905c4): one session per
  charge, the map saved under its name, the progress on the books;
  `robot.map_status` reports `house.percent_mapped`; `map_explore complete`
  closes the map, `map_explore fresh` asks for `confirmed` before a new map.
  A finished map is frozen and the duck navigates on it.
- **The session-end rule** (fec3863): a session that has no frontier in
  reach and no unknown piece of 4.5 m² or more touching known floor finds
  the house mapped (furnished maps always keep frontier cells along walls).
- **Journeys on their own loop** (1adc36b, `explore/navigate.rs`): budget,
  pose, arrival and the stick (f98b42b: turns closed on odometry's yaw, a
  stand after every turn and every 0.4 m), the default since 9e84781;
  `robot.go_to` and the exploration's travel to its frontiers (67cde18) both
  use it.
- **Twins**: house2's wall truth (763170e); the generated houses
  casa_libera, casa_arredata and **casa_grande** (d60e067, 9016e8c: 9 × 7 m,
  seven rooms, a corridor turning 90°, two holes, never used to tune
  anything); the twin's scripts and viewer in `scripts/twin/`.
- **Measuring tools**: ATE/RPE (`traj_metrics.py`), the deterministic replay
  bench (`maploc/examples/trajectory.rs`), `wake_bench.py`,
  `maploc/examples/wake_match.rs`, `quack-nav/examples/drop_replay.rs`,
  `map_vs_truth.py`, `room_fit.py`, `LOOP_LOG`; `.mdlg` recordings stamp
  robotd's and tofd's clocks so a replay runs as live (f38b341).
- **CI** (429ba16): unit tests, golden routes, property tests, a replay
  regression and the paper twin gate on fixed seeds on every push; since
  a5c83e8 also `scripts/knobs.py --check`.
- **`docs/knobs.md`**: every environment variable the code reads, generated
  from the code.

### Changed

- **Knobs renamed: `QUACKSAT_*` → `QK_*`, with no alias** (81d466f,
  a5c83e8). The old names are not read any more.
- **169 → 122 knobs** (b7e11ee, 9975422, 76295c5, b1e776e): the knobs of
  concluded experiments were removed, each at its measured default;
  `QK_*` 72 → 35, `MAPLOC_*` 26 → 18. The removed names are listed at the
  top of [docs/todo-map.md](docs/todo-map.md).
- **Loop closures may correct the heading by 4° at most** (c6aae96,
  `MAPLOC_LOOP_CAP_YAW` default 0.45 → 0.07 rad). Twenty replayed sessions:
  ATE RMS mean 0.0946 → 0.0891 m. On the twin the heading's median error
  went from 0.97–1.26° to 0.63–0.92°; map and journeys unchanged.
- **The daemon's socket is `/run/quack-nav/nav.sock`** (was
  `/run/quack-nav.sock`; 7334989, ADR 0006): under systemd it lives in the
  unit's `RuntimeDirectory`, handed to the `robot` group at 0660.
- **`places_path` defaults to `/var/lib/quack-nav/places.json`**, with no
  fallback to `/var/lib/quacksat/` (a5c83e8).
- **Pinned to daemon-v0.14.4** (990d165; 0.1.0 used daemon-v0.10.0).
- The route walked with the string pulled 0.6 m at most, none beside a drop
  (3f2d00b); turning in place past the gait's dead zone (b8cebfe).
- The twin's viewer reaches the daemon over `QUACK_NAV_SOCKET`.

### Removed

- **The explorer's old leg path** (9975422): `walk_leg`, the guarded legs,
  the seal and the widening, with 29 knobs. It ran only with
  `QK_EXPLORE_NAV=0`.
- **The old journey path** (1fffbac): every journey runs `navigate.rs`.
- Experiments measured and not adopted: the traverse, the rim memory, the
  layered costmap and thinned walls (e26fa4a); the DWA local planner
  (a2708e9); Regulated Pure Pursuit (e5e7aaa); maploc's particle filter at
  boot and the ray judge (b1e776e).
- Public API nothing called (76295c5, b1e776e, 8fa4f28):
  `maploc::Mapper::boot_search`, `ExploreHandle::forget_ground`,
  `Grid::unknown_around`, `Control::request_method`, the head/look limits
  and `HOLD_STRAIGHT_MAX` in `quack_duck::body`, `frontier::waypoint`.
- The twin viewer's `QUACK_NAV_MCP` / `QUACK_NAV_MCP_TOKEN` (briefly
  `QUACKSAT_MCP` / `QUACKSAT_TOKEN`), replaced by `QUACK_NAV_SOCKET`
  (twin scripts only).

### Fixed

- A resumed session's re-anchor carries the odometry edge into it (8d5ffeb):
  replayed, wall error 6.9 → 4.6 cm on average, worst trajectory error
  1.95 → 0.16 m.
- A relocalization's pose is not carried off by the freeze its own jump
  caused (7e2825b): 20 replayed sessions, mean ATE 0.1045 → 0.0975 m.
- A covered sensor is not a hole (953285b): 18 phantom holes on the
  apartment's bed gone.
- Two valleys that cross resolve each other (54a49a9).
- No map saved while the duck is lost; no boot resumes on a pose nothing
  could judge (41e5ca3); after a fall the job waits for the pose (dfa044c).
- "Exploration complete" is final, and a finished map stays finished after
  a `go_to` (8643067, 0954d3d); after `fresh`, `map_explore` reports the new
  map (4aa11ba).
- The homecoming's boot search never saves over the house (e8d9482).

### Breaking changes for integrators

- Environment: every `QUACKSAT_*` knob is read only as `QK_*`; the removed
  knobs are ignored silently (list in docs/todo-map.md).
- Sockets and paths: `nav.sock` moved to `/run/quack-nav/nav.sock`;
  `places.json` is read only from `places_path` (default
  `/var/lib/quack-nav/places.json`) — move an old file by hand.
- Config: `[map] explore_turn` still loads but is ignored.
- Rust API: `Mapper::boot_search` and the other items above are gone;
  `explore::Job::new`, `to_goal`, `start` and `start_goto` lost the turning
  hand (`Job::new(known, max_s, ask, now)`).
- Logs: maploc's status line no longer carries the `boot` field.
- Twin viewer: `QUACK_NAV_MCP*` replaced by `QUACK_NAV_SOCKET`.

## [0.1.0] - 2026-09-22

The navigation split out of quacksat (ADR 0006, 747b233) with its history:
`quack-duck` (robotd's lane) and `quack-nav` (the map client of robotd's
`maploc`, the cliff guard, the costmap planner, the places registry, the
explorer, the homecoming, the twelve `robot.*` tools, the paper twin and
`quack-navd` on `/run/quack-nav.sock`), against daemon-v0.10.0. 60 tests.

