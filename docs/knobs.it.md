# Le manopole

Ogni variabile d'ambiente che il codice legge, generata dal codice da
`scripts/knobs.py` (il CI controlla che sia aggiornata — non modificarla a
mano). Le `QK_*` sono di quack-nav, le `MAPLOC_*` di maploc; le altre dei
banchi (`maploc/examples`, `quack-nav/examples`) e degli script del
gemello. Le manopole che si chiamavano `QUACKSAT_*` sono `QK_*` dal
2026-09-30. La descrizione è il commento del codice (in inglese).

| Variabile | Letta come | Dove | Cosa dice il codice |
|---|---|---|---|
| `QK_ANCHOR_DROP_M` | numero (default ANCHOR_NOT_NEAR_DROP_DEFAULT_M) | `quack-nav/src/explore/mod.rs` | Not beside a drop: a revisit there turns the duck round in the mouth of the passage it is entering. |
| `QK_ARC_RESERVE_M` | numero (default ARC_RESERVE_M) | `quack-nav/src/explore/gait.rs` |  |
| `QK_BACK_NO_STEP_M` | numero (default 0.30) | `quack-nav/src/explore/gait.rs` | No blind step back at all with a booked drop this close to the body, whatever its bearing, on the trail or not (`QK_BACK_NO_STEP_M`). |
| `QK_BACK_REORIENT` | accesa salvo 0 | `quack-nav/src/explore/gait.rs` | `QK_BACK_REORIENT=0`: after the step back, head for the most open floor and walk a leg there, as before. |
| `QK_BACK_S` | numero (default 1.5) | `quack-nav/src/explore/gait.rs` | The step back: this gait needs about a second to start moving at all, so a shorter one moves nothing (measured); and no more often than this. |
| `QK_BLIND_CONE_LANE` | accesa salvo 0 | `quack-nav/src/explore/guarded.rs` | The cone widens at short range to the lane's own angle — at 0.25 m a 12 cm lane is 26°, and a cube 0.25 m ahead at 18° inside the lane was outside the 15° cone, walked over and dragged 1.9 m (sideA016r, 2026-09-22). |
| `QK_CENTRE` | 1 accesa, 0 spenta, altrimenti il default del chiamante | `quack-nav/src/explore/mod.rs` | Beside a drop, the middle of the way: the aim slid across the heading to where the wall (or the thing) on one side and the rim on the other are as far. |
| `QK_CLIFF_MARGIN_M` | numero (default 0.25) | `quack-nav/src/tools.rs` |  |
| `QK_CLOSE_LOOK` | 1 accesa, 0 spenta, altrimenti il default del chiamante | `quack-nav/src/explore/close_look.rs` | `QK_CLOSE_LOOK=0`: no close looks. |
| `QK_COST_HUG` | numero (default COST_HUG_DEFAULT) | `quack-nav/src/frontier.rs` |  |
| `QK_COST_LANE` | numero (default COST_LANE_DEFAULT) | `quack-nav/src/frontier.rs` |  |
| `QK_COST_UNKNOWN` | numero (default COST_UNKNOWN_DEFAULT) | `quack-nav/src/frontier.rs` |  |
| `QK_CURVE_RAD` | numero (default CURVE_RAD) | `quack-nav/src/explore/gait.rs` |  |
| `QK_DEADBAND_RAD` | numero (default DEADBAND_RAD) | `quack-nav/src/explore/gait.rs` |  |
| `QK_DROP_INFLATE` | numero (default DROP_INFLATE_DEFAULT) | `quack-nav/src/frontier.rs` |  |
| `QK_DROP_LEG_S` | numero (default DROP_LEG_S) | `quack-nav/src/explore/journey.rs` |  |
| `QK_DROP_PLAN_RADIUS_M` | numero (assente: spenta) | `quack-nav/src/explore/mode.rs` |  |
| `QK_DROP_REACH_M` | numero (default DROP_REACH_M) | `quack-nav/src/explore/books.rs` |  |
| `QK_EDGE_DISCRIMINATE` | accesa salvo 0 | `quack-nav/src/explore/books.rs` | `QK_EDGE_DISCRIMINATE=0`: every sensed drop goes on the books as a hole, as before. |
| `QK_EXPLORE_NAV` | 1 accesa, 0 spenta, altrimenti il default del chiamante | `quack-nav/src/explore/navigate.rs` | `QK_EXPLORE_NAV=0`: the exploration walks to its frontiers on its own guarded legs (`walk_leg`), as before 2026-09-29. |
| `QK_FLOOR_STRIKE` | accesa solo con 1 | `quack-nav/src/explore/books.rs` |  |
| `QK_GAP_LANE_HALF_M` | numero (default GAP_LANE_HALF_M) | `quack-nav/src/explore/mod.rs` |  |
| `QK_GAP_LEG_RESERVE_M` | numero (default GAP_LEG_RESERVE_M) | `quack-nav/src/explore/mod.rs` |  |
| `QK_GAP_LEG_S` | numero (default GAP_LEG_S) | `quack-nav/src/explore/mod.rs` |  |
| `QK_GAP_MAX_M` | numero (default GAP_MAX_M) | `quack-nav/src/explore/mod.rs` |  |
| `QK_GUARD_ARC_FULL` | accesa solo con 1 | `quack-duck/src/body.rs` | Turning costs little forward speed: 0.110 m/s at vyaw 0.7 against 0.121 straight, measured on the human drive (2026-09-07). |
| `QK_HOLD_HEADING` | accesa solo con 1 | `quack-nav/examples/paper_twin.rs`, `quack-duck/src/body.rs` | The heading hold (`tools::timed_move_held`, 2026-09-18): on a walking leg that is not an arc the taps cancel the veer — measured 1–3 cm of lateral drift per metre in place of 1–14, the heading within ±4°. |
| `QK_INFLATE_M` | numero (assente: spenta) | `quack-nav/src/frontier.rs` | The inflation the planner runs with: the body's half-width, 0.10 (`QK_INFLATE_M`). |
| `QK_LANE_HALF_M` | numero (default LANE_HALF_M) | `quack-nav/src/explore/mod.rs` |  |
| `QK_LANE_RAILS` | numero (default usize::MAX) | `quack-nav/src/map.rs` | How many rails [`Grid::lane_clear`] samples across the lane: 3 is the old behaviour, anything more means every half cell. |
| `QK_LEG_RESERVE_M` | numero (default LEG_RESERVE_M) | `quack-nav/src/explore/mod.rs` |  |
| `QK_LOOKAHEAD_M` | numero (default LOOKAHEAD_M) | `quack-nav/src/explore/journey.rs` |  |
| `QK_LOW_BOOK_PUSH_M` | numero (default LOW_BOOK_PUSH_M) | `quack-nav/src/explore/guarded.rs` |  |
| `QK_MAP_STAND_S` | numero (default LEG_STOP_S) | `quack-nav/src/explore/mapping.rs` | How long a mapping leg stands. See [`Job::stop_s`]. |
| `QK_MOUTH_M` | numero (default PASSAGE_MOUTH_M) | `quack-nav/src/explore/guarded.rs` |  |
| `QK_NO_GUARDS` | 1 accesa, 0 spenta, altrimenti il default del chiamante | `quack-nav/src/explore/journey.rs` | `QK_NO_GUARDS`: every leg, kick and pulse goes through `robot.move`, blind, and the route check is off — the planner alone (the frozen map, the books, the margins) brings the duck home. |
| `QK_ORACLE_HOLES` | un valore | `quack-nav/src/oracle.rs` | The process's oracle, from the knobs (none set: `None`). |
| `QK_ORACLE_POSE` | un valore | `quack-nav/src/oracle.rs` | The process's oracle, from the knobs (none set: `None`). |
| `QK_ORACLE_WALLS` | un valore | `quack-nav/src/oracle.rs` | The process's oracle, from the knobs (none set: `None`). |
| `QK_PASSAGE_BOOK` | 1 accesa, 0 spenta, altrimenti il default del chiamante | `quack-nav/src/explore/guarded.rs` | `QK_PASSAGE_BOOK=0`: the sensor's rim beside the body is not put on the books by the passage law (measuring). |
| `QK_PASSAGE_CLIFF_MARGIN_M` | numero (default 0.25) | `quack-nav/src/explore/guarded.rs` | The margin from an edge a passage leg asks of the guard. |
| `QK_PASSAGE_HUG` | 1 accesa, 0 spenta, altrimenti quella del modo | `quack-nav/src/explore/mode.rs` |  |
| `QK_PASSAGE_LAW` | 1 accesa, 0 spenta, altrimenti il default del chiamante | `quack-nav/src/explore/guarded.rs` |  |
| `QK_PASSAGE_MIN_W` | numero (default PASSAGE_MIN_W_M) | `quack-nav/src/explore/guarded.rs` | `QK_PASSAGE_MIN_W`: 0.30 since 2026-09-16 (was 0.50, then 0.42): the width is measured wall-to-(rim point − its 0.10 radius), so the 0.44 m strip east of the twin's stairwell reads 0.30–0.34 and the 0.55 m west passage r… |
| `QK_PASSAGE_SENSOR` | accesa con 1 (2: di più); un valore | `quack-nav/src/explore/guarded.rs` | Measurement switches for the passage (`=1` turns each on). |
| `QK_PROP_TURN` | accesa salvo 0 | `quack-nav/src/explore/gait.rs` |  |
| `QK_PULL_DEVIATION_M` | numero (default 0.20) | `quack-nav/src/frontier.rs` | A straight run may stray this far from the Dijkstra route it replaces: the staircase is smoothed, the route is not redrawn — a diagonal across the room cut corners and brushed walls Dijkstra had kept away from (the user'… |
| `QK_PULL_ROUTE` | accesa salvo 0 | `quack-nav/src/frontier.rs` | `QK_PULL_ROUTE=0` leaves Dijkstra's staircase as it is. |
| `QK_REACH_TO_FLOOR` | accesa salvo 0 | `quack-nav/src/explore/books.rs` | `QK_REACH_TO_FLOOR=0`: the reach behind a rim ignores where the floor comes back (see `record_drops`). |
| `QK_REANCHOR` | accesa salvo 0 | `quack-nav/src/explore/journey.rs` | Aim at the farthest point of the path the body can walk to in a straight line, instead of the one a fixed number of cells ahead. |
| `QK_REFUSED_REARM` | accesa solo con 1 | `quack-nav/src/explore/gait.rs` | `QK_REFUSED_REARM`: `0` one-shot per job, `1` after every walked leg, else (default) once the body has moved [`REARM_DIST_M`]. |
| `QK_RIM_OFF` | 1 accesa, 0 spenta, altrimenti il default del chiamante | `quack-nav/src/explore/mod.rs` |  |
| `QK_RIM_TOUR` | 1 accesa, 0 spenta, altrimenti il default del chiamante | `quack-nav/src/explore/rim_tour.rs` | `QK_RIM_TOUR=0`: no rounds of the holes. |
| `QK_ROUTE_CHECK_M` | numero (default ROUTE_CHECK_M) | `quack-nav/src/explore/guarded.rs` |  |
| `QK_SEAL` | 1 accesa, 0 spenta, altrimenti quella del modo | `quack-nav/src/explore/mode.rs` |  |
| `QK_SMOOTH_PATH` | accesa salvo 0 | `quack-nav/src/explore/journey.rs` |  |
| `QK_SPIN_RAD` | numero (default 0.6) | `quack-nav/src/explore/gait.rs` | `QK_SPIN_RAD`: a leg whose aim is more than this off the nose turns in place first (closed on the yaw, [`Job::align`]) instead of walking a curve — a curve from a standstill drifts sideways for its first second, into a w… |
| `QK_SPIN_WATCH` | accesa salvo 0 | `quack-nav/src/explore/gait.rs` | `QK_SPIN_WATCH=0`: turn in place without watching the sensor, as before. |
| `QK_STICK_CAREFUL` | 1 accesa, 0 spenta, altrimenti il default del chiamante | `quack-nav/src/explore/navigate.rs` | Walk to `goal` by `deadline` (see the module): `Done` "arrived at", `Failed` with why, `Stopped`. |
| `QK_STRAIGHT_LOOK_M` | numero (default STRAIGHT_LOOK_M) | `quack-nav/src/explore/journey.rs` |  |
| `QK_STRAIGHT_RAD` | numero (default STRAIGHT_RAD) | `quack-nav/src/explore/gait.rs` |  |
| `QK_STRING_PULL_M` | numero (default STRING_PULL_M) | `quack-nav/src/explore/journey.rs` |  |
| `QK_TRAIL` | accesa salvo 0 | `quack-nav/src/explore/books.rs` |  |
| `QK_TRAIL_LEG` | accesa salvo 0 | `quack-nav/src/explore/books.rs` | `QK_TRAIL_LEG=0`: no doorway margins on the trail, for measuring. |
| `QK_TRAVEL_OFF_UNKNOWN` | 1 accesa, 0 spenta, altrimenti il default del chiamante | `quack-nav/src/explore/navigate.rs` | `QK_TRAVEL_OFF_UNKNOWN=1`: the exploration plans off the unknown (see `unknown_walled`). |
| `QK_TRUSTED_FLOOR` | 1 accesa, 0 spenta, altrimenti quella del modo | `quack-nav/src/explore/mode.rs` |  |
| `QK_TURN_IN_PLACE` | accesa salvo 0 | `quack-nav/src/explore/gait.rs` | `QK_TURN_IN_PLACE=0` turns the old way (kick, then yaw) everywhere. |
| `QK_WALL_FIT` | 1 accesa, 0 spenta, altrimenti quella del modo | `quack-nav/src/explore/mode.rs` |  |
| `QK_WALL_MARGIN_M` | numero (default 0.18) | `quack-nav/src/tools.rs` |  |
| `MAPLOC_CONFIRM_TRAVEL` | numero (default 0.5) | `maploc/src/mapper.rs` | At boot, how far the body must have moved between the window that nominated a candidate and the one that confirms it. |
| `MAPLOC_CONTINUOUS` | presente = accesa (qualsiasi valore) | `maploc/examples/replay.rs` |  |
| `MAPLOC_GRAPH_DEBUG` | presente = accesa (qualsiasi valore) | `maploc/src/pipeline.rs` |  |
| `MAPLOC_HEAD_LEAD_MS` | numero (default 5_000_000) | `quack-nav/src/mapd/mod.rs`, `maploc/src/bench.rs` | `MAPLOC_HEAD_LEAD_MS`, as robotd read it: 5 ms by default — with 0 the standing drift was +0.17/+0.22°/min, with 20 and 40 it turned negative (twin, 2026-09-16). |
| `MAPLOC_HYP_LEAD` | numero (default 3) | `maploc/src/mapper.rs` | How many hits the leading hypothesis must have over the runner-up before it is nominated. |
| `MAPLOC_HYP_TRAVEL` | numero (default 1.0) | `maploc/src/mapper.rs` | How far the body must have got from where it first saw a hypothesis before the hypothesis can be believed — the chord, not the path. |
| `MAPLOC_LOCAL_AFTER_BOOT` | accesa salvo 0 | `maploc/src/mapper.rs` | `MAPLOC_LOCAL_AFTER_BOOT=0`: every loss on a resumed map searches the whole map with every hypothesis, as before 2026-09-28. |
| `MAPLOC_LOOP_CAP_YAW` | numero (default 0.45) | `maploc/src/loop_closer.rs` | `MAPLOC_LOOP_CAP_YAW` overrides it, for measuring on the twin. |
| `MAPLOC_LOOP_SIGMA_YAW` | numero (default 0.24) | `maploc/src/pipeline.rs` | `MAPLOC_LOOP_SIGMA_YAW` overrides the heading's. |
| `MAPLOC_MAX_RANGE` | numero (default 3.0) | `maploc/src/accumulator.rs` | `MAPLOC_MAX_RANGE` overrides it, for the bench. |
| `MAPLOC_MCL` | accesa solo con 1 | `maploc/src/mapper.rs` | `MAPLOC_MCL=1` runs the particle filter (`mcl.rs`, wired to nothing before this) as a boot search on a resumed map: it proposes, the still windows judge, exactly as the brute-force search's candidates are judged. |
| `MAPLOC_MCL_N` | numero (default 800) | `maploc/src/mapper.rs` |  |
| `MAPLOC_MCL_RESID` | numero (default 0.08) | `maploc/src/mapper.rs` |  |
| `MAPLOC_MCL_TRAVEL` | numero (default 0.10) | `maploc/src/mapper.rs` |  |
| `MAPLOC_MCL_YAW` | numero (default 0.8) | `maploc/src/mapper.rs` | A lock is not a candidate until the body has swept this much yaw and moved this far since the seed: the filter has no motion gate of its own (`mcl.rs`), and a stationary 45° wedge locks on a mirror image as happily as on… |
| `MAPLOC_MIRROR_COLS` | presente = accesa (qualsiasi valore) | `maploc/examples/evaluate.rs` | MAPLOC_MIRROR_COLS=1 replays every frame with its columns reversed. |
| `MAPLOC_MULTI_HYP` | accesa salvo 0 | `maploc/src/mapper.rs` | `MAPLOC_MULTI_HYP=0` turns the multi-hypothesis boot search off, for measuring against the single-best agreement it replaces. |
| `MAPLOC_RAW` | presente = accesa (qualsiasi valore) | `maploc/examples/replay.rs` |  |
| `MAPLOC_RAY_JUDGE` | accesa solo con 1 | `maploc/src/mapper.rs` | `MAPLOC_RAY_JUDGE=1` judges candidates along the ray (`relocalize::score_pose_rays`) instead of by endpoints alone. |
| `MAPLOC_SETTLE` | accesa solo con 1 | `maploc/src/mapper.rs` |  |
| `MAPLOC_SHADOW` | accesa salvo 0 | `maploc/src/mapper.rs` | `MAPLOC_SHADOW=0`: no shadow map at boot. |
| `MAPLOC_SHADOW_ASKS` | numero (default 2) | `maploc/src/mapper.rs` |  |
| `MAPLOC_SHADOW_EVERY_S` | numero (default 30.0) | `maploc/src/mapper.rs` | How often the shadow asks, how many answers in a row must agree, and what an answer must be (the homecoming's adoption rule, see quack-nav's `HomecomingConfig`, measured on 27 replayed wakes: 626 of 655 right answers pas… |
| `MAPLOC_VALLEY_CROSS` | accesa salvo 0 | `maploc/src/mapper.rs` | Two valleys that cross resolve each other (see `Mapper::valley_blocks`); `MAPLOC_VALLEY_CROSS=0` for the valley test alone. |
| `MAPLOC_VERBOSE` | presente = accesa (qualsiasi valore) | `maploc/examples/replay.rs` |  |
| `MAPLOC_WATCHDOG_RESCUE` | accesa salvo 0 | `maploc/src/mapper.rs` | `MAPLOC_WATCHDOG_RESCUE=0`: the watchdog judges a window at the carried pose alone, as before 2026-09-28. |
| `AB_REPO` | un valore (script) | `scripts/twin/houses/final_house.py` |  |
| `ACC_RANGE` | numero (default mapper_cfg.accumulator.max_range_m) | `maploc/examples/evaluate.rs` | `ACC_RANGE`: how far a beam may be and still reach the map. |
| `ALIGN_BEAMS` | numero (assente: spenta) | `maploc/examples/align_maps.rs` |  |
| `ALIGN_RESID` | numero (assente: spenta) | `maploc/examples/align_maps.rs` |  |
| `ASK_EVERY_S` | numero (default 60.0) | `maploc/examples/wake_match.rs` |  |
| `BOOK` | un valore | `quack-nav/examples/drop_replay.rs` | `BOOK=<ground.json>:<map>`: the book's holes too, each checked against the floor strike (explore::books' rule, replayed): which phantoms it would take off, and whether it ever takes a true rim. |
| `CORR_LOG` | un percorso, o un valore | `maploc/examples/trajectory.rs` | `CORR_LOG=<file>`: every tracking correction a window made, against the truth — which of them moved the pose toward it and which away (casa_arredata, 2026-09-29: the windows pulled the pose 3.5-7 cm north one after the o… |
| `CUT_ON_CORR` | numero (default mapper_cfg.tracking.cut_on_correction_m) | `maploc/examples/evaluate.rs` |  |
| `DEGEN_LOG` | un percorso, o un valore | `maploc/examples/trajectory.rs` | `DEGEN_LOG=<file>`: every relocalization the search confirmed or the valley test refused, with the pose's error against the truth and the scan's conditioning there — the valley test and the Hessian's eigenvalues, side by… |
| `FROZEN` | accesa solo con 1 | `maploc/examples/trajectory.rs` | `FROZEN=1`: the map frozen, as quack-navd's rounds run on a house already mapped — nothing inks while the pose tracks. |
| `HUBER` | numero (default slam_cfg.optimizer.huber_delta) | `maploc/examples/evaluate.rs` |  |
| `LOOP_CAP` | numero (default slam_cfg.loops.max_correction_cap_m) | `maploc/examples/evaluate.rs`, `maploc/examples/trajectory.rs` |  |
| `LOOP_CAP_YAW` | numero (default slam_cfg.loops.max_correction_cap_rad) | `maploc/examples/evaluate.rs`, `maploc/examples/trajectory.rs` |  |
| `LOOP_GAP` | numero (default slam_cfg.loops.min_index_gap as f32) | `maploc/examples/evaluate.rs` |  |
| `LOOP_LOG` | un percorso, o un valore | `maploc/examples/trajectory.rs` | `LOOP_LOG=<file>`: every closure, what it moved the pose by and the heading's error against the truth before and after — from the nearest truth sample, so judged only while the duck stands still. |
| `LOOP_MIN_CORR` | numero (default slam_cfg.loops.min_correction_m) | `maploc/examples/evaluate.rs` |  |
| `LOOP_PER_SUBMAP` | numero (default slam_cfg.loops.max_correction_per_submap_m) | `maploc/examples/evaluate.rs` |  |
| `LOOP_PER_SUBMAP_YAW` | numero (default slam_cfg.loops.max_correction_per_submap_rad) | `maploc/examples/evaluate.rs`, `maploc/examples/trajectory.rs` |  |
| `LOOP_RADIUS` | numero (default slam_cfg.loops.radius_m) | `maploc/examples/evaluate.rs` |  |
| `LOOP_SIGMA_XY` | numero (default slam_cfg.loops.edge_sigma_xy) | `maploc/examples/evaluate.rs`, `maploc/examples/trajectory.rs` |  |
| `LOOP_SIGMA_YAW` | numero (default slam_cfg.loops.edge_sigma_yaw) | `maploc/examples/evaluate.rs`, `maploc/examples/trajectory.rs` |  |
| `LOOP_SPREAD` | numero (default slam_cfg.loops.verify_max_spread_m) | `maploc/examples/evaluate.rs` |  |
| `LOOP_VERBOSE` | presente = accesa (qualsiasi valore) | `maploc/examples/evaluate.rs` |  |
| `MAP_FALLBACK` | un valore (script) | `scripts/twin/viewer/body_with_map.py` |  |
| `MAP_LOAD_AT_S` | numero (assente: spenta) | `quack-nav/examples/drop_replay.rs`, `maploc/examples/trajectory.rs` | `MAP_LOAD_AT_S=<t>`: a fresh map until `t` seconds into the recording, the saved one from then on, as the daemon boots live (see `bench::replay_loading`); unset, the saved one from the start. |
| `MAP_LOAD_CARRY` | accesa salvo 0 | `maploc/examples/trajectory.rs` |  |
| `MAP_SESSION` | un percorso, o un valore | `quack-nav/examples/drop_replay.rs`, `maploc/examples/evaluate.rs`, `maploc/examples/trajectory.rs` | `MAP_SESSION=<file>` replays into a map saved by an earlier run, starting lost: the boot-relocalization question, on the bench. |
| `MAX_T` | numero (default f32::INFINITY) | `maploc/examples/evaluate.rs` |  |
| `MICRODUCK` | un valore (script) | `scripts/twin/houses/run_house.py` |  |
| `MICRODUCK_RL` | un valore (script) | `scripts/twin/houses/run_house.py` |  |
| `MICRODUCK_START` | un valore (script) | `scripts/twin/viewer/maploc_overlay.py` |  |
| `MIN_RES_DEG` | numero (default 1.5) | `maploc/examples/submap_fit.rs` |  |
| `MIN_RES_M` | numero (default 0.03) | `maploc/examples/submap_fit.rs` |  |
| `ODOM_LOG` | un percorso, o un valore | `maploc/examples/trajectory.rs` | `ODOM_LOG=<file>`: the raw odometry the mapper was fed, on the Unix clock, for its increments against the truth's. |
| `ODOM_SIGMA_XY` | numero (default slam_cfg.odom_sigma_xy) | `maploc/examples/evaluate.rs`, `maploc/examples/trajectory.rs` | How much the graph believes odometry against a closure: a closure 5° wrong turned casa_arredata's first session 5° and walked the pose 0.47 m off across the living room (x13, 2026-09-29), odometry being near perfect on t… |
| `ODOM_SIGMA_YAW` | numero (default slam_cfg.odom_sigma_yaw) | `maploc/examples/evaluate.rs`, `maploc/examples/trajectory.rs` |  |
| `OTHER_HOUSE` | un valore | `maploc/examples/wake_match.rs` |  |
| `OUT_SESSION` | un percorso, o un valore | `maploc/examples/evaluate.rs` | `OUT_SESSION=<file>`: the map this replay built, saved the way the robot saves one — so a bench run and a live run are scored by the same tools (quacksat's mapquality.py, dump_frame). |
| `POLICY_DIR` | un valore (script) | `scripts/twin/houses/run_house.py` |  |
| `POSEERR_DT` | un valore (script) | `scripts/twin/houses/poseerr.py` |  |
| `PROPTEST_CASES` | numero (default 256) | `quack-nav/tests/route_properties.rs` |  |
| `QN_REPO` | un valore (script) | `scripts/twin/houses/run_house.py` |  |
| `QUACK_NAV_MCP` | un valore (script) | `scripts/twin/viewer/body_with_map.py` |  |
| `QUACK_NAV_MCP_TOKEN` | un valore (script) | `scripts/twin/viewer/body_with_map.py` |  |
| `QUACK_NAV_SOCKET` | un valore (script) | `scripts/twin/viewer/body_with_map.py` |  |
| `REJECT` | numero (default slam_cfg.optimizer.reject_sigmas) | `maploc/examples/evaluate.rs` |  |
| `REJECT_MAX` | numero (default slam_cfg.optimizer.reject_max as f32) | `maploc/examples/evaluate.rs` |  |
| `RELOC_AGREE` | numero (default mapper_cfg.relocalize_agree_windows as f32) | `maploc/examples/evaluate.rs` |  |
| `RELOC_DEBUG` | presente = accesa (qualsiasi valore) | `maploc/src/mapper.rs`, `maploc/src/relocalize.rs` | A wake-up believes nothing it has not walked for. |
| `REPLAY_HEAD_DT_MS` | numero (assente: spenta) | `maploc/src/bench.rs` | `REPLAY_HEAD_DT_MS=<ms>`: each depth frame takes the head's pose from the robot-state sample nearest its own time plus this, instead of the last sample before it — to measure what the pairing of the head with the frames … |
| `ROUNDS_ONLY` | un valore (script) | `scripts/twin/houses/final_house.py` |  |
| `SAVE_SESSION` | un percorso, o un valore | `maploc/examples/trajectory.rs` | `SAVE_SESSION=<file>`: the map the replay built, saved as the live daemon saves it — for `dump_frame` and `map_vs_truth.py`. |
| `SLANT` | numero (assente: spenta) | `maploc/examples/submap_fit.rs` | `SLANT=x0,x1,y0,y1`: how many of this submap's wall cells fall in that box. |
| `TRACK` | accesa solo con 1 | `maploc/examples/evaluate.rs` |  |
| `TRACK_COND` | numero (default mapper_cfg.tracking.min_conditioning) | `maploc/examples/evaluate.rs` |  |
| `TRACK_IMPROVE` | numero (default mapper_cfg.tracking.min_improvement) | `maploc/examples/evaluate.rs` |  |
| `TRACK_LOG` | un percorso, o un valore | `maploc/examples/trajectory.rs` | `TRACK_LOG=<file>`: the tracked pose every 0.2 s, and every note by name the moment it comes — to see what moves a pose no correction or closure accounts for. |
| `TRACK_MAX` | numero (default mapper_cfg.tracking.max_correction_m) | `maploc/examples/evaluate.rs` |  |
| `TRACK_MAX_AFTER` | numero (default mapper_cfg.tracking.max_residual_after_m) | `maploc/examples/evaluate.rs` |  |
| `TRACK_MIN_BEFORE` | numero (default mapper_cfg.tracking.min_residual_before_m) | `maploc/examples/evaluate.rs` |  |
| `TRACK_PRIOR_XY` | numero (default mapper_cfg.tracking.prior_sigma_xy) | `maploc/examples/evaluate.rs` |  |
| `TWIN_WORK` | un valore (script) | `scripts/twin/houses/aggregate.py`, `scripts/twin/houses/final_house.py`, `scripts/twin/houses/modes_test.py` … |  |
| `UNC_BEAMS` | numero (default u.independent_beams) | `maploc/examples/trajectory.rs` |  |
| `UNC_FLOOR` | numero (default u.match_floor_m) | `maploc/examples/trajectory.rs` |  |
| `UNC_SKIP` | numero (default u.skip_recent_submaps as f64) | `maploc/examples/trajectory.rs` |  |
| `UNC_XY` | numero (default u.xy_var_per_m.sqrt() | `maploc/examples/trajectory.rs` |  |
| `UNC_YAW_M` | numero (default u.yaw_var_per_m.sqrt() | `maploc/examples/trajectory.rs` |  |
| `UNC_YAW_RAD` | numero (default u.yaw_var_per_rad.sqrt() | `maploc/examples/trajectory.rs` |  |
| `WAKE_SPAWNS` | un valore (script) | `scripts/twin/houses/wake_bench.py` |  |
| `WAKE_TURN` | un valore (script) | `scripts/twin/houses/wake_bench.py` |  |
