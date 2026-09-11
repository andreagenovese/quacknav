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
- [x] Audit of the pose losses on the twin → `docs/study/maploc-audit.md`
      (2026-09-06): the twin's odometry is near truth, maploc tracks by
      dead reckoning and its loop closures walk the pose off at map-noise
      level until the watchdog calls a kidnap; a bench matrix on five
      recordings and a scan-to-map tracking correction (opt-in) on the
      worktree branch `maploc-study`. Next: rebuild robotd with the tight
      closure allowance, rerun both regressions, report to Pollen.
- [x] Rebuilt robotd from `maploc-study` with the tight closure allowance
      (0.03 m per submap, cap 0.30) and reran both regressions (2026-09-06,
      run 57): 30 min, 37 m, zero tracking lost, zero falls, pose error
      against truth median 0.12 m / max 0.28 (run 49 on upstream: 0.3–0.5 m
      from minute 5, lost at 15); tour 7/9, zero lost, "ingresso" recognized
      at 0.29 m. Spot manoeuvres still mint same-place submaps (86 submaps,
      189 closures in the 12-minute tour, pose median 0.23 m): next, tie a
      closure's plausibility to the distance actually travelled between the
      two submaps, not to the submap index gap.
- [x] Explorer: the three defects behind the unexplored south (2026-09-06,
      diagnosed on the paper twin with the explorer's own decisions logged
      by target room): (1) "straight when clear" only checked mapped walls,
      so it aimed across the stairwell and the cliff guard refused; now the
      straight lane also keeps clear of every drop and local obstacle on the
      books. (2) With the target behind and no room for an arc (the east
      doorway against the cabinet: 84 refusals in one spot), the explorer
      had no in-place turn; now a kick-then-spin closed on the yaw, three
      per spot before it counts as a refusal. (3) The cliff lane (±0.30 m)
      fitted neither passage beside the stairwell (0.44 and 0.54 m); now
      ±0.22, and a recorded drop occupies 0.10 m in the costmap instead of
      0.20. Also found on the way: an accepted leg that moves the duck
      nothing (flank against something under the sensor's minimum range)
      is now a refusal, not a leg repeated 270 times; a drop on the books
      is never forgotten by the unseal recovery; no blind step back with a
      drop beside or behind; the step guard judges an arc along its
      starting heading too (the fall of run 58: an arc whose end heading
      the sensor never swept turned into the stairwell); and every leg,
      including the head-for-space and spin kicks, is played through the
      gait model against the drops on the books first (margin 0.15 m).
      Paper twin, 30 seeds: refusals median 184 → 41, coverage 29.6 →
      32.7 %, kitchen 39 → 60 %, bumps 27 → 20, bath reached in 2 runs and
      corridor S in 4 (never before), zero falls. MuJoCo run 59: coverage
      33 % (run 57: 26 %), 2 refusals in 28 min (87), zero falls, one lost
      recovered in 6 s; tour 6/9 with zero lost and zero falls, the three
      misses at the kitchen door on the way back (the tour script's
      straight-line steering, not the explorer). Still closed: the south
      behind the stairwell on MuJoCo.
- [x] The passage beside the stairwell, second pass (2026-09-06 afternoon):
      the paper twin showed a phantom drop recorded 17 cm outside the hole
      (the refusal handler recorded the *nearest* drop of any bearing, a
      stale sighting from a stand, at the current pose) — now the drop in
      the lane ahead is recorded; the blunt "no blind step back with a drop
      beside" rule left a twin standing 25 minutes in the west passage
      (wall ahead, hole beside, no escape) — now the step back is played
      through the gait model against the drops on the books, like a leg;
      and run 59's "frontiers remain but none reachable" was two sensor
      points at the east door (doorpost, cabinet corner) sealing a 0.42 m
      gap from 2.4 m away — obstacles recorded farther than 1 m may now be
      forgotten when they, not the map, seal the rest of the flat (three
      times per run). Blocking frontier cells near drops was tried and
      dropped: it sent the duck south first and cost the kitchen. Paper
      twin, 30 seeds: same coverage and refusals as before (33 %, 42),
      corridor S over half in 10 runs (5), bath in 3 (2), zero falls.
      MuJoCo run 60: 27 % in 30 min, 10 refusals, zero falls, zero lost,
      pose median 0.10 m; the one leg toward the west passage was refused
      by the drop-path guard (a curved leg bending toward the recorded
      edge) and the south stayed closed. Restore point of the morning's
      state in `private/drives/savepoints/explorer-ok-2026-09-06/`.
- [x] Passage beside a drop (2026-09-06 afternoon, user's go): with a drop on
      the books within 1 m and the planned path running through a gap of
      0.43–0.9 m between a mapped wall and the drops, the explorer turns in
      place onto the passage's axis (the path's direction 0.4–1.0 m ahead,
      kept while drops stay near), then takes 1.5 s straight legs with a
      gentle centring between wall and drops (`steer: false` on the step,
      so the guard's wall-hug does not peel the leg into the hole — it
      did). For the planner a drop is worth 0.17 m of radius (plus the
      costmap's 0.15), so the 0.44 m passage east of the stairwell is not
      routed through any more; the guard's drop-path margin is 0.10 m plus
      the drop's 0.10 (the user's 0.20). The spin counts a right turn as a
      left turn by the rest of the circle (the gait spins left whatever
      the sign): closed on the yaw the other way it stopped facing the
      wrong wall with the hole behind. Tried and dropped: a narrower cliff
      lane (0.18: the twin bumps everywhere), an axis search by clearance
      (a fall), blocking frontiers near drops. Paper twin, 30 seeds:
      median 33 % as before, third quartile 41 (34), refusals 48 (43),
      bumps 15 (24), corridor S over half in 21 runs (10), bath in 8 (4),
      zero falls, but 7 runs under 25 % (3) — traced to the path-derived
      axis bending toward the hole in the passage, and to a stale drop
      sighting the twin's always-fresh frames produce (a twin artefact).
      MuJoCo run 63: 42 % in 30 min (best ever; 62: 34, 59: 33), bath 68 %,
      corridor S 94 %, living room 53 %, zero falls, zero lost, pose
      median 0.08 m.
- [x] Evening (2026-09-06): closed-loop alignment for passage entry after a
      turn-in-place probe on the twin (timed commands vary threefold, right
      turns work, coast 5–10°); every drop the sensor saw goes on the books
      after a stand (from frames ≤ 2 s old); drops are worth 0.12 m to the
      planner. maploc (`maploc-study`): a uniqueness test on the relocalize
      search (runner-up basin), a two-search agreement gate, and a give-up
      after 8 windows lost that resumes on odometry — the bench replays of
      runs 64 and 65 no longer relocalize 3 m away. Paper twin: the sensor
      model now ages stand frames and adds a fresh centred frame (the
      always-fresh frames produced phantom drops). New metric: 90-minute
      budget, complete = every room at ≥ 80 % of its ceiling, completion
      time, zero falls → 20/30 complete, coverage at the ceiling (55 %),
      corridor S 30/30, bath 24/30, zero falls; but completion time is the
      budget: slivers keep the duck wandering (147 m). Next: defer small
      frontiers and finish when only slivers remain; the 6 seeds that never
      go south and the 4 sealed north; `robot.go_to` on the finished map.
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

**How we measure (adopted 2026-09-08).** A single MuJoCo run cannot tell
ten points of coverage from noise: across runs 70–77 the same explorer
scored between 31 % and 53 %. So coverage, refusals, walking distance and
room dwell are decided on the paper twin with **ninety seeds per
condition, paired seed by seed** (the twin is deterministic per seed, so
the same seed under both conditions is the same house and the same luck);
a difference counts when the seeds that improve clearly outnumber those
that worsen, not when a median moves. MuJoCo is kept for what the paper
twin cannot model — falls, lost poses, doorposts, the depth sensor's own
mistakes — and for safety, where a single fall is a result. Both twins
report zero falls before anything is called an improvement.

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
      (7) Head for space (user's rule, run 49): after a panorama and
      after a step back the duck turns toward the heading with the
      longest run of known free floor on the map (24 samples, at least
      0.6 m) and takes one guarded straight leg there before the planner
      has its say — where a step back's 40° swing left the beak was
      chance.
      (8) Map-versus-sensor agreement (runs 50–51): at every stand the
      sensor's obstacles within 1.5 m, in directions where the map has a
      wall, either sit on it (within 0.35 m: agree) or lie beyond it
      (disagree) — seeing through a mapped wall is the one thing a true
      pose cannot do. Floor the map shows beyond an obstacle is not
      evidence: a map under construction misses every low piece of
      furniture, and counting it (run 50) gave 40 false alarms and 15
      panoramas in half an hour. Ten or more beyond, and three times those
      on the wall (three-and-half gave nine false alarms in run 51, the
      pose being right), make a doubtful stand; two in a row earn a panorama so the
      mapper can close a loop, six in a row end the job as "position
      lost" instead of mapping on a false pose (run 49 spent fifteen
      minutes 3–5 m off, "tracked").
      (9) Finish the room first (run 52): beyond 2.5 m a frontier's score
      grows with its distance, so a sliver at hand beats a wide opening
      two rooms away — the criss-crossing of the flat seen in every map
      picture so far. explore_lite, the ROS standard, ranks by distance
      minus size and blacklists a frontier after 30 s without progress;
      ours is cost per cell with this locality factor and the refused
      list. (10) Doorway mode (run 52): a passage between the body's
      width and 0.6 m is a doorway — the leg steers onto its axis, takes
      1.5 s steps and asks `map_step` for doorway margins (`gap`: frontal
      0.15 m, slack 0.05, lane ±0.115 m; the leg's own room test uses the
      same lane and a 0.20 m reserve — with the corridor's it never even
      proposed a step through, run 53). The 0.42 m door to the study
      never let the duck through with the corridor's margins (a scripted
      drive: 17 steps back, never past the threshold), where a person drove
      it through at once. The doorway is also recognised by the sensor
      (something on both sides within 0.6 m of each other, ahead and
      near): a low cabinet the map has not inked is a doorpost all the
      same. Run 55: the duck went through the 0.42 m door on its own for
      the first time, 28 % of the flat in thirty minutes (record), 122
      legs, 79 refusals, kitchen 49 %, bedroom 37 %, bathroom touched.
      (11) The cliff guard judges a drop in a lane too (±0.30 m of the
      line walked; an arc along the heading it ends on), not the front
      half: the stairwell beside the path made the 0.54 m passage between
      it and the wall impassable, and a drop edge on the books had a
      0.45 m radius — 0.6 m with inflation — sealing the same passage on
      the map; now 0.20 m (user's observation, run 56).
- [x] The paper twin (2026-09-06, user's idea): `quacksat-core/examples/
      paper_twin.rs` runs the real explorer (`explore.rs`, `frontier.rs`,
      `tools::plan_step` — the guards of `map_step`, now a pure function)
      against a kinematic model of the duck in the apartment's boxes
      (`apartment.world.json`, from Pollen's sim scene): the gait as
      measured (0.114 m/s, 0.65 rad/s per unit of yaw, right veer, no
      turning in place from a standstill, backing only with +yaw), the
      sensor as rays with the head sweep, the map grown at stands, the
      pose as truth plus a random walk the stands pull back, an optional
      kidnap. The explorer reaches it through the `Body` trait (the real
      `Robot` implements it too), with a virtual clock. Thirty runs of
      thirty simulated minutes take thirteen seconds: median coverage
      29.6 % (18–36), 114 legs, 184 refusals, 47 m — the MuJoCo twin's
      band (26–28 %, ~110 legs, 80–170 refusals, 30–35 m). Output per run:
      a watch-format log and a `map.frame`, so `mapshot.py` draws it and
      `mosaic.py` tiles the runs; the user wants to *see* the thousands of
      simulations. Select here, confirm on MuJoCo, validate on hardware.
- [x] The obstacle ladder (2026-09-07, user's idea): the same flat as
      bare walls, plus the stairwell, plus the big furniture, then full —
      thirty seeds each at a ninety-minute budget, to see what each class
      of obstacle costs. Walls alone: 70 % (the ceiling), 17 min, one
      refusal — but twelve of thirty walked to the budget on a finished
      map, chasing slivers. The stairwell alone: 120 refusals, all at its
      edge. Two fixes measured on the ladder: (1) big frontiers first
      (≥ 20 cells, wherever they are) and a finish criterion — no group
      of that size left anywhere on the map, reachable or not, the ring
      around a drop not counted, and twelve rounds without thirty new
      free cells end the job; (2) the escape from the stairwell's corner:
      a step back is tried at full, half and 0.8 s (the shortest with half
      the drop margin) before it is given up, since a full one swung the
      simulated path onto the corner and, with none taken, the duck stood
      refusing the same leg to the budget (131 refusals, seed 3). After:
      walls 30/30 finished, 16.5 min median; walls + stairwell 30/30, 17
      min, 8 refusals (from 89), 70 %; + furniture 27/30, 42 min; full
      flat 22/30 complete (from 20), 76 min median, 100 refusals (from
      134), coverage at the ceiling, no falls on 120 runs. Left: the
      furniture's cost (min coverage 28 % on one seed at level 3), the
      eight incomplete seeds of the full flat (kitchen or west room).
- [x] Run 70 and what it taught (2026-09-07 afternoon): the MuJoCo gate
      on the ladder build, from a clean boot — 73 minutes, 34 %, pose
      within 5–25 cm throughout, no loss, no fall, but a quarter of an
      hour "sealed in by local obstacles" in the bedroom and the south
      never tried. Two causes, found by replanning offline on the run's
      own map (`quack-places/examples/replan.rs`: a saved frame, a pose,
      the books, a trail): (1) five phantom drops on the bed (the ToF's
      floor rows read the low box as a drop; the mechanism in the
      simulator is not pinned down) plus the margins sealed the door the
      duck had walked in through; (2) the twenty-five drops around the
      stairwell each killed frontier cells within 0.42 m and erased the
      entrance of the 0.54 m passage: the south's 112-cell frontier was
      gone with the books, there without them. Fixes: **the trail** — the
      body's own path, a point every 5 cm, is a lane the planner may
      always use, whatever the inflation and the books say (the body was
      there, at the body's width); a drop kills frontiers within 0.08 m
      of its radius, an obstacle still within 0.30. The paper twin now
      puts phantom drops on low furniture (`low` in the world file, a
      tenth of the floor beams that land on the box within half a metre
      past its face). Full flat with phantoms, thirty seeds: 17/30
      complete without the trail, 21/30 with it, 23/30 with the drop
      kill too (from 22 without phantoms); min coverage 26 → 36 %; walls
      and stairwell levels unchanged; no falls in 240 runs. Offline, the
      run-70 map now shows the south frontier reachable with the books.
- [x] Room stays, measured (2026-09-07 evening, the user's eye): from
      the truth trails the longest stay in one room was 5–11 minutes on
      runs 59–67 and 15–32 from run 69 on. The paper twin now reports it
      (`stay_max`, `stays5`, `end_s` in the summary; `private/drives/
      dwell.py` for the MuJoCo logs, shown at every snapshot). The
      suspect — the refused list cleared again after every walked leg —
      was measured three ways on the full flat with phantoms: after every
      leg 23/30 complete, 76 min, 168 refusals; once per job 18/30, 61
      min, 114; once the body has moved a metre from the last clearing
      21/30, 66 min, 126. The median stay is 12 minutes in all three: on
      the paper twin the re-arm is not what keeps the duck in a room —
      furniture is (walls + stairwell 5 min, + big furniture 10, full
      flat 12). Default now: clear again after a metre (`REARM_DIST_M`;
      `QUACKSAT_REFUSED_REARM` 0/1 for measuring). MuJoCo's doorway
      stalls ("no room ahead" at the posts) remain the open cost.
- [ ] Doorway exits (2026-09-07 night, open). The trail now outlives a
      job like the drops do (the second segment of a run inherits it).
      The obvious fix for the doorway spins — turn toward the aim instead
      of the configured hand when there is no room — lost on the paper
      twin (full flat 21/30 → 16/30 complete, refusals 126 → 150; walls +
      stairwell refusals 9 → 31, a 26-minute stay) and stays behind
      `QUACKSAT_TURN_AIM=1`. A cheap MuJoCo probe (`private/drives/
      exit_test.py`: drive into the NE bedroom from a clean boot, explore,
      time the exit) does not reproduce run 71 either: on a fresh map the
      duck leaves in 30–100 s, because the frontier is just outside the
      door; the run-71 case is a mapped north half, a far target and
      fifty entries on the books after fifteen minutes. So the doorway
      cost is measured only by the dwell metric on full runs. Next idea
      to test there: when spins alternate sign with no leg between, take
      the planned path's first cells as the heading (they are free by the
      map) and allow a shorter leg (0.6 s) through a sensed gap.
- [x] The trail as evidence for the guards, the arc's true advance, the
      turn in place in tight quarters (2026-09-07 late night, all measured
      on the full flat with phantoms, thirty seeds, ninety minutes; base
      55 %, 21/30 complete, 126 refusals, 43 bumps, longest stay 41 min):
      (1) **a leg whose first 0.30 m lies on the trail is judged with the
      doorway's reserve** (0.20 m, not the corridor's 0.35) — the body was
      there at its own width; the cliff guard and the sensor's obstacle in
      the narrow lane still have their say. 26/30 complete, 104 refusals,
      34 bumps, longest stay 31 min; kept, on by default. (2) The arc's
      advance, measured on the human drive: 0.110 m/s forward at vyaw 0.7
      against 0.121 straight — not the quarter the explorer and the
      mapping-step guard assumed, which is why the duck ended against
      walls when turning (the user's observation). Judging arcs with the
      true advance is right and loses badly on the paper twin, where a
      bump costs nothing: explorer 55 → 38 %, 10/30; guard 55 → 40 %.
      Both behind switches (`QUACKSAT_ARC_FULL`, `QUACKSAT_GUARD_ARC_FULL`),
      off, a debt to settle on MuJoCo where a bump has a price. (3) The
      user's rule — in tight quarters a heading change beyond 35° is a
      turn in place (kick, then spin), not an arc: halves the longest
      stays (31 → 18 min, doorways only) but costs completes (26 → 20) and
      refusals (104 → 182); tighter triggers cost more (14/30). Behind
      `QUACKSAT_SPIN_TIGHT=1` (doorways only), off. **Measured on MuJoCo
      overnight (runs 73–76b, 60 min each from a clean boot):** control
      53 % (bath 81 % at minute 49, the first time on MuJoCo; every room
      at 80 % of its ceiling by then; a pose loss at minute 58, resumed
      unverified 0.8 m off), 42 refusals, 69 spins, 0 stalls; explorer arc
      at the true advance 34 %, 114 spins, 21 stalls, south never tried;
      guard at the true advance 34 %, 77 refusals, 4 pose losses; turn in
      place in doorways 31 %, 140 spins, 19 stalls, a 28-minute stay in
      the bedroom. No falls in any. The paper twin's verdict holds on
      MuJoCo: all three stay off. Run 76's first attempt ended after five
      minutes in the stairwell passage — three "sealed in" attempts within
      thirty seconds reach STUCK_MAX and end the job: the finish was too
      hasty there. Fixed on 2026-09-08: a "sealed in" attempt counts only
      thirty seconds after the previous one or once the body has moved
      0.20 m since (`STUCK_GAP_S`, `STUCK_MOVE_M`); paper twin neutral
      (walls + stairwell identical, full flat 24/30 vs 26/30, noise; no
      falls). **Run 73's pose loss on the bench** (recording 1788809590,
      `private/drives/runs/73-control/bench/replay.txt`): the replay
      never loses the pose — true error median 5–36 cm throughout and
      15–17 cm in the last ten minutes, where live was at 51–89 cm and
      declared the loss. Replay and live agree for forty minutes (1–16
      cm) and diverge after: the loss is live-only, the known
      live-versus-replay discrepancy, now with a clean case. Measured
      ingredients: the raw odometry of an exploring duck drifts 0.4–1.2
      m (spins and back-offs; the human drive drifted 13 cm), and loop
      closures in the living room move the pose 10–24 cm each in bursts.
      robotd's live log was not archived for run 73 (overwritten by the
      later runs); `mujoco_run.sh` now keeps it with every run. **Panorama and the bath door** (2026-09-08): the two-minute stop the
      user saw in run 73 was a panorama (eight stands of 8 s with a turn
      between) triggered by the map-versus-sensor guard in the living
      room; after it the target changed because the map had changed and
      the old frontier was gone — `pick()` keeps a target only while it
      is alive, which is right. Six-second stands were tried and put
      back: an earlier sector measurement found them half-swept, and the
      paper twin cannot see the difference (its stand scan is instant).
      The bath has no door to the office: wall wG runs unbroken from x
      0.5 to 4.0 at y −1, so the bath's only exit is its gap at x 0.5 (y
      −2.5..−1.7) and the office's only entrance is the hall's gap (y
      −0.2..0.6) — every route between them passes the stairwell, west
      side (0.54 m) or the east strip between the hole and wall wC (0.44
      m). Run 73's "long way round" was the only way.
- [x] The passage, three ways at once, and a step back with a side
      (2026-09-08, the user's rules: "if it does not centre within those
      5 cm it will never pass", "hug the wall — a bump is a bump, the hole
      is not", "back up the other way too"). (1) The sides of the passage
      are refined by what the sensor sees beside the body — the wall on
      one side, the drop edge on the other — instead of the map's wall
      and the drops on the books, both of which move with the pose error.
      (2) With a drop on one side the line held is a body's half-width
      and a little from the wall (`HUG_M` 0.16), not the middle: the drop
      edge goes from 5 to ~18 cm outside the guard's lane. (3) A passage
      leg carries `passage`, and the mapping step's cliff guard, when it
      sees the wall at the body's side, judges a 0.17 m lane instead of
      0.22 — the flag alone changes nothing. The passage's minimum width
      rose to 0.50 m: at 0.43 the 0.44 m strip east of the stairwell
      trapped two runs in thirty once the body hugged the wall. (4) A
      step back has a side. Measured on the twin (`backprobe.py`): from a
      standstill only a positive yaw moves the body, but half a second of
      it gets the gait stepping and then −0.7 backs 0.23 m turning −87°,
      and a zero yaw backs straight (−13°); the caller prefers a side
      (tail away from a drop, the mirror of the arc that met the wall —
      which retraces the way in), both phases are judged against the
      drops, and among the clear sides the one whose path lies on the
      trail wins; the paper twin models the same gait. Paper twin, thirty
      seeds, ninety minutes, no fall in ninety runs: walls + stairwell
      17.7 → 16.6 min, refusals 10 → 6; big furniture 51 → 37 min,
      refusals 50 → 42, path 95 → 68 m; full flat 64 → 51 min, longest
      stay 13 → 10 min, 24/30 complete (26 before, noise). On MuJoCo the
      first passage test from a spawn at the south mouth (`MICRODUCK_START`
      added to the twin's body server, `passage_test.py`) timed out twice
      without trying the passage: on an empty map the explorer preferred
      the bath and the living room. Redesigned: spawn inside the mouth. **Measured from the slot's mouth** (spawn (−0.70, −1.10) facing
      north, where the west wall starts and the only near frontier is the
      hall; six attempts per condition, eight minutes each): baseline 3
      passed, 2 fell, 1 timed out, crossing 211 s; sensor centring + wall
      hug 5 passed, 1 fell, 0 timed out, 202 s. Every fall has the same
      signature — a leg refused for the drop, then a blind three-second
      step back off the trail, and the body in the hole seconds later:
      the primitive is not what falls, the blind step back beside the
      hole is. First cut of the rule ("near a drop, back up only over the
      trail") deadlocked at the mouth: 5 of 6 attempts spent the budget
      refusing, 112 "a drop lies where a step back would go" in one run,
      because the duck had no trail yet and every heading held a drop.
      Refined and being measured: off the trail near a drop, only the
      0.8 s step back, and only when the drop is AHEAD (a drop beside the
      body is the case that fell). Geometry to remember: the west wall of
      the stairwell starts at y = −1.0 while the hole runs to y = −1.4, so
      the "passage" is a 30 cm slot at the hole's north end; south of it
      the west side opens into the living room, which is why a duck
      spawned at y = −1.6 always chose the living room instead. **What the falls actually were, and the fix** (2026-09-08): with the
      short step back the duck walked again (15 legs against 0) but still
      fell — during the passage's own *alignment*, not during a leg. So
      the mechanism is every blind manoeuvre beside the hole, the turn in
      place included: it runs in quarter-second chunks without looking and
      drifts about 15 cm, which beside the stairwell is the whole margin.
      Two changes: a turn in place now reads the depth sensor between
      chunks and stops where it stands if an edge is within 0.30 m of the
      beak (on by default — it can only end a blind manoeuvre earlier);
      and near a drop, off the trail, the step back is the 0.8 s one and
      only with the drop AHEAD. Five attempts with the whole package: 5
      passed, 0 fell, 0 timed out (against 3/2/1 for the baseline and
      5/1/0 for sensor + hug). Read honestly: the spin watch never fired
      in those five, and four of the five crossings walked ZERO legs — the
      duck spawns in the middle of the slot and 0.55 m of drift from the
      panorama and one short step back is enough to "pass", so what this
      measures is surviving the first two minutes beside the hole, not
      walking the slot. The short step back is the safety win (six short
      backs, no fall, against three falls in twelve with the three-second
      one). Walking the slot is still untested: next spawn is y = −1.45,
      south of the hole, which forces 0.9 m through it. **Harder spawn, and why the package stays off** (2026-09-08): from
      y = −1.45, 0.9 m of slot to walk, four attempts each: the package
      crossed once in 251 s with six walked legs — the day's only genuine
      crossing — the baseline never crossed in four; no fall either way,
      because from south the west side is open and the duck usually goes
      to the living room instead. Safety over the whole slot work: no
      fall in nine attempts with the package against two in ten without.
      But the paper ladder says the package costs coverage, and the pieces
      compound (full flat, thirty seeds: baseline 24–26/30 complete, 119 m
      walked; sensor + hug 18/30, 97 m; trail-only backing 21/30, 78 m;
      all three 16/30, 55 m — the duck stops walking and ends early and
      incomplete). So all four stay OFF by default, and the day's keepers
      are the two that cost nothing: the sensor-watched turn in place and
      the short step back beside a drop. **The likely reason, and the next
      idea**: on the paper twin every low box wears phantom drops, so
      "near a drop" is the normal state and the rule silences the recovery
      everywhere; in the real flat the only drop is the stairwell. A hole
      and a furniture edge can be told apart — a box edge shows a drop AND
      an obstacle at the same bearing, a hole shows a drop with nothing
      behind it. Worth measuring: it would let the strict rules apply only
      beside true holes. **Built and measured the same day**: `record_drops` now asks, for
      every sensed drop, whether an obstacle stands at the same bearing
      (within 0.12 rad) and about the same distance (0.35 m) — if so it is
      that obstacle's edge and goes on the books as an obstacle, not a
      hole. The depth guard's refusals are untouched: safety never rests
      on this judgement. Paper twin, thirty seeds: on the full flat it
      calls 1282 drops edges and 392 holes (three quarters of them were
      furniture) and coverage, refusals and metres walked are unchanged
      (55.2 %, 100, 120 m; 21/30 complete against the baseline's 24–26,
      inside the spread we have seen); on walls + stairwell it calls ZERO
      edges — the true hole is never mistaken — and that level is
      identical to the baseline. With the strict package on top, the
      full flat recovers part of what the package costs (16 → 19/30
      complete) but not all of it, so the package stays off and the
      discriminator goes on by default. Still to confirm on MuJoCo, where
      it should also unseal the bedroom door that five phantom drops on
      the bed closed for a quarter of an hour in run 70: that needs a full
      sixty-minute run. **Run 77 and ninety paired seeds** (2026-09-08): on MuJoCo the
      mechanism is fixed — the books end the run with 6 holes and 90
      obstacles (run 70's were nearly all drops) and nothing at all beside
      the bed, so the phantom drops seal no doorway now; no fall, no lost
      pose. Coverage was 37 %, inside the 31–53 % band single runs wander
      in, so it settles nothing. The paper twin settles it: ninety seeds
      per condition, paired seed by seed, coverage better on 30, worse on
      27, unchanged on 33, mean difference +0.6 points against a spread of
      9.6 — indistinguishable from zero; complete 58/90 against 59/90;
      finish time and refusals unchanged; no fall either way. So the
      discriminator earns its place for what it fixes (a box edge no
      longer closes a door, and the strict rules can be reserved for true
      holes), not for coverage. 
- [x] The automatic search on the leg's knobs (2026-09-08, first step of
      the learned-leg track): twelve constants of the leg planner — the
      lane, the reserves ahead for a straight leg, an arc and a doorway,
      the doorway lane and width, the three heading thresholds, the aim
      point, the straight look and the doorway leg's length — are now
      readable from the environment (`QK_*`, each defaulting to the value
      measured before, so nothing changes unless a search sets it), and
      `private/drives/legsearch.py` samples them at random, sixty trials
      of thirty seeds, scoring how many seeds finish the whole house with
      a fall anywhere disqualifying the trial. The best trial beat the
      defaults on three fresh paired batches (seeds 91–180, 271–360,
      361–450): 72 seeds gained against 42 lost over 270 pairs, p ≈ 0.005,
      complete 175/270 → 205/270, no fall in 540 runs. **And then it did
      not survive the other levels.** Rounded and measured on fresh seeds:
      the full flat gains nothing that reaches significance (63 → 68,
      p = 0.53), walls + stairwell keeps its 90/90 but takes 64 % longer
      (18.0 → 29.5 min) with four and a half times the refusals (8 → 36),
      and big furniture loses (67 → 62). The search was scored on the full
      flat alone and overfitted to it. Nothing adopted; the defaults stand
      untouched. Next time the score must be the three levels together —
      the tool takes it as a change of one line — and the winner must be
      confirmed on fresh seeds of every level before it is believed. **Second search, scored on the three levels together** (2026-09-08):
      the winner of sixty trials finished faster on paper (full flat 64 →
      52 min, big furniture 48 → 41) but bought that speed with refusals
      (113 → 163, 10 → 60, 65 → 92) and, on ninety fresh seeds of every
      level, gained nothing: +31 seeds against −30, p = 1.00, and walls +
      stairwell went from 18 to 23 minutes. Two searches, two honest
      nulls. The lesson is about the search, not the explorer: thirty
      seeds per trial cannot see an effect smaller than the ±5 houses a
      ninety-seed batch already wanders, so random search over twelve
      knobs at that budget selects noise, and the confirmation on fresh
      seeds is what keeps it from being adopted. If it is worth another
      go, it needs ninety seeds per trial and the three or four knobs the
      ablation showed matter, not twelve — otherwise the defaults, each
      measured one at a time against a fault it fixed, stand. 
- [x] `go_to`, point to point on the map already built (2026-09-09). The
      planner gained `path_to`: the same Dijkstra on the same costmap the
      frontier planner uses — known floor cheap, unknown dear, walls and
      the books impassable, the walked trail always passable — from the
      duck to one point, with the goal snapped to the nearest passable
      floor so a target against a wall still works. The explorer gained a
      goal mode: when a job carries one, the planner aims there instead of
      at a frontier and the job ends on arrival; everything else — the
      legs, the passage beside a drop, the guards, the books, the
      recoveries — is the mapping job's own machinery, extracted into one
      shared `walk_leg`. The paper twin takes `--goto x,y`: it maps first,
      then walks there, and writes both routes into the frame so the
      picture shows the planned one under the walked one. Thirty seeds,
      fifty minutes of mapping then a crossing of the flat: **arrived
      30/30**, no fall, stopping 0.14 m from the point, 242 s, no refusal
      at the median, walking 6.5 m against a plan of 5.1 (a ratio of 1.13:
      the follower aims at a point ahead on the path and cuts nothing).
      What is missing before it is a tool: `go_to(place)` on the places
      registry rather than raw coordinates, and the upstream goal RPC when
      it exists. **Confirmed on MuJoCo** (2026-09-09): fifteen minutes of mapping,
      then `robot.go_to` to a point 2.04 m away in the north corridor —
      arrived in 65 s with nine legs and one refusal, stopping 0.17 m from
      the point on its own map (0.27 m by the simulator's truth, the
      difference being maploc's own pose error at that moment). The tool
      refuses before walking when the map shows no way there. What is left
      for the tool to be finished: nothing on this side — `go_to(place)`
      by name is in, the upstream goal RPC would only replace the
      follower, not the plan.
 Also: the tour from the dock on this
      build (tour72) reached 6/9, 0 lost, 0 falls — run 69's was 8/9; the
      three misses are the return legs south of the kitchen, steered by
      the script's straight lines.
- [ ] A learned leg (2026-09-07 night, user's question: could the duck
      be trained to explore instead of following rules?). What we have is
      a rule explorer: frontier planner, leg planner, guards. The learning
      that fits is hybrid: keep the planner and the guards (a fall is
      forbidden, not learned), learn only the choice of the leg — the
      part tuned by hand with switches tonight. The paper twin is the
      gym: 30 runs of 90 minutes in 15 s, ~2500× real time. Steps: (a)
      an automatic search over the ten leg parameters (reserves, arc and
      spin thresholds, the trail leg) with the thirty-seed ladder as the
      score — no network yet; (b) human drives recorded with the duck's
      own observations (the 8×8 ToF, not the truth); (c) a small policy
      (state: a local map window, sensed obstacles and drops, bearing to
      the frontier; action: the leg) trained by imitation and refined by
      reinforcement on the paper twin, behind the guards as an
      alternative `leg()`, then MuJoCo, then hardware. Caveats: the
      paper twin does not price bumps and has no doorposts, so a policy
      trained there learns its gaps (three "right" fixes lost there
      tonight); the map and pose stay maploc's; a policy's failures are
      not legible. microduck-lab (jonathanhawkins, Apache-2.0) trains the
      gait (61 obs → 14 actuators, PPO on a Mac) — the layer below ours,
      and a sign the pipeline is Mac-feasible.
- [ ] A steadier gait to try (2026-09-07 night, user's find):
      alertform/microduck-walking, a fork of microduck_rl with one reward
      change (body angular-velocity penalty −0.05 → −0.3): 18 % less yaw
      wobble, 26 % better yaw stability and velocity tracking than the
      official policy, fewer falls, ONNX for the official stack, Apache-
      2.0, CUDA to retrain. Our three measured gait troubles are all yaw:
      the right veer (2.9 °/s), timed turns varying threefold with the
      step phase, arcs advancing like straight legs. Cheap trial: load
      their ONNX in the twin's body server, repeat the spin probe and a
      short human drive (veer, arc advance), then one 60-minute run
      against run 73. It does not give a turn in place from a standstill.
      A non-official policy on the real duck is a separate, careful call. **Tried on the night of 2026-09-08:** the fork ships no ONNX (only
      the recipe and a checkpoint left on its author's machine), so the
      ablation was reproduced in microduck-lab on the Mac: two walkers,
      same seed, 3M steps each (3 min), `W_ANG_VEL_XY` 0.05 and 0.3.
      Both learned to stand still — 0.000 m/s at a 0.3 command in the
      lab's own evaluator, and not a step in the twin — which the lab's
      README documents as the CPU-budget trap: "1.5M steps from scratch
      buys 'do not fall', nothing more", the distilled warm start walks
      but collapses under a million steps of fine-tuning, "nobody has yet
      shown what budget exploits it". So the Mac route cannot produce a
      walker to compare; the faithful route is the GPU pipeline
      (microduck_rl on Hugging Face Jobs, the user's cost and go). Kept:
      `WALK_ONNX` on the twin launcher and `private/drives/gaitprobe.py`
      (official policy on the twin: 0.090 m/s straight with a −3.5 °/s
      veer, kick-then-spin ±28–30 °/s both ways, backing −0.135 m/s; a
      client that does not drain robotd's pushes stops being heard after
      a minute — the probe drains now).
- [ ] Route memory, three levels (2026-09-07, user's direction): the
      trail (above, per job); a persistent route graph on quack-places —
      places joined by walked legs with their statistics (times walked,
      duration, refusals, back-offs, drops seen, pose jumps), the best
      route by safety first and time second, tried against the map's
      shorter proposal now and then and kept only if it walked cleaner, a
      failed route penalised, not deleted; and the metric map under both.
      Routes anchored to places, not to coordinates, and verified while
      walked with the map-versus-sensor check: maploc's pose is what it
      is, and the session resets at boot.
- [ ] A map library and boot relocalization (2026-09-07, user's
      direction): several saved maps; at boot the duck sweeps (panorama,
      a full turn if needed), tries each map with the global search under
      the uniqueness and agreement gates, takes the one confident match,
      else starts a new map and asks "Qui dove siamo?". Upstream today:
      one session file, resumed trusting the last pose, no boot search,
      `robot.map` and `robot.map_wipe` only. Needed upstream (prototype on
      `maploc-study`): `robot.map_list/load/save`, load = start hard-lost
      and search; bench it with a saved session and a recording that
      starts elsewhere (the kidnap test). Caveats: the 8×8 ToF's signature
      of a room is poor (the uniqueness gate is the defence); the session
      is schema-less bincode, so saved maps die with a format bump.
- [x] The three calls exist, on the local branches (2026-09-09).
      `robot.map_save <name>` copies the live map into a `maps/` directory
      beside the working session; `robot.map_list` says what is there,
      with sizes and dates; `robot.map_load <name>` makes one live and
      starts the mapper hard-lost inside it, so what comes back is the map
      and never the pose. A name is 1 to 64 characters of letters, digits,
      `-` and `_`, refused rather than sanitised: a caller that meant
      `../../etc/passwd` hears no. A wipe clears the live map and leaves
      the library standing. mediad carries the three, btd refuses them,
      the updater calls them unknown; `robotctl robot map-save|map-list|
      map-load` drives them by hand. quacksat exposes the same three as
      tools, spelled by method name through `Control::request_method`,
      because the released `duck-ipc-proto` has none of them — an older
      robotd answers METHOD_NOT_FOUND and the duck says "this robot's
      software has no map library yet" instead of failing obscurely.
      Measured against a fake robotd, over robotctl and through the MCP
      surface: an empty library lists nothing, a save appears in the
      listing, `../evil` is refused, an unknown name says so, a load is
      adopted with the mapper searching, and a wipe leaves the library
      alone. Where it lives: `microduck-pr202` branch `maploc-study`
      commit 4692340, quacksat `maploc-track` commit d875888 — local,
      unpushed, and asked of upstream in docs/study/upstream-asks.md §5.
- [x] Recognition on the IPC (2026-09-09). The alignment moved out of
      the bench example into `maploc::align` and onto the wire as
      `robot.map_match` (candidates, best first: the name, where the live
      map sits inside the saved one, the wall residual, the share of live
      floor laid on saved walls, and a score) and `robot.map_adopt`
      (trade the live map for the saved one, composing the transform with
      wherever the robot stands at that moment, so the client need not
      freeze it). The adopted pose is set before the mapper is built, so
      the confirming window judges it and not the pose the saved run
      ended at; the robot comes up suspect, not tracked. `robotctl robot
      map-match|map-adopt` drives both by hand.
- [x] The homecoming, and it works (run home2, 2026-09-09).
      `quacksat-core/src/homecoming.rs`, off unless `[homecoming]
      enabled = true`: at boot load the newest saved map and stand still
      for a minute in case the mapper confirms a pose by itself; if it
      does not, wipe, explore, and ask the map-to-map question every
      three minutes; adopt when two asks name the same map in the same
      place (within 0.3 m) with the live map bigger the second time.
      On the twin, booting in the kitchen 3.5 m from the dock with run
      71's map in the library: the boot search found nothing in its
      minute, as expected; the first ask, four minutes in with 339 wall
      cells, named the map at (−3.40, 1.00) against a true spawn of
      (−3.50, 1.30); the second, three minutes later with 551 cells,
      said (−3.40, 0.95); it adopted, and the pose it took was
      (−0.66, 1.99) against a true (−0.57, 1.82) — **19 cm**. Each ask
      cost 0.6–0.7 s of paused mapping. The two-ask rule is what makes
      this safe without a calibrated threshold, and it is cheap: the
      answer was already right at the first ask. Run home3, from the same
      spawn, did it again independently — asks at 417 and 575 cells, both
      naming the same place within 5 cm, adopted at (0.16, 2.05) against
      a true (0.05, 2.13), **14 cm** — and then picked exploring back up
      on the adopted map. That last part needed two fixes home2 found:
      wait for the running job to actually stop before adopting (a
      panorama takes a minute, and `robot.map_explore` answers "already
      running" rather than starting), and stand still afterwards until
      the mapper confirms the adopted place, because the explorer will
      not start without a pose it trusts.
- [x] The negative control exists, and it refuted the rule (2026-09-09).
      `sim-maploc/houses/flat_b.xml` is a second twin house — five rooms
      around a central hall, on a plan taller than it is wide, where the
      apartment is a corridor with rooms either side on a plan wider than
      it is tall. Its first draft had two rooms with no doorway at all
      (wall segments meeting exactly where the gap belonged) and furniture
      across a third, which is why `private/drives/housecheck.py` now
      exists: it grows every solid thing by the duck's own width and
      floods the floor from the spawn, naming what it cannot reach. Flat B
      is 100 % reachable at 14, 20 and 25 cm of clearance; flat A drops to
      86 % at 25.
      Put in flat B with only flat A in the library, the duck **adopted
      flat A**. The asks were (−0.25, 1.75) 0.116, (−1.40, −1.85) 0.119,
      (−1.80, −0.60) 0.133, (−1.80, −0.45) 0.124 — the last two 15 cm
      apart with the map grown from 728 to 810 cells, which is exactly
      what the two-ask rule was told to accept. So "a wrong candidate does
      not survive its own map growing" is false as written: a wrong
      candidate can sit still for two asks four minutes apart.
      What did separate the two houses, on this evidence, is the score
      itself: 0.066–0.092 in its own house against 0.116–0.133 in the
      other, with no overlap. And the failure was at least quiet — the
      mapper never confirmed the adopted pose, so the duck stopped instead
      of walking off convinced; but it had already thrown away the map of
      flat B it had built, which says adoption should be on trial and
      reversible rather than final.
- [x] The instrument was wrong before the rule was (2026-09-09, evening).
      A dry run in the duck's OWN house — thirteen asks, every one naming
      the right place within 15 cm — showed the score *degrading* as the
      map grew: 0.064 at 321 wall cells, 0.103 at 766. So "in the right
      house the fit improves" was an artefact of two measurements, and
      worse, the two houses' score ranges nearly met (own 0.064–0.103,
      other 0.110–0.142): a ceiling at 0.10 would have refused the right
      house six times in thirteen.
      The cause is not the house but the question. The saved map covers
      46 % of the flat; as the live map grows past its edges, more and
      more live wall cells land where the saved map has no opinion, and
      "how far is the nearest saved wall" there answers nothing. So
      `maploc::align` now judges the residual **on the overlap only** —
      cells where the saved map is sure of something — and reports the
      overlap share as its own number. Re-scored offline, the same pairs:
      own house 0.109, 0.116, 0.144 against another house 0.224, 0.227 —
      a factor of two where there had been seven thousandths. The same
      big live map of flat A scores 0.144 against flat A's map and 0.227
      against flat B's. The runner-up margin orders them the same way:
      0.51–0.79 when right, 0.96–0.99 when wrong — a wrong winner is
      indistinguishable from its own second choice, which is what not
      recognising a place actually looks like.
- [ ] Acceptance must be ABSOLUTE, never "the best of the library"
      (2026-09-09, user's point): the duck may be in a house that is in no
      map it holds, so the question has to have "none of these" as an
      answer. A map is adopted because it clears a bar of its own; the
      ranking between maps may only order candidates that already cleared
      it. Three outcomes: exactly one clears it → adopt; more than one →
      refuse, since a robot cannot be in two houses and the real one may
      be in neither; none → stay on the fresh map and keep exploring,
      which is the ordinary case the first time it is switched on
      anywhere.
- [x] Measured with the overlap instrument, both houses, live
      (2026-09-09, night). Thirteen asks in flat A against flat A's saved
      map: 0.107–0.138, every one naming the right place. Fourteen asks in
      flat B with BOTH maps in the library: it named `flat_b` every time,
      0.043–0.086, at a pose that never moved more than 10 cm; and in the
      same asks the wrong map scored 0.187 with 72 % overlap — the same
      overlap as the right one, so it is not being penalised for covering
      less, it simply is not that house. Offline the other way round:
      0.224 and 0.227. So everything measured so far separates at
      **0.16**: worst right 0.138, best wrong 0.187.
      The runner-up margin does not survive the move from the bench to
      the live runs — 0.74–0.96 in flat A while it was right — so it stays
      as information and not as a rule. Repetition does survive: three
      consecutive asks agreeing within 15 cm happen 5 times in 13 in flat
      A and 9 times in 14 in flat B, so demanding three is affordable.
      Note for the map itself: flat A's own map scores 0.107–0.138 in its
      own house where flat B's scores 0.043 in its own, because `home` is
      an old map with its own drift baked in and `flat_b` was built the
      same evening. A better map is recognised better — the two goals are
      one goal.
- [ ] Still to measure with the overlap instrument: both dry series
      re-run live (does the score stay flat as the map grows?), flat B
      with BOTH maps in the library (the right answer is flat_b alone),
      and a third house — the duck in C with A and B in the library must
      say none. Then the bar, from the distributions.
- [ ] Next, and measured before deciding: `[homecoming] dry_run = true`
      asks every two minutes and writes down what it *would* have done,
      so a run yields the whole series instead of stopping at its first
      mistake. Two series to collect — flat B against flat A's map, and
      flat B against both maps once B is mapped and saved — and then a
      rule chosen from the distributions: a ceiling on the score, a
      tighter agreement radius than 0.30 m (the false positive was 15 cm),
      three asks instead of two, or the winner having to beat the
      runner-up across maps. Also owed: the dock case measured live, and
      the places registry carried across a swap by the same transform.

- [ ] A window cannot form while the pose is suspect (2026-09-10, found
      while building the travel bench). After `robot.map_adopt`, standing
      and turning on the spot produces windows of 15–48 beams, which the
      mapper discards as too thin (`min_window_beams` 60) — so the
      confirmation the adopted pose needs can never arrive, and the duck
      stays lost on a map whose pose was right to a few centimetres. The
      same stands while tracking produce composites of 1000–2400 beams.
      The suspect is the accumulator's vote: it keeps a beam only when
      several frames of the window saw its endpoint cell, and while the
      pose is suspect the head is sweeping ±0.9 rad to widen the view, so
      consecutive frames look elsewhere and few cells collect votes. A
      defence against noise that empties the window exactly when the
      window matters most. This is probably also why boot relocalization
      always looked weak. To measure: composite beams per window with the
      sweep on and off, and the vote's `min_frames` against the sweep
      rate.

## 2b. One map, kept true (2026-09-09, the user's direction)

The library goes on the shelf. **One map**: the duck keeps it, improves it
as it navigates, and replaces it when told to explore afresh. What matters
instead is that the one map is right — "una mappa perfetta, senza derive o
spostamenti" — and that the duck moves quickly from place to place on it.
Recognition survives the change, simplified: at boot the question is no
longer *which* house but *this one, yes or no*, which is the same
measurement against a library of one, at the bar above. Refusing costs
nothing: it explores and asks.

- [x] A number for "perfect" (2026-09-09). `private/drives/mapquality.py`
      scores a map against the house itself, not against another map: the
      walls are in the world file, so the truth is available. It fits the
      map to the truth rigidly first — where the map's origin sits is an
      accident of where the duck booted — and what survives the fit is the
      quality: displacement percentiles, the share beyond 10 cm, ghosts
      beyond 25 cm (a drifted map draws the same corridor twice, and the
      second copy lands in open floor), and coverage of the true wall
      surfaces. `maploc`'s new `dump_frame` example turns a saved session
      into the same JSON a live map frame carries, so a map on disk and a
      map in flight are scored by one tool.
      Where we stand, and it is not where the direction asks:

      | map | median | 90th | beyond 10 cm | ghosts | coverage |
      |---|---|---|---|---|---|
      | run 71 (the reference) | 0 cm | 20 cm | 21.0 % | 6.9 % | 46 % |
      | flat A, 30 min tonight | 0 cm | 25 cm | 18.7 % | 8.0 % | 39 % |
      | flat B, 30 min tonight | 0 cm | 20 cm | 21.6 % | 3.4 % | 39 % |

      The median is zero — most mapped walls sit exactly on true ones —
      and a fifth of them are out by more than 10 cm, with 3–8 % doubled.
      That fifth is the target.
- [x] Where the fifth came from, and half of it is gone (2026-09-10).
      The pose correction between closures was already on, so the suspect
      was the loop closer — and it was, but not for the reason expected.
      Closures are worth having: replaying a session with none at all
      leaves 27 % of walls misplaced against 21 % with them. What was
      wrong is how much they were believed. The edge told the optimizer
      that two submaps' relative pose was known to 5 cm — a cell and a
      half — so the graph bent to satisfy every closure the flat's
      repeated rectangles produced.
      Widened to 40 cm and 14°, over seven replayed sessions in two
      houses: better on six of seven, mean misplaced wall 21.1 % → 13.4 %,
      mean doubled wall 5.4 % → 3.4 %. On the reference recording
      20.8 % → 3.6 %. Live, half an hour in flat A: doubled walls
      **8.0 % → 1.1 %**, the 90th percentile 25 cm → 15 cm, the worst
      55 cm → 40 cm; misplaced walls barely moved (18.7 → 17.1), a live
      run being a different path and not a controlled comparison.
      And the map that came of it recognised its house better: the ask
      series went from 0.107–0.138 to **0.058–0.112**, margins from
      0.74–0.96 to 0.34–0.43. A truer map is an easier map to recognise.
- [ ] What is NOT fixed: the sensitivity itself. The same recording, with
      the edge confidence moved a little, still lands anywhere between
      3.6 % and 29 % of misplaced wall — the outcome is chaotic in that
      parameter, and 40 cm is a more tolerant place to stand rather than a
      cure. A Huber kernel on the loop edges was the obvious treatment and
      it was measured: with 40 cm edges nothing reaches 1.5 sigmas so it
      never fires; with 5 cm edges it fires and the result is a lottery
      (one recording +17 points, another −21). It ships off,
      `OptimizerConfig::huber_delta`, with the measurement beside it. A
      real cure has to make one bad closure unable to dominate — a
      switchable constraint, a consensus over closures, or validating a
      closure against the map it would produce.
- [x] Instruments for this work: `mapquality.py` (a map against the
      house's own walls), `maploc`'s `dump_frame` (a saved session as a
      live map frame) and `evaluate`'s `OUT_SESSION` (the map a replay
      built, saved the way the robot saves one). Together: replay any
      recording under any setting and score the map that comes out, in
      about fifteen seconds. Also, usefully, a map scored against the
      wrong house reads 40–50 % misplaced against ~20 % for the right
      one, so map quality doubles as an identity check.
- [ ] Moving quickly. Every leg today is walk-then-stand, and the stand is
      there to *map*. On floor already mapped with a pose it trusts, the
      duck does not need it: `go_to` can walk continuously with the depth
      sensor as its only guard. Measure first — how long from the kitchen
      to the bedroom as it stands — then the fast mode, and the same
      number after.

- [x] How fast is it, and the stands are not the fat (2026-09-10).
      `private/drives/speed_test.py` and `speed_run.sh`: the duck maps on
      its own for seven minutes, then is asked to cross what it mapped,
      with the truth taken from the body server. Two three-metre journeys
      take **221 s and 192 s**, arriving within 9-15 cm — a speed made
      good of 0.013-0.015 m/s against a walking speed of 0.121. Nine
      tenths of the journey is not progress.
      The obvious cure — drop the stand between legs on floor already
      mapped, standing every fifth leg to keep the pose honest — was built
      and measured, and does not pay: 235 s and 214 s, no faster, and the
      duck walked 8.5 m and 8.7 m to make the same three metres, with
      seven legs that failed to move it against one and eleven back-offs
      against four. So it ships off (`QK_FAST_GOAL=1` turns it on).
      What that taught: the stand is not only how the stop reaches the
      map, it is also what puts a fresh frame and a corrected pose in
      front of the next leg's plan. Walking on from a stale plan bumps
      into corners the plan did not know about.
      Where the journey actually goes: 7-8 m walked to make 3, thirteen
      turns in place along the way, and about 60 s of the 200 spent
      walking. **The path and the spins are the target**, not the stands —
      a follower question, and behind it the pose: mid-run the belief was
      0.70 m from the truth, which is enough to make the planner see walls
      where there is floor.
      (A word to the next reader: `robot.go_to` speaks the duck's map
      frame, whose origin is where it booted. Handing it world
      coordinates sent it walking at a point outside the flat and cost a
      run. `speed_test.py` converts now.)

- [x] Halved: the string pulled straight, and no sightseeing on the way
      (2026-09-10). Two changes, each measured on the same two
      three-metre journeys:
      **The aim.** A grid path is a staircase, and the follower aimed at
      the point eight cells along it, so the wanted heading swung half a
      quadrant and alternated — thirteen turns in place per journey, each
      asking for 64° to 101°. It now aims at the farthest point of the
      path it can reach in a straight line, walls and books both clear
      (`QK_SMOOTH_PATH=0` restores the old way). Spins 13 → 7, and legs
      that failed to move the duck, back-offs and refusals all → 0.
      **The sightseeing.** Timing every turn of the explorer's loop showed
      the ordinary leg is 4.4 s and perfectly fine, while one or two turns
      per journey took 78 s and 113 s — panoramas, in the middle of a
      journey across floor already mapped, worth 60–76 % of the whole
      trip. A panorama is how a mapping job learns a room it has not seen;
      a journey has nothing to learn from one, and if it does meet unknown
      floor the guards refuse the step and the planner routes round it.
      Skipped when the job has a goal. (Also: a panorama saw a stop
      request only when it finished, so `map_explore {stop}` sat unanswered
      for a hundred seconds. It checks between steps now.)
      Together: **221 s → 106 s and 192 s → 124 s**, path over straight
      line 2.48 → 1.67 and 2.07 → 1.89, speed made good 0.013–0.015 →
      **0.025–0.029 m/s**, arriving as accurately as before (15–21 cm).
      The longest turn of the loop went from 113 s to 8.6 s.
      What is left is the leg itself: 1.5 s of walking and 3 s of standing,
      so still a fifth of the walking speed. Dropping the stand was
      measured and did not pay *before* these two changes; worth asking
      again now that the path is straight.

- [x] The stand stays, decided by repetition (2026-09-10). Five
      three-metre journeys under each setting, after the panorama and aim
      fixes: full 3 s stand 135 · 136 · 133 · 135 · 185 s; short 1.5 s
      87 · 126 · 86 · 156 · 197; none 63 · 110 · 113 · 137 · 81. The
      medians fall (135 → 126 → 110) but the spreads overlap almost
      entirely, so on five samples the difference is not distinguishable
      from the run-to-run noise — the same noise that gave 106 s and 135 s
      for the identical configuration on two different runs.
      What is distinguishable: with the full stand four of five journeys
      land within three seconds of each other, and no leg fails to move
      the duck and nothing backs off. Without it, one and three; with none,
      five and eleven. So the stand buys predictability and a body that
      does not walk into things, at a price hidden inside the noise. It
      stays (`QK_FAST_GOAL=1`, `QK_FAST_STAND_S` to revisit).
- [ ] What the floor-scrubbers do that we could (2026-09-10, the user's
      question — why do they map a whole floor without a millimetre of
      error?). Most of the answer is that they play another game: a
      spinning LIDAR sees 360° at once, thousands of points a second, and
      a full outline of a room has exactly one way to fit the map, where a
      45° wedge on a flat wall can slide along it freely — which is our
      aliasing, our doubtful loop closures and our mirror-image matches,
      all at once. And they roll on encoders over a flat floor where we
      walk, with a sensor on a head on a body that bobs.
      Three of their tricks are within reach, and worth taking in this
      order:
      **Fit lines.** The map on their phone is not a raw grid: walls are
      snapped to straight segments and right angles. A house is made of
      segments and our grid does not know it. This attacks the very number
      we are trying to lower.
      **Anchor on the dock.** They return to it and re-anchor every run,
      which erases accumulated drift against a landmark that never moves.
      The twin has a dock and we use it for nothing.
      **Follow the wall first.** Their first pass is the perimeter, and
      keeping a wall in view is what makes the estimate well-conditioned.
      Frontier exploration is excellent at covering and poor at anchoring.

- [x] The duck in the house with curves (2026-09-10). Flat C mapped for
      25 minutes, then scored against its own geometry:

      | | flat A | flat B | flat C |
      |---|---|---|---|
      | median | 0 cm | 0 cm | **5 cm** |
      | 90th | 25 cm | 20 cm | **42 cm** |
      | beyond 10 cm | 18.7 % | 21.6 % | **32.9 %** |
      | doubled | 8.0 % | 3.4 % | **18.9 %** |

      The picture says what the numbers do not. The round island and the
      curved partition are mapped cleanly — the two things most feared.
      What is missing is **the big gentle bay**: the floor beneath it is
      explored, so the duck walked there, but the wall itself survives
      only in patches. The reason is not curvature but grazing incidence —
      a wide concave wall seen from inside returns echoes along itself,
      and a grazing beam either does not come back or is dropped by the
      filter that separates floor from wall. The same sensor traces a
      cylinder at one metre and misses a soft wall at three.
      So 32.9 % does not read "curves are hard"; it reads **"what the
      sensor sees edge-on does not reach the map"**, and flat C simply has
      much more of it. That will be true of the real duck too, and worse,
      since a real wall reflects less kindly than a simulated one.
      It also settles the straightening question from the other side: in a
      house like this a line-fitter would have the least real evidence
      exactly where the map is poorest, and would fill the gap with its
      own invention. Straightness belongs in the pose correction, not in
      the drawing of walls.
      (Two of the three instruments were quietly showing the wrong house
      until this run: the map picture drew flat A's boxes over flat C's
      map, and then drew flat C's turned walls unturned. Both fixed. The
      quality figures were never affected — that tool learned about
      rotation first.)

- [x] **A correction: the measuring instrument was choosing bad
      alignments** (2026-09-10, late). `mapquality.py` fits the map to the
      house rigidly before scoring, and the fit minimised the *median*
      distance — which is minimised beautifully by an alignment that nails
      a subset of walls and ruins the rest. It chose one: the same flat C
      map read 36.3 % beyond 10 cm at the fit it picked (−8°) and 22.2 %
      four degrees away. The fit now minimises the share beyond 10 cm
      itself, with the median only as a tie-breaker.
      Every figure measured before this is biased, and the wild
      non-monotonic jumps that made a parameter look chaotic were largely
      the fit hopping between alignments. Re-measured, the three live maps
      of this evening read:

      | | flat A | flat B | flat C |
      |---|---|---|---|
      | beyond 10 cm | 4.9 % | 11.2 % | 22.5 % |
      | doubled | 0.9 % | 1.5 % | 15.1 % |
      | coverage | 49 % | 49 % | 51 % |

      Both decisions of the day survive the correction, one of them larger
      than it looked. The loop-closure widening (5 cm → 40 cm), paired per
      recording at the same range: flat C 29.3 % → 14.4 %, flat A 13.4 % →
      1.9 % and 14.2 % → 8.0 %, flat B 13.2 % → 5.4 % — four of four, with
      doubled walls in flat C falling from 20.6 % to 0.5 %.
- [ ] **The accumulator keeps only the nearest two metres**
      (`AccumulatorConfig::max_range_m` = 2.0, "the sensor's noise past
      here costs more than the coverage buys"). The sensor reaches four.
      That is why the far half of an open room is never inked, and why
      flat C's wide bay is missing from its map — not curvature, and not
      grazing incidence: the simulated ToF is a plain raycast with no
      incidence model at all, which is a correction to what this file said
      an hour earlier.
      Raising it to 3 m, paired per recording: flat C 14.4 % → **1.2 %**
      with coverage 48 % → 62 %; flat A 8.0 % → 1.1 %; flat A 1.9 % →
      8.8 %; flat B 5.4 % → 5.1 %. Two better, one worse, one level — and
      the shape of it makes sense, since a narrow flat has little beyond
      two metres to gain and only the noise to lose. Promising, not
      decided: it wants repeats, and it wants the real sensor's noise at
      three metres, which is upstream's number and not the twin's.

- [ ] A walking policy that can turn on the spot (2026-09-11, from the
      user: uduckmoves.com, a community registry of Microduck policies,
      Apache-2.0, 18 moves, 8 with a hardware claim). Two facts first: the
      registry's "Alpha Dynamic Walk" has the same SHA256 as the
      `alpha_walking.onnx` we already run, so that entry is our own gait;
      and robotd's loader feeds its input by the name `obs` and reads the
      first output whatever it is called, so any 61 → 14 model is a
      drop-in as far as the tensors go.
      `backlash.onnx` (Genesis, fine-tuned with ±1° of simulated gearbox
      play on every servo) is 61 → 14 and **walks**, so the observation
      layout is compatible. Probed against alpha on the twin, one rep:

      | | alpha | backlash |
      |---|---|---|
      | straight | +0.123 m/s | +0.128 m/s |
      | veer | −4.8 °/s | −8.5 °/s |
      | **spin from a standstill** | **+0.2 °/s** | **+28 / −32 °/s** |
      | arc +0.7 | +38.3 °/s | −1.3 °/s (wrong way) |
      | arc −0.7 | −32.0 °/s, advancing | −26.8 °/s, not advancing |
      | backing | −0.122 m/s | −0.084 m/s |
      | falls | 0 | 0 |

      Turning in place is the capability the explorer most lacks: today
      every heading change in a tight place costs a kick forward first,
      which is where the thirteen spins per three metres, several refused
      legs and some of the back-offs come from. But the arcs are broken —
      asked to arc left it turns one degree the wrong way — and the
      explorer lives on arcs, with `GAIT_M_PER_S` and the arc-advance
      fractions all measured on alpha.
      So: a serious candidate, not a swap. What it wants next is a full
      run in the twin (map quality, falls, journey times) and, if that
      holds, the follower re-tuned around a gait that can pivot — the
      spin-in-place branch stops being a last resort. And the caveat that
      travels with everything from this registry: none of the Genesis
      policies has ever walked on a physical duck.

- [x] The policy that pivots loses anyway (2026-09-11). A full 25-minute
      mapping run in flat A with each gait, same code, same house:

      | | alpha | backlash |
      |---|---|---|
      | beyond 10 cm | **1.6 %** | 14.0 % |
      | ghosts | **0.0 %** | 5.8 % |
      | coverage | 46 % | 43 % |
      | falls | 0 | 0 |
      | spins | 33 | **27** |
      | legs refused | **2** | 11 |
      | back-offs | **7** | 14 |

      Turning in place does pay a little — six fewer spins — but the
      broken arcs cost five times the refusals and twice the back-offs,
      and the veer (−8.5 °/s against −4.8) becomes pose drift, and drift
      smears the map. For this robot **a clean walk matters more than a
      manoeuvrable one**: a gait that goes where it is pointed makes a
      better map, and a better map makes everything else easier. So alpha
      stays, and the registry's value to us is the fact that a 61 → 14
      policy drops in at all — the door is open when a better-behaved one
      appears.
      That alpha run is also the best live map measured so far: 1.6 %
      misplaced wall and no doubled wall at all. It is the reference to
      beat, and it is the repeat that was missing on the three-metre
      range — live, in flat A, three metres is excellent. Last night's
      contradiction is now confined to flat C, where there is one
      measurement.
- [ ] While the duck walks, the ToF reaches nothing but the guards
      (2026-09-11, the user's question). `Mapper::frame` opens with
      `if !self.was_still { return false }`: in stop-and-scan every frame
      that arrives while the body moves is dropped before anything looks
      at it. The same frames do feed the cliff guard, the step guards and
      the explorer's books — safety reads the sensor continuously — but
      nothing seen in motion ever reaches the map. At 15 Hz a three-second
      leg throws away some forty frames, and over a run that is most of
      the data collected.
      The reason is sound: walking, the pose is dead reckoning and the
      trunk bobs, and ink laid at a wrong pose is the smear we spent a
      night removing. But `MapperConfig::continuous` already exists and we
      have never measured it, and now there is an instrument to judge it
      with. The middle ground worth trying first: **carve free floor while
      moving, ink walls only from stands** — free space forgives a pose
      error where a wall does not, and floor coverage (39–51 %) is exactly
      what is short and what sends the duck into those ninety-second
      panoramas.

- [x] **Live and replay differ, and the clock is NOT why** (2026-09-11,
      corrected the same morning). Flat C's second run mapped at 36.3 % of
      wall beyond 10 cm and 27.1 % doubled; **its own recording, replayed
      through the same code with the same parameters, gives 8.5 % and
      1.7 %**. Same data, four times the error — the September anomaly,
      found again and this time with a number on it.
      The first explanation written here was wrong and is retracted: the
      live worker does stamp frames with `started.elapsed()` rather than
      the sensor's `at_us`, but **the recorder stamps each record with its
      own `started.elapsed()` too, and the replay uses that**, so both
      paths run on the same clock, scheduling jitter included. Checked and
      ruled out with it: posture construction, the status filter, the
      column mirror, `continuous`, and the mapper's determinism (no wall
      clock and no unseeded RNG in the live path; the one HashMap that
      matters is only queried, and mcl already breaks ties by coordinate).
      What is established is that the two maps differ in *content*, not
      just in alignment: live 815 wall cells and 121 submaps, replay 600
      and 114. A deterministic mapper fed the same sequence cannot do
      that, so the sequences differ — and finding out how is the next
      thing to do. The plan: replay one recording twice and compare the
      maps byte for byte, which separates "the bench is not deterministic"
      from "the recording is not what the mapper saw".
- [x] Flat C at three metres, four runs (2026-09-11): 22.5 % at two
      metres, then 36.3 %, **3.7 %** and **4.8 %** at three, with coverage
      51 → 34, 60, 55 %. The 36.3 % was the run above, the one the clock
      spoiled. So three metres holds in the curved house too, and the
      bench agreed all along.

- [x] **The bench is trustworthy after all** (2026-09-11, afternoon). The
      live-versus-replay divergence did not reproduce under control. A
      flat C run kept its daemon log, and the live mapper's own tally
      matched the replay of that run's recording almost exactly — 61 635
      odometry samples against 61 905, 17 243 frames against 17 319, 278
      windows against 280, 100 submaps against 100, the differences being
      the five seconds between the last status line and the end of the
      file. The maps then agreed to a tenth of a point: 21.7 % of wall
      beyond 10 cm live, 21.6 % replayed; 15.5 % doubled against 15.0 %.
      So the morning's 36.3 %-against-8.5 % was almost certainly a
      mispaired recording on our side — its replay showed 114 submaps and
      600 wall cells where the live session had 121 and 815, which is not
      how the same run looks. The September anomaly stays open as a
      question, but nothing here supports it, and the replay bench can be
      trusted to stand in for a live run.
- [x] **What the mapping costs, which we had never asked** (2026-09-11,
      the user's point: the duck's processor is an RK3566, four Cortex-A55
      and a gigabyte, and the mapping thread shares them with the wake
      word and a 50 Hz control loop). Replaying the same 1238-second
      session: 1.08 s of CPU at two metres of accumulator range, 1.30 s at
      three, 1.46 s at four — so **the change to three metres costs 20 %
      more CPU**, and the mapping as a whole runs at about 950× real time
      on one Mac core, a thousandth of it.
      Scaled to an A55 at perhaps a tenth of the speed, that is 80–120×
      real time: roughly one per cent of one core, and twenty per cent
      more of that is nothing. Three metres is affordable.
      Three caveats travel with it: the replay measures the mapper alone,
      where the live daemon also renders a grid every second and writes a
      seven-megabyte session every minute onto flash; the heavy work is
      not the average but the events — a brute-force relocalize, a burst
      of loop closures — and those land exactly when the pose is most
      needed; and these are the twin's data volumes.
      It also undercuts this morning's load experiment, which is
      downgraded: if mapping uses one per cent of a core, five busy cores
      should not have mattered, and 11.0 % against 18.2 % on one pair,
      inside a house whose runs range from 3.7 % to 36.3 %, says nothing
      yet. **From here on, every parameter gets a CPU column.**

- [x] **Why one run in three comes out smeared, and it is not a
      parameter** (2026-09-11). Seven recordings of the same house,
      replayed with a tally of everything the mapper did beside the
      quality of the map it made:

      | recording | misplaced | doubled | windows | **closures** |
      |---|---|---|---|---|
      | 1789073165 | 1.2 % | 0.2 % | 388 | 22 |
      | 1789109158 | 3.0 % | 0.0 % | 330 | 32 |
      | 1789112972 | 3.2 % | 0.1 % | 293 | 33 |
      | 1789110708 | 4.7 % | 0.0 % | 336 | 19 |
      | 1789076196 | 8.5 % | 1.7 % | 345 | 29 |
      | 1789114222 | 12.6 % | 5.0 % | 290 | **15** |
      | 1789122541 | 21.6 % | 15.0 % | 280 | **15** |

      The two worst runs closed fifteen loops each; the good ones nineteen
      to thirty-three. The comparison is clean — those two and the 3.2 %
      run all lasted exactly 1200 s. So closures are not what smears a
      map, a map without them drifts, which agrees with disabling them
      entirely costing 27 %.
      And a loop closes only where the duck passes near where it has
      already been. Frontier exploration never goes back on purpose, so
      **how many closures a run gets is an accident of its shape** — which
      is the whole bimodality.
      Two attempts to fix it in the graph, both measured, neither a cure:
      widening the closure search from 1.5 m to 2.5 m rescues five
      recordings of seven (21.6 → 14.2, 12.6 → 11.5, 8.5 → 4.7) and ruins
      the two best (1.2 → 17.5, 3.2 → 16.1), so the median gets worse and
      the CPU cost rises 50–70 %; and disowning the worst loop edge
      (`OptimizerConfig::reject_sigmas`, now in the optimizer) helps a
      little everywhere — 21.6 → 18.7, 1.2 → 0.9 — and ships off.
- [ ] **So the fix belongs in the walking, not in the solver**: make the
      duck close loops on purpose. It already knows where it has been (the
      trail), and `go_to` can take it there. Every few minutes, break off
      exploring, return to a well-mapped place, stand, and resume — the
      floor-scrubber's dock trick and its perimeter pass, in our own
      terms. That is the next thing to build, and the instrument to judge
      it by is the tally above: a run that re-anchors should show more
      closures and less spread, not just a better mean.

- [x] Re-anchoring built and measured, and the theory behind it refuted
      (2026-09-11, `explore.rs`). Every three minutes the explorer breaks
      off, walks back to the nearest point of its own trail that it left a
      while ago, stands there, and resumes — the floor-scrubber's return
      to its dock, with `go_to` doing the walking. `QK_REANCHOR=0` turns
      it off. Sixteen runs of flat C:

      | | median | mean | worst |
      |---|---|---|---|
      | no returns (7 runs) | 4.7 % | 7.8 % | 21.6 % |
      | **returns, gentle (6)** | **3.6 %** | **4.9 %** | **12.3 %** |
      | returns, forced (3) | 6.6 % | 8.4 % | 17.2 % |

      The gentle version helps, mostly in the bad tail. The forced one —
      destinations five metres back instead of ten, a ten-second stand so
      the visit becomes a submap of its own, a wider search — is **no
      better than doing nothing**, and it refutes the idea it was built
      on. More returns did raise the closure count, and the maps did not
      follow: one run closed twenty-two loops and still misplaced 17.2 %
      of its walls with 11.7 % doubled, where twenty-two closures had
      given 0.7 % before. More chances to close is also more chances to
      close against the wrong place, and a wrong closure draws the
      corridor twice.
      So the closure count is **correlated with a good map and does not
      cause one**. Most likely both follow from the shape of the walk: a
      run that stays compact drifts less and revisits more. Sixteen runs
      now span 0.5 % to 21.6 % with every parameter we have tried, and
      what would settle it is a measure of the drift itself rather than of
      its consequences — the pose against the truth, second by second,
      which the twin can give and we have never plotted.
      The gentle parameters are what ships.
- [ ] Follow upstream for a `robot.goto`-style RPC (planner + follower
      exist in the crate, not wired). If nothing appears by December,
      propose it as a PR on the Pollen repo with the duck in hand.
- [ ] `go_to(place)` tool on top of it: plan, follow, report arrival or
      failure; the ToF avoidance in M9 is upstream's job, not ours.
- [ ] `look_at` via the existing `robot.look`.

- [x] **The drift itself, at last** (2026-09-11 evening,
      `private/drives/posetrack.py` and `driftlook.py`). Every number
      until now measured the consequence of drift on a finished map; this
      writes the belief beside the truth once a second while the duck
      works, as *displacements from each one's own start*, so the two
      frames need not be aligned. `driftlook.py` then prints the shape of
      the run and every jump over 5 cm with the daemon's lines from those
      seconds.
      Two runs of flat C, and three things we did not know: the drift does
      not grow, it **oscillates** — 3 → 6 → 22 → 1 → 15 cm in one run,
      5 → 4 → 28 → 15 → 6 in the other — so the system really does correct
      itself, repeatedly; the live pose is **better than the maps
      suggested**, never past 35 cm and mostly under 15, median 7–9 cm;
      and the jumps have mixed causes, some landing on a tracking
      correction or a loop closure and some on nothing at all in the log,
      which is odometry walking away between stands.
      The consequence matters more than the numbers. If the pose wanders
      to 25 cm and comes back, but a fifth of the map's walls are more
      than 10 cm out with some drawn twice, then **the fault is not the
      pose but that ink stays where it was laid**: a wall inked at 25 cm
      of drift stays there after the pose recovers, and the same wall seen
      twice from poses 25 cm apart is the doubled corridor.
      Both runs were good maps (6.6 % and 2.4 %, no ghosts) with the pose
      over 10 cm for 37–41 % of the time — a quiet validation of
      stop-and-scan, where ink goes down only at stands and a stand is
      just after a correction. What is owed now is the same trace on a
      *bad* run, which happens about one in three. The tracker is attached
      to every run from here on.

- [x] **Five interventions, one signature** (2026-09-11, the evening's
      result and a negative one). Widening the closure search, a Huber
      kernel, disowning the worst loop edge, forcing re-anchoring, and
      cutting the submap when the pose is corrected: every one of them
      improves four or five recordings of seven and ruins the rest, with
      swings of ten to twenty-five points either way, and none moves the
      median. The map's quality is **chaotically sensitive** — a submap
      boundary moved, an edge weight changed, and which closures happen
      reorganises the whole map. All five ship off or gentle.
      What that means for the work: stop chasing the mean by tuning.
      Either reduce the variance, or let the duck notice a bad map and
      redo it.
- [x] The duck cannot yet tell a bad map from a good one by itself
      (2026-09-11). Two self-checks tried, neither usable:
      **its own window agreement** — how well each still window matches
      what is already drawn — does not predict the truth at all: over the
      seven recordings the worst map (21.6 % of wall misplaced) agrees
      with its own windows at 0.018, better than a 3.0 % map's 0.022.
      Every window is judged at its own pose, so a map that is locally
      consistent and globally wrong passes.
      **Doubled walls found in the map alone** (`maphealth.py`: a wall
      cell, mapped floor, and another wall cell within 30 cm — a shape a
      house does not have) is weakly informative: it puts the worst run
      top at 1.0 % and the best bottom at 0.4 %, but gives 0.4 % to a run
      that was 12.6 % wrong. Promising in principle, too weak as built.
      A usable self-check is what would let the duck keep one map honestly
      — the direction of the whole track — so it is worth another attempt:
      a global consistency measure rather than a local one.

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
  0.5 m. Third case, same evening: run 49 from scratch, 5000 cells, Mac
  idle — live pose 0.6–0.8 m off from minute 10, tracking dropped at
  minute 15, then relocalized 3–5 m wrong and stayed there "tracked";
  the replay of that recording (`1788627740.mdlg`) tracked throughout,
  0.49 m worst. So neither load nor map size: robotd's live maploc
  differs from its bench. Our defence is a map-versus-sensor agreement
  check at stands (a false pose has the sensor seeing walls where the
  map shows floor) — see the explorer.
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
