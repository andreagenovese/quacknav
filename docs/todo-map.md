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
- [x] A better pose does not make the journey faster — it makes the
      arrival true (2026-09-12). With the correction's absolute bar in
      place, five three-metre journeys across flat A
      (`private/drives/speed_run.sh`), the same route as the September
      baseline: **82 · 135 · 171 · 133 · 111 s** against **135 · 136 ·
      133 · 135 · 185**. Median 133 against 135 — the same number. Path
      over straight line 1.51 to 2.50, the same wandering; 19 turns in
      place; no back-offs, no stalls, no falls.
      What did change is underneath. `posetrack` ran throughout: drift
      median **3.5 cm while mapping and 7.8 cm across the journeys, worst
      18.9 cm**, where the run that set the baseline had the belief 0.70 m
      from the truth mid-journey. The duck used to stop 15 cm from the
      goal *on a map that was itself two thirds of a metre out*; now the
      15 cm is nearly all of the error there is.
      So the pose was never what made the journey slow, and the earlier
      note that a 0.70 m belief "makes the planner see walls where there
      is floor" did not survive the test: the planner wanders just as much
      with a pose ten times better. What is left to blame is the leg
      itself — 1.5 s of walking to 3 s of standing — and the follower's
      habit of turning in place. Worth timing the loop again with the
      journeys separated from the mapping.
      (The map the journeys crossed: 9.9 % of wall beyond 10 cm, 0.1 %
      ghosts, on seven minutes of mapping rather than twenty.)

- [x] The journey's seconds, counted (2026-09-12). The explorer times
      every pass of its own loop, so the five journeys above could be
      opened up without another run — `private/drives/legtime.py` splits
      a log at the `robot.go_to` calls and attributes each pass. Over
      631 s and 111 legs:

      | | seconds | share |
      |---|---|---|
      | standing | 333 | **52.8 %** |
      | walking | 173 | 27.3 % |
      | turning in place | 89 | 14.1 % |
      | everything else | 6 | 1.0 % |

      Nothing is hiding. Planning, the wait for a fresh map frame, the
      refusals — together about one second in a hundred. **Half the
      journey is the duck standing still**, three seconds at a time, and
      the stand exists to give the mapper a still window.
      The turns in place are 12 over five journeys but 7.4 s each, and
      they clump: one journey lost 37 s to five of them and took 170 s
      where its twin took 133.
      This reopens the September decision. The stand was kept then
      because dropping it cost legs that failed to move the duck and
      back-offs — with a belief 0.70 m from the truth, the stand was also
      what kept the pose honest. The pose is now 8 cm. `QK_FAST_GOAL=1`
      (stand every fifth leg on floor already mapped) deserves the same
      five journeys again.

- [x] Now the stand can go — a quarter off the journey (2026-09-12,
      `QK_FAST_GOAL=1`, the same five journeys again). September said the
      stand had to stay; that was with a belief 0.70 m from the truth,
      where the stand was also what kept the pose honest. With the
      correction's bar in place it is no longer paying for that, and the
      answer flips:

      | | with the stand | standing every fifth leg |
      |---|---|---|
      | speed made good, per journey | .026 .024 .018 .022 .029 | **.041 .052 .015 .033 .034** |
      | median | 0.024 m/s | **0.034 m/s** |
      | all five, straight line over total time | 0.023 m/s | **0.029 m/s** |
      | seconds for the five | 631 | **462** |

      Faster on four of the five. Where the seconds went instead:
      standing 333 → 51 s, walking 173 → 223, turning in place 89 → 98,
      and a new 58 s inside the legs themselves — the step's own
      alignment pulses and guards, which the leg never asked for and
      which grow when no stand has just corrected the heading.
      **What it costs.** The duck wanders more: 2.56 m walked per metre
      made good against 1.94. The pose is slightly worse but holds —
      drift median 9.4 cm against 7.8, worst 19.5 against 16.6, so the
      every-fifth-leg stand is doing its job. No falls, no relocalize.
      **The bad journey.** One of the five took 198 s and walked 12.2 m
      to make 3.0 — seven turns in place costing 54.6 s and 39.5 s of
      alignment pulses. (First read as a place in the house, because the
      slow one with the stand crossed the same way. The second pair below
      shows otherwise: every series of five has one bad journey, in either
      direction, under either setting.)
      On five samples and with that outlier this is worth one more pair
      before it becomes the default; the effect is larger than the
      run-to-run noise that hid September's 10 %, but not by so much that
      one repetition is a waste.

- [x] Twenty journeys, and the stand comes off — `QK_FAST_GOAL` is now
      the default (2026-09-12). A second pair of series, one each way, to
      settle the five-sample doubt. Ten journeys per setting:

      | | standing every leg | standing every fifth |
      |---|---|---|
      | speed made good, median | 0.024 m/s | **0.032 m/s** |
      | mean | 0.023 | **0.032** |
      | all twenty, straight line over total time | 0.022 m/s | **0.028 m/s** |
      | walked per metre made good | 2.12 | 2.60 |
      | drift across the journeys, median | 7.4 cm | 10.8 cm |
      | stopped from the goal, median | 0.14 m | **0.12 m** |

      The hurried duck is faster in **78 of the 100 pairings**, and a
      permutation test on the difference of means gives a one-sided
      p = 0.016 — so this is not the run-to-run noise that hid
      September's 10 %. Twenty-nine per cent faster counting every journey
      end to end. Standing was 53 % of a journey's seconds and is now
      11 %; walking is 27 % and is now 49 %.
      **Turned on by default** in `explore.rs`, with `QK_FAST_GOAL=0` to
      put the stand back. Only with a goal: a mapping job still stands
      every leg, which is what it is for.
      **What it costs**, and it should be watched: the duck wanders half
      again as far, and the pose drifts half again as much (10.8 cm
      against 7.4 median; worst 22.5 against 23.2, so the tail is the
      same). The fifth-leg stand is what holds that, and it is the first
      thing to change if the drift climbs on the real duck, whose sensor
      is noisier than the twin's.
      **The bad journey is luck, not a place.** Every series of five has
      one that takes two to four times as long and walks 10-12 m to make
      3 — with the stand and without, in both directions across the flat.
      Chasing that outlier is worth more than another second off the
      median: it is 30-40 % of the total time in every series.

- [x] Told out loud, and it went (2026-09-12). The whole chain run
      together for the first time since the map got good, on the twin:
      wake word → VAD → recognizer → qwen3:8b with the tool catalogue →
      robotd → the body. The duck mapped for six minutes, was taught
      `cucina` where it stood and `salotto` 3.6 m away, and was then
      asked, out loud:

      > — Dove ti trovi adesso?
      > — Sono nel salotto! Quack!          (`robot.where_am_i`)
      > — Portami in cucina.
      > — Vado in cucina! Quack!            (`robot.go_to {"place":"cucina"}`)

      It chose the tool itself, from the name it had been taught minutes
      before, and set off. `private/drives/voicedemo.{sh,py}` runs it;
      `fakevoice.py` stands in for the two ends we have no servers for on
      this Mac — an OpenAI-dialect recognizer that returns what the
      scripted mic just "said", and a voice that writes down what the duck
      replies. Everything between them is the real thing. `voice.py` is
      that mic: 48 kHz stereo zeros, and on cue the *shape* of someone
      speaking, which is all the `energy` wake mode needs.
      **What it did not do: arrive.** It walked 3.5 m of the 3.6 and ran
      out of the tool's 300 s budget 1.70 m short — 68 legs, 16 % of the
      time turning in place, 19 % in alignment pulses, two stalls and a
      refusal. The bad journey, and this time it was the one that had been
      asked for.
      **And nobody was told.** `go_to` returns at once, the model says "I
      am going" and never looks again, so the duck stood 1.70 m from the
      kitchen and said nothing. Reporting the end of a journey — a second
      turn, a spoken "I am there" or "I could not get through" — is
      missing and is what a user would notice first.
      **Two product fixes on the way there.** Asked to go to the kitchen,
      the model first called `robot.map_explore`: the system prompt spoke
      only of remembering places, and `go_to`'s description opened with
      "Walk to a place the duck knows". The prompt now says that going to
      a room by name is `go_to` and never `map_explore`, and the tool
      description opens with the words a person actually says. An 8B model
      at home needs that; a large one would not.
      **Two harness bugs, both mine, both worth remembering.** A sentence
      said while the duck is still speaking is dropped on purpose (no echo
      cancellation, so the mic hears its own voice) and the backend drains
      what is left — the demo now waits for quiet and says it again if it
      was missed. And the verdict read the coloured log for
      `tool=robot.go_to`, which the escape codes sit inside: a successful
      journey was reported as a refusal.

- [x] And the whole conversation, end to end (2026-09-12, after the
      model was re-downloaded — the Mac had been cleared and Ollama's
      weights with it, which turned one run into three `404`s):

      > — Dove ti trovi adesso?
      > — Sono nel salotto.                     (`robot.where_am_i`)
      > — Portami in cucina.
      > — Sono in camino per la cucina.         (`robot.go_to {"place":"cucina"}`)
      > — Ci sei arrivata in cucina?
      > — Sì, sono arrivata in cucina!          (`robot.where_am_i`)

      Three metres walked in about 110 s, stopping **0.42 m** from the
      anchor it had been taught — inside the place's 1.5 m radius, so the
      last answer is not a boast but a reading. Both answers came after a
      tool call: the duck looked before it spoke.
      Two things to keep honest about it. The Italian is an 8B model's
      Italian ("Il mappatura", "Sono in camino"), and the promise of
      "circa 30 secondi" was invented — the duck has no idea how long a
      journey takes and nothing in the catalogue tells it. And it still
      only reports when asked: the arrival is knowable, not announced.

- [x] What the furniture is worth, and what it is not (2026-09-13). A
      bare copy of flat A — same walls, same doorways, same floors, not a
      stick of furniture, 100 % reachable — as a control, so the duck's own
      wandering could be told apart from the world's obstruction. Twelve
      journeys there against nine in the furnished flat, same code:

      | | furnished | bare |
      |---|---|---|
      | median journey | 99 s | **80 s** |
      | speed made good | 0.031 m/s | **0.040 m/s** |
      | **walked per metre made good** | **2.06** | **2.13** |
      | worst journey | 181 s | 131 s |
      | journeys past 150 s | 2 of 9 | **0 of 12** |
      | stalls over the run | 13 | **16** |
      | refusals | 72 | 33 |

      Emptying the house makes the duck a quarter faster and takes away the
      disasters — but **it does not make it wander less**, and it does not
      stop it stalling. Two numbers settle two questions:
      **The wandering is ours.** 2.13 m walked per metre made good with
      nothing in the way at all, against 2.06 with a flat full of
      furniture. Whatever draws those loops, it is not the world.
      **And a stall is mostly the gait, not a bump.** Sixteen of them in an
      empty flat against thirteen in a furnished one. The earlier finding
      that three quarters of stalls happen within 35 cm of furniture was
      true and misleading: flat A has furniture nearly everywhere, and the
      duck stalls just as often where there is none.
      What the furniture does cost is refusals — 72 against 33 — and the
      long tail, because a stall beside a real object writes a phantom into
      the books and that is what sends a journey the long way round.
      So: the bare flat is the bench for follower work, where the noise is
      one source quieter; the furnished flat stays the house the duck is
      judged in. And recognising objects, when it comes, will not be what
      makes the duck walk straight — it does not walk straight in a room
      with nothing in it.

- [x] The look-ahead stretched, and the ceiling found (2026-09-13).
      Aiming a metre along the path instead of 0.4 m was measured over
      twelve journeys in the empty flat against twenty: median 86 s
      against 80, 0.036 m/s made good against 0.040, 2.44 m walked per
      metre against 2.11 (one-sided p = 0.84 — no gain anywhere). A far
      aim holds the heading steady but points across the corners the grid
      path goes round, and the leg that follows it comes back. Reverted to
      0.4; `QK_GOAL_LOOKAHEAD_M` keeps it measurable. That is the third
      rebuild of the follower in a row that changed nothing — aim held,
      turn made proportional, look-ahead stretched — so the next question
      had to be what the body can do at all.
      **How straight can this duck walk?** `private/drives/straightline.py`
      drives the body open-loop, no map and no aim, ten seconds at a time
      with room ahead, and reads the twin's truth:

      | | path over chord | off the line | yaw drift |
      |---|---|---|---|
      | vyaw 0, no trim | 1.15 · 1.24 · 1.46 | 0.35–0.42 m | +0.7° · **−47.9°** · +31.1° |
      | with the 0.08 trim | 1.23 · 1.24 · 1.24 | 0.04–0.15 m | −7.7° · +4.5° · −5.4° |

      Two things fall out. **The trim is not a nicety**: without it the
      duck swings thirty to fifty degrees in a metre and ends 40 cm off
      the line it was told to walk — no follower can hold a line on a body
      like that, and the trim was measured once, from a human drive, on a
      twin. It will have to be measured again on the real duck, and the
      duck should probably measure it itself.
      **And the gait's own floor is 1.24.** Told to walk straight, with
      the trim on and nothing in the way, the duck still covers a quarter
      more ground than the chord — the waddle is a curve. So a journey can
      never do better than about 0.097 m/s with this gait, and the 2.11 m
      per metre it walks today is 1.24 of gait and 1.70 of route and
      turning on top of it. **The body is not what is slow.**

- [x] The gait's turn curve, measured and inverted — and the journeys do
      not care (2026-09-13). `private/drives/yawcurve.py` sweeps the yaw
      request and reads the twin's truth, three trials a point, each one
      thrown away if the body was not actually walking (the first sweep
      measured a duck wedged in a corner: 0.003 m/s forward and "13 % of
      the asked rate", which is a wall, not a gait — the user spotted it
      in the viewer).

      | asked | delivered, raw gait | with quacksat's 0.08 trim | with the trim and the gains |
      |---|---|---|---|
      | left | 65 % | 82 % | **95 %** |
      | right | 80 % | 62 % | **101 %** |

      Two gait facts fall out. **The body delivers about 70 % of a turn
      request**, flat across the range, so every correction the follower
      computes arrives short. **And it is asymmetric**: left and right
      differ by fifteen points, and the trim — which exists to stop the
      duck veering right when told to go straight — swaps which side is
      weak. `GaitConfig` has carried `yaw_gain_left`/`yaw_gain_right` since
      the beginning, documented and never filled in; 1.34 and 1.58 make
      the body deliver what it is asked, verified by the same sweep. The
      corrected command is now clamped at 0.9 rad/s, past which the curve
      is flat and only the walk slows (0.124 m/s at 0.15, 0.101 at 0.9).
      **And the journeys do not move.** Eight journeys with the gains
      against twenty without, in the empty flat: 84 s against 80, 0.037 m/s
      made good against 0.040, 2.15 m walked per metre against 2.11
      (p = 0.73). The heading error at the start of a leg is still 30° at
      the median. The corrections now arrive in full, and the error is
      re-created anyway — so it was never the delivery.
      The gains stay: a body that does what it is told is worth having
      whatever the journey time says, and on the real duck this curve will
      have to be measured again anyway (it is the first December job).

- [ ] **Where the wandering actually lives**, now that the follower has
      been rebuilt four times for nothing (aim held, turn made
      proportional, look-ahead stretched, turn gains calibrated — every one
      of them null). The walked distance decomposes, in the empty flat:

      | | factor |
      |---|---|
      | the house itself: shortest walk the walls allow | 1.19 |
      | the planner's route against that best | 1.20 |
      | the gait's own waddle, measured open-loop | 1.24 |
      | what is left for the follower | 1.19 |
      | **measured, walked per metre made good** | **2.11** |

      The follower's share is the smallest of the three that can be
      changed, and four attempts at it have moved nothing. **The route is
      the bigger slack**: the planner hands the duck 1.43× the straight
      line where the house allows 1.19×, and that 20 % is grid staircases
      and the holes in a map built by a duck that stops to look. That is
      where to go next — not another controller.

- [x] Unknown floor at a lower price: null (2026-09-13). The planner has
      always crossed unknown cells at three times the cost of known floor
      (`COST_UNKNOWN` 30 against `COST_FREE` 10), which on a house mapped
      through a 45° wedge is enough to send the duck a long way round a gap
      it has simply never looked at. At 13 instead, over eight journeys in
      the empty flat against twenty-eight: mean speed made good 0.0434 m/s
      against 0.0430, and **a cheap-unknown journey beats a dear-unknown
      one in exactly 50 of 100 pairings**. The medians flatter it (74 s
      against 84, 1.88 m walked per metre against 2.14) and the pairings
      say that is the small sample talking. The route handed to the duck at
      the start barely moved either: 1.43x against 1.46x.
      Left at 30; `QK_COST_UNKNOWN` keeps it measurable.

- [ ] **What maploc ships that we have never used** (scanned 2026-09-13).
      The daemon's own surface is small — `enabled`, `mode`, `map_path`,
      `wipe_on_boot`, `search_sweep`, `record_dir` — and only `mode` is at
      an untried value (`stop_and_scan`; `continuous` integrates while
      walking, and is the next experiment). `search_sweep` is already on,
      which is what makes a stop a ~150° composite instead of whichever 45°
      wedge the head faces.
      Three modules inside the crate are wired to nothing:
      **`planner.rs`** — the same A*, but with a greedy line-of-sight
      simplification inside the planner, "so the duck doesn't get a noisy
      zig-zag from the 8-connected grid expansion". Ours never smooths the
      path; it smooths the aim, downstream.
      **`follower.rs`** — turn-then-go with hysteresis: enter "go" below
      0.25 rad of heading error, leave above 0.45, forward speed scaled by
      `cos(yaw_err)`. Its comment says why: *"a bipedal gait oscillates yaw
      every step; a single threshold made forward motion stutter on/off
      around it."* That is the weave this to-do has spent a day on, named
      and solved by the people who wrote the gait. It cannot be adopted
      literally — it commands `vx = 0` while turning, and this gait does
      not turn from a standstill (0.2°/s, measured) — but the hysteresis
      and the cosine are exactly what our always-arc follower lacks.
      **`mcl.rs`** — a particle filter that relocalizes against a saved
      grid, narrowing a cloud of poses over a few seconds. Our branch runs
      the brute-force `relocalize.rs` instead, and the homecoming explores
      for minutes before it can ask. Worth trying at boot.

- [x] Continuous mapping, all the way down (2026-09-13). The mode that
      inks every frame while walking, tried because the user asked why a
      duck that travels never fills the unknowns in — in stop-and-scan it
      cannot, since the map only takes what it sees standing still.

      | continuous, in the empty flat | n | median | m/s mean | walked/m | beats plain | maps, % wall beyond 10 cm |
      |---|---|---|---|---|---|---|
      | as shipped, stands kept | 25 | 71 s | 0.0430 | 2.09 | 59/100 | 1.1 · 1.3 · 2.5 · 16.6 · 6.9 · 5.7 |
      | stands removed (`QK_MAP_STAND_S=0`) | — | — | — | — | — | **position lost after two minutes** |
      | corrected while walking (`continuous_correct_s`) | 4 | 64 s | 0.0485 | 1.92 | 57/100 | **12.0 · 23.2** |

      **What it does well.** Journeys add map: +2335 and +4494 known cells
      per five journeys, where stop-and-scan adds none. Stalls fall (9
      against 16), the route shortens (1.41x against 1.46x), and it wins
      59 of 100 pairings on speed — the only edge over plain
      stop-and-scan any mode has shown.
      **What is wrong with it, read from `mapper.rs`.** `frame()` inks and
      returns; the tracking correction lives inside the still-window path
      and is never called. Continuous is odometry plus loop closures for
      as long as the robot walks. Its drift is 17.7 cm at the median in
      the worst run against 9 for stop-and-scan, 32 cm at the peak, and
      that run's map is 16.6 % misplaced. Take the stands away — which in
      this mode ink nothing — and the robot's own watchdog declares the
      position lost after two minutes: the stand was slowing the bleed,
      not stopping it.
      **The fix we tried, and why it made things worse.** A rolling window
      — the frames of the last second, composed at their own poses, no
      vote (a vote empties a moving window: the first attempt corrected
      the pose zero times) — matched against the map every second with
      the same bars as a still window. It fires: 97 corrections of 771
      attempts in a run, residuals 0.069 → 0.015. And the maps are the
      worst of the day, 12.0 % and 23.2 %, one run's drift peaking at
      51 cm. Continuous inks every frame at once, so a correction toward a
      wrong patch of map is inked before anything can refute it — the
      positive feedback the absolute bar exists to stop, with no window
      left to stop it. Knob kept at zero. A correction that could work
      here would hold the ink until the pose is confirmed, which is a
      different mapper and an upstream design question.
      **Verdict: off.** `MAPLOC_MODE` stays `stop_and_scan`.
      **A retraction.** The "continuous with the head sweep" arm (sweep1,
      sweep2 — 44/100, maps 6.9 and 5.7 %) never had the sweep. The twin
      launcher copies `target/debug/robotd` and the evening's builds were
      `--release`; the binary in use was two days old. So that arm is an
      accidental repeat of plain continuous, and its numbers stand in the
      table above as such. The sweep in continuous is untested. (The
      launcher's binary is now checked with `strings` after every build;
      the trap is in memory.)
      **And the head never pans in continuous** — `search_sweep` is gated
      on stop-and-scan in robotd's loop — which is why that mode maps more
      cells and covers less wall surface (41 % against 47 %). The patch
      that lets it sweep in both modes, and narrower while walking, is in
      the fork, unmeasured.

- [x] The lane sampled whole: the first follower change that paid
      (2026-09-14). `lane_clear` used to check three rails — the centre
      and the two edges of the body — every half cell along the heading;
      with 5 cm cells and a 16 cm half-width that left 8 cm on each side no
      rail touched, and a wall cell there, the width of a table leg or a
      door post, passed as clear. It now samples every half cell across as
      well. Pinned by a test that puts one wall cell in that gap and shows
      the rails missing it; an adversarial review (four lenses, every
      finding re-read by a refuter) found no runtime defect.
      Measured where the bumps live, the furnished flat, arms interleaved
      run for run:

      | in the furnished flat | n | median | m/s mean | walked/m | stalls per 100 legs | at furniture | space refusals | turns in place | **beats rails** |
      |---|---|---|---|---|---|---|---|---|---|
      | three rails | 8 | 118 s | 0.029 | 2.58 | 7.1 | 8 | 2 | 23 | — |
      | **whole width** | 9 | **78 s** | **0.039** | **2.21** | **4.8** | 6 | **0** | **13** | **74/100** |

      A third fewer stalls, the turns in place nearly halved, no refusal
      for want of room, and 74 of 100 pairings — where four follower
      changes before it won 45–49. Mechanism, as read: a leg that used to
      start toward a post the map had but the rails did not see now starts
      elsewhere, so the stall, the back-off and the turn that followed it
      never happen. (Two earlier full-width series were void — every goal
      refused with "already on its way" — because the bench's own
      reachability probe left its go_to running; it waits now.)
      `QK_LANE_RAILS=3` keeps the rails measurable. Unmeasured: the same
      change in the empty flat, where there is little to bump.

- [x] Commit to the aim — turn-then-go with hysteresis, adapted from
      maploc's unused `follower.rs` — measured and refuted (2026-09-14). A
      judge panel (three independent designs, three judges) chose it and
      wrote its predictions down first; the twin then refused nearly all of
      them. Empty flat, four series an arm, interleaved:

      | | old follower | commit to the aim | predicted |
      |---|---|---|---|
      | journeys | 13 | 17 | |
      | median | 67 s | 90 s | ≤ 72 s |
      | m/s mean | 0.046 | 0.034 | ≥ 0.048 |
      | walked per metre | 1.80 | 2.51 | ≤ 1.92 |
      | stalls | 14 | 33 | |
      | turns in place | 6 | 18 | unchanged |
      | "go" legs | — | 28 % | ≥ 60 % |
      | heading error, all legs | 32° | 35° | ≤ 22° |
      | sign flips between "go" legs | — | 44 % | ≤ 15 % |
      | **beats the old** | — | **28/100** | ≥ 60 |

      Why, as read from the numbers: the entry gate is the deadband, 0.25
      rad, and a body that begins every plan 30° off almost never clears
      it — so the duck lived in the turning legs, which are the timed
      curves and arcs, and turned in place three times as often. The old
      follower's wider straight band (0.35 rad) and its willingness to walk
      while still a little off were doing more than they looked.
      Kept behind `QK_COMMIT=1`, off. What survives of the panel's work is
      the observation that the held aim of 2026-09-12 was never held (a
      path point 0.15 m further on turns up every 0.18 m walked), which is
      recorded with that entry.

- [x] MCL at boot: wired, benched, off (2026-09-14). `maploc/src/mcl.rs`
      — the particle filter its own header says "the runtime wires under
      pending_relocalize" — was wired to nothing. Two designs came out of
      a read-and-design workflow (three readers, two designers, one
      refuter each); both were refuted on details and both refuters
      confirmed the same facts: no motion gate of its own, a wall
      threshold that did not match the mapper's (200 against 150), and a
      likelihood that scored beams into *unmapped* cells as if the map
      were complete — which drags the cloud toward whatever is mapped.
      Built the smaller one, inside the mapper: on a resumed map the
      filter is seeded (a fifth around the saved pose), fed every frame and
      every odometry tick while lost — walking or standing, which the
      still-window search cannot use — and when it locks and the body has
      swept 0.8 rad and moved 0.10 m, its pose goes into `pending_reloc`
      like any brute-force candidate, for the next still window to judge.
      Unmapped cells now score a flat 0.20 and are left out of the lock
      residual. `MAPLOC_MCL=1`; `_N`, `_YAW`, `_TRAVEL`, `_RESID` to sweep.
      On the replay bench, two recordings booted on the saved map of run 71:

      | recording | search | relocalized at | right? (vs truth, next 30 s) | lost again | final vs truth |
      |---|---|---|---|---|---|
      | 1788872069 | brute force | 252.6 s | **yes** — 0.01 | — | 0.036 |
      | 1788872069 | **MCL** | **35.9 s** | **no** — 0.36 · 0.29 · 0.25, yaw 58° off | at 54.7 s; brute force fixed it at 240.6 s | 0.044 |
      | 1788929139 | brute force | 220.4 s | so-so — 0.20 · 0.16 | — | 0.357 |
      | 1788929139 | **MCL** | **38.6 s** | so-so — 0.12 · 0.18 · 0.12 · 0.20 | — | **0.118** |

      Six times faster to a verdict, and one verdict in two was wrong: a
      lock with the yaw 58° off that the still window confirmed (residual
      0.036, under the bar) and the watchdog caught nineteen seconds later.
      The gates do not help — the sweep over yaw 0.8/1.5 rad, travel
      0.10/0.30 m and lock residual 0.05/0.08 changed nothing, because by
      36 s the body has cleared all of them and the lock had been waiting.
      The alias is the filter's, and the fix is the one both refuters
      named: a uniqueness test before proposing — score the locked pose
      and the best rival basin with the brute-force matcher and propose
      only if the rival is worse by the same 0.6 the brute force demands
      of itself. Not built; off until it is. (The earlier boot bench,
      onmap-1788872069.txt, relocalized at 24.1 s with the code of that
      day; today's brute force takes 252 s on the same recording — the
      tracking bar of 2026-09-12 refuses what that day accepted. Noted,
      not chased.)

- [x] MCL at boot, with the uniqueness test: never wrong on the bench,
      once better, no longer six times faster (2026-09-14). Before the
      filter's lock is proposed, the locked pose is scored on the last
      still-window composite and the brute-force search is asked for the
      best basin elsewhere (farther than 0.40 m or 35°); the lock is
      proposed only if it beats that rival by the ratio the brute force
      demands of itself (`uniqueness_ratio` 0.6) with enough judged beams,
      else the cloud is re-seeded half around it and moves on.

      | recording | search | relocalized at | right? | final vs truth |
      |---|---|---|---|---|
      | 1788872069 | brute force | 252.6 s | yes (0.01) | 0.036 |
      | 1788872069 | MCL, no test | 35.9 s | **no** (0.36), lost at 54.7 s | 0.044 |
      | 1788872069 | **MCL + uniqueness** | 252.6 s | yes (0.01) — the alias refused, the brute force's own verdict | 0.036 |
      | 1788929139 | brute force | 220.4 s | so-so (0.20 · 0.16) | 0.357 |
      | 1788929139 | MCL, no test | 38.6 s | so-so (0.12 · 0.18) | 0.118 |
      | 1788929139 | **MCL + uniqueness** | **174.4 s** | **yes (0.06 · 0.03 · 0.07)** | **0.013** |

      The test does what it was built for: the 58°-off lock on the first
      recording never reaches the window, and on the second the filter's
      proposal at 174 s is the right one where the brute force's at 220 s
      was a quarter metre out — the run ends 0.013 m from the truth
      against 0.357. What is gone is the speed: a lock that passes the
      test needs a composite the brute force can judge, and those come
      only from still windows, so the filter can no longer beat the
      windows to a verdict by minutes; it beats them to the *right*
      verdict. Two recordings are two; on this evidence it is safe and
      sometimes better, and it stays behind `MAPLOC_MCL=1` until the twin
      has booted on it a dozen times.

- [x] The uniqueness test, corrected by review: right on both, three
      times faster than the brute force (2026-09-14). The adversarial
      review of the first gate got one lens through before the session
      limit, and that lens found three faults that held on the code: the
      lock was judged at the filter's *current* pose against a composite
      measured at the last stand, without carrying it back; `own` and the
      rival were scored with different metrics (`score_pose`, observed
      endpoints only, against `score_offsets`, every beam), so the ratio
      was not the brute force's test; and a refused lock was re-seeded
      around the refused pose, where it re-locked in 25 frames and met the
      same window again. All three fixed: the lock is carried back to the
      window's pose and proposed as a candidate at that window; `own` is
      the basin the brute-force search itself finds at the lock and the
      rival the best basin elsewhere, same numbers on both sides; a
      refusal seeds the cloud on the rival basins and restarts the motion
      gates; one judgement per window; a thin window is no verdict.

      | recording | brute force | first gate | **corrected gate** |
      |---|---|---|---|
      | 1788872069 | 252.6 s, right, final 0.036 | 252.6 s, right | **73.3 s, right (0.01–0.05), final 0.009** |
      | 1788929139 | 220.4 s, a quarter metre out, final 0.357 | 174.4 s, right | **72.6 s, right (0.03–0.05), final 0.020** |

      No alias, no loss, and a verdict in 73 s where the brute force
      takes 220–253. Still two recordings and still `MAPLOC_MCL=1`; the
      twin boot test — a dozen wake-ups on the saved map, with
      `duckwatch.py` reading the moment it says home — is what would make
      it the default.

- [x] Twelve wake-ups on the saved map, and what the twin said
      (2026-09-14). Six spawns in six rooms of the furnished flat, each
      once with the particle filter and once without; the homecoming
      stands and turns for up to 240 s, then wipes and explores; 420 s in
      all. Judged by the duck's belief against the truth at the moment it
      says home. (The turn had to be fixed first: `vyaw` alone does not
      turn this gait, and a four-second stand never let a window close —
      see the homecoming commit. The first wake-up after the fix came home
      in 126 s at 0.15 m.)

      | spawn | without the filter | with the filter |
      |---|---|---|
      | kitchen | 117 s, **4.89 m, 174°** | 132 s, 0.10 m, 2° |
      | living room | 6 s, **2.17 m, 180°** | 6 s, **2.17 m, 180°** |
      | corridor | 111 s, 0.05 m, 1° | 111 s, 0.13 m, 4° |
      | office | 138 s, **1.76 m, 179°** | never |
      | bedroom | 12 s, **3.85 m, 138°** | 89 s, **5.90 m, 89°** |
      | bathroom | never | never |
      | **right / wrong / never** | **1 / 4 / 1** | **2 / 2 / 2** |

      **Six of nine confirmations are wrong.** The wake-up path confirms
      mirror images, and in both arms, because the filter's proposals were
      refused every time by the uniqueness test (5 of 6, 11 of 11, 21 of
      21 locks) and the confirmations came from the still-window search
      that runs regardless. Two kinds of wrong: the flat's repeated
      rectangles confirmed 180° out after a minute or two of turning (the
      kitchen, the office); and the *saved pose* confirmed in six seconds
      when the duck had been carried to another room (the living room, the
      bedroom) — one window that agrees with the map at the pose the
      session ended at, and a duck switched on across the flat believes it
      is where it was switched off.
      So the boot search neither helps nor hurts on this evidence — it
      proposed nothing wrong, and nothing right — and the fault it was
      wired to cure is deeper than the filter: the still-window
      confirmation itself accepts aliases at boot, the soft seed most of
      all. The uniqueness test that now gates the filter's lock is what
      those confirmations lack, and applying it to them is the next thing
      to try. Until then the homecoming is not to be trusted after a carry,
      which is the case it exists for. `MAPLOC_MCL` stays opt-in.

- [x] Twelve wake-ups, third round: the six-second aliases are gone, the
      filter's are not (2026-09-14). Three fixes to the wake-up went in
      first, each found by a wake-up that failed: the turn is a kick (vyaw
      alone does not turn this gait), the stand is six seconds (four never
      let a window close), and a refused turn backs away instead of being
      retried against a bed. And the "home in six seconds" of the earlier
      round was not an alias at all: the homecoming read the map frame in
      the same second it loaded the map, and that frame — from the map
      just thrown away — still said `tracking`. It now believes only a
      frame newer than the one it started with.

      | spawn | brute force | particle filter |
      |---|---|---|
      | kitchen | 195 s, **4.94 m** | 48 s, **3.65 m, 91°** |
      | living room | 114 s, 0.08 m, 3° ✓ | 66 s, 0.38 m, **168°** |
      | corridor | 111 s, 0.09 m, 1° ✓ | 33 s, **3.11 m, 172°** |
      | office | 153 s, **1.71 m** | 132 s, **4.66 m, 179°** |
      | bedroom | fell at 3 s (the twin's fall at enable) | 180 s, 0.32 m, 0° ✓ |
      | bathroom | never | never |
      | **right / wrong / never** | **2 / 2 / 1** | **1 / 4 / 1** |

      With the filter the wake-up is faster and wronger: four aliases in
      five, at 33–132 s, nearly all 180° out. In the first round its locks
      were refused every time; now, re-seeded on the rivals and with the
      gates restarted, it keeps trying until a window comes along on which
      the alias *is* the unique fit — in a kitchen of rectangles that
      window exists for the mirror image too — and the next window,
      turned in place a metre away, agrees. The two-recording bench had
      said the opposite because on it the duck was exploring: its
      windows came from metres apart, and that is what tells an alias
      from the truth. **Uniqueness on one window is not evidence; travel
      between windows is.** The multi-hypothesis path already says so
      (two windows and 1.5 m), and a wake-up that turns in place never
      reaches it.
      `MAPLOC_MCL` stays off. The next wake-up change is not another gate:
      it is to make the duck walk — a few turns, then short guarded legs
      — and to refuse any confirmation at boot without half a metre of
      travel between the window that nominated a pose and the one that
      confirms it.

- [x] Why the bathroom "never" comes home, and the office comes home
      wrong (2026-09-14, the user's guess: the bathroom was never mapped).
      Right. On the saved map the wake-ups were judged against, 92% of
      the bathroom's floor cells are still unknown and 70% of the
      office's; the kitchen, living room and corridor are mapped through.
      A duck that wakes in the bathroom sees walls the map has never
      drawn, so no basin fits and the search runs out its four minutes;
      one that wakes in the office sees a room drawn by a third, and the
      best fit is somewhere else. Two "never/wrong" rows of the tables
      above are therefore coverage, not recognition, and the fix is in
      the tour, not the matcher: map the rooms before asking to be
      recognised in them. The wake-up series stays as is (the four other
      rooms are the evidence), and the next map for a wake-up bench is
      taken after a tour that enters every room.

- [ ] velstand, main's walk since policy set v5, measured against alpha
      (2026-09-14, after the merge; the user's rule for the day: gait
      probes in a big empty room, or the probe measures the furniture).
      Upstream's `velstand.onnx` is one network that walks on a twist and
      stands still at zero command (`stand = "none"`); alpha is the pair
      every number so far came from. The twin launcher takes
      `GAIT=alpha|velstand`; the policy sets are parked in `private/`
      since main no longer ships them. A new scene, `arena.xml` — one
      empty 12 × 12 m room — replaces the flats for calibration: in the
      flat the sweeps had been throwing trials away against walls, in the
      arena every trial was kept.

      | arena, open loop | alpha (trim 0.08) | velstand (raw) |
      |---|---|---|
      | straight, 10 s: walked | 1.50 m (0.150 m/s) | 1.29 m (0.129 m/s) |
      | path over chord | 1.23 | 1.23–1.34 |
      | yaw drift in 10 s | −20° … +17° | −42° … −78° (right) |
      | bias at zero request | ≈ −0.05 rad/s | **−0.13 rad/s** |
      | raw gain left / right | — | 0.66 / 0.46 |
      | turn in place from a standstill | 1.0° / 1.5° in 5 s | 0.6° / 0.5° in 5 s |
      | after a 1 s kick, vx 0 vyaw ±0.7 | +103° / −160° in 5 s | +99° / −148° in 5 s |

      So the gait laws survive the new network: no turning from a
      standstill, kick then spin, a right veer to trim out — only more of
      it. velstand's own calibration, fitted from the raw curve and
      corrected once: **trim 0.16, gains 1.63 left / 1.58 right**, which
      the sweep then returns at 104 % / 102 % of what is asked, every
      trial kept (`private/drives/quacksat-velstand.toml`). Alpha's, for
      the record in the same arena: 98 % / 98 %.
      One thing the calibration turned up for both gaits: **the 0.9 yaw
      clamp is not the ceiling.** Alpha's raw curve went flat at 0.9
      asked, but with its gains on it climbs on (asked 0.9 → 0.81 / 1.00
      achieved), and velstand reaches 0.9 rad/s achieved at 1.63 sent —
      at the price of forward speed (0.08 m/s at the top). The clamp is
      now `[gait] yaw_max` (default 0.9, unchanged for alpha; 1.7 in the
      velstand file). Whether the journeys want the extra turn is a
      separate measurement.
      **The first series was void, and the reason is upstream's:** on
      velstand maploc never saw the duck still — `still=false moving=true
      window_frames=0` through the whole lap, the explorer finished with
      "0 windows" and no cell inked. robotd's `moving` is `busy || label
      == "walk"`, and with no standing network the label is `walk` while
      standing. Fixed in the worktree (`Step::walking`: the label when a
      standing network exists, the standing threshold when none does —
      `7329594`); the same flag feeds `safeToRestart`, which would have
      told the updater "walking" forever. Written up in
      `docs/study/upstream-asks.md` §6a. After the fix a standing sweep
      closes windows again (`kept=171 windows=4` in ten seconds).
      **Journeys, and the verdict: velstand stays off** (`vel1 → alp1 →
      vel2 → alp2`, then `vel3`, furnished flat, interleaved):

      | | alpha (alp1, alp2) | velstand (vel1, vel2, vel3) |
      |---|---|---|
      | series completed | 2 of 2 | **1 of 3** — vel2 and vel3 fell during a turn in the first minutes and never got up |
      | journey time, median | 94 s | 113 s |
      | made good | 0.033 m/s | 0.031 m/s |
      | walked / straight | 2.62 | 1.95 |
      | stalls (8 / 4 journeys) | 17 | 3 |
      | route cost | 1.46× | 1.52× |
      | pose drift, median / max | 0.12 / 0.29 m · 0.06 / 0.20 m | 0.05 / 0.12 m |
      | walls within 10 cm | 39 % · 43 % | 35 % |
      | **beats the baseline** | — | **50 / 100** |

      On the one series that finished, velstand walks straighter and
      stalls less, and is slower for it — a coin, 50/100, as the arena
      numbers predicted (0.129 against 0.150 m/s straight, less to gain
      from a follower that already wanders 1.95 for 1). What settles it is
      the other two series: **with `stand = "none"` there is nothing to
      get the duck up.** Alpha's standing network is what rights a fallen
      twin (the explorer's "did not get up" is a wait for exactly that);
      velstand has no such network, upstream's `limp_fall` hands back to a
      standing network and so is off by default for it, and a fallen
      velstand duck lies where it fell — the final frame says `seated:
      true` and every journey is refused. Two of three series lost that
      way is not a gait that can be left alone in a house.
      Kept: the `GAIT` arm on the launcher, the velstand calibration
      file, the arena, the `yaw_max` knob (default unchanged), and the
      robotd fix — that one is right whichever network walks. Alpha
      remains the gait; revisit when upstream gives velstand a way up
      (a stand-up skill, or `limp_fall` with a standing target).

- [ ] Walk before believing (2026-09-14, evening). Three pieces, then a
      round of six wake-ups against round three.
      **The mapper** (`9e54857`): a hypothesis' travel is the chord from
      where it was first seen, not the path — a duck turning on the spot
      drifts a decimetre a kick, twelve kicks "walked" 1.5 m inside a
      30 cm circle, and that is how the mirror image got confirmed. At
      boot a candidate is confirmed only after 0.5 m between nomination
      and confirmation (`MAPLOC_CONFIRM_TRAVEL`), pending meanwhile. Test
      in a room with a shelf: sixteen kicks never confirm, legs across the
      room come home at the truth.
      **The homecoming** (`f8afb08`): three kicks of a turn, then two 3 s
      legs per turn, all under `map_step`'s guards; a refused step backs
      away, then turns. The sensor guard works before the map vouches
      (it needed a tracked pose; on a saved map there is none).
      **And the find of the day** (`2d797ba`): the first walking wake-up
      had every leg refused by "something 0.37 m ahead" — in the empty
      house too, every heading. A live ToF frame projected with the
      kinematics put every near-row floor return 7–20 cm *above* the
      floor. `robot.state.head` is the *commanded* head (the sweep's
      offsets), and the cliff watch had been projecting with it since it
      was written; the walking policy holds neck and head pitched down
      0.23 / 0.46 rad. So the floor was an obstacle 0.37 m ahead at the
      sweep's bearing. The explorer never tripped on it only because its
      legs are ≤1.5 s (needed ≈0.35 m < 0.37) — which is also why they
      are 1.5 s — and the drop detector's expected floor carried the same
      tilt. The head now comes from the measured `joints`. After the fix:
      nearest obstacle 1.37 m (the real wall), a 3 s leg accepted.
      **Every explorer number before this carries that phantom**: the
      journeys, the leg length, the type-A "phantom obstacle" — to be
      re-measured before any old verdict is trusted.

      | spawn | round 3 (turning on the spot) | round 4 (walking) |
      |---|---|---|
      | kitchen | 195 s, **4.94 m** ✗ | 195 s, 0.21 m, 1° ✓ |
      | living room | 114 s, 0.08 m ✓ | 153 s, 0.84 m, 33° ~ |
      | corridor | 111 s, 0.09 m ✓ | 156 s, 0.44 m, **123°** ✗ |
      | office | 153 s, **1.71 m** ✗ | 168 s, **1.46 m, 179°** ✗ |
      | bedroom | fell | never (no candidate in 7 min, 13 of 21 legs refused by the bed) |
      | bathroom | never | never |
      | **right / wrong / never** | 2 / 2 / 1 (+1 fell) | **1 (+1 sloppy) / 2 / 2** |

      Not better, and said so. The kitchen is right now (it was the
      mirror image); the corridor became a *heading* alias — position
      nearly right, 123° out: two parallel walls look the same after
      0.5 m of leg, and the 180° twin keeps agreeing until a doorway
      comes into view. Half a metre is not evidence in a corridor; a
      change of geometry is. The rule that is missing is on the ranking,
      not the distance: the leader needs only one hit more than the
      runner-up, and one is noise. So a lead margin (`MAPLOC_HYP_LEAD`,
      3, `f763a3a`) and round five with the hypotheses traced:

      | spawn | round 3 | round 4 | round 5 (lead 3) |
      |---|---|---|---|
      | kitchen | 4.94 m ✗ | 0.21 m ✓ | 0.20 m, heading 157° (?) · rerun: never |
      | living room | 0.08 m ✓ | 0.84 m ~ | 0.82 m ~ (same offset, same spot) |
      | corridor | 0.09 m ✓ | 123° ✗ | never · rerun with turn-away: never |
      | office | 1.71 m ✗ | 179° ✗ | 170° ✗ |
      | bedroom | fell | never | never |
      | bathroom | never | never | never |

      What the traces say, and it is not what the lead margin was for:
      1. **Hits are not evidence either.** In the corridor the leader had
         ×36 against ×7 — and was the 180° twin. A 7 s stand closes two
         or three windows, every one of them from the same spot, and
         they all agree with the twin as much as with the truth. Only
         the chord counts, and the chord stayed at 0.8 m in seven
         minutes.
      2. **Mobility is the ceiling.** 12–15 of ~20 legs refused per run,
         by real furniture at 0.3–0.45 m (the kitchen aisle between
         island and counter is 0.85 m wide; the corridor 1.5 m): a 3 s
         leg needs 0.7 m clear ahead, blind legs point wherever the last
         turn left them, and turning away from the named side
         (`c1efa02`) did not change the count. The duck moved 0.6–0.8 m
         in seven minutes where the gate wants 1.0 plus 0.5.
      3. **Coverage explains the office and the bathroom** (70 % and
         92 % unknown on the saved map; the bedroom is mapped, 15 %). A
         duck waking in an unmapped part of a room can only match the
         mapped part, and in a rectangle the mapped part seen from the
         other end *is* the mirror image — the office alias every round.
      4. The living room's 0.8 m is the same offset at the same spot
         twice, where round three came home within 8 cm a metre away:
         the saved map is probably off there itself. To measure.
      5. The kitchen's "157°" heading in round five is unverified (the
         harness printed one yaw then); the rerun with both yaws never
         came home (12 legs refused in the aisle). Open.

      **Where this leaves the wake-up.** The gates are right — nothing
      turning on the spot gets confirmed any more, and the kitchen's
      mirror image is gone — but blind legs cannot buy the metre and a
      half of travel they demand in a furnished room. The next step is
      not another gate: the homecoming should walk with the explorer's
      own legs (planned on what the sensor sees, guarded the same way,
      steering along corridors) — "explore until recognised", the flow
      the user described on day one — and the wake-up bench needs a map
      that covers every room. Kept on: chord travel (1.0 m), the
      confirmation chord (0.5 m), the lead margin (harmless, not the
      lever), turns-then-legs with back-off and turn-away.

- [x] The journeys without the phantom (2026-09-15, 00:30 — the user's
      order: re-measure exploration and go_to first, then the wake-up
      with the explorer's legs, then the honesty of "I don't know").
      `fix1`, `fix2` against `alp1`, `alp2` of the same evening — the
      same code but for the cliff watch's head (`2d797ba`):

      | | with the phantom (alp1, alp2) | without (fix1, fix2) |
      |---|---|---|
      | journey time, median | 94 s | 104 s |
      | made good | 0.033 m/s | 0.029 m/s |
      | walked / straight | 2.62 | 2.29 |
      | stalls (8 journeys) | 17 | 11 |
      | worst journey | 207 s | 234 s (three of 14–16 m in fix2) |
      | pose drift, median | 0.12 · 0.06 m | 0.06 · 0.06 m |
      | walls within 10 cm | 39 · 43 % | 42 · 36 % |
      | **beats the baseline** | — | **33 / 100** |

      Straighter and fewer stalls, but not faster, and `fix2` looped
      three times. The phantom was not what made the journeys slow; the
      sensor now tells the truth, and that stays because it is true, not
      because it paid. What it changes is the *ceiling*: legs longer than
      1.5 s are no longer refused by the floor, so the leg length can be
      measured as a knob for the first time — later, after the wake-up.
- [x] Round six: explore until recognised (2026-09-15, night). The
      flow the user described on day one was already built (`robot.map_match`
      and adoption on two agreeing asks, 2026-09-09) and the wake-up bench
      had never let it run: 240 s of boot search, then a 420 s budget that
      ended before the second ask. Now: boot search 90 s, then explore
      and ask every 120 s, budget 900 s, and the open item closed on the
      way — an **absolute bar** on the answer (`[homecoming]
      adopt_max_score` 0.16: right answers scored 0.043–0.138, wrong ones
      0.187 and up, 2026-09-09). Same six spawns, `run 71`'s map (office
      70 % and bathroom 92 % unknown), position read at adoption and
      25 s later:

      | spawn | round 5 (boot search only) | round 6 (explore and recognise) |
      |---|---|---|
      | kitchen | 0.20 m, heading ? | 405 s, **12 cm, 1°** → 9 cm, 0° at +25 s |
      | living room | 0.82 m | 363 s, **19 cm** → 17 cm, 15° |
      | corridor | never | 477 s, **8 cm** at adoption (it had walked into the bedroom) |
      | bedroom | never | 426 s, **16 cm** |
      | office (70 % unknown) | 170° ✗ | 360 s, **wrong: adopted the bedroom** (2.3 m) — see below |
      | bathroom (92 % unknown) | never | 633 s, **7 cm** → 15 cm: walked out to the living room and recognised itself there |
      | **right / wrong / never** | 1 / 1 / 3 | **5 / 1 / 0** |

      Five of six, every one within 20 cm, and the bathroom is the best
      of them: a room the map does not hold, and the duck did not
      invent it — it explored until it reached a room the map does hold.
      The office is the failure that matters, and its numbers say why:
      a fresh map of a room the saved map barely has fits the identical
      neighbour (the bedroom) at 0.091 and 0.093 — under the bar. Two
      things in the same answers separate it from every right adoption
      of the night: the runner-up's margin (0.97 and 0.84 against
      0.23–0.68) and how much the live map grew between the two asks
      (+24 % against +61–213 %). Both are now refusal bars
      (`adopt_max_margin` 0.80, `adopt_min_growth` 1.5, `e20f367`);
      refusing costs two minutes of exploring, adopting wrongly costs
      every go_to after it. (The margin was set aside on 2026-09-09
      because right answers in flat A reached 0.96 then; if it refuses
      a right one now, the cost is another ask, not a wrong home.)
      Office re-run with the bars: **no adoption in 15 min** — the first
      two asks refused on the margin (0.84), the next three on growth
      (+40 %, +9 %, +6 %: the same answer repeated is not a second look),
      and the last, with the map grown to 722 cells, already moving
      toward the truth at 0.150. "I don't know" is the right answer for
      a room the map does not hold; given more time it recognises itself
      on the way out, as the bathroom did. Round seven (all six spawns
      with the three bars) queued as the confirmation.
      Two bench faults found and fixed on the way, both worth knowing:
      the harness read the pose before the adoption had finished (the
      exploring job takes up to a minute to stop) and called a fall in
      the first three seconds, when the duck is still rising from its
      seated boot; and an orphaned robotd from a killed queue held the
      IPC socket, so five wake-ups in a row "fell" on a daemon wired to
      a dead body. `try-maploc-local.sh up` now kills our own orphans.
      Open: the reported heading disagrees with the truth by ~55° in two
      runs while the position tracks the body (rec11: 15°, rec14: 0° —
      so not systematic); to look at, not urgent, since position is
      what go_to consumes.
- [x] Round seven: the bars, confirmed and one more found (2026-09-15,
      02:00). Same six spawns, explore-and-recognise with score ≤ 0.16,
      margin ≤ 0.80, growth ≥ 1.5×:

      | spawn | round 7 |
      |---|---|
      | kitchen | 375 s, 12 cm, 2° → 9 cm, 3° |
      | living room | 417 s, 23 cm → 16 cm |
      | corridor | 375 s, 6 cm → **1 cm, 0°** |
      | bedroom | 429 s, 21 cm → 9 cm, 5° |
      | bathroom (92 % unknown) | none in 900 s — five asks refused on the margin (0.95–1.00), the sixth above the bar: **honest** |
      | office (70 % unknown) | first run void (the previous robotd was still exiting and the new one died on the lock — `up` now waits); re-run: **wrong, the bedroom again** |
      | **right / wrong / none** | **4 / 1 / 1** |

      The office's second failure is on me: an ask refused on the margin
      was still kept as the first of the pair, and the next one (0.076,
      margin 0.55, +52 %) agreed with it — a refused answer confirming
      itself. Fixed: a refusal seeds nothing (`5de7d7e`). And the office
      shows the fourth signal, which is the one that names the problem:
      **overlap**, the share of the live map's walls that land where the
      saved map has an opinion. Every right adoption of the night sat at
      0.74–0.81; the office at 0.69, 0.66 and 0.52 — most of what the
      duck had mapped lies where the saved map has never been, so
      whatever fits is the identical neighbour. `adopt_min_overlap` 0.70,
      with the office and a kitchen control re-run below.
      **With the floor:** the office says "I don't know" for the whole
      15 minutes — the bedroom proposed six times, refused six times (one
      on the margin at 0.82, five on overlap at 0.49–0.61); the kitchen
      control comes home at 513 s, **6 cm**, overlap 0.74–0.76, the growth
      bar costing it one ask. Where the wake-up stands now, on a map that
      holds four rooms of six: every mapped room recognised within
      20 cm (1–23 cm at adoption, 1–17 cm after 25 s of walking), the
      two unmapped rooms answered honestly or by walking out into a
      mapped one, and no wrong adoption survives the four bars. What
      remains is the map, not the recogniser: a tour that enters every
      room makes the bathroom and the office ordinary rooms.
- [ ] The map that holds every room (2026-09-15, morning — the user's
      order: the full map and the per-room quality first, and the
      SLAM's mathematics checked on the way).
      **The maths:** read module by module in
      `docs/study/maploc-math-review.md`. The geometry is right
      everywhere it was checked — SE(2), the matcher's Jacobians, the
      distance transform, the graph optimiser's linearisation (redone by
      hand), the loop closer's frames, our eigen-projection, the
      odometry's anchor model. What is wrong is a model, not a formula:
      the judge that confirms a candidate pose scores endpoints only and
      never asks whether a beam *crossed* a wall — the blindness every
      alias of the week lived in. A judge along the ray
      (`score_pose_rays`, `af9fed4`) exists now, with a test that fools
      the endpoint judge and not the ray one; on the two boot recordings
      it fixed one alias and made another (a true pose in a map with
      doubled walls "crosses" phantom walls too), so it stays off
      (`MAPLOC_RAY_JUDGE=1`) until it tolerates map noise. Also found:
      floor returns are thrown away (`flatten` keeps wall hits only), so
      the map never learns free floor except along wall beams — the
      "only 0.08 m of known floor" refusals come from there.
      **Coverage, per room** (share of floor cells the map has an
      opinion on):

      | map | kitchen | living | corridor | bedroom | office | bathroom |
      |---|---|---|---|---|---|---|
      | run 71 (the wake-up bench's) | 81 % | 91 % | 93 % | 85 % | 30 % | 8 % |
      | full1, explorer 30 min | 85 % | **13 %** | 95 % | 68 % | **92 %** | 6 % |

      Complementary maps; the explorer spent full1's last ten minutes
      aiming at the living-room door and never went through. Both the
      living room and the bathroom lie past the two 0.45–0.55 m lanes
      either side of the stairwell, and a tour steered on the twin's
      truth with `map_step` (`roomtour.py`) was refused 158 times there
      in eight minutes — the guard's lane and margin do not fit the
      lane. Growing a saved map by hand also does not work the naive way:
      a loaded map starts lost, and with the chord gate nothing turning on
      the spot is ever confirmed (my own gate, doing its job). So the
      map is grown the way the duck itself does it — boot on it, recognise
      it by exploring, adopt, keep exploring (`growmap.sh`).
      **Growing it, three runs:** the adoption rule needed two more
      corrections on the way — the growth bar refused five identical
      right answers on a map past its first minutes (×1.5, then ×1.2;
      now 1.0, no growth demanded, the margin and the overlap caught
      both wrong adoptions on their own) and three agreeing asks in a
      row replaced the pair (`adopt_asks`, `d2cb2a8`, `cdcc932`). Then:

      | map | kitchen | living | corridor | bedroom | office | bathroom | walls ≤10 cm |
      |---|---|---|---|---|---|---|---|
      | full1 (30 min from scratch) | 85 % | 13 % | 95 % | 68 % | 92 % | 6 % | 52 % |
      | grow1 (+12 min) | 86 % | 17 % | 95 % | 89 % | 91 % | 5 % | 57 % |
      | grow2 (+30 min) | 86 % | 19 % | 95 % | 88 % | 95 % | 7 % | — |

      The explorer finishes the room it is in before leaving it, and
      spent grow1 and grow2 finishing the bedroom and the office; the
      living room and the bathroom are the only frontiers left. grow3 in
      progress. (`full5`, a run that did not adopt and explored a fresh
      map instead, went into both by itself — the lanes are open.)
- [x] The narrow passage, as a formula (2026-09-15, the user's demand:
      "the room is there, the duck is small, by hand it passes, it has
      to pass on its own"). What refused it: the drop guard wanted 0.35 m
      of margin in a ±0.22 m lane along the *current heading*, and the
      centring between walls counted mapped walls only — the stairwell is
      never a wall on the map, so beside it nothing centred the body, it
      drifted to the edge and the edge was refused.
      **The model** (`quack-places/src/passage.rs`): a passage is two
      lateral boundaries L, R — mapped walls, drops and obstacles the
      sensor saw — sampled at the body and 0.3 m on. Width
      `W = min(L+R)`; offset from the middle `e = (L−R)/2`; skew of the
      heading against the axis `θ = ½[atan(ΔL/0.3) − atan(ΔR/0.3)]`;
      yaw `vyaw = 0.4·e/(W/2) + 0.6·θ` clamped to ±0.5; and **whether it
      fits is asked of the leg as steered**, rolled out with the measured
      gait against the boundary lines: at least `b + τ` = 0.10 + 0.05 m
      on each side, where a side the body already stands inside of must
      be left (never approached, 3 cm gained) — brushing a wall is a
      bump, brushing a drop is a fall, and the drop guard judges the same
      rolled-out leg with its own rule. In `map_step`: legs ≤ 1.5 s,
      doorway margins, the passage's yaw over the caller's, side
      boundaries remembered for a whole head sweep (frames kept 8 s), a
      wall beside the body taken to go on into the unknown, things on
      the heading line not counted as sides, refusals that name the turn.
      **Measured** (`lanetest.sh`, the 0.54 m west lane): from the north
      7 steps / 53 s then 6 / 45 s; from the south 6 / 44 s; every
      refusal left was a heading error the refusal itself named. The day
      before: 158 refusals in eight minutes, never through. And the
      explorer alone (`full5`, on a fresh map): into the living room and
      the bathroom by itself, the first time since the flat was built.
      **Dry simulations** (`legsim.py`, `explsim.py`, images in
      `private/drives/runs/legsim/`): a journey A→B with today's follower
      bumps the kitchen island at leg 7 (an arc believed to advance 3 cm/s
      advances 11) and never arrives; with every leg simulated on the
      map before walking it arrives in 14 legs, 3.98 m for 3.11, no bump.
      The exploration model says the same thing the twin then said: a
      leg simulated without the passage law is too timid (zero bumps,
      and two rooms never entered); the two go together.
      Also from the user's question "does it plan at every stop, and
      does it check where the arc ends?": yes at every stop (A* to the
      goal, aim 0.4 m along the route), and no — the arc's end is not
      checked, its advance is modelled at a quarter of the truth, and a
      refusal turns to the freer side, not to the aim. Next after the
      map: simulate every leg before walking it (as in the passage), and
      turn toward the aim.
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

- [x] **The pose was the illness after all, and the cure came from the
      trace** (2026-09-11, late — the strongest result of the twin work).
      Five drift traces, not two, and the correlation is monotone across
      every one: median drift 6.8 cm gave a map 2.4 % wrong, 9.0 gave
      6.6 %, 10.4 gave 11.1 %, 11.6 gave 11.5 %, 14.0 gave 12.2 %. The
      earlier conclusion that the pose was innocent came from two runs and
      was wrong.
      The trace then showed the mechanism rather than suggesting it. The
      tracking correction pulls the pose onto the map; **that repairs
      drift while the map is right and reinforces it once the map is
      wrong**. In the worst run the jumps upward each landed on a
      correction — 19 → 37 cm, 15 → 29, 34 → 47 — every one of them
      improving its window's residual while walking away from the truth,
      until the pose passed a metre. Ink laid at that pose then made the
      map worse still: a feedback loop.
      Turning the correction off is better on aggregate (median 4.7 % →
      3.4 %, mean 7.8 → 4.9) but ruins two recordings that the correction
      was saving — 3.0 % → 14.6 % and 1.2 % → 6.4 %. It is not a good or
      bad mechanism; it is one with no guard.
      The guard is in the numbers: the corrections that hurt each ended
      with a residual around 0.055 m, those that helped at 0.009–0.026. So
      a correction must now **also land below an absolute bar**
      (`TrackingConfig::max_residual_after_m`, two centimetres), not
      merely improve on where it started:

      | | median | mean | worst |
      |---|---|---|---|
      | as it was | 4.7 % | 7.8 % | 21.6 % |
      | correction off | 3.4 % | 4.9 % | 14.6 % |
      | **with the bar** | **1.2 %** | **2.5 %** | **10.9 %** |

      Better on six of seven recordings and on all three statistics at
      once, which none of the five earlier attempts managed; at three
      centimetres instead of two it is 2.1 / 4.8 / 16.0. The two runs that
      had been the worst come out at 0.0 % and 0.9 %. Live confirmation
      running.

- [x] The bar confirmed live (2026-09-12). Three fresh runs of flat C
      with `max_residual_after_m` at two centimetres, against the seven
      that came before it:

      | | median | mean | worst |
      |---|---|---|---|
      | before | 4.7 % | 7.8 % | 21.6 % |
      | **after** | **2.0 %** | **4.1 %** | **8.6 %** |

      The bad tail is what went: one run in three used to finish past
      12 %, and one reached 21.6 %. The pose is steadier too — median
      drift 5.9, 6.1 and 6.5 cm, never past 26 cm, where the worst run
      before had passed 127.
      One of the three (8.6 %, 2.8 % doubled) had the *best* pose of the
      lot — median 6.1 cm, never past 24 — so a good pose no longer
      guarantees a good map. That is a different fault from the one just
      cured: not a drift the median can see but something local and rare,
      a handful of windows inked at a bad moment or a closure that moved
      submaps already written. Both traces exist for that run, so it can
      be looked for rather than guessed at.

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
