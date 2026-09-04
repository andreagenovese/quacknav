# ADR 0005: Mapping and localization — consume robotd's maploc

- Status: accepted
- Date: 2026-09-04
- Inputs: `docs/todo-map.md` (2026-08-31 version, now superseded),
  upstream PR 127 "Maploc: mapping & localization as a robotd-hosted
  subcrate", PR 202 "maploc on the MuJoCo twin", PR 126 "ToF
  reprojection & gaze IK", `apirrone/microduck_maploc_rs`, ADR 0001
  (separate repo), ADR 0004 (agent protocol)

## Context

quacksat's second track after voice is giving the duck a map of the
house and a sense of where it is, so the agent can answer "where are
you" and act on "go to the kitchen". The first plan (2026-08-31) assumed
the board could not carry it: everything off-board, camera streamed to
a GPU server running a monocular visual SLAM, AprilTags on door frames
as a cheap first relocalization, a semantic scene graph on top.

A survey of the Pollen repos on 2026-09-03 changed the premise:

- Pollen's own engineer wrote `microduck_maploc_rs` for the prototype
  runtime: 2D submap SLAM on the 8×8 ToF plus contact odometry, with
  loop closure, pose-graph optimization, Monte Carlo relocalization from
  a saved map, and an A* planner with a path follower. Pure Rust, sized
  for the Radxa Zero 3's four Cortex-A55 cores.
- PR 127 (2026-08-21, open, unreviewed, conflicting with main at the
  time of writing) absorbs it as a `maploc/` subcrate hosted by robotd on
  a niced worker thread, fixes six bugs that had made the prototype's
  results noisy and its relocalization unreliable, and exposes the map
  over IPC: `robot.map` subscription, `map.frame` notifications at ~1 Hz
  (pose in the map frame, tracking flag, trinary occupancy grid),
  `robot.map_wipe`. Off by default via `[maploc]` in robotd.toml.
- PR 202 (2026-09-02) ran it on the MuJoCo twin and, after fixing three
  simulator-side issues, measured the tracked pose within ~6 cm of ground
  truth while raw odometry drifted up to 0.35 m.
- PR 126 (merged) gave robotd the ToF reprojection with floor filtering
  and a `robot.look` gaze RPC.
- Still missing upstream: a goal/navigation RPC (planner and follower
  ported but dormant) and MCL boot relocalization wired into robotd.

The author's stated preference is to run mapping on the robot itself.

## Decision

### 1. Mapping and localization are robotd's job; quacksat consumes

quacksat does not implement SLAM, on board or off. It subscribes to
`robot.map` as the same unprivileged client it already is for
`robot.state` (padd pattern, ADR 0001): it never opens the ToF, the
camera or the odometry, and if it goes silent nothing about the map
changes. A robotd without `robot.map` (METHOD_NOT_FOUND) simply leaves
the feature off.

### 2. quacksat owns the layer above the map

What the map lacks is meaning. quacksat adds:

- a **places registry**: named poses in the map frame, taught by voice
  or by the agent, keyed by the map session so a wipe or a failed
  restore invalidates them instead of silently pointing elsewhere;
- **agent tools** on the existing protocol (ADR 0004): `where_am_i`,
  `list_places`, `remember_place`, `forget_place`; later `go_to` and
  `look_at`. Exposed through the bridge allowlist, the duck-side MCP
  server and the `direct` backend like every other robot tool;
- a **guided mapping tour**: stop-and-scan mapping needs someone to walk
  the duck with pauses; quacksat narrates progress from `windows` and
  `n_submaps`, it does not pretend the map builds itself.

### 3. Navigation waits for an upstream goal RPC

`go_to` needs the planner and follower that already live in the maploc
crate. Rather than duplicating a follower over `robot.move` intents in
quacksat, we follow upstream for a `robot.goto`-style RPC and, if none
appears by the time the hardware arrives (December 2026), propose it as
a PR on the Pollen repo, consistent with ADR 0001's "patch upstream via
PR when needed".

### 4. The off-board visual track is demoted, not deleted

Camera-based semantics (what is in the room, `where_is(object)`) remain
a later optional phase, only if places plus `where_am_i` prove
insufficient. When it comes it stays local: home video never leaves the
local server.

## Consequences

- The near-term work is Mac-testable without the duck: the IPC client,
  the places registry and the tools run against recorded `map.frame`
  sequences, and the MuJoCo twin path in PR 202 can build a real map on
  the Mac.
- We depend on an unmerged PR. The client pins the API version it was
  built against and expects a bump; nothing ships in a release until
  the upstream shape settles.
- Place labels are only as durable as the saved session until boot
  relocalization lands upstream. The registry design must make that
  visible to the user rather than answer with a stale place.
- CPU is shared: maploc is the heaviest thing robotd can run, and the
  wake word runs on the same four cores. The budget is measured in
  December, and `[maploc]` stays opt-in on the robot.
- The GPU server, the recording pipeline and the SLAM model evaluation
  from the previous plan are dropped from the roadmap. If phase 4 ever
  starts, it gets its own ADR.
