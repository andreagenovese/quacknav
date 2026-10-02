# Changelog

All notable changes to quack-nav. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[Semantic Versioning](https://semver.org/) (pre-1.0: a minor version may
break things). Italian copy: [CHANGELOG.it.md](CHANGELOG.it.md).

Every number below was measured on the MuJoCo or paper twins, never on a
physical duck; the details are in [docs/results.md](docs/results.md) and
[docs/todo-map.md](docs/todo-map.md).

## [Unreleased]

### Added

- **`robot.move` keeps a cliff guard, always on.** The timed move the
  voice agent ("go forward") and quack-control's Advanced tab drive with
  had no guard at all: at a stair's edge it walked off. Now, before the
  first command and at every tick, the depth sensor's newest frames
  (walking ones too, the blind leg's rule: a true hole, not a wall's foot,
  two frames agreeing) are read in the move's lane (0.17 m half-width):
  a hole whose near edge is within 0.40 m ends the move with one explicit
  zero, whatever the pose says and with or without a map. The reply says
  so: `{"done": false, "stopped": "a drop ahead (depth sensor): …",
  "walked_s": …, "cliff_guard": "on"}`; a complete move answers
  `"done": true` as before, with `cliff_guard`. Forward moves only — the
  sensor looks forward and down: backing up and sidesteps are `"not
  covered"`, a turn in place `"not judged"`, no sensor `"off"`. The
  guard is the callers' alone (nav.call from a client: the voice front
  end, quack-control, any socket client): the explorer's legs, the
  homecoming's pulses and steps and the rim tour move through
  `tools::internal_move`, unguarded as before, with their own guards
  tuned to approach a rim — given the always-on guard too, an exploration
  on casa_arredata (x25) had its legs stopped 17 times at the corridor's
  hole and sealed itself in. On
  the twin (casa_grande): walking at the hole's rim, stopped with the
  trunk 0.56 m short of it and every further forward call refused; nine
  calls over 4 m of open floor, no false stop.
- **`robot.map_step` on an uncertain pose judges by the sensor alone.**
  With the pose lost or `untrusted` (moved while it rested) the map's
  checks — the wall ahead, the passage's sides, the wall-hug, the centring,
  the clearance in the reply — would be judged at a place the duck may not
  be: they are left out, only the depth sensor's checks apply (the drop
  guard, things in the way, "stand first" where it has not looked), and the
  reply's new `checks` says `"position uncertain: checks from the sensor
  only"` (otherwise `"map and sensor"`), `clearance` null, with a hint.
- **Driving the duck around by hand keeps its position**, checked: the
  rest's verdict on motion is robotd's step label, which any client's
  walking sets — Pollen's teleop through robotd, quack-nav never told,
  as much as `robot.move` — so a drive wakes the rest as "motion" and the
  pose follows odometry, trusted; only motion the gait does not explain
  (odometry moved past 5 cm or 0.15 rad with no walking: a carry; a sit;
  a fall) makes it `untrusted`. On the twin: resting, driven 2 m straight
  through robotd at 50 Hz like `padd` — woke as "motion", never
  untrusted, the map pose 1 cm from the truth's displacement, the next
  stand's windows integrating. Pollen's teleop bypasses quack-nav's cliff
  guard, and robotd has none: said in the README's safety note and asked
  upstream ([docs/study/upstream-asks.md §8](docs/study/upstream-asks.md)).
- **The duck's own motion is said, and STOP stops it for good.** When
  the duck walks by itself — the homecoming's walk-and-look at boot (new
  `explore.state` `searching`), the exploring the homecoming starts
  (no confirmation, "starting a fresh map and exploring instead"; a
  progressive session; after adopting the saved map), the relocalization
  before a job — `robot.map_status` says so (`explore.self_started`, the
  reason, a hint) and quack-navd logs it once at warn. The user's stop
  (`robot.go_to`/`robot.map_explore` `{"stop": true}`, quack-control's
  STOP) now covers all of it, the boot's search included, and acts before
  the robot's lock: the leg in flight ends within a tick and the stand is
  cut short (on the twin the STOP answered in 0.15 s and the duck coasted
  3 cm; before, it waited 7–8 s for the step to end, 0.18–0.33 m walked).
  It then holds — `explore.stopped_by_user`, "stopped by the user; not
  searching again until asked" — until a job is asked for: the homecoming
  starts nothing more. A `robot.move` or `robot.map_step` while the duck
  moves on its own stops that motion and obeys (`stopped_own` in the
  reply); after a STOP they work at once. `robot.map_step` is now refused,
  like `robot.move`, while a job the user asked for runs.
- **Rest: long idle stands** (maploc). A minute still with no job driving
  the body (no `go_to`, exploration, rim tour, homecoming), the mapper
  rests: no window is integrated or corrects the pose, odometry alone
  carries it, and a *watch* — one window, the head swept once (6 s) —
  is judged against the map as the rest begins and every two minutes,
  nothing applied. Agreement keeps the rest; a pose the tracking's own
  match would move wakes it for the windows to correct; a residual past
  0.06 m, or 0.01 m past the rest's best, wakes them too the first time,
  and after that, twice in a row, makes the pose doubtful: lost, as the
  watchdog makes it, and searched for within a metre. A job, a move or a
  push nobody commanded ends the rest on its first tick (a `go_to` on the
  twin: 0.12 s from the call). `robot.map_status` and `map.frame` carry
  `resting` and `rest_watch` (the last verdict, its residual and age);
  quack-navd logs each rest, watch and wake. `MAPLOC_REST=0` keeps every
  stand awake. The twin's standing duck turns and slides by itself (its
  standing network, not quack-nav): over thirty-minute stands it turned
  up to 139° and slid 12-22 cm, and the pose stayed 5.5-7.9 cm from the
  truth on average; four recorded long stands replayed, rest against
  awake, even (the mean error 0.057 against 0.088 m on one, 0.046 against
  0.037 on another). quack-navd spends 2.0-2.35 % of a Mac core resting
  against 2.72 % standing awake; the replay, 46 % less CPU. Forty
  replayed sessions, ATE as before (mean 0.0920 -> 0.0913 m). The duck
  may be moved anywhere while it rests: two watches the map contradicts, a
  push past a nudge (5 cm, 0.15 rad of odometry with nobody walking it), a
  sit or a fall make the pose `untrusted` — searched over the whole map
  as at a boot, never resumed unverified, and only when a job asks: a
  `go_to` or `map_explore` then answers `relocalizing: true`, the
  homecoming's walk-and-look search finds the pose
  (`quack-nav/src/relocate.rs`, `explore.state` "relocalizing"), and the
  job starts once maploc confirms it — or fails with nothing walked
  toward the goal. On the twin: carried 3.3 m into another room, found in
  70 s and arrived 0.02 m off; a 0.35 m silent carry, found in 71 s,
  arrived 0.15 m off. Details in
  [docs/todo-map.md](docs/todo-map.md).

- **Places from a map view**: `robot.list_places` gives each place's `at`
  (the anchor `go_to` walks to, map metres); `robot.remember_place` with
  `x` and `y` names a point of mapped floor instead of where the duck
  stands. What quack-control's page needs to pin places and name a tapped
  point; nothing else changes.
- **The control contract** ([docs/control-contract.md](docs/control-contract.md)):
  what quack-navd offers a client that manages it — quack-control's local
  web page first. `nav.knobs` lists the knobs quack-navd reads and writes
  their env file, `/var/lib/quack-nav/knobs.env` (config `knobs_env`),
  which the unit now reads at every start (`EnvironmentFile=-`);
  `nav.restart` saves the session and exits for systemd to start the
  daemon again (`Restart=always`), and outside systemd says so. Both are
  methods, not tools: an agent never sees them. `scripts/knobs.py` also
  writes `quack-nav/src/knobs.json`, the machine-readable list (checked by
  `--check`). The twin reads `$STATE/knobs.env` too, and
  `scripts/twin/twin.sh restart-navd` restarts quack-navd alone.

### Changed

- **The shadow map works in localize** (maploc): its own mapper took the
  live map's configuration, frozen there, and inked nothing; it is now
  never frozen and never rests.
- **A relocalization no longer inks the frozen map** (maploc, localize).
  The window that confirmed a pose was inked while the mapper was still
  lost, so every boot and every recovery added a submap to the saved
  house (627 -> 628 on the twin's casa_grande), and a stand that began
  after it judged its windows against its own ink. Thirty replayed
  sessions on a frozen map: ATE mean 0.1183 -> 0.1169 m, 4 better and 4
  worse by more than 5 mm.

- **Pinned to daemon-v0.15.0** (API 37; was daemon-v0.14.4, API 34):
  Pollen's `duck-ipc-proto` and `kinematics` at that tag. API 35–37 are
  additive — `robot.state` gains the servos' measured `velocities` (rad/s)
  and `currents_ma` (present current, mA), optional on the wire, and
  `update.status` two fields of `updaterd`'s; `kinematics` is unchanged.
  For the board: `quack-navd` behaves the same, against robotd 0.15.0 or
  a board still on 0.14.4 (the new blocks are absent there and nothing
  reads them yet). robotd 0.15.0 publishes them by default (`[control]
  publish_velocity_and_load`), about 10–12 % more bytes per state frame.
  The twin runs it (`scripts/twin/README.md`).

### Fixed

- **A standing duck's pose no longer creeps along a wall.** At a stand,
  every still window's tracking correction took the last window's answer
  as its prior, so corrections of a centimetre added up along a direction
  the scene barely pins: on casa_grande (localize mode, after a `go_to`)
  the twin's duck stood fourteen minutes before one long wall and its map
  pose walked 1.56 m along it in 132 corrections, each improving its
  window's residual, until every `go_to` failed "no way to … on the map".
  maploc's stand now keeps its windows' prior, in position, where the stand
  began (the heading's stays the pose's own, which corrects odometry's yaw
  drift): replayed, the same recording stays within 9 cm of where it
  stopped (1.62 m before). Forty replayed sessions of ten runs: ATE RMS
  mean 0.0913 -> 0.0920 m, 14 better and 14 worse by more than 5 mm —
  noise. `trajectory`'s `FROZEN=1` now also freezes the map a
  `MAP_LOAD_AT_S` load builds, and its `CORR_LOG` carries each correction's
  conditioning.
- **Named places survive a power-on.** The registry made every place
  stale whenever the map reported fewer submaps than ever seen, and every
  boot with the homecoming starts on a fresh map before it loads the saved
  one: any tool call in between ("go to the kitchen") lost them all — one
  "places: the map was reset" per session on casa_grande, `cucina` and
  `soggiorno` stale after two restarts on the twin. A place now belongs to
  the map it was taught on (a lineage the registry keeps with the saved
  map's name): `pending` until the duck is confirmed on that map, `usable`
  then; `other_map` while another saved map is live; `stale` only when its
  map is gone (saved over by a new one, or wiped unsaved).
  `robot.list_places` adds `state`, `map` and `live_map` (`stale` stays,
  meaning "not on this map"), `robot.where_am_i` `pending_places`;
  `robot.go_to` says why a named place cannot be reached now. A version-1
  `places.json` is read and migrated ([README-places](quack-nav/README-places.md#which-map-a-place-belongs-to)).
- docs/knobs.md and `nav.knobs` give a default named by a constant as its
  value (`QK_DROP_INFLATE` 0.05, not `DROP_INFLATE_DEFAULT`), a knob with no
  comment above its read takes the sentence of its file that names it, and
  the twin's oracle knobs (`QK_ORACLE_*`) are left out of `nav.knobs`.
- The install examples ssh in as `microduck`, the board image's account
  (Pollen's #340), not `radxa`.
- The GitHub release's text: CI makes the release notes' relative links
  absolute to the files at the tag (`scripts/ci/release_body.py`); on the
  release page they led nowhere (fixed by hand on v0.2.0-rc1).

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
- **The board's binary from CI**: an `aarch64` job cross-builds
  `quack-navd` on every push (artifact `quack-navd-aarch64-linux` with its
  sha256, glibc symbols checked against the 2.31 floor); a `v*` tag attaches
  it to the GitHub release, a prerelease when the tag has `-rc`.
- **Cross-building for the board** (`scripts/cross-build.sh`):
  `cargo zigbuild` for `aarch64-unknown-linux-gnu` with the glibc floor
  pinned at 2.31, no Docker; the binary needs glibc 2.30 and ran in a
  Debian 13 arm64 container. README, "Building for the duck".
- **Installing on the duck** (`scripts/install-on-duck.sh <user@host>`):
  binary, unit, service account and — only when absent — the config
  (`quack-nav/quack-nav.example.toml`, new) over ssh, an old
  `/var/lib/quacksat/places.json` copied across, the service enabled and
  restarted; rerun, it upgrades. README, "Installing on the duck", with the
  manual commands, checks, upgrade and uninstall; run in a systemd Debian 13
  arm64 container, not yet on a board.

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
- Startup errors name their path: a socket that cannot be bound (the nav
  socket, the map socket), a config file that cannot be read or parsed, and
  the non-fatal warnings (robotd unreachable, the places registry, the saved
  session) say which file and what to do — run by hand without
  `/run/quack-nav/`, the daemon said only `No such file or directory`.

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

