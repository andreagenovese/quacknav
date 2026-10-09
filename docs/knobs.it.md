# Le manopole

Ogni variabile d'ambiente che il codice legge, generata dal codice da
`scripts/knobs.py` (il CI controlla che sia aggiornata — non modificarla a
mano). Le `QK_*` sono di quack-nav, le `MAPLOC_*` di maploc; le altre dei
banchi (`maploc/examples`, `quack-nav/examples`) e degli script del
gemello. Le manopole che si chiamavano `QUACKSAT_*` sono `QK_*` dal
2026-09-30. La descrizione è il commento del codice (in inglese).

Sull'anatra stanno in `/var/lib/quack-nav/knobs.env`, letto dall'unit
(`EnvironmentFile=-`) a ogni avvio. Un client le modifica senza una shell:
`nav.knobs` sul socket nav elenca quelle di quack-navd (leggibili da una
macchina, `quack-nav/src/knobs.json`, generato con questa pagina) e scrive il
file, `nav.restart` le applica — la pagina di quack-control ha un editor per
entrambe. Ogni manopola richiede quel riavvio: l'ambiente si legge all'avvio
del processo.

| Variabile | Letta come | Dove | Cosa dice il codice |
|---|---|---|---|
| `QK_ALIGN_KICK` | 1 accesa, 0 spenta, altrimenti quella del modo | `quack-nav/src/explore/mode.rs` |  |
| `QK_ANCHOR_DROP_M` | numero (default 1.0) | `quack-nav/src/explore/mod.rs` | Not beside a drop: a revisit there turns the duck round in the mouth of the passage it is entering. |
| `QK_BACK_NO_STEP_M` | numero (default 0.30) | `quack-nav/src/explore/gait.rs` | No blind step back at all with a booked drop this close to the body, whatever its bearing, on the trail or not (`QK_BACK_NO_STEP_M`). |
| `QK_BACK_REORIENT` | accesa salvo 0 | `quack-nav/src/explore/gait.rs` | `QK_BACK_REORIENT=0`: after the step back, head for the most open floor and walk a leg there, as before. |
| `QK_BACK_S` | numero (default 1.5) | `quack-nav/src/explore/gait.rs` | The step back: this gait needs about a second to start moving at all, so a shorter one moves nothing (measured); and no more often than this. |
| `QK_BLIND_CONE_LANE` | accesa salvo 0 | `quack-nav/src/explore/guarded.rs` | The cone widens at short range to the lane's own angle — at 0.25 m a 12 cm lane is 26°, and a cube 0.25 m ahead at 18° inside the lane was outside the 15° cone, walked over and dragged 1.9 m (sideA016r, 2026-09-22). |
| `QK_CLIFF_MARGIN_M` | numero (default 0.25) | `quack-nav/src/tools.rs` | Stop this far short of a drop's edge — farther than a wall's margin, because an edge is not a bump. |
| `QK_CLOSE_LOOK` | 1 accesa, 0 spenta, altrimenti il default del chiamante | `quack-nav/src/explore/close_look.rs` | `QK_CLOSE_LOOK=0`: no close looks. |
| `QK_COST_HUG` | numero (default 30) | `quack-nav/src/frontier.rs` | `QK_COST_HUG=0` restores the flat price. |
| `QK_CURVE_RAD` | numero (default 1.0) | `quack-nav/src/explore/gait.rs` |  |
| `QK_DROP_INFLATE` | numero (default 0.05) | `quack-nav/src/frontier.rs` | `QK_DROP_INFLATE` to measure. |
| `QK_DROP_PLAN_RADIUS_M` | numero (default 0.12 (guarded: 0.20)) | `quack-nav/src/explore/mode.rs` |  |
| `QK_DROP_REACH_M` | numero (default 0.30) | `quack-nav/src/explore/books.rs` | `QK_DROP_REACH_M=0` goes back to points. |
| `QK_EDGE_DISCRIMINATE` | accesa salvo 0 | `quack-nav/src/explore/books.rs` | `QK_EDGE_DISCRIMINATE=0`: every sensed drop goes on the books as a hole, as before. |
| `QK_GAP_LANE_HALF_M` | numero (default 0.115) | `quack-nav/src/explore/mod.rs` |  |
| `QK_GAP_MAX_M` | numero (default 0.6) | `quack-nav/src/explore/mod.rs` |  |
| `QK_GUARD_ARC_FULL` | accesa solo con 1 | `quack-duck/src/body.rs` | Turning costs little forward speed: 0.110 m/s at vyaw 0.7 against 0.121 straight, measured on the human drive (2026-09-07). |
| `QK_INFLATE_M` | numero (default 0.12) | `quack-nav/src/frontier.rs` | The inflation the planner runs with: 0.12, a little over the body's half-width, 0.10 (`QK_INFLATE_M`). |
| `QK_LANE_HALF_M` | numero (default 0.16) | `quack-nav/src/explore/mod.rs` |  |
| `QK_LANE_YIELD_M` | numero (default 0.20) | `quack-nav/src/frontier.rs` | `QK_LANE_YIELD_M`: [`LANE_YIELDS_TO_DROP_M`] for an experiment. |
| `QK_NO_GUARDS` | 1 accesa, 0 spenta, altrimenti il default del chiamante | `quack-nav/src/explore/journey.rs` | `QK_NO_GUARDS`: every leg, kick and pulse goes through `robot.move`, blind, and the route check is off — the planner alone (the frozen map, the books, the margins) brings the duck home. |
| `QK_ORACLE_AS_MAPPED` | accesa solo con 1 | `quack-nav/src/oracle.rs` | `QK_ORACLE_AS_MAPPED=1`: the truth drawn as a mapper draws a house — the holes unknown (no floor ever seen there) instead of wall, and the inside of each box of the truth's `boxes` unknown past a 5 cm band (nothing sees … |
| `QK_ORACLE_BOOK` | un valore | `quack-nav/src/oracle.rs` | `QK_ORACLE_BOOK=<truth.json>`: the true holes' rims on the books at start, every 10 cm (what a ground book holds for a mapped house), for the journeys on the oracle's map (`scripts/rl/twin_ab.py`). |
| `QK_ORACLE_HOLES` | un valore | `quack-nav/src/oracle.rs` | `QK_ORACLE_HOLES=<truth.json>`: its `holes` (`[x0, x1, y0, y1]`, metres) drawn into that map as wall — the planner keeps off them. |
| `QK_ORACLE_POSE` | un valore | `quack-nav/src/oracle.rs` | `QK_ORACLE_POSE=<host:port>`: the pose the navigation reads is the simulator's trunk, read as `poseerr.py` reads it, at 20 Hz. |
| `QK_ORACLE_WALLS` | un valore | `quack-nav/src/oracle.rs` | `QK_ORACLE_WALLS=<walls.toml>`: the map the navigation plans on is drawn from the truth's wall segments (centimetres, the MuJoCo world frame, as `scripts/twin/houses/*.toml` hold them): every cell inside the house free, … |
| `QK_PULL_DEVIATION_M` | numero (default 0.20) | `quack-nav/src/frontier.rs` | A straight run may stray this far from the Dijkstra route it replaces: the staircase is smoothed, the route is not redrawn — a diagonal across the room cut corners and brushed walls Dijkstra had kept away from (the user'… |
| `QK_PULL_ROUTE` | accesa salvo 0 | `quack-nav/src/frontier.rs` | `QK_PULL_ROUTE=0` leaves Dijkstra's staircase as it is. |
| `QK_REACH_TO_FLOOR` | accesa salvo 0 | `quack-nav/src/explore/books.rs` | `QK_REACH_TO_FLOOR=0`: the reach behind a rim ignores where the floor comes back (see `record_drops`). |
| `QK_REANCHOR` | accesa salvo 0 | `quack-nav/src/explore/journey.rs` | `QK_REANCHOR=0`: no trip back to mapped floor now and then to close a loop while the exploration walks new floor (see the explorer's run). |
| `QK_RIM_TOUR` | 1 accesa, 0 spenta, altrimenti il default del chiamante | `quack-nav/src/explore/rim_tour.rs` | `QK_RIM_TOUR=0`: no rounds of the holes. |
| `QK_RL_POLICY` | un valore | `quack-nav/src/rlnav/mod.rs` | `QK_RL_POLICY`: the pilot (docs/rl-pilot.md) that flies the stick's legs; |
| `QK_RL_TRACE` | un valore | `quack-nav/src/rlnav/trace.rs` | `QK_RL_TRACE`: what the duck did and saw, leg by leg, for the calibration (`quack-rl`'s `rl_calib`, docs/rl-pilot.md). |
| `QK_SPIN_WATCH` | accesa salvo 0 | `quack-nav/src/explore/gait.rs` | `QK_SPIN_WATCH=0`: turn in place without watching the sensor, as before. |
| `QK_STICK_CAREFUL` | 1 accesa, 0 spenta, altrimenti il default del chiamante | `quack-nav/src/explore/navigate.rs` |  |
| `QK_STRAIGHT_RAD` | numero (default 0.35) | `quack-nav/src/explore/gait.rs` |  |
| `QK_TRAIL` | accesa salvo 0 | `quack-nav/src/explore/books.rs` | Off with `QK_TRAIL=0`, for measuring. |
| `QK_TRUSTED_FLOOR` | 1 accesa, 0 spenta, altrimenti quella del modo | `quack-nav/src/explore/mode.rs` |  |
| `QK_TURN_IN_PLACE` | accesa salvo 0 | `quack-nav/src/explore/gait.rs` | `QK_TURN_IN_PLACE=0` turns the old way (kick, then yaw) everywhere. |
| `QK_WALL_MARGIN_M` | numero (default 0.18) | `quack-nav/src/tools.rs` |  |
| `MAPLOC_CONFIRM_TRAVEL` | numero (default 0.5) | `maploc/src/mapper.rs` | At boot, how far the body must have moved between the window that nominated a candidate and the one that confirms it. |
| `MAPLOC_CONTINUOUS` | presente = accesa (qualsiasi valore) | `maploc/examples/replay.rs` |  |
| `MAPLOC_GRAPH_DEBUG` | presente = accesa (qualsiasi valore) | `maploc/src/pipeline.rs` |  |
| `MAPLOC_HEAD_LEAD_MS` | numero (default 5) | `quack-nav/src/mapd/mod.rs`, `maploc/src/bench.rs` | `MAPLOC_HEAD_LEAD_MS`, as robotd read it: 5 ms by default — with 0 the standing drift was +0.17/+0.22°/min, with 20 and 40 it turned negative (twin, 2026-09-16). |
| `MAPLOC_HYP_LEAD` | numero (default 3) | `maploc/src/mapper.rs` | How many hits the leading hypothesis must have over the runner-up before it is nominated. |
| `MAPLOC_HYP_TRAVEL` | numero (default 1.0) | `maploc/src/mapper.rs` | How far the body must have got from where it first saw a hypothesis before the hypothesis can be believed — the chord, not the path. |
| `MAPLOC_LOCAL_AFTER_BOOT` | accesa salvo 0 | `maploc/src/mapper.rs` | `MAPLOC_LOCAL_AFTER_BOOT=0`: every loss on a resumed map searches the whole map with every hypothesis, as before 2026-09-28. |
| `MAPLOC_LOOP_CAP_YAW` | numero (default 0.07) | `maploc/src/loop_closer.rs` | `MAPLOC_LOOP_CAP_YAW` overrides it, for measuring on the twin. |
| `MAPLOC_LOOP_DEBUG` | accesa solo con 1 | `maploc/src/loop_closer.rs` | `MAPLOC_LOOP_DEBUG=1`: one line per accepted closure on stderr — the submaps, the correction it asks for, the match's residual and beams, the witnesses' spread — to tell the closures that turn a map wrong. |
| `MAPLOC_MIRROR_COLS` | accesa solo con 1 | `maploc/examples/evaluate.rs` | MAPLOC_MIRROR_COLS=1 replays every frame with its columns reversed. |
| `MAPLOC_MULTI_HYP` | accesa salvo 0 | `maploc/src/mapper.rs` | `MAPLOC_MULTI_HYP=0` turns the multi-hypothesis boot search off, for measuring against the single-best agreement it replaces. |
| `MAPLOC_RAW` | presente = accesa (qualsiasi valore) | `maploc/examples/replay.rs` |  |
| `MAPLOC_REST` | accesa salvo 0 | `maploc/src/mapper.rs` | `MAPLOC_REST=0`: no stand ever rests, as before the rest (2026-10-01). |
| `MAPLOC_SHADOW` | accesa salvo 0 | `maploc/src/mapper.rs` | `MAPLOC_SHADOW=0`: no shadow map at boot. |
| `MAPLOC_SHADOW_ASKS` | numero (default 2) | `maploc/src/mapper.rs` | `MAPLOC_SHADOW_EVERY_S` and `MAPLOC_SHADOW_ASKS` override them. |
| `MAPLOC_SHADOW_EVERY_S` | numero (default 30.0) | `maploc/src/mapper.rs` | How often the shadow asks, how many answers in a row must agree, and what an answer must be (the homecoming's adoption rule, see quack-nav's `HomecomingConfig`, measured on 27 replayed wakes: 626 of 655 right answers pas… |
| `MAPLOC_VALLEY_CROSS` | accesa salvo 0 | `maploc/src/mapper.rs` | Two valleys that cross resolve each other (see `Mapper::valley_blocks`); `MAPLOC_VALLEY_CROSS=0` for the valley test alone. |
| `MAPLOC_VERBOSE` | presente = accesa (qualsiasi valore) | `maploc/examples/replay.rs` |  |
| `MAPLOC_WATCHDOG_RESCUE` | accesa salvo 0 | `maploc/src/mapper.rs` | `MAPLOC_WATCHDOG_RESCUE=0`: the watchdog judges a window at the carried pose alone, as before 2026-09-28. |
| `AB_REPO` | un valore (script) | `scripts/twin/houses/final_house.py` |  |
| `ACC_RANGE` | numero (default mapper_cfg.accumulator.max_range_m) | `maploc/examples/evaluate.rs` | `ACC_RANGE`: how far a beam may be and still reach the map. |
| `ALIGN_BEAMS` | number (unset: none) | `maploc/examples/align_maps.rs` | `ALIGN_BEAMS` and `ALIGN_RESID` override the two knobs. |
| `ALIGN_RESID` | number (unset: none) | `maploc/examples/align_maps.rs` | `ALIGN_BEAMS` and `ALIGN_RESID` override the two knobs. |
| `ASK_EVERY_S` | numero (default 60.0) | `maploc/examples/wake_match.rs` | The homecoming's map-to-map question, asked offline, with the answer known: a recording replayed from `start_s` into a FRESH map — a duck woken there, its map starting where it stands — and every `ASK_EVERY_S` the fresh … |
| `ASK_FOR_S` | numero (default 900.0) | `maploc/examples/wake_match.rs` | `ASK_EVERY_S` (60) and `ASK_FOR_S` (900) set the cadence and the span. |
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
| `MAP_LOAD_AT_S` | number (unset: none) | `quack-nav/examples/drop_replay.rs`, `maploc/examples/trajectory.rs` | `MAP_SESSION` / `MAP_LOAD_AT_S` as `maploc`'s `trajectory` example: the saved map the session resumed on, loaded when the daemon loaded it. |
| `MAP_LOAD_CARRY` | accesa salvo 0 | `maploc/examples/trajectory.rs` |  |
| `MAP_SESSION` | un percorso, o un valore | `quack-nav/examples/drop_replay.rs`, `maploc/examples/evaluate.rs`, `maploc/examples/trajectory.rs` | `MAP_SESSION` / `MAP_LOAD_AT_S` as `maploc`'s `trajectory` example: the saved map the session resumed on, loaded when the daemon loaded it. |
| `MAX_T` | numero (default f32::INFINITY) | `maploc/examples/evaluate.rs` |  |
| `MICRODUCK` | un valore (script) | `scripts/twin/houses/run_house.py` |  |
| `MICRODUCK_RL` | un valore (script) | `scripts/twin/houses/run_house.py`, `scripts/rl/twin_ab.py` |  |
| `MICRODUCK_START` | un valore (script) | `scripts/twin/viewer/maploc_overlay.py` |  |
| `MIN_RES_DEG` | numero (default 1.5) | `maploc/examples/submap_fit.rs` |  |
| `MIN_RES_M` | numero (default 0.03) | `maploc/examples/submap_fit.rs` |  |
| `ODOM_LOG` | un percorso, o un valore | `maploc/examples/trajectory.rs` | `ODOM_LOG=<file>`: the raw odometry the mapper was fed, on the Unix clock, for its increments against the truth's. |
| `ODOM_SIGMA_XY` | numero (default slam_cfg.odom_sigma_xy) | `maploc/examples/evaluate.rs`, `maploc/examples/trajectory.rs` | How much the graph believes odometry against a closure: a closure 5° wrong turned casa_arredata's first session 5° and walked the pose 0.47 m off across the living room (x13, 2026-09-29), odometry being near perfect on t… |
| `ODOM_SIGMA_YAW` | numero (default slam_cfg.odom_sigma_yaw) | `maploc/examples/evaluate.rs`, `maploc/examples/trajectory.rs` |  |
| `ONLY` | un valore (script) | `scripts/twin/odoprobe.py` |  |
| `OTHER_HOUSE` | un valore | `maploc/examples/wake_match.rs` |  |
| `OUT_SESSION` | un percorso, o un valore | `maploc/examples/evaluate.rs` | `OUT_SESSION=<file>`: the map this replay built, saved the way the robot saves one — so a bench run and a live run are scored by the same tools (quacksat's mapquality.py, dump_frame). |
| `POLICY_DIR` | un valore (script) | `scripts/twin/houses/run_house.py` |  |
| `PORT` | un valore (script) | `scripts/rl/twin_ab.py` |  |
| `POSEERR_DT` | un valore (script) | `scripts/twin/houses/poseerr.py` |  |
| `PROPTEST_CASES` | numero (default 256) | `quack-nav/tests/route_properties.rs` | `PROPTEST_CASES=5000` for a long hunt. |
| `QN_REPO` | un valore (script) | `scripts/twin/houses/run_house.py` |  |
| `QUACK_NAV_SOCKET` | un valore (script) | `scripts/twin/viewer/body_with_map.py` |  |
| `REJECT` | numero (default slam_cfg.optimizer.reject_sigmas) | `maploc/examples/evaluate.rs` |  |
| `REJECT_MAX` | numero (default slam_cfg.optimizer.reject_max as f32) | `maploc/examples/evaluate.rs` |  |
| `RELOC_AGREE` | numero (default mapper_cfg.relocalize_agree_windows as f32) | `maploc/examples/evaluate.rs` |  |
| `RELOC_DEBUG` | presente = accesa (qualsiasi valore) | `maploc/src/mapper.rs`, `maploc/src/relocalize.rs` |  |
| `REPLAY_HEAD_DT_MS` | number (unset: none) | `maploc/src/bench.rs` | `REPLAY_HEAD_DT_MS=<ms>`: each depth frame takes the head's pose from the robot-state sample nearest its own time plus this, instead of the last sample before it — to measure what the pairing of the head with the frames … |
| `ROUNDS_ONLY` | un valore (script) | `scripts/twin/houses/final_house.py` |  |
| `ROUTE_POINTS` | presente = accesa (qualsiasi valore) | `quack-nav/examples/route_on_map.rs` | with `TRUTH_HOLES=<truth.json>` (the twin's truth) also to a true hole, and with `ROUTE_POINTS=1` the route. |
| `SAVE_SESSION` | un percorso, o un valore | `maploc/examples/trajectory.rs` | `SAVE_SESSION=<file>`: the map the replay built, saved as the live daemon saves it — for `dump_frame` and `map_vs_truth.py`. |
| `SLANT` | number (unset: none) | `maploc/examples/submap_fit.rs` | `SLANT=x0,x1,y0,y1`: how many of this submap's wall cells fall in that box. |
| `SPOT` | un valore (script) | `scripts/twin/odoprobe.py` |  |
| `STATE` | un valore (script) | `scripts/rl/twin_ab.py` |  |
| `TRACK` | 1 accesa, altrimenti spenta; assente: mapper_cfg.tracking.enabled | `maploc/examples/evaluate.rs` |  |
| `TRACK_COND` | numero (default mapper_cfg.tracking.min_conditioning) | `maploc/examples/evaluate.rs` |  |
| `TRACK_IMPROVE` | numero (default mapper_cfg.tracking.min_improvement) | `maploc/examples/evaluate.rs` |  |
| `TRACK_LOG` | un percorso, o un valore | `maploc/examples/trajectory.rs` | `TRACK_LOG=<file>`: the tracked pose every 0.2 s, and every note by name the moment it comes (a rest's watch and end in full) — to see what moves a pose no correction or closure accounts for. |
| `TRACK_MAX` | numero (default mapper_cfg.tracking.max_correction_m) | `maploc/examples/evaluate.rs` |  |
| `TRACK_MAX_AFTER` | numero (default mapper_cfg.tracking.max_residual_after_m) | `maploc/examples/evaluate.rs` |  |
| `TRACK_MIN_BEFORE` | numero (default mapper_cfg.tracking.min_residual_before_m) | `maploc/examples/evaluate.rs` |  |
| `TRACK_PRIOR_XY` | numero (default mapper_cfg.tracking.prior_sigma_xy) | `maploc/examples/evaluate.rs` |  |
| `TRUTH_HOLES` | un valore | `quack-nav/examples/route_on_map.rs` | with `TRUTH_HOLES=<truth.json>` (the twin's truth) also to a true hole, and with `ROUTE_POINTS=1` the route. |
| `TWIN_REPO` | un valore (script) | `scripts/rl/twin_ab.py` |  |
| `TWIN_WORK` | un valore (script) | `scripts/twin/houses/aggregate.py`, `scripts/twin/houses/final_house.py`, `scripts/twin/houses/modes_test.py` … |  |
| `UNC_BEAMS` | numero (default u.independent_beams) | `maploc/examples/trajectory.rs` |  |
| `UNC_FLOOR` | numero (default u.match_floor_m) | `maploc/examples/trajectory.rs` |  |
| `UNC_SKIP` | numero (default u.skip_recent_submaps as f64) | `maploc/examples/trajectory.rs` |  |
| `UNC_XY` | numero (default u.xy_var_per_m.sqrt()) | `maploc/examples/trajectory.rs` |  |
| `UNC_YAW_M` | numero (default u.yaw_var_per_m.sqrt()) | `maploc/examples/trajectory.rs` |  |
| `UNC_YAW_RAD` | numero (default u.yaw_var_per_rad.sqrt()) | `maploc/examples/trajectory.rs` |  |
| `VIEWER` | un valore (script) | `scripts/rl/twin_ab.py` |  |
| `WAKE_MODE` | un valore (script) | `scripts/twin/houses/wake_bench.py` |  |
| `WAKE_SPAWNS` | un valore (script) | `scripts/twin/houses/wake_bench.py` |  |
| `WAKE_TURN` | un valore (script) | `scripts/twin/houses/wake_bench.py` |  |
