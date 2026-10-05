# The knobs

Every environment variable the code reads, generated from the code by
`scripts/knobs.py` (CI checks it is current — do not edit by hand).
The `QK_*` knobs are quack-nav's, `MAPLOC_*` maploc's; the rest are the
benches' (`maploc/examples`, `quack-nav/examples`) and the twin's scripts.
The knobs once named `QUACKSAT_*` are `QK_*` since 2026-09-30.

On the duck they live in `/var/lib/quack-nav/knobs.env`, read by the unit
(`EnvironmentFile=-`) at every start. A client edits them without a shell:
`nav.knobs` on the nav socket lists quack-navd's own (machine-readable,
`quack-nav/src/knobs.json`, generated with this page) and writes the file,
`nav.restart` applies them — quack-control's page has an editor for both.
Every knob needs that restart: the environment is read at the process's start.

| Variable | Read as | Where | What the code says |
|---|---|---|---|
| `QK_ALIGN_KICK` | 1 on, 0 off, else the mode's own | `quack-nav/src/explore/mode.rs` |  |
| `QK_ANCHOR_DROP_M` | number (default 1.0) | `quack-nav/src/explore/mod.rs` | Not beside a drop: a revisit there turns the duck round in the mouth of the passage it is entering. |
| `QK_BACK_NO_STEP_M` | number (default 0.30) | `quack-nav/src/explore/gait.rs` | No blind step back at all with a booked drop this close to the body, whatever its bearing, on the trail or not (`QK_BACK_NO_STEP_M`). |
| `QK_BACK_REORIENT` | on unless 0 | `quack-nav/src/explore/gait.rs` | `QK_BACK_REORIENT=0`: after the step back, head for the most open floor and walk a leg there, as before. |
| `QK_BACK_S` | number (default 1.5) | `quack-nav/src/explore/gait.rs` | The step back: this gait needs about a second to start moving at all, so a shorter one moves nothing (measured); and no more often than this. |
| `QK_BLIND_CONE_LANE` | on unless 0 | `quack-nav/src/explore/guarded.rs` | The cone widens at short range to the lane's own angle — at 0.25 m a 12 cm lane is 26°, and a cube 0.25 m ahead at 18° inside the lane was outside the 15° cone, walked over and dragged 1.9 m (sideA016r, 2026-09-22). |
| `QK_CLIFF_MARGIN_M` | number (default 0.25) | `quack-nav/src/tools.rs` | Stop this far short of a drop's edge — farther than a wall's margin, because an edge is not a bump. |
| `QK_CLOSE_LOOK` | 1 on, 0 off, else the caller's default | `quack-nav/src/explore/close_look.rs` | `QK_CLOSE_LOOK=0`: no close looks. |
| `QK_COST_HUG` | number (default 30) | `quack-nav/src/frontier.rs` | `QK_COST_HUG=0` restores the flat price. |
| `QK_CURVE_RAD` | number (default 1.0) | `quack-nav/src/explore/gait.rs` |  |
| `QK_DROP_INFLATE` | number (default 0.05) | `quack-nav/src/frontier.rs` | `QK_DROP_INFLATE` to measure. |
| `QK_DROP_PLAN_RADIUS_M` | number (default 0.12 (guarded: 0.20)) | `quack-nav/src/explore/mode.rs` |  |
| `QK_DROP_REACH_M` | number (default 0.30) | `quack-nav/src/explore/books.rs` | `QK_DROP_REACH_M=0` goes back to points. |
| `QK_EDGE_DISCRIMINATE` | on unless 0 | `quack-nav/src/explore/books.rs` | `QK_EDGE_DISCRIMINATE=0`: every sensed drop goes on the books as a hole, as before. |
| `QK_GAP_LANE_HALF_M` | number (default 0.115) | `quack-nav/src/explore/mod.rs` |  |
| `QK_GAP_MAX_M` | number (default 0.6) | `quack-nav/src/explore/mod.rs` |  |
| `QK_GUARD_ARC_FULL` | on only if 1 | `quack-duck/src/body.rs` | Turning costs little forward speed: 0.110 m/s at vyaw 0.7 against 0.121 straight, measured on the human drive (2026-09-07). |
| `QK_INFLATE_M` | number (default 0.12) | `quack-nav/src/frontier.rs` | The inflation the planner runs with: 0.12, a little over the body's half-width, 0.10 (`QK_INFLATE_M`). |
| `QK_LANE_HALF_M` | number (default 0.16) | `quack-nav/src/explore/mod.rs` |  |
| `QK_NO_GUARDS` | 1 on, 0 off, else the caller's default | `quack-nav/src/explore/journey.rs` | `QK_NO_GUARDS`: every leg, kick and pulse goes through `robot.move`, blind, and the route check is off — the planner alone (the frozen map, the books, the margins) brings the duck home. |
| `QK_ORACLE_AS_MAPPED` | on only if 1 | `quack-nav/src/oracle.rs` | `QK_ORACLE_AS_MAPPED=1`: the truth drawn as a mapper draws a house — the holes unknown (no floor ever seen there) instead of wall, and the inside of each box of the truth's `boxes` unknown past a 5 cm band (nothing sees … |
| `QK_ORACLE_BOOK` | a value | `quack-nav/src/oracle.rs` | `QK_ORACLE_BOOK=<truth.json>`: the true holes' rims on the books at start, every 10 cm (what a ground book holds for a mapped house), for the journeys on the oracle's map (`scripts/rl/twin_ab.py`). |
| `QK_ORACLE_HOLES` | a value | `quack-nav/src/oracle.rs` | `QK_ORACLE_HOLES=<truth.json>`: its `holes` (`[x0, x1, y0, y1]`, metres) drawn into that map as wall — the planner keeps off them. |
| `QK_ORACLE_POSE` | a value | `quack-nav/src/oracle.rs` | `QK_ORACLE_POSE=<host:port>`: the pose the navigation reads is the simulator's trunk, read as `poseerr.py` reads it, at 20 Hz. |
| `QK_ORACLE_WALLS` | a value | `quack-nav/src/oracle.rs` | `QK_ORACLE_WALLS=<walls.toml>`: the map the navigation plans on is drawn from the truth's wall segments (centimetres, the MuJoCo world frame, as `scripts/twin/houses/*.toml` hold them): every cell inside the house free, … |
| `QK_PULL_DEVIATION_M` | number (default 0.20) | `quack-nav/src/frontier.rs` | A straight run may stray this far from the Dijkstra route it replaces: the staircase is smoothed, the route is not redrawn — a diagonal across the room cut corners and brushed walls Dijkstra had kept away from (the user'… |
| `QK_PULL_ROUTE` | on unless 0 | `quack-nav/src/frontier.rs` | `QK_PULL_ROUTE=0` leaves Dijkstra's staircase as it is. |
| `QK_REACH_TO_FLOOR` | on unless 0 | `quack-nav/src/explore/books.rs` | `QK_REACH_TO_FLOOR=0`: the reach behind a rim ignores where the floor comes back (see `record_drops`). |
| `QK_REANCHOR` | on unless 0 | `quack-nav/src/explore/journey.rs` | `QK_REANCHOR=0`: no trip back to mapped floor now and then to close a loop while the exploration walks new floor (see the explorer's run). |
| `QK_RIM_TOUR` | 1 on, 0 off, else the caller's default | `quack-nav/src/explore/rim_tour.rs` | `QK_RIM_TOUR=0`: no rounds of the holes. |
| `QK_RL_POLICY` | a value | `quack-nav/src/rlnav/mod.rs` | `QK_RL_POLICY`: the pilot file (docs/rl-pilot.md) that flies the stick's legs; |
| `QK_RL_TRACE` | a value | `quack-nav/src/rlnav/trace.rs` | `QK_RL_TRACE`: what the duck did and saw, leg by leg, for the calibration (`quack-rl`'s `rl_calib`, docs/rl-pilot.md). |
| `QK_SPIN_WATCH` | on unless 0 | `quack-nav/src/explore/gait.rs` | `QK_SPIN_WATCH=0`: turn in place without watching the sensor, as before. |
| `QK_STICK_CAREFUL` | 1 on, 0 off, else the caller's default | `quack-nav/src/explore/navigate.rs` |  |
| `QK_STRAIGHT_RAD` | number (default 0.35) | `quack-nav/src/explore/gait.rs` |  |
| `QK_TRAIL` | on unless 0 | `quack-nav/src/explore/books.rs` | Off with `QK_TRAIL=0`, for measuring. |
| `QK_TRUSTED_FLOOR` | 1 on, 0 off, else the mode's own | `quack-nav/src/explore/mode.rs` |  |
| `QK_TURN_IN_PLACE` | on unless 0 | `quack-nav/src/explore/gait.rs` | `QK_TURN_IN_PLACE=0` turns the old way (kick, then yaw) everywhere. |
| `QK_WALL_MARGIN_M` | number (default 0.18) | `quack-nav/src/tools.rs` |  |
| `MAPLOC_CONFIRM_TRAVEL` | number (default 0.5) | `maploc/src/mapper.rs` | At boot, how far the body must have moved between the window that nominated a candidate and the one that confirms it. |
| `MAPLOC_CONTINUOUS` | set = on (any value) | `maploc/examples/replay.rs` |  |
| `MAPLOC_GRAPH_DEBUG` | set = on (any value) | `maploc/src/pipeline.rs` |  |
| `MAPLOC_HEAD_LEAD_MS` | number (default 5) | `quack-nav/src/mapd/mod.rs`, `maploc/src/bench.rs` | `MAPLOC_HEAD_LEAD_MS`, as robotd read it: 5 ms by default — with 0 the standing drift was +0.17/+0.22°/min, with 20 and 40 it turned negative (twin, 2026-09-16). |
| `MAPLOC_HYP_LEAD` | number (default 3) | `maploc/src/mapper.rs` | How many hits the leading hypothesis must have over the runner-up before it is nominated. |
| `MAPLOC_HYP_TRAVEL` | number (default 1.0) | `maploc/src/mapper.rs` | How far the body must have got from where it first saw a hypothesis before the hypothesis can be believed — the chord, not the path. |
| `MAPLOC_LOCAL_AFTER_BOOT` | on unless 0 | `maploc/src/mapper.rs` | `MAPLOC_LOCAL_AFTER_BOOT=0`: every loss on a resumed map searches the whole map with every hypothesis, as before 2026-09-28. |
| `MAPLOC_LOOP_CAP_YAW` | number (default 0.07) | `maploc/src/loop_closer.rs` | `MAPLOC_LOOP_CAP_YAW` overrides it, for measuring on the twin. |
| `MAPLOC_LOOP_DEBUG` | on only if 1 | `maploc/src/loop_closer.rs` | `MAPLOC_LOOP_DEBUG=1`: one line per accepted closure on stderr — the submaps, the correction it asks for, the match's residual and beams, the witnesses' spread — to tell the closures that turn a map wrong. |
| `MAPLOC_MIRROR_COLS` | on only if 1 | `maploc/examples/evaluate.rs` | MAPLOC_MIRROR_COLS=1 replays every frame with its columns reversed. |
| `MAPLOC_MULTI_HYP` | on unless 0 | `maploc/src/mapper.rs` | `MAPLOC_MULTI_HYP=0` turns the multi-hypothesis boot search off, for measuring against the single-best agreement it replaces. |
| `MAPLOC_RAW` | set = on (any value) | `maploc/examples/replay.rs` |  |
| `MAPLOC_REST` | on unless 0 | `maploc/src/mapper.rs` | `MAPLOC_REST=0`: no stand ever rests, as before the rest (2026-10-01). |
| `MAPLOC_SHADOW` | on unless 0 | `maploc/src/mapper.rs` | `MAPLOC_SHADOW=0`: no shadow map at boot. |
| `MAPLOC_SHADOW_ASKS` | number (default 2) | `maploc/src/mapper.rs` | `MAPLOC_SHADOW_EVERY_S` and `MAPLOC_SHADOW_ASKS` override them. |
| `MAPLOC_SHADOW_EVERY_S` | number (default 30.0) | `maploc/src/mapper.rs` | How often the shadow asks, how many answers in a row must agree, and what an answer must be (the homecoming's adoption rule, see quack-nav's `HomecomingConfig`, measured on 27 replayed wakes: 626 of 655 right answers pas… |
| `MAPLOC_VALLEY_CROSS` | on unless 0 | `maploc/src/mapper.rs` | Two valleys that cross resolve each other (see `Mapper::valley_blocks`); `MAPLOC_VALLEY_CROSS=0` for the valley test alone. |
| `MAPLOC_VERBOSE` | set = on (any value) | `maploc/examples/replay.rs` |  |
| `MAPLOC_WATCHDOG_RESCUE` | on unless 0 | `maploc/src/mapper.rs` | `MAPLOC_WATCHDOG_RESCUE=0`: the watchdog judges a window at the carried pose alone, as before 2026-09-28. |
| `AB_REPO` | a value (script) | `scripts/twin/houses/final_house.py` |  |
| `ACC_RANGE` | number (default mapper_cfg.accumulator.max_range_m) | `maploc/examples/evaluate.rs` | `ACC_RANGE`: how far a beam may be and still reach the map. |
| `ALIGN_BEAMS` | number (unset: none) | `maploc/examples/align_maps.rs` | `ALIGN_BEAMS` and `ALIGN_RESID` override the two knobs. |
| `ALIGN_RESID` | number (unset: none) | `maploc/examples/align_maps.rs` | `ALIGN_BEAMS` and `ALIGN_RESID` override the two knobs. |
| `ASK_EVERY_S` | number (default 60.0) | `maploc/examples/wake_match.rs` | The homecoming's map-to-map question, asked offline, with the answer known: a recording replayed from `start_s` into a FRESH map — a duck woken there, its map starting where it stands — and every `ASK_EVERY_S` the fresh … |
| `ASK_FOR_S` | number (default 900.0) | `maploc/examples/wake_match.rs` | `ASK_EVERY_S` (60) and `ASK_FOR_S` (900) set the cadence and the span. |
| `BOOK` | a value | `quack-nav/examples/drop_replay.rs` | `BOOK=<ground.json>:<map>`: the book's holes too, each checked against the floor strike (explore::books' rule, replayed): which phantoms it would take off, and whether it ever takes a true rim. |
| `CORR_LOG` | a path, or a value | `maploc/examples/trajectory.rs` | `CORR_LOG=<file>`: every tracking correction a window made, against the truth — which of them moved the pose toward it and which away (casa_arredata, 2026-09-29: the windows pulled the pose 3.5-7 cm north one after the o… |
| `CUT_ON_CORR` | number (default mapper_cfg.tracking.cut_on_correction_m) | `maploc/examples/evaluate.rs` |  |
| `DEGEN_LOG` | a path, or a value | `maploc/examples/trajectory.rs` | `DEGEN_LOG=<file>`: every relocalization the search confirmed or the valley test refused, with the pose's error against the truth and the scan's conditioning there — the valley test and the Hessian's eigenvalues, side by… |
| `FROZEN` | on only if 1 | `maploc/examples/trajectory.rs` | `FROZEN=1`: the map frozen, as quack-navd's rounds run on a house already mapped — nothing inks while the pose tracks. |
| `HUBER` | number (default slam_cfg.optimizer.huber_delta) | `maploc/examples/evaluate.rs` |  |
| `LOOP_CAP` | number (default slam_cfg.loops.max_correction_cap_m) | `maploc/examples/evaluate.rs`, `maploc/examples/trajectory.rs` |  |
| `LOOP_CAP_YAW` | number (default slam_cfg.loops.max_correction_cap_rad) | `maploc/examples/evaluate.rs`, `maploc/examples/trajectory.rs` |  |
| `LOOP_GAP` | number (default slam_cfg.loops.min_index_gap as f32) | `maploc/examples/evaluate.rs` |  |
| `LOOP_LOG` | a path, or a value | `maploc/examples/trajectory.rs` | `LOOP_LOG=<file>`: every closure, what it moved the pose by and the heading's error against the truth before and after — from the nearest truth sample, so judged only while the duck stands still. |
| `LOOP_MIN_CORR` | number (default slam_cfg.loops.min_correction_m) | `maploc/examples/evaluate.rs` |  |
| `LOOP_PER_SUBMAP` | number (default slam_cfg.loops.max_correction_per_submap_m) | `maploc/examples/evaluate.rs` |  |
| `LOOP_PER_SUBMAP_YAW` | number (default slam_cfg.loops.max_correction_per_submap_rad) | `maploc/examples/evaluate.rs`, `maploc/examples/trajectory.rs` |  |
| `LOOP_RADIUS` | number (default slam_cfg.loops.radius_m) | `maploc/examples/evaluate.rs` |  |
| `LOOP_SIGMA_XY` | number (default slam_cfg.loops.edge_sigma_xy) | `maploc/examples/evaluate.rs`, `maploc/examples/trajectory.rs` |  |
| `LOOP_SIGMA_YAW` | number (default slam_cfg.loops.edge_sigma_yaw) | `maploc/examples/evaluate.rs`, `maploc/examples/trajectory.rs` |  |
| `LOOP_SPREAD` | number (default slam_cfg.loops.verify_max_spread_m) | `maploc/examples/evaluate.rs` |  |
| `LOOP_VERBOSE` | set = on (any value) | `maploc/examples/evaluate.rs` |  |
| `MAP_FALLBACK` | a value (script) | `scripts/twin/viewer/body_with_map.py` |  |
| `MAP_LOAD_AT_S` | number (unset: none) | `quack-nav/examples/drop_replay.rs`, `maploc/examples/trajectory.rs` | `MAP_SESSION` / `MAP_LOAD_AT_S` as `maploc`'s `trajectory` example: the saved map the session resumed on, loaded when the daemon loaded it. |
| `MAP_LOAD_CARRY` | on unless 0 | `maploc/examples/trajectory.rs` |  |
| `MAP_SESSION` | a path, or a value | `quack-nav/examples/drop_replay.rs`, `maploc/examples/evaluate.rs`, `maploc/examples/trajectory.rs` | `MAP_SESSION` / `MAP_LOAD_AT_S` as `maploc`'s `trajectory` example: the saved map the session resumed on, loaded when the daemon loaded it. |
| `MAX_T` | number (default f32::INFINITY) | `maploc/examples/evaluate.rs` |  |
| `MICRODUCK` | a value (script) | `scripts/twin/houses/run_house.py` |  |
| `MICRODUCK_RL` | a value (script) | `scripts/twin/houses/run_house.py`, `scripts/rl/twin_ab.py` |  |
| `MICRODUCK_START` | a value (script) | `scripts/twin/viewer/maploc_overlay.py` |  |
| `MIN_RES_DEG` | number (default 1.5) | `maploc/examples/submap_fit.rs` |  |
| `MIN_RES_M` | number (default 0.03) | `maploc/examples/submap_fit.rs` |  |
| `ODOM_LOG` | a path, or a value | `maploc/examples/trajectory.rs` | `ODOM_LOG=<file>`: the raw odometry the mapper was fed, on the Unix clock, for its increments against the truth's. |
| `ODOM_SIGMA_XY` | number (default slam_cfg.odom_sigma_xy) | `maploc/examples/evaluate.rs`, `maploc/examples/trajectory.rs` | How much the graph believes odometry against a closure: a closure 5° wrong turned casa_arredata's first session 5° and walked the pose 0.47 m off across the living room (x13, 2026-09-29), odometry being near perfect on t… |
| `ODOM_SIGMA_YAW` | number (default slam_cfg.odom_sigma_yaw) | `maploc/examples/evaluate.rs`, `maploc/examples/trajectory.rs` |  |
| `OTHER_HOUSE` | a value | `maploc/examples/wake_match.rs` |  |
| `OUT_SESSION` | a path, or a value | `maploc/examples/evaluate.rs` | `OUT_SESSION=<file>`: the map this replay built, saved the way the robot saves one — so a bench run and a live run are scored by the same tools (quacksat's mapquality.py, dump_frame). |
| `POLICY_DIR` | a value (script) | `scripts/twin/houses/run_house.py` |  |
| `PORT` | a value (script) | `scripts/rl/twin_ab.py` |  |
| `POSEERR_DT` | a value (script) | `scripts/twin/houses/poseerr.py` |  |
| `PROPTEST_CASES` | number (default 256) | `quack-nav/tests/route_properties.rs` | `PROPTEST_CASES=5000` for a long hunt. |
| `QN_REPO` | a value (script) | `scripts/twin/houses/run_house.py` |  |
| `QUACK_NAV_SOCKET` | a value (script) | `scripts/twin/viewer/body_with_map.py` |  |
| `REJECT` | number (default slam_cfg.optimizer.reject_sigmas) | `maploc/examples/evaluate.rs` |  |
| `REJECT_MAX` | number (default slam_cfg.optimizer.reject_max as f32) | `maploc/examples/evaluate.rs` |  |
| `RELOC_AGREE` | number (default mapper_cfg.relocalize_agree_windows as f32) | `maploc/examples/evaluate.rs` |  |
| `RELOC_DEBUG` | set = on (any value) | `maploc/src/mapper.rs`, `maploc/src/relocalize.rs` |  |
| `REPLAY_HEAD_DT_MS` | number (unset: none) | `maploc/src/bench.rs` | `REPLAY_HEAD_DT_MS=<ms>`: each depth frame takes the head's pose from the robot-state sample nearest its own time plus this, instead of the last sample before it — to measure what the pairing of the head with the frames … |
| `ROUNDS_ONLY` | a value (script) | `scripts/twin/houses/final_house.py` |  |
| `SAVE_SESSION` | a path, or a value | `maploc/examples/trajectory.rs` | `SAVE_SESSION=<file>`: the map the replay built, saved as the live daemon saves it — for `dump_frame` and `map_vs_truth.py`. |
| `SLANT` | number (unset: none) | `maploc/examples/submap_fit.rs` | `SLANT=x0,x1,y0,y1`: how many of this submap's wall cells fall in that box. |
| `STATE` | a value (script) | `scripts/rl/twin_ab.py` |  |
| `TRACK` | 1 on, else off; unset: mapper_cfg.tracking.enabled | `maploc/examples/evaluate.rs` |  |
| `TRACK_COND` | number (default mapper_cfg.tracking.min_conditioning) | `maploc/examples/evaluate.rs` |  |
| `TRACK_IMPROVE` | number (default mapper_cfg.tracking.min_improvement) | `maploc/examples/evaluate.rs` |  |
| `TRACK_LOG` | a path, or a value | `maploc/examples/trajectory.rs` | `TRACK_LOG=<file>`: the tracked pose every 0.2 s, and every note by name the moment it comes (a rest's watch and end in full) — to see what moves a pose no correction or closure accounts for. |
| `TRACK_MAX` | number (default mapper_cfg.tracking.max_correction_m) | `maploc/examples/evaluate.rs` |  |
| `TRACK_MAX_AFTER` | number (default mapper_cfg.tracking.max_residual_after_m) | `maploc/examples/evaluate.rs` |  |
| `TRACK_MIN_BEFORE` | number (default mapper_cfg.tracking.min_residual_before_m) | `maploc/examples/evaluate.rs` |  |
| `TRACK_PRIOR_XY` | number (default mapper_cfg.tracking.prior_sigma_xy) | `maploc/examples/evaluate.rs` |  |
| `TWIN_WORK` | a value (script) | `scripts/twin/houses/aggregate.py`, `scripts/twin/houses/final_house.py`, `scripts/twin/houses/modes_test.py` … |  |
| `UNC_BEAMS` | number (default u.independent_beams) | `maploc/examples/trajectory.rs` |  |
| `UNC_FLOOR` | number (default u.match_floor_m) | `maploc/examples/trajectory.rs` |  |
| `UNC_SKIP` | number (default u.skip_recent_submaps as f64) | `maploc/examples/trajectory.rs` |  |
| `UNC_XY` | number (default u.xy_var_per_m.sqrt()) | `maploc/examples/trajectory.rs` |  |
| `UNC_YAW_M` | number (default u.yaw_var_per_m.sqrt()) | `maploc/examples/trajectory.rs` |  |
| `UNC_YAW_RAD` | number (default u.yaw_var_per_rad.sqrt()) | `maploc/examples/trajectory.rs` |  |
| `VIEWER` | a value (script) | `scripts/rl/twin_ab.py` |  |
| `WAKE_MODE` | a value (script) | `scripts/twin/houses/wake_bench.py` |  |
| `WAKE_SPAWNS` | a value (script) | `scripts/twin/houses/wake_bench.py` |  |
| `WAKE_TURN` | a value (script) | `scripts/twin/houses/wake_bench.py` |  |
