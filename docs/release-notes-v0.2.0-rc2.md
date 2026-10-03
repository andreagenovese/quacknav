# quack-nav v0.2.0-rc2 — release notes

2026-10-03. Italian copy: [release-notes-v0.2.0-rc2.it.md](release-notes-v0.2.0-rc2.it.md).
The full list of changes is in [CHANGELOG.md](../CHANGELOG.md); the previous
candidate's notes are in [release-notes-v0.2.0-rc1.md](release-notes-v0.2.0-rc1.md).

## What this is

A **second release candidate, still validated on the twins only** — the
MuJoCo twin of the Microduck (now Pollen's released robotd daemon-v0.15.0
and `microduck_rl`'s body) and the paper twin — **not yet on a real duck**.
The physical duck arrives in December 2026. Until a first session on it,
take every number below as the twin's, not a promise.

rc1 made the duck map, come home and navigate. rc2 is about the hours in
between, when the duck stands, gets picked up, or is driven by hand — and
about what a control page needs from it.

## What changed since rc1

- **Rest, and finding itself before a job.** A duck left standing for a
  minute with nothing to do now *rests*: odometry carries the pose, nothing
  is corrected, and every two minutes one window (the head sweeping once)
  is judged against the map. If the map disagrees twice, or the duck is
  carried, sat down or falls, the pose becomes `untrusted`. The duck does
  not wander off to look for itself: the next `go_to` or `map_explore`
  first walks and looks until it knows where it is (`relocalizing`), then
  sets out — or fails with nothing walked toward the goal.
- **The creep fix.** A standing duck's pose no longer walks along a long
  wall: at a stand the corrections kept stacking on each other along the
  direction the wall barely pins; now they are anchored where the stand
  began.
- **Places belong to their map.** A named place is tied to the map it was
  taught on, so a power-on no longer makes every place stale ("go to the
  kitchen" refused after each boot). A place is `usable`, `pending` (the
  duck has not found itself on that map yet), `other_map` or `stale` (its
  map is gone). `places.json` moves to version 2 and migrates by itself.
- **robotd 0.15.0.** Pollen's crates are pinned to daemon-v0.15.0 (API 37,
  additive); quack-navd runs the same against a board on 0.14.4.
- **The control contract.** What a client that manages quack-navd uses —
  first of all [quack-control](https://github.com/andreagenovese/quack-control),
  the local web page that shows the map and drives the duck — written down
  in [control-contract.md](control-contract.md): the two sockets, the map
  stream, places from a map view (`at`, a place taught at a tapped point),
  `nav.knobs` (the knobs' env file the unit reads at every start) and
  `nav.restart`. The twin's viewer now serves the head camera with
  mediad's own call, so the page reads the twin and the duck alike.
- **Move guards and self-started motion.** `robot.move` from a caller (the
  voice agent's "go forward", quack-control's Advanced tab) keeps the depth
  sensor's cliff guard always on: a hole within 0.40 m ahead stops it, map
  or no map. `robot.map_step` on an uncertain pose judges by the sensor
  alone. When the duck moves on its own (the homecoming's search, its
  exploring, a relocalization) it says so (`self_started`), and the user's
  STOP stops all of it within a tick and holds until a job is asked for.
  Driving the duck by hand — including Pollen's teleop — keeps its
  position.
- **A done map stays frozen, turned maps are counted right.** A house
  declared done is frozen as soon as it is loaded or adopted, so a power-on
  in `stop_and_scan` no longer inks the map it navigates. A map started
  where the duck stood, turned from the house, no longer counts the
  unknown beyond its walls as house ("55 %, stuck" became 83 %).
- **The README opens with a GIF** of a `go_to` on the MuJoCo twin.

## Headline results

On the MuJoCo twin (casa_grande unless said otherwise); sources in
[todo-map.md](todo-map.md) §2d, its entries of 2026-10-01 to 2026-10-03.

- **Coming home, the navigation's way.** The wake bench booting as the
  navigation does (`WAKE_MODE=localize`; 14 spawns on casa_grande and
  casa_arredata, then the same turned 180°): **28 of 28 wakes confirmed
  right**, none wrong, **median 87 s** (75–141 s), 0.01–0.15 m from the
  truth, 0 falls.
- **The stand creep.** The recording where a fourteen-minute stand walked
  the pose 1.56 m along a wall, replayed: the pose stays within **9.4 cm**
  of where it stopped (**1.62 m** before). Forty replayed sessions: ATE as
  before (noise).
- **Rest.** A thirty-minute rest after a `go_to`, the twin's standing duck
  turning and sliding by itself: pose error **mean 10.0 cm, worst 17.2**
  (awake, ten minutes at the same spot: mean 12.0, worst 18.2); a `go_to` woke it 0.13 s
  after the call. quack-navd 2.0–2.35 % of a Mac core resting against
  2.72 % awake.
- **Moved while resting.** Carried 3.3 m into another room and turned 86°:
  untrusted at once, found in 70 s, arrived 0.02 m off. After **~16 h**
  standing, the pose 1.38 m off and untrusted, a `go_to` to the kitchen
  **relocalized in 97 s** (6 steps) and **arrived 0.21 m** from the true
  kitchen.
- **The map** (x26, the final-house protocol explored from nothing, both
  houses): walls on the truth **98 %**, floor known 94–96 %, **0 phantom
  drops**.
- **A stop_and_scan life** (boot on the saved done map, "explore from
  scratch" from the kitchen, `complete`, then two reboots elsewhere):
  **9/9 `go_to` arrived** (0.09–0.30 m from the goal), **0 falls**, the
  exploration declared the house **done on its own at 86 %**. With the done
  map frozen, two boots kept 495 submaps through 16 minutes and two
  `go_to`.
- **The guards.** Facing a hole from 1.15 m, a `robot.move` stopped with the
  trunk 0.56 m short of the rim; nine calls over open floor, no false stop.
  A STOP during the boot's search answers in 0.15 s with 3 cm of coasting
  (7–8 s and up to 0.33 m before).
- **The paper twin gate** (CI, fixed seeds): the same to the byte as rc1 —
  explore 40 runs, 0 falls, mean coverage 53.2 %; `go_to` 30/30. 182 tests
  pass (1 ignored).

## Build, install, run

The board's binary is attached to this release (`quack-navd-aarch64-linux`,
with its sha256), built by CI with `scripts/cross-build.sh`.

Follow the [README](../README.md#running-it). For the board (Radxa Zero 3,
aarch64, Debian 13): [Building for the duck](../README.md#building-for-the-duck),
then `scripts/install-on-duck.sh microduck@<duck>` installs or upgrades it
with its systemd unit ([Installing on the duck](../README.md#installing-on-the-duck);
the board image's account is `microduck` since Pollen's #340) — tried in a
systemd container, not yet on a board. The MuJoCo twin is in
[scripts/twin/README.md](../scripts/twin/README.md).

## Upgrading from rc1

- **robotd**: pinned to daemon-v0.15.0 (API 37). A board still on 0.14.4
  works the same.
- **The unit**: rerun `scripts/install-on-duck.sh` — the new unit reads
  `/var/lib/quack-nav/knobs.env` (`EnvironmentFile=-`; the config's new
  `knobs_env` names the same file).
- **Places**: `places.json` is rewritten as version 2 on first use (a
  version-1 file migrates: current places wait for the first saved map the
  duck is confirmed on). rc1 refuses a version-2 file: keep a copy if you
  may go back. `robot.list_places` adds `state`, `map`, `live_map` and
  `at`; the old `stale` flag now also means `other_map`.
  `robot.remember_place` takes `x`, `y` and is refused while the live map
  is unknown.
- **`robot.move`** (callers): a forward move may end early or be refused at
  a drop — `"done": false`, `stopped`, `walked_s`; every reply carries
  `cliff_guard`, and `stopped_own` when it stopped the duck's own motion.
- **`robot.map_step`**: on a lost or `untrusted` pose only the sensor's
  checks apply (`checks`, `clearance` null); refused while a job the user
  asked for runs.
- **`robot.go_to` / `robot.map_explore`**: on an untrusted pose the answer
  carries `relocalizing: true` and the job starts after the pose is
  confirmed; `{"stop": true}` also stops the duck's own motion and holds
  until a job is asked for.
- **Status fields**: `robot.map_status` and `map.frame` add `resting`,
  `rest_watch` and `untrusted`; `explore.state` can be `relocalizing` or
  `searching`; `explore` adds `self_started` and `stopped_by_user`.
- **`robot.map_load` / `robot.map_adopt`** freeze a house declared done and
  answer `frozen`; `robot.map_explore` on it is refused as before, a
  session under another `save_as` thaws it.
- **Rust API**: `places::Place::generation` became `map`; the registry's
  `generation()` and `observe()` are gone.

## Known limitations

- **A STOP with nothing running still holds the homecoming**: it starts
  nothing on its own until a job is asked for.
- **The stick retries a step that returned at once** (refused, robotd lost,
  the user's hold): it reads it as a stall, three as a bump, and books an
  obstacle at the nose.
- **An untrusted pose stays untrusted until a job**, even when the duck is
  put back where it was: the next `go_to` searches first.
- **Steps instead of holes are untested**: a step down or up was never
  tried on the twin.
- **Temporary obstacles need re-measuring** on the current journey path,
  and what happens when one is taken away again.
- **robotd has no drop stop**: Pollen's teleop drives through robotd, so it
  bypasses quack-nav's cliff guard and can walk off a stair (asked
  upstream, [study/upstream-asks.md](study/upstream-asks.md) §8).
- **The twin's standing duck drifts**: the simulator's stand policy turns
  it about 0.1°/s and slides it (8.5 cm in nine minutes) with a zero command;
  whether the real duck does is unmeasured, and the rest numbers above
  include it.
- **quacksat does not speak on its own**: arrivals are reported only when
  asked, and the duck's own motion is not announced (quack-navd has no
  channel to push it).
- **CPU on the RK3566 is unmeasured**, as are cell pitch and map extent on
  hardware.
- Still open from rc1: the pose can stall for ~1.5 s in fast spins, slow
  wakes east of the apartment's stairwell, the covariance is not an alarm
  yet. See [todo-map.md](todo-map.md#risks-and-open-questions).

## What v0.2.0 final needs

A first session on the real duck: the stack running on the RK3566 against
the released robotd, with the guards on and someone beside it, its numbers
written down beside the twin's.
