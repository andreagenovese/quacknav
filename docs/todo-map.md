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
- [x] `robot.map` subscription in quacksat-core (robotd client): decode
      `map.frame`, keep the latest frame, expose pose + tracking + grid.
      Gate on API version; a robotd without `robot.map` answers
      METHOD_NOT_FOUND and the feature stays silently off
      (2026-09-04: `quacksat-core/src/map.rs`, `[map]` config, wired in
      the binary, `map_watch` example; both paths checked live against
      the MuJoCo twin and a main-branch `robotd --fake`).
- [ ] `robotd --fake` support: check whether the fake serves `robot.map`;
      if not, a fixture that replays a recorded frame sequence.
- [x] Detect "map frame changed" (wipe, restore failure, relocalization
      after a session reset) and invalidate anything anchored to it
      (2026-09-04: `MapStatus::epoch`, bumped on a `seq` regression or
      the submap count falling to zero — conservative, since the wire
      carries no session id; a restart that restores a session also
      bumps it).

- [x] Operational independence from the voice satellite: the map lane,
      the registry and the four tools live in the `quack-places` crate
      (2026-09-04), with no dependency on quacksat-core — hostable by a
      daemon of its own or moved to its own repository unchanged. The
      transports (bridge, MCP server) stay in quacksat; extracting the MCP
      server into a shared crate is the one step a standalone daemon
      still needs. The systemd unit fences resources (Nice, CPUWeight,
      memory caps) so the satellite never costs robotd its loop.

## 2. Places and `where_am_i` (no camera, no server)
- [x] Places registry: named poses in the map frame, keyed by map session
      identity, stored under quacksat's own state dir. Taught by voice
      ("this is the kitchen") or by the agent (2026-09-04:
      `quacksat-core/src/places.rs`, JSON at `[map] places_path`, several
      anchors per name, a persisted generation that goes stale on a map
      reset — the map lane's epoch or a submap count below the registry's
      high-water mark).
- [x] Agent tools: `where_am_i()` → nearest place + distance + tracking
      confidence; `list_places()`, `remember_place(name)`,
      `forget_place(name)`. Expose them in the bridge allowlist, the
      duck-side MCP server and the `direct` backend (2026-09-04: in the
      one catalog every path serves — bridge via session.start, duck-side
      MCP, direct; tools act on a `tools::Robot` that owns the robotd
      lane, the map lane and the registry. Checked live over MCP against
      the MuJoCo twin: teach, recognize, walk away, wipe → stale,
      re-teach).
- [x] Mapping walk as a guided behavior: the agent (or the user) drives a
      stop-and-scan tour; quacksat reports `windows`/`n_submaps` progress
      in words. Needs `[maploc] enabled = true` on the robot (2026-09-04:
      `robot.map_status` in quack-places — numbers plus a hint — and
      `robot.map_step` in quacksat-core — a timed walk then a stand of
      6 s by default, reporting `new_windows`; the agent chains steps
      and narrates. The stand is inside the tool so stillness is
      guaranteed regardless of LLM latency; a step fits the bridge's
      30 s tool timeout. Checked live on the MuJoCo twin, which also
      taught two things: the walking policy does not step below about
      0.25 m/s commanded, so the move cap went from 0.2 to Pollen's own
      gamepad value of 0.3; and a blind step into a wall the map had not
      inked toppled the duck — now `map_status`/`map_step` report the
      clearance in four directions from the grid, and `map_step` refuses
      to walk into a mapped wall or into unmapped space right at the
      beak. The duck stood back up by itself and maploc relocalized.
      Then a two-room tour on the twin — corridor, kitchen and back, 26
      steps, one refusal, no falls, "ingresso" recognized at 26 cm on
      return — and the stairwell: the map stopped the duck 36 cm short
      only because the floor over the hole was *unknown*; once a wall
      beyond it is seen, the rays crossing the hole mark it free. The
      depth frames do see it (bottom row 43–47 cm on floor, 100–119 cm
      or nothing over the hole, 44 cm expected), so `quack-places` grew a
      **cliff guard** (`cliff.rs`): it reads tofd's stream and the head
      pose, reprojects with Pollen's `kinematics` crate, and calls a
      missing or 1.5×-long return where the floor should be a drop;
      `map_status` reports it and `map_step` refuses to walk toward it.
      Also: a 25 cm wall margin, and a wall within 20 cm on one side
      steers the step away. `robot.move` keeps no margin on purpose:
      approaching something to pick it up must get right next to it).
- [x] Handle `seated`/`tracking = false` honestly in the answers ("I am
      not sure where I am, I need to stand and look around") (2026-09-04:
      `where_am_i` answers `known: false` with the reason; teaching is
      refused. Caveat seen on the twin: before `robot.enable` robotd
      reports `seated = false` even on a seated duck — the flag comes from
      the controller, which exists only once enabled — so a fresh map's
      pose is "trusted" at boot; `robot.state`'s driving flag could gate
      it later).

- [x] Map everything (2026-09-04): `robot.map_explore` — frontier
      exploration in `quack-places/src/frontier.rs` (frontier groups,
      wall-inflated BFS paths, a waypoint per leg) driven by a background
      job in `quacksat-core/src/explore.rs` that walks with `map_step`
      (so every guard applies) on its own robotd lane, blocks frontiers
      it could not reach, backs off from drops, and stops when nothing
      reachable is left. When it reaches a nameless area it leaves a
      question; the `direct` backend asks it out loud (`ask_phrase`) and
      the answer flows into `remember_place`. The `agent` backend needs a
      protocol event for the same — still to do.
- [x] Explorer tuned on the twin (2026-09-04, runs 22–26): three layers,
      the way robot vacuums do it. The map plans (Dijkstra over a costmap
      with walls inflated 0.15 m, unknown floor dearer than known); the
      sensor answers only for what the map does not know (a hit becomes a
      local obstacle of 0.05 m plus inflation — 0.10 sealed a 0.4 m
      corridor beside a console); a stand re-maps the unknown. A leg is
      sized to the floor ahead: frontal margin 0.25 m plus 0.10 m of
      gait slack, and a *corridor test* — the body is 0.19 m wide (twin
      hull), with 0.06 m to spare per side a corridor must be 0.31 m —
      that turns toward the wider side instead of refusing. Frontiers
      are ranked by path cost per frontier cell (capped at 40 cells),
      not by distance alone: nearest-first spent 40 % of a run on the
      slivers around the start and reached four of six rooms in twelve
      minutes; the default budget is now thirty (run 28, 30 min: 39 m of
      true travel, 25 % of the flat's floor against 20 %, still four rooms
      of six — and maploc's pose drifted up to 1.9 m for five minutes with
      `tracking` still true, until a loop closure snapped it back). `map_step`
      itself now *shortens* a step to the floor in front of it (result
      `shortened`) and refuses only when less than a second of walking
      fits — the guided tour's fixed 3 s legs had started failing on the
      sensor's objects. Recoveries:
      nose against something by the map *or* the sensor → a bounded step
      back; six refusals with no leg between, or "nothing reachable from
      here" while frontiers remain → forget the local obstacles within
      0.6 m, set that frontier aside, back out, stand (three times, then
      give up honestly). The step back always uses a positive yaw: the
      twin's gait backs up only that way (measured twice; negative yaw
      leaves the body where it is). Twelve-minute runs from a clean boot:
      84–94 legs, 14–16 m of true travel, ~80 submaps, 24–58 loop
      closures, no falls, the stairwell edges recorded as drops. Still
      open: it circles for minutes on small frontiers behind low objects
      near the start; maploc's pose drifts up to 0.5 m in the north-east
      room (upstream); the "which room?" question needs its agent-protocol
      event.
- [x] Right-hand rule (2026-09-05, user's idea, on trial): walk straight
      and keep to the middle between mapped walls (`map_step` steers
      toward the centre line when both walls are within 1.2 m, dead band
      5 cm); when the way on is blocked, turn to the same hand every time
      (`[map] explore_turn`, right by default) unless the body has no room
      to swing there. "The wider side" changed its mind at every leg and
      oscillated in tight spots; one hand gets around an obstacle and
      along a wall to the next doorway. The frontier planner stays on top
      to choose where to go, to see doors on the other hand, islands, and
      the end. Caveat: on the twin right turns are the weak side (a right
      arc turns a third of a left one), so the hand is a config knob.
      Committed separately for an easy revert.
- [x] `[gait]` corrections (2026-09-05, user's idea): `yaw_trim` and
      `yaw_gain_left/right`, applied last to every walking command the
      satellite sends while going forward; defaults off (0, 1, 1). Why:
      measured on the twin, a straight 3 s leg veers about 20° right every
      time (six legs: -9° to -24°, one outlier), while the turning
      response is noisy but not one-sided (±0.7 turn alike; ±0.3 nearly).
      So what looked like "always steering right" is the gait itself
      veering when told to go straight; `yaw_trim = 0.2` on the twin. Zero
      on hardware until measured there. If the knobs prove brittle, the
      next step is self-calibration: the achieved yaw per commanded yaw,
      leg by leg, from the map pose.
- [x] Back and turn, and a lesson about blind manoeuvres (2026-09-05,
      user's observation): "back off and replan" closed a circle — the
      step back swings the tail one way, the gait veers the other on the
      way back to the same target — so the step back is now followed by a
      quarter turn to the configured hand, then a full mapping stand. The
      stand matters: run 31, with back-and-turn and only a second's stop,
      lost the map in two minutes (a wall inked 40 cm off, the pose in
      unknown, a false "no frontier left"); runs with few spins kept the
      pose within 0.5 m. maploc maps at stands and trusts odometry in
      between, and a bipedal gait's odometry is worst in tight arcs. Also:
      "done" now needs the map to have no frontier cells at all — else it
      is "sealed in" and, from the second attempt, the planner squeezes
      with the body's own half-width (0.10 m) instead of the 0.15 m
      margin, which is how the duck gets out of the bedroom pocket between
      bed, nightstand and wardrobe it kept ending run 29 and 30 in.
- [x] Gait calibration from a human drive (2026-09-05, user's idea): the
      user drove the twin with the arrow keys for 21 minutes (a curses
      teleop that records command and true pose at 10 Hz, six spots
      named), 85 m, no falls, all six zones, 57 % of the floor mapped in
      one go against the explorer's best 25 %. The gait, measured on 178 s
      of straight walking: 0.114 m/s and a right veer of 2.9 ± 2.5 °/s —
      real but a third of what scripted 3 s legs from a standstill showed;
      turns ±0.7 with vx 0.3 give 25.5 and 26.5 °/s, so no side asymmetry
      (the factory calibration holds; `yaw_gain` stays 1/1, `yaw_trim`
      0.08 not 0.2); turning in place (vx 0, vyaw 0.7) does work at about
      17 °/s, noisily; backing straight works at 0.08 m/s when the gait
      is already stepping, where from a standstill it needs a yaw. The
      driver kept a median 0.38 m from the nearest obstacle while
      advancing, 10th percentile 0.19 m — our 0.25 m frontal margin and
      0.15 m inflation are in the human's range. Data under
      `private/drives/`.

- [ ] Map memory and relocalization on our side (2026-09-05, user's ask):
      even before Pollen wires boot relocalization, the duck should not
      lose its map and its place names at every power cycle. To study:
      what `robot.map` exposes that could be saved (the grid and pose are
      published; the submap graph is not), whether robotd's maploc can be
      handed a saved session (PR 127's `wipe_on_boot` suggests a session
      file exists), and failing that a quacksat-side fallback — keep the
      last grid, match the fresh map against it (2D scan-to-map or
      grid-to-grid alignment) once a few submaps exist, and re-anchor the
      places registry to the new frame. Talk to upstream first.
- [ ] Iteration after the human drive (2026-09-05, on trial in run 39):
      (1) the frontier is the target, the *standing point* is 0.5 m
      short of it along the path (`Frontier::stand`) — a frontier sits by
      definition against walls and furniture, walking onto it put the
      beak on them every time; a stand short of it maps it as well.
      (2) Back off, then straight on: the yaw a step back needs is already
      a 40° correction (measured); the quarter turn after it faced the
      side wall and the next leg curved back — gone. (3) Centring in
      `map_step` is relative to the passage width: full correction against
      one wall of a 0.4 m corridor, where the old gain gave a tenth of it.
      Only on the explorer's own legs (`centre: true`): on top of an
      outside driver's steering it read as a stranger's hand on the wheel
      and broke the guided tour's return leg twice (5 and 6 of 9).
      Also: frontier cells within 0.3 m of a sensor-seen obstacle are not
      frontiers (the map's un-inked wall gaps made false frontiers along
      the east wall), groups need 8 cells, arrival is 0.3 m from the
      standing point. (4) The sensor's obstacle test is a *lane* the
      body's width (±0.16 m of the line the duck would walk), not a ±23°
      cone: from half a metre the cone held the posts of a 0.4 m doorway
      and the duck never tried a narrow passage (user's observation).
      (5) The panorama (user's idea): the sensor sees the front hemisphere
      at a stand, so at the start and on arriving where more than half of
      the floor within 1.5 m is unknown the duck turns in place in four
      80° steps, closed on the map's own yaw, standing at each — the
      whole circle seen before choosing; never twice within a metre.
      Tuned on runs 43–47: the gait does not turn in place from a
      standstill at all, so each step is a one-second walking kick then
      yaw only (~30°/s, 15 cm of drift); closing the step on the 1 Hz map
      yaw overshot 30°, on the state stream's odometry yaw with a 20°
      early stop the 45° steps come out at 48–54° (eight steps, 401°);
      stands of 8 s, six left sectors half-swept. Measured by 30° sector
      after the panorama: the inner ring (to 0.8 m) known 74–100 % in
      eleven sectors of twelve, the twelfth being the stairwell; the outer
      gaps sit behind the wall stubs. Cost: 1.5 min per panorama, a third
      less map in ten minutes when taken at every unknown spot (3040 vs
      4440 cells), refusals down from 32 to 8; so it is taken only at the
      start and where more than half the floor ahead within 1.5 m is
      unknown, never twice within 2.5 m (user's choice).
      (6) Straight when clear (user's rule): the leg aims at the standing
      point itself whenever the straight line to it, up to 2 m, has no
      mapped wall within the body's lane, and follows the grid path — a
      zigzag by nature, whose 0.4 m look-ahead gave every leg a small
      steer, and every small steer summed to a duck turning on the spot —
      only when something is in the way. Heading corrections start at 15°
      (dead band 0.25 rad, gain 0.6, at most 0.2 rad/s).

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
- **Live maploc drifts where its own replay does not (2026-09-05).** On the
  twin, with a 94-submap map inherited from a human tour, the live
  `robot.map` pose jumped by up to 4.6 m and `tracking` flipped, while
  `maploc/examples/evaluate` replaying the same `.mdlg` recording tracked
  the whole session within 4 cm median, 0.29 m max in that very window,
  never lost. CPU load was ruled out (release build, paced mic, idle Mac).
  So robotd's live pipeline — frame timing, the still gate, the search
  sweep, or dropped frames — differs from the bench. Recording
  `microduck-pr202/recordings/1788604159.mdlg` (79 min) and the replay log
  (`private/drives/replay-1788604159.txt`) are the evidence to hand
  upstream. Until it is understood, exploring on a large inherited map is
  unreliable on the twin; from-scratch runs (small maps) stayed within
  0.5 m.
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
