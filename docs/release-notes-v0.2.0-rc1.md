# quack-nav v0.2.0-rc1 — release notes

2026-10-01. Italian copy: [release-notes-v0.2.0-rc1.it.md](release-notes-v0.2.0-rc1.it.md).
The full list of changes is in [CHANGELOG.md](../CHANGELOG.md).

## What this is

A **release candidate validated on the twins** — the MuJoCo twin of the
Microduck (Pollen's released robotd daemon-v0.14.4 and `microduck_rl`'s
body) and the paper twin — **not yet on a real duck**. The physical duck
arrives in December 2026; then the numbers that only hardware can give
(cell pitch, map extent, CPU cost on the RK3566) get measured
([todo-map](todo-map.md#risks-and-open-questions)). Until a first session on
the real duck, take every number below as the twin's, not a promise.

Since 0.1.0 the mapper runs inside `quack-navd` against the released robotd,
the pose goes out at 20 Hz, the duck comes home to a saved map faster and
never wrongly, explores a house a charge at a time, and every journey runs
one loop. The knobs went from 169 to 122.

## Headline results

On the MuJoCo twin unless said otherwise; sources in
[results.md](results.md) and [todo-map.md](todo-map.md) §2d.

- **No fall.** 0 falls in 11 exploration sessions (5 h 30) and 51 journeys
  on three houses (the release's protocol); none since, on casa_grande
  (four sessions, 16 journeys, 8 wakes) and in the five final-house runs
  that validated the loop-closure cap.
- **Journeys arrive.** 46/51 (90 %) at the release's A/B, where the
  earlier build arrived 32/51 (63 %); since then every journey arrived: 16/16
  on casa_grande, and 6/6 and 8/8 per round in the final-house runs.
- **The map.** Walls on the truth 95–100 % in the final-house runs, floor
  known 94–96 %; casa_grande 99 % of its walls on the truth, 0 phantom
  drops. Mean wall error 3.1–4.5 cm on house2 and casa_libera at the
  release.
- **Coming home.** On the wake bench (12 spawns across two houses, then the
  same turned 180°) 23 of 24 wakes confirmed right, none wrong, medians
  87–123 s (126–192 s before the shadow map); casa_grande 8/8, median 84 s
  (72–111 s), 2–16 cm from the truth. At the release every confirmed pose
  was 0.07–0.17 m from the truth.
- **The heading.** With loop closures capped at 4° the heading's median
  error was 0.63–0.92° in all five runs, against 0.97–1.26° in all four
  without; 20 replayed sessions, mean ATE 0.0946 → 0.0891 m.
- **The paper twin gate** (CI, fixed seeds): explore 40 runs, 0 falls, mean
  coverage 53.2 %; `go_to` 30/30. 153 tests pass (1 ignored).

## Build, install, run

Follow the [README](../README.md#running-it): build, the configuration,
the systemd unit, and the paper twin. For the board (Radxa Zero 3,
aarch64, Debian 13), `scripts/cross-build.sh` cross-builds `quack-navd`
from a Mac or a Linux machine without Docker
([Building for the duck](../README.md#building-for-the-duck)), and
`scripts/install-on-duck.sh radxa@<duck>` installs or upgrades it with its
systemd unit ([Installing on the duck](../README.md#installing-on-the-duck)) —
tried in a systemd container, not yet on a board. The MuJoCo twin is in
[scripts/twin/README.md](../scripts/twin/README.md).

## Upgrading from 0.1.0

- **Knobs**: every `QUACKSAT_*` environment variable is read only as
  `QK_*` — there is no alias. The knobs of concluded experiments are gone
  (removed names at the top of [todo-map.md](todo-map.md)); a removed knob
  that is still set is ignored silently. The ones that exist are in
  [knobs.md](knobs.md).
- **Sockets**: `quack-navd` listens on `/run/quack-nav/nav.sock` (was
  `/run/quack-nav.sock`); with `[maploc] enabled` the map is on
  `/run/quack-nav/map.sock`. Callers (quacksat included) must follow.
- **Places**: `places_path` defaults to `/var/lib/quack-nav/places.json`,
  and the old `/var/lib/quacksat/places.json` is not read any more: move it
  (`scripts/install-on-duck.sh` copies it across; the README has the command).
- **robotd**: pinned to daemon-v0.14.4 (API 34). daemon-v0.15.0 is validated
  on the twin only on the branch `microduck-015`.
- **Config**: `[map] explore_turn` still loads and does nothing.
- **Rust API**: `maploc::Mapper::boot_search`, `ExploreHandle::forget_ground`,
  `Grid::unknown_around`, `Control::request_method`, the head limits in
  `quack_duck::body` and `frontier::waypoint` are gone; `explore::Job::new`
  is `Job::new(known, max_s, ask, now)`, and `to_goal`, `start` and
  `start_goto` lost their turning-hand argument too.
- **Logs**: maploc's status line has no `boot` field.
- **Twin viewer**: `QUACK_NAV_MCP` / `QUACK_NAV_MCP_TOKEN` are gone; it asks
  `QUACK_NAV_SOCKET`.

## Known limitations

- **Pose stalls in fast spins**: during a fast spin in place the map pose
  can stay still for up to ~1.5 s, then catch up (seen on casa_arredata).
- **Phantom drops near holes**: a rim booked with a bad pose lands 20–35 cm
  off the true one (about 1 in 50 drops); beside a hole that narrows a
  passage for the planner.
- **Slow wakes east of the apartment's stairwell**: the windows refute a
  right seed for minutes.
- **The covariance is not an alarm yet**: honest in the mean on a fresh map,
  too sure on a resumed one, and it did not flag a 0.35 m drift (σ 0.08 m).
- **Stop-and-scan mapping needs a guided tour**: somebody has to walk the
  duck around with pauses; the tour is still to be designed.
- **CPU on the RK3566 is unmeasured**, as are cell pitch and map extent on
  hardware; maploc is the most CPU-hungry thing the duck can do.
- Journeys are slower than the earlier build's (median 106–111 s against
  66–101 s); "how far along" errs low; everything is twin-only. See
  [results.md, known limits](results.md#known-limits).

## What v0.2.0 final needs

A first session on the real duck: the stack running on the RK3566 against
the released robotd, with the guards on and someone beside it, its numbers
written down beside the twin's.
