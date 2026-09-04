# TODO — House map and localization

Status: decided, not started (ADR 0005). Prerequisite: quacksat talks
(done) and Pollen's `maploc` lands in robotd (upstream PR 127, open).
Principle: **mapping and localization run on board, inside robotd**;
quacksat consumes the map over IPC as an unprivileged client (padd
pattern) and owns only the layer above it: named places, agent tools,
and later semantics.

Supersedes the 2026-08-31 version of this file, which assumed an
off-board visual SLAM on a GPU server. That track is demoted to an
optional last phase; see ADR 0005 for why.

## What upstream provides (or will)

- `maploc/` subcrate hosted by robotd (PR 127): 2D submap SLAM on the
  8×8 ToF plus contact odometry, loop closure, pose graph, MCL
  relocalization, A* planner and path follower (the last two ported but
  dormant). `[maploc]` in robotd.toml, off by default; session persisted
  and restored on boot; optional head sweep at every stop.
- IPC (API v17 in the PR): `robot.map` subscription → `map.frame`
  notifications at ~1 Hz with pose in the map frame, `tracking` flag,
  grid origin/pitch, trinary grid (unknown/free/wall) as base64,
  `n_submaps`, `n_loops`, `windows`, `still`, `seated`. `robot.map_wipe`
  resets the session.
- Already on main: `kinematics::tof::Reprojector` (floor filter,
  head-pose-aware beams) and `robot.look` gaze IK (PR 126).
- Validated on the MuJoCo twin (PR 202): tracked pose within ~6 cm of
  ground truth, raw odometry drifting up to 0.35 m.

## 0. Track upstream
- [ ] Watch PR 127 and PR 202 until merged; note the final API version
      and any change to `MapFrame`/`MapStreamResult`.
- [x] Detailed study of the maploc data flow (what enters robotd, how it
      is processed, what comes out) → `docs/study/maploc-dataflow.md` +
      `.mermaid` (2026-09-04).
- [x] Reproduce the offline bench on the Mac: `maploc` `evaluate`/`replay`
      examples on the committed `.mdlg` recordings (2026-09-04: builds in
      seconds, pure Rust; all four rows of PR 202's table reproduced to
      the millimetre; clean run 0.031 m mean wall error, 8 submaps,
      0 loops; mirrored run replayed with `MAPLOC_MIRROR_COLS=1` closes
      3 loops).
- [x] Try the MuJoCo twin path (`scripts/duck-sim` + `sim-maploc/`) on the
      Mac: the only pre-December way to see a map being built live
      (2026-09-04: works — real `robotd --sim` + `tofd --sim` + MuJoCo
      body, stop-and-scan route of 245 s, 59 windows, 7 submaps, tracked
      pose ~4 cm from truth at the return, 0.035 m mean wall error; the
      live `map.frame` stream was read from a plain socket client).

## 1. quacksat consumes the map
- [ ] `robot.map` subscription in quacksat-core (robotd client): decode
      `map.frame`, keep the latest frame, expose pose + tracking + grid.
      Gate on API version; a robotd without `robot.map` answers
      METHOD_NOT_FOUND and the feature stays silently off.
- [ ] `robotd --fake` support: check whether the fake serves `robot.map`;
      if not, a fixture that replays a recorded frame sequence.
- [ ] Detect "map frame changed" (wipe, restore failure, relocalization
      after a session reset) and invalidate anything anchored to it.

## 2. Places and `where_am_i` (no camera, no server)
- [ ] Places registry: named poses in the map frame, keyed by map session
      identity, stored under quacksat's own state dir. Taught by voice
      ("this is the kitchen") or by the agent.
- [ ] Agent tools: `where_am_i()` → nearest place + distance + tracking
      confidence; `list_places()`, `remember_place(name)`,
      `forget_place(name)`. Expose them in the bridge allowlist, the
      duck-side MCP server and the `direct` backend.
- [ ] Mapping walk as a guided behavior: the agent (or the user) drives a
      stop-and-scan tour; quacksat reports `windows`/`n_submaps` progress
      in words. Needs `[maploc] enabled = true` on the robot.
- [ ] Handle `seated`/`tracking = false` honestly in the answers ("I am
      not sure where I am, I need to stand and look around").

## 3. `go_to` (needs an upstream goal RPC)
- [ ] Follow upstream for a `robot.goto`-style RPC (planner + follower
      exist in the crate, not wired). If nothing appears by December,
      propose it as a PR on the Pollen repo with the duck in hand.
- [ ] `go_to(place)` tool on top of it: plan, follow, report arrival or
      failure; the ToF avoidance in M9 is upstream's job, not ours.
- [ ] `look_at` via the existing `robot.look`.

## 4. Later, optional: semantics from the camera (off-board)
- [ ] Only if places + `where_am_i` prove insufficient: frames from
      mediad (`get_frame` or WebRTC), a local server that labels what the
      duck sees and enriches the places registry ("kitchen: oven,
      fridge"). Home video never leaves the local server.
- [ ] Open-vocabulary scene graph and `where_is(object)` /
      `describe_surroundings()` stay in this phase.

## Risks and open questions
- PR 127 is unreviewed and conflicting with main: the IPC shape may
  still change. Build against a pinned API version, expect a bump.
- Boot relocalization is not wired in robotd yet: place labels survive
  only as long as the saved session does. The registry must be keyed by
  session and tolerate a reset.
- Stop-and-scan mapping is deliberate work: somebody has to walk the
  duck around with pauses. Design the guided tour, do not assume it.
- Cell pitch, map extent and CPU cost on the RK3566 are to be measured
  on hardware (December); maploc is "the most CPU-hungry thing the robot
  can do", and quacksat's wake word shares the same four cores.
- Multiple ducks: one map per robot for now; a shared map is upstream's
  problem if it ever comes.
- No license file on `microduck_maploc_rs`; the code inside the Pollen
  repo is Apache-2.0. We consume it over IPC, we do not vendor it.
