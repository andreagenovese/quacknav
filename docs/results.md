# Results — what quack-nav does today, measured

The numbers of the preview release (branch `turn-in-place`, 2026-09-25), all
on the **MuJoCo twin** of the Microduck (Pollen's released robotd
daemon-v0.14.4, `microduck_rl`'s body server) unless said otherwise. The
physical duck has not run this code yet. Every figure below comes from a
script in `scripts/twin/` or the paper twin, and can be run again.

## The criteria

A release is "working" when all of these hold on the three test houses.
Where a criterion is met the table says so; where it is not, how far off it
is — the share of criteria met is the progress we report from release to
release.

| # | Criterion | Today | |
|---|---|---|---|
| 1 | No fall, exploring or going places, on the three houses | 0 falls in 11 exploration sessions (5 h 30) and 51 journeys | ✅ |
| 2 | Homecoming: never a wrong pose; ≥ 90 % confirmed | 0 wrong (every confirmation 0–20 cm from the truth); 15 of 17 boots confirmed (88 %): two stood down in casa_arredata | ⚠️ |
| 3 | ≥ 90 % of go_to arrive | 46 / 51 (90 %): casa_libera 15/15, casa_arredata 16/18, house2 15/18 | ✅ |
| 4 | ≥ 90 % of every room's floor mapped, exploring a charge at a time | house2 93–99 % (four sessions), casa_libera 99–100 % (done by itself after three); casa_arredata's bathroom 45 % | ⚠️ |
| 5 | No phantom drop on the books | 1 of 64 (house2: the stairwell's west rim booked 34 cm into the passage beside it) | ⚠️ |
| 6 | Map walls within 5 cm of the truth on average | house2 4.5 cm, casa_libera 3.1 cm; casa_arredata 17 cm after a resume (3.8 cm before it) | ⚠️ |
| 7 | Documented: what it does, how it was measured, what it does not yet do | this page, ADR 0008, the README | ✅ |

**3 of 7 met, 4 in part: 5 of 7 counting a partial as half — 71 %.**

## The houses

- **house2** — the `microduck_rl` apartment: six rooms and a corridor with a
  stairwell (a 0.40 × 0.70 m hole) in it; the passages beside the hole are
  0.54 m (west) and 0.44 m (east) wide.
- **casa_libera** — five bare rooms, six doors, no furniture, no holes.
- **casa_arredata** — five furnished rooms and a corridor: two narrow gaps
  (0.5 and 0.6 m), a 7 cm object on the floor, a stairwell in the corridor
  (a 0.49 m passage beside it on the way to the bathroom) and a sunken
  corner in the living room.

Truth for scoring — walls, holes, rooms — is the scene itself.

## How it is measured

1. **Exploring, a charge at a time**: from nothing, sessions of 30 minutes
   (the battery, on the twin), up to four; between sessions the twin is
   restarted and the duck must find the saved map and itself on it
   (homecoming) before it explores on. After each session: the share of
   each room's floor the map knows (from the recording, against the scene),
   the share the duck itself reports, and falls.
2. **Declared complete**: if the duck has not found the house done by
   then, the user's "exploration complete" closes the map as it is.
3. **The drop book** — every drop on the books against the true rims:
   within 10 cm, within 20 cm, further (a phantom).
4. **Coming home and going places**: three restarts on the finished map
   (the homecoming must freeze it and explore nothing), each followed by a
   go_to to every room and back; the pose against the truth at every
   confirmation.
5. **The A/B**: the same restarts and tours on `main`'s build (6bed9b8),
   the same map, the same book.

## The numbers

### Exploring, a session of 30 minutes at a time

| House | Session | Homecoming | Reported | Truth | Falls |
|---|---|---|---|---|---|
| house2 | 1 | — | 48 % | 61 % | 0 |
| house2 | 2 | home, explored on (175 s) | 56 % | 65 % | 0 |
| house2 | 3 | home, explored on (95 s) | 85 % | 94 % | 0 |
| house2 | 4 | home, explored on (85 s) | 89 % | 96 % | 0 |
| casa_arredata | 1 | — | 59 % | 69 % | 0 |
| casa_arredata | 2 | home, explored on (155 s) | 65 % | 87 % | 0 |
| casa_arredata | 3 | no pose, stood down (972 s) | — | — | 0 |
| casa_arredata | 4 | no pose, stood down (977 s) | — | — | 0 |
| casa_libera | 1 | — | 81 % | 87 % | 0 |
| casa_libera | 2 | home, explored on (170 s) | 96 % | 100 % | 0 |
| casa_libera | 3 | home, explored on (190 s) | 96 % | 100 % | 0 |

"Reported" is the duck's own `house.percent_mapped`; "truth" the rooms'
known floor weighted by their area. casa_libera found itself done in the
third session; house2 was declared complete by the user after the fourth
(89 % reported, 96 % true); casa_arredata's map was declared complete by the
protocol itself after the second — its two next boots could not find
themselves on it to take the user's word.

### The drop book after exploring

| House | Drops | On the rim (≤ 10 cm) | Near (≤ 20 cm) | Phantom (> 20 cm) |
|---|---|---|---|---|
| house2 | 33 | 29 | 3 | 1 |
| casa_arredata | 31 | 30 | 1 | 0 |
| casa_libera | 0 | 0 | 0 | 0 |

### Coming home and going places on the mapped house (A/B)

| House | Build | Homecomings confirmed | Pose vs truth | go_to arrived | Median | Falls |
|---|---|---|---|---|---|---|
| house2 | new | 3/3 | 0.08–0.13 m | 15/18 | 106 s | 0 |
| house2 | main | 3/3 | — | 10/18 | 89 s | 0 |
| casa_arredata | new | 3/3 | 0.12–0.17 m | 16/18 | 111 s | 0 |
| casa_arredata | main | 3/3 | — | 7/18 | 101 s | 0 |
| casa_libera | new | 3/3 | 0.07–0.11 m | 15/15 | 111 s | 0 |
| casa_libera | main | 3/3 | — | 15/15 | 66 s | 0 |

### Trajectory error, the standard way (ATE, RPE)

Added 2026-09-25 (ADR 0009, step 0), from the same exploration sessions:
the pose against the twin's truth every 5 s, as ATE (absolute trajectory
error, RMSE, as recorded and after the best rigid alignment) and RPE (the
error of the motion over each metre walked). "Live" is what the duck
reported during the run; "replay" is the same recording replayed through
the mapper on the bench (`maploc/examples/trajectory.rs`), which gives the
same numbers every time and is what phase two's changes are measured on.
The live figure is a little worse because, in these sessions (before
2026-09-29), the live pose was read from the map frame, published about
once a second, while the truth is read at the instant: up to a second of
walking at 0.12 m/s is in it. Since 2026-09-29 the pose also goes out
between frames every 50 ms (`map.pose`, see below). These files carry
no heading, so the RPE is of the displacement in the world frame; the
sampler records the heading from now on.

| House | | Walked | ATE RMSE | aligned | max | RPE per metre |
|---|---|---|---|---|---|---|
| house2 | live | 116 m | 0.143 m | 0.117 m | 0.57 m | 0.090 m (9.0 %) |
| house2 | replay | 118 m | 0.127 m | 0.111 m | 0.51 m | 0.082 m (8.2 %) |
| casa_arredata | live | 53 m | 0.164 m | 0.118 m | 0.37 m | 0.088 m (8.8 %) |
| casa_arredata | replay | 52 m | 0.126 m | 0.098 m | 0.35 m | 0.076 m (7.6 %) |
| casa_libera | live | 69 m | 0.119 m | 0.094 m | 0.30 m | 0.083 m (8.3 %) |
| casa_libera | replay | 63 m | 0.101 m | 0.080 m | 0.28 m | 0.079 m (7.9 %) |

`scripts/twin/houses/traj_metrics.py` computes them from a sampler file and
writes TUM trajectories for `evo`, which gives the same ATE to the
millimetre (checked on house2's sessions).

### Phase two, step 1: the pose's uncertainty, measured

Developed on `phase-2` (ADR 0009), now on `main`; on the replay bench, the
same sessions as above.

**A covariance on the pose.** maploc keeps a 3×3 covariance (x, y, yaw) as
an EKF does: odometry grows it (5 cm per √metre, 1.7° per √radian turned),
each window the map judges shrinks it by the scan matcher's normal matrix
over the residual. It changes no decision — the replayed trajectories are
the same to the byte — and `robot.map_status` reports it as
`pose_uncertainty` (one standard deviation, and the direction the pose is
least sure of). Checked against the truth with the NEES (2 is honest, and
95 % of the errors inside the 95 % ellipse):

| House | Session | | σ (median) | NEES | inside 95 % | r(σ, error) |
|---|---|---|---|---|---|---|
| house2 | 1 | fresh | 0.070 m | 2.35 | 88 % | +0.08 |
| casa_arredata | 1 | fresh | 0.075 m | 1.54 | 98 % | −0.25 |
| casa_libera | 1 | fresh | 0.081 m | 1.42 | 98 % | +0.02 |
| house2 | 2 | resumed | 0.067 m | 3.21 | 88 % | +0.59 |
| house2 | 3 | resumed | 0.071 m | 4.75 | 74 % | +0.16 |
| house2 | 4 | resumed | 0.068 m | 2.35 | 89 % | +0.08 |
| casa_arredata | 2 | resumed | 0.072 m | 6.08 | 64 % | +0.46 |
| casa_libera | 2 | resumed | 0.071 m | 4.16 | 79 % | −0.38 |
| casa_libera | 3 | resumed | 0.081 m | 0.60 | 100 % | +0.62 |

Honest in the mean on a fresh map; too sure of itself on a resumed one,
where the error is mostly the saved map's own offset from the house, which
no window can see. And not yet an alarm: its correlation with the error
moment by moment ranges from −0.38 to +0.62. Two things were learnt on the
way. Weighted as the scan matcher's `H` has it, a window counted every one
of its beams and the σ sat at 2.5 cm against errors of 8 (NEES 30–60): it
is weighted as one beam at 8 cm now. And a window judged against the map it
has just drawn agrees with the drifting pose that drew it — the σ ignored
the drift entirely (r ≈ 0) until windows were judged only against the
submaps older than the last twelve.

**The valley test against the Hessian's eigenvalues.** 118 relocalization
decisions replayed (6 resumed sessions, 19 boots of the final tours), each
against the truth:

| Decision | | right pose | wrong pose |
|---|---|---|---|
| confirmed | 25 | 25 | 0 |
| refused by the valley test | 91 | 84 | 7 |

The valley test is very cautious — 92 % of what it refused was right,
most within 10 cm — and that is why coming home was slow, and failed twice
in casa_arredata (since 2026-09-29 the shadow map, below, gives the windows
a seed and halves the wait). But the Hessian cannot replace it: the 7 wrong poses (all
in casa_arredata, 6 in one boot, an alias about 4 m off) are well
conditioned, eigenvalue ratios up to 0.71; they are another basin that
fits, not a direction that slides. That is a global question, for step 4's
multi-hypothesis localization. The valley test stays.

**Settling after a resume** — a resumed session corrects its pose but inks
nothing until two windows in a row agree and move it less than 2 cm and
1° — was measured on the six resumed sessions and is **off** by default
(`MAPLOC_SETTLE=1` on the bench, removed 2026-09-30):

| Session | walls mean / p90, off | on | ATE, off | on |
|---|---|---|---|---|
| house2 2 | 6.5 / 15.9 cm | 4.6 / 7.9 cm | 0.158 m | 0.112 m |
| house2 3 | 6.3 / 18.7 | 6.3 / 16.1 | 0.144 | 0.157 |
| house2 4 | 4.6 / 9.6 | 4.8 / 10.9 | 0.099 | 0.109 |
| casa_arredata 2 | 6.0 / 13.9 | 6.9 / 16.1 | 0.159 | 0.178 |
| casa_libera 2 | 3.0 / 5.5 | 3.1 / 5.2 | 0.124 | 0.115 |
| casa_libera 3 | 3.1 / 5.2 | 3.1 / 5.2 | 0.057 | 0.064 |

One session much better, the rest even or a little worse — and worse on
the one it was written for, because casa_arredata's second session was not
harmed by its resume at all (see the known limits below).

### Since the release (2026-09-28..30)

On the twin and on the replay bench; the release's tables above are
unchanged.

- **The pose at 20 Hz** (e92aa14). mapd sends a light `map.pose` every
  50 ms between the 1 Hz `map.frame`s; the map lane folds it into the
  frame of the same seq, and after a wipe, load or adopt no pose goes out
  until the new map's first frame. The live yaw error as sampled went from
  17° RMS (a pose up to a second old, in turns) to about 2°.
- **Replay as live** (f38b341). The `.mdlg` recording stamps robotd's and
  tofd's clocks (odometry records of 53 bytes; the 45-byte ones are still
  read) and the bench pairs the head as live does (interpolation at the
  frame's stamp, the 5 ms lead, the pending queue). A stamped session
  replays within 2–4 cm (median) of the live pose.
- **The homecoming's adoption rule** (3b5d4df): overlap at least 0.50
  (was 0.70), margin at most 0.50 (was 0.80), an ask every 60 s (was
  180; the twin's 120), three agreeing asks as before. On 27 replayed
  wakes (`maploc/examples/wake_match.rs`, 920 asks, 655 of them right):
  the old rule passed 565 right answers and 9 of the other house, the new
  one 626 and none wrong.
- **The shadow map** (10f5a22, 2247634, 9c37473). A mapper resumed lost on
  a saved map keeps a fresh map of its walk and every 30 s asks the saved
  map where it fits (`align::match_maps`). Two agreeing answers at margin
  ≤ 0.5, or four at ≤ 0.8, once the duck has walked 0.5 m, give a soft
  seed — the fit composed with the odometry since the shadow began — that
  two windows confirm. `MAPLOC_SHADOW=0` turns it off. 37 wakes replayed:
  confirmed 13 → 35, median 123 → 92 s, none wrong. On the twin's wake
  bench, off (before) and on:

  | Bench | | right | median casa_arredata | median apartment | wrong | falls |
  |---|---|---|---|---|---|---|
  | w3, spawns across the house | before | — | 174 s | 192 s | — | — |
  | w3 | shadow | 11/12 | 87 s | 105 s | 0 | 0 |
  | w4, the same spawns turned 180° | before | — | 135 s | 126 s | — | — |
  | w4 | shadow | 12/12 | 105 s | 123 s | 0 | 0 |

- **A relocalization's jump no longer drags its own pose** (7e2825b).
  After a relocalization the jump froze the session's last submap; its
  closures moved the old chain, and the newly opened submap (a leaf)
  pulled the just-confirmed pose 0.43 m and 7° off — at the north end of
  the apartment's corridor (x17, session 4) the duck tracked 0.55 m off
  what looked like an alias and was not. The new node is now joined as a
  relocalization and that tick's optimization does not move it. 20
  replayed sessions: mean ATE 0.1045 → 0.0975 m; the x17 case 0.242 →
  0.107 m.
- **A covered sensor is not a hole** (953285b). A valid ToF zone under
  30 mm is the sensor against something; a frame with a quarter or more
  of such zones proposes no drop. The apartment's duck with its head over
  the bed's blanket read 0–1 cm in all 64 zones and had booked 18 phantom
  holes on the bed (x16).
- **A new house, casa_grande, for the whole stack** (d60e067, 9016e8c):
  9 x 7 m, seven rooms, a corridor 1.2 m wide that turns 90°, furniture
  with 0.6 m or more around it, two low things (7 and 25 cm), a
  stairwell and a sunken corner, and nothing that blocks (every goal
  joins every other with 0.35 m of clearance; `gen.py ... casa_grande`).
  Never used to tune anything; on the twin, with all of the above: four
  exploration sessions and two rounds of journeys, 16/16 arrived (median
  84 s), 0 falls; the wake bench from its eight goals 8/8 right, median
  84 s (72–111), 2–16 cm; the map's walls 99 % on the truth, 95 % of the
  true floor known, the rooms 93–98 %, 0 phantom drops.
- **A session that has been everywhere it can finds the house mapped**
  (fec3863). Frontier cells are always left on a furnished map (the band
  of uncertain cells along every wall and piece of furniture), and the
  unknown left sums to the furniture's footprint and the holes (10.3 m²
  in casa_grande): its last sessions ended "stuck" and the house was
  never done. Now what decides is the largest unknown piece touching
  known floor — 1.0–3.9 m² on the finished maps of three houses, 5.5–7.3
  on first sessions still exploring: no frontier within reach and no
  piece of 4.5 m² or more, and the house is mapped.

- **Tried and left off.** Loop closures' heading sigma at 0.5 rad: better
  on 8 replayed sessions, worse on 4 more; back to 0.24
  (`MAPLOC_LOOP_SIGMA_YAW`, removed 2026-09-30). maploc's parameters are judged on 12
  sessions or more from now on. A strike of booked holes where the floor
  was later seen (`QK_FLOOR_STRIKE=1`, removed 2026-09-30): under an unseen 11–15 cm pose
  error it also struck true rim points.
- **daemon-v0.15.0** (API 37) is validated on the twin only on the branch
  `microduck-015`: four sessions per house, no regression. There a
  turning head was measured to cost the map next to nothing (3.2–3.3 cm
  of residual from 0.05 to 1 rad/s; 3.46 cm on the 1 % of frames above).
  `main` is pinned to it since 2026-10-01.

### Overhangs and stairwell corners (2026-10-07)

On the MuJoCo twin, rounds of journeys on the books of an earlier
exploration (`final_house.py`, `ROUNDS_ONLY=1`), the frozen `main` as the
control on the same maps.

- **The nose against something the map does not have** (44f9902). Under
  apartment's coffee table (its top 0.14–0.22 m above the floor, the head
  at 0.25 m) the map has free floor; the body pushed on the table's edge
  for minutes, sliding a few centimetres a step, so the "under a
  centimetre" stall never counted, and the pose slid with it — 0.32 m off
  after three minutes in one round, then the duck walked into the hole
  with it. The step log now carries what the depth sensor sees in the lane
  (`ahead`): the edge at 0.10 m in 7–9 of 8 frames for some 40 steps. On a
  journey, two forward steps in a row with something in the lane within
  0.15 m in most of the last 0.6 s' frames are a stall: a turn of 20° at
  least, and what was seen goes on the books unless a map wall is within
  0.15 m of it or a booked drop within 0.5 m (the exploration keeps its
  rules). Four rounds a house: it fired once a round in apartment, at the
  table (24 of 24 journeys arrived), never in casa_arredata, and 15 times
  in casa_ingombra (11 of 11, `main` 4 of 5).
- **A walked lane yields to a booked drop within 0.20 m** (9bf7356, was
  0.12). casa_arredata's stairwell had its north rim booked to x 2.45 and
  nothing at its north-east corner; the lanes walked there kept the bath
  route 9–12 cm from the true hole (`route_on_map` with `TRUTH_HOLES`), and
  the duck fell at that corner in two rounds, with 9–11 cm of pose error.
  At 0.20 the route keeps 16 cm (21 cm on the other run's book); no leg of
  the saved tours of apartment, casa_arredata or casa_libera grew or
  closed. Filling the gaps between booked rim points did nothing there:
  the chord runs inside the hole, not round its corner.
- **The A/B, identical tests** (`scripts/twin/houses/final_ab.sh`, 2026-10-07): the frozen
  `main` (aa400b0) and this branch side by side, two twins at a time on
  the same house, the same books, four rounds a house, and casa_ingombra
  two rounds each.

  | | apartment | casa_arredata | casa_ingombra | falls | median s (apartment / arredata) |
  |---|---|---|---|---|---|
  | `main` | 23 / 24 | 24 / 24 | 7 / 12 (3 refused) | 1 | 122 / 109 |
  | this branch | 24 / 24 | 24 / 24 | 10 / 12 (1 refused) | 0 | 120 / 99 |

  The touch fired 2–4 times a round in apartment and 12 times in
  casa_ingombra, never in casa_arredata; `main`'s one miss in apartment was
  g2 through the coffee table, its fall in casa_ingombra on the way to the
  office.

- **Booked before the bump, the prudent way** (branch `seen-ahead`,
  2026-10-07). On a journey, something the depth sensor keeps seeing in
  the lane within 0.30 m (in at least half of the last 0.6 s' frames,
  three at least) goes on the books where the map has free floor, no map
  wall within 0.15 m and no booked drop within 0.5 m — the scuff stall
  that came with it the first time stays out. On the quack-rl bench (420
  journeys) the stick went from 89.8 % to 94.3 % arrived, tip-overs 35 ->
  16; the pilot v3-r6 from 90.2 % to 94.0 %, 10 -> 4. The same A/B as
  above against `main` (2e3979e):

  | | apartment | casa_arredata | casa_ingombra | falls | mean s (apartment / arredata / ingombra) |
  |---|---|---|---|---|---|
  | `main` | 24 / 24 | 24 / 24 | 9 / 12 | 1 | 113 / 104 / 90 |
  | `seen-ahead` | 24 / 24 | 24 / 24 | 9 / 12 | 0 | 111 / 101 / 122 |

  It booked 5–13 things a round in apartment, 33 in casa_ingombra's two
  rounds, none in casa_arredata; `main`'s fall was a turn in place in
  apartment's corridor, no hole near. casa_ingombra's journeys took longer
  round what was booked.

- **A turn in place that learns its coast** (63385be, branch
  `stick-dither`). A turn closed on odometry ended 35–44° past it on the
  MuJoCo twin, whatever its size: 249 of `main`'s 432 journey turns were
  followed by a turn the other way, the duck dithering ±40° at rims and
  tables, and the earlier A/B's one fall of `main` came in such a dither.
  On a journey the map's yaw at the next leg now says how far the body
  went; the excess is folded into a coast (prior 17°, at most 40°) the
  next turns stop short by — learned, so the duck's own gait will set it.
  The same A/B against `main` (2d4574d):

  | | apartment | casa_arredata | casa_ingombra | falls | mean s (apartment / arredata / ingombra) | turns / undone (apartment + arredata) |
  |---|---|---|---|---|---|---|
  | `main` | 24 / 24 | 24 / 24 | 10 / 12 | 0 | 112 / 98 / 109 | 398 / 232 |
  | `stick-dither` | 24 / 24 | 24 / 24 | 11 / 12 | 0 | 90 / 89 / 87 | 200 / 5 |

  The coast settled at 25–35°.

- **Tried and not kept: the journey's rules on the exploration's travel**
  (branch `explore-rules`, 2026-10-07/08). The touch stall, the booking
  ahead and the turn's coast on the exploration's legs too, against
  `main` exploring each house from nothing (`scripts/twin/houses/explore_ab.sh`:
  sessions of 30 min, four at most, then two rounds of journeys on its own
  books). No fall and no phantom drop on either side, the rooms' coverage
  within a point or two; casa_arredata done by itself in three sessions
  (`main` not done in four), casa_libera in two (`main` in one); every
  journey after arrived on both. Even, so the exploration keeps `main`'s
  rules; one exploration a side is too few to call a point either way.

- **Tried and not kept: go_to books what it meets** (branch
  `journey-books`). Booking what the sensor sees ahead before the bump, a
  "scuff" stall (a step that moved under 40 % of a step), facing a hole
  before booking it and a last plan at the tightest margins: against
  `main`'s 47 of 48 journeys and no fall in four rounds a house, 18 of 24
  with 3 falls, then 23 of 24 with 1, then 20 of 24 with 1. A booking of
  a map wall's offset image pushed the route toward a stairwell; the
  scuff counted a step on open floor as a bump.
- **Twin noise.** Two falls of one verification came within 0.1 s of each
  other in two separate twins, both simulators slowed from 49 to 27–29
  odometry ticks a second the five seconds before: the host, not the duck.
  Verifications run two twins at a time from now on.

### daemon 0.16.1 and velstand (2026-10-08)

On the MuJoCo twin at Pollen's daemon-v0.16.1 with `microduck_rl` at
upstream `develop` and policy set v7; the same protocol as above (four
rounds a house on the same books, casa_ingombra two).

- **alpha, daemon 0.15.0 against 0.16.1**: 24/24 + 24/24 both;
  casa_ingombra 10/12 against 11/12; no fall either side.
- **velstand measured** in an empty arena (`scripts/twin/gaitprobe.py`):
  0.115 m/s at vx 0.3 (alpha 0.119), 6° of veer in 10 s straight (alpha
  25°; set v7 trained its bias out), 0.62 rad/s per unit of yaw walking,
  a turn in place at ±1.5 of 44–51°/s, the same dead zone as alpha.
  Robotd's odometry is within 1–2 % of the truth for both, straight, on
  apartment's rug and in step-and-stand legs (`odoprobe.py`).
- **Why velstand first failed**: a standing velstand duck is labelled
  "walk" (no standing network), so the mapper never saw it still and,
  since 0.16.1, robotd's idle glancing took the head — 0/24, the homecoming
  standing down after 16 min. Then, read as still the moment the twist
  ended, its windows began while the body still moved (0.1–0.55 s,
  `settleprobe.py`; alpha 0–0.3) and the 2 s stands kept 1.4 s of
  stillness: 9 pose corrections a round (alpha 30), the pose 0.3–0.5 m
  off in long journeys, once 1.3 m. With the settle (0.6 s) and stands
  that much longer: three rounds, 18/18, the pose 6 cm median, 34–41
  corrections a round.
- **alpha against velstand, the same code and stack**:

  | | apartment | casa_arredata | casa_ingombra | falls | pose (median / p90) | mean s (apartment / arredata) |
  |---|---|---|---|---|---|---|
  | alpha | 24 / 24 | 23 / 24 | 11 / 12 | 1 | 8 / 16, 7 / 11 cm | 92 / 90 |
  | velstand | 24 / 24 | 24 / 24 | 10 / 12 | 0 | 7 / 16, 6 / 10 cm | 103 / 98 |

  velstand's one miss, casa_ingombra's bedroom, ran out of time; alpha's
  fall was in casa_arredata.

### v0.3.0-rc1: the verifications against main (2026-10-09)

On the MuJoCo twin at daemon-v0.16.1, `main` (707db6f) against the release
candidate, two twins at a time, the same books.

| test | main | v0.3.0-rc1 |
|---|---|---|
| journeys, velstand (apartment + casa_arredata, four rounds; casa_ingombra two) | 48/48 + 11/12, no fall | 48/48 + 12/12, no fall |
| journeys, alpha | 48/48 + 11/12, no fall | 48/48 + 12/12, no fall |
| exploration from nothing, velstand (three houses, then two rounds of journeys) | coverage 61–100 % a room, 107 drops booked, none phantom, 34/34, no fall | the same coverage, 93 drops, none phantom, 34/34, no fall |
| the pilot against the stick, velstand, casa_ingombra two rounds | the stick: 4 of 5, then tipped over against a thing near the start (no hole) | the pilot v3-r7-velstand: 12/12, no fall |

- **alpha against velstand, exploring** (the same code, 2026-10-08): no
  fall and no phantom drop either way, the same final coverage; velstand
  slower in the first session (casa_libera done in two sessions against
  one; casa_arredata's bedroom at 11 % after one against 63 %), as its
  stands are 0.6 s longer. Its homecomings came closer: 21/21 confirmed,
  median error 3 cm (alpha 20/20, 8 cm), median 110 s (alpha 85).
- **The guard against a hole by a wall** (cliff.rs): a drop with an
  obstacle at its bearing within 0.25 m either side read as a box's edge;
  on the velstand-tuned bench a brain choosing at random stepped into a
  stairwell set against a wall (stairwell 300038). An obstacle explains a
  drop now only when it stands no farther than the drop + 5 cm, in the
  guard's test only (the books' booking by that rule had put phantom drops
  beside casa_ingombra's toy on 2026-10-06). Reckless brains: 0 holes in
  3,780 journeys under each of the velstand and the MuJoCo calibrations;
  the bench unchanged (the stick 93.3 %), the pilot v3-r7 96.7 → 97.1 %;
  paper twin 53.0 % / 30/30.

### Moving obstacles: four rules (2026-10-12/13)

People and pets are things the map never has: a duck meeting one books
it, and a booked person used to stay booked after leaving. Four rules,
each a knob (docs/knobs.md), now on by default:

1. **Seen ahead from 0.35 m** (`QK_SEEN_AHEAD_M`, 0.30 before): something
   in the lane is booked a little sooner, before the bump.
2. **A step back after a bump or a touch** (`QK_BACK_STEP_S`, 0.6 s; 0
   off): before turning away, only onto floor the map knows.
3. **Booked things forgotten on seen floor** (`QK_FORGET_FREE`, 0 off):
   what a journey booked leaves the books when the sensor sees the floor
   where it stood, three frames within a second. Drops are never
   forgotten.
4. **Approaching movers booked from 0.6 m** (`QK_MOVER_AHEAD_M`; 0 off):
   something in the lane coming nearer faster than the body walks.

**Bench** (`quack-rl`, 420 journeys a walk; the stick, main → the four
rules): velstand 94.8 → 94.0 % arrived, tip-overs 18 → 13, bumps a
journey 2.59 → 1.88; alpha (MuJoCo calibration) 93.1 → 94.3 %, tip-overs
17 → 13, bumps 4.66 → 3.45; no hole either way. The pilot is unchanged
within noise (95.2 → 96.0 %, 96.0 → 96.0 %). `passers`, a bench-only
family (someone in the doorway when the duck gets there, leaving 2-12 s
later): with rule 3 alone the pilot's mean time 71.7 → 67.0 s
(velstand) and 72.3 → 69.3 s (alpha).

**Twin** (velstand, `main` 4c5a2fa against the candidate, two twins at a
time, the main arm with the four knobs off):

| test | main | the four rules |
|---|---|---|
| journeys, apartment, four rounds | 24/24, median 114 s | 24/24, median 124 s |
| journeys, casa_arredata, four rounds | 24/24, median 99 s | 24/24, median 99 s |
| journeys, casa_ingombra, two rounds | 12/12, mean 105 s | 12/12, mean 104 s |
| exploring from nothing, three houses | coverage 92 / 79 / 97 %, one fall (apartment, session 3), 3 phantom drops | 92 / 81 / 95 %, no fall, 4 phantom drops |
| casa_viva (three movers), four rounds | 24/24, mean 97 s, 22 bump episodes | 24/24, mean 97 s, 18 |
| casa_porta (someone in the office's only door), four rounds | 24/24, mean 82 s, 6 bump episodes | 24/24, mean 83 s, 7 |

- casa_viva is casa_arredata with someone in the kitchen's door, someone
  walking the corridor and a cat; casa_porta has one person standing in
  the office's door, 0.7 m wide, who steps aside 10-20 s after the duck
  comes within 0.5 m (`scripts/twin/houses/gen.py`, the twin's
  `TWIN_MOVERS`). A bump episode is the body touching a mover, touches
  less than 2 s apart counted once.
- After exploring, the journeys on the house it made: 12/12 a house on
  both sides, but in casa_libera `main`'s homecoming stood down twice
  (after about 975 s), so its journeys there were not run; the
  candidate's 10/10.
- The apartment's phantom drops lie on the same strip by the stairwell
  as main's (0-2 there in earlier runs).
- **Rule 3 never fired on the twin.** A person booked as a few 5 cm
  points never seals a 0.7 m door: the planner finds no way at its usual
  margins, then goes through at the tightest the body allows once the
  person moves, and what was booked falls behind the duck, where the
  sensor never sees it again. Kept on for the bench's gain; its benefit
  on the twin is unshown so far.
- Verdict: no regression anywhere, fewer tip-overs and bumps on the
  bench, the arrivals within noise. Merged with the four rules on.
- A truncated last record (a recording stopped mid-write) made
  `maploc`'s `evaluate` panic and the coverage come out empty; it now
  evaluates up to it.

### The band: 2.5D at the duck's height (2026-10-14/16)

The band (README "Using it", `maploc::band`) is read by the navigation as
wall. Three twin runs, velstand, two twins at a time, four sessions of
30 minutes exploring from nothing then four rounds of journeys on the
house it made, apartment and casa_arredata:

| run | arm | apartment journeys | "at the tightest" a round | casa_arredata journeys | exploring declared done |
|---|---|---|---|---|---|
| 1 | main e24c6ef | 24/24, median 109 s | 0-1 | 24/24, median 104 s | neither house in 4 sessions |
| 1 | the band as first built | 24/24, median 99 s | 16-23 (rounds 2-4) | 24/24, median 94 s | apartment session 2, casa_arredata 4 |
| 2 | wary: beside a 2D wall, the band needs saturated evidence | 24/24 | 0-17 | 24/24 | casa_arredata 3; the apartment "stuck" twice |
| 2 | snap: returns near the submap's own 2D wall left to it | 24/24, median 109 s | 0 | 17/24 (bath 0/4) | apartment 3 |
| 3 | main e24c6ef | 24/24, median 114 s | 0 | 24/24, median 89 s | casa_arredata 4 |
| 3 | **snap (merged)** | **24/24, median 114 s** | **0** | **24/24, median 94 s** | **apartment 3, casa_arredata 3** |

No fall in any arm. "At the tightest" counts the planner going through
"at the tightest the body allows" for want of a way at its usual margins.

- **Why the first band slowed the apartment.** It thickened the walls by
  a cell or two on their free side: replayed, 67 % of the cells only the
  band blocked touched a wall of the 2D map, and their evidence was
  saturated, not noise. Two passages narrowed (the corridor along the
  bath's wall, a kitchen stool by the wall), and g4 took 220-231 s
  instead of about 110 in two rounds.
- **Wary did not help:** most of those cells were saturated, so a
  stricter threshold beside the walls left them, the slow journeys stayed
  and the duck declared itself sealed in on open floor twice.
- **Snap** removed them where it matters: no tight passage in any round.
  Its casa_arredata journeys in run 2 failed the bath 4/4 with "no way on
  the map"; the frozen map drawn back (`examples/band` on the saved
  session) had the bath reachable with the band at 15 cm of clearance, and
  the difference was the drop book: the corridor's hole before the bath
  door charted at 3 rim points against 9. Run 3, the confirmation, had
  the bath at 94 % from the first session and 24/24 journeys.
- Truth coverage per room at the end, run 3: apartment main 92-100 %,
  snap 94-100 %; casa_arredata main 62-100 %, snap 60-99 % (the bedroom,
  62 / 60 %, is as low in every arm of all three runs).
- The twin's depth sensor casts one ray per zone centre (microduck_rl
  `sim/tof.py`); the real sensor's zone is about 5.6° wide. How a wide
  zone grazing a wall lands in the band is unmeasured until hardware.
- Verdict: no regression in reach, falls or journey time, and the
  exploration ends sooner. The band ships with snap on.

### daemon 0.17.6 and policy set v8 (2026-10-19)

Pollen's daemon-v0.16.2 (#393) moved the policy floor to set v8, whose
velstand "tracks yaw commands"; ducks update themselves, so a duck in
December runs 0.17.x. Branch `daemon-0.17`: built against daemon-v0.17.6
(API 44), velstand's numbers per daemon line (`quack-duck/src/daemon/`),
`daemon = "auto"` choosing the line from robotd's `hello`. Frozen `main`
(7844b26) on daemon-v0.16.1 + set v7 against the branch on daemon-v0.17.6
+ set v8, velstand both, two twins at a time.

- **Set v8 measured** (`gaitprobe.py`, the arena, three trials, beside v7
  under the same load): 0.095 m/s at vx 0.3 (v7 0.115); 11–24° of veer to
  the left in 10 s straight; 1.1 rad/s per unit of yaw walking (v7 0.62);
  a turn in place at ±1.0 of 63–68°/s, where v7 barely moved, at ±1.5 of
  89–108°/s; 12–23° of coast after a spin. Still within 0.4 s of the
  twist's end, as v7. The 0.17 line asks 1.0 for a turn, trims -0.03.
- **The exploration first dithered on v8.** Its turns in place were closed
  on odometry with no coast, and v8's go on further (journeys: closed at
  30°, 77° round by the map; v7 64°): the duck overshot past its own
  threshold and turned back, 256 turns to v7's 112 in a first session,
  the apartment 58 % mapped against 83 %. On the 0.17 line the exploration
  now learns the coast as journeys do (`coast_while_exploring`; off on
  0.16 and for alpha). The numbers below are with it.

| test | main, 0.16.1 + v7 | branch, 0.17.6 + v8 |
|---|---|---|
| journeys, apartment, four rounds | 24/24, median 114 s | 24/24, median 124 s |
| journeys, casa_arredata, four rounds | 24/24, median 94 s | 24/24, median 104 s |
| journeys, casa_ingombra, two rounds | 12/12, mean 116 s | 12/12, mean 119 s |
| casa_viva (three movers), four rounds | 24/24, mean 95 s, 19 bump episodes | 24/24, mean 108 s, 29 |
| casa_porta (a person in a door), four rounds | 24/24, mean 83 s, 7 bump episodes | 24/24, mean 89 s, 10 |
| exploring from nothing: apartment | 92 %, done at session 3 | 80 %, stuck at session 3 (a 4.7 m² piece unreached) |
| exploring from nothing: casa_arredata | 65 %, stuck | 75 %, stuck |
| exploring from nothing: casa_libera | 99 %, done at session 1 | 98 %, done at session 1 |
| journeys on the house it made (apartment / arredata / libera) | 12/12, 10/12, 10/10 | 12/12, 12/12, 10/10 |
| wake-ups, apartment / casa_arredata (localize) | 6/6, 6/6; median 117 / 111 s | 6/6, 6/6; median 144 / 108 s |

No fall in any test on either side.

- **Slower by the walk**: v8 is 17 % slower forward, and the journeys took
  6–15 s more.
- **More bumps with movers** (29 against 19 in casa_viva, 10 against 7 in
  casa_porta), all with the person walking the corridor; per minute 0.67
  against 0.50. Earlier runs of the same test gave 18–22 on one side, so
  this is at the edge of the noise; it is the one figure to watch.
- **Exploring**: the floor known per room by the truth is level (the
  apartment 93–100 % against 92–100 % after session 2); the apartment's
  80 % is the duck's own share, held down by one unknown piece it found no
  way to; in casa_arredata the branch mapped more and reached the bath
  main's map had no way to.
- **The same build on daemon-v0.16.1** (`daemon = "auto"` took 0.16 from
  robotd): two rounds of apartment journeys, 12/12, median 119 s, no fall.
- Harness fixes found on the way: `twin.sh down` now stops what a lost pid
  file left (a robotd left over owned the socket, and the next one
  exited); `explore_ab.sh` starts each house from an empty state.
- Verdict: no regression in reach, falls or wake-ups on daemon 0.17.6;
  slower by the walk; the bumps with movers to be watched. The pilot was
  trained on v7 and does not fly on the 0.17 line until retrained there
  (`DAEMON=0.17 GAIT=velstand calibrate.sh`); the stick drives.

#### Retraining the pilot on the 0.17 line (2026-10-20)

The stick's traces on daemon-v0.17.6 + set v8 (casa_ingombra and
casa_arredata, three tours each, 34/36) went through `DAEMON=0.17
GAIT=velstand calibrate.sh`. Two faults of the simulator's showed first:

- **It turned in place only from |vyaw| 1.45** (set v7's ask): on the 0.17
  line a turn asks 1.0, so the simulated duck could not turn from a
  standstill, and `rl_calib` found no turns in the traces. The thresholds
  now follow the walk's `turn_in_place_rad_s` (alpha and set v7 unchanged),
  `rl_calib` takes `--gait`/`--daemon` and its prior on velstand 0.17 is
  the gait probe's (0.095 m/s, 1.1 rad/s per unit, 65°/s).
- **The stick's traces understate the pilot's curving steps**: a fit on
  them gave 0.66 rad/s per unit of yaw on a short step, where the pilot's
  own steps (±0.35, ±0.7) turn 0.88 on the twin. The second round was
  calibrated on the first pilot's traces.

| arm (twin, daemon-v0.17.6 + set v8) | casa_ingombra | casa_arredata |
|---|---|---|
| stick | 24/24, median 110 s (an earlier pass 22/24) | 24/24, 97 s |
| first pilot (stick's traces) | 19/24, mean 134 s | — |
| second pilot (its own traces) | 22/24, 114 s | 24/24, 105 s |

No fall anywhere. On the bench (the calibrated simulator) both passed
the gate (94.3 % and 94.5 % against the stick's 92.9 % and 91.2 %), on the
twin neither beat the stick: the first dithered at the two narrow
passages (turns left and right, waits, back-offs), the second missed two
journeys on time. Not shipped: on the 0.17 line the stick drives, as it
did through the A/B above. The pilots stay in `rl-runs/calib-v8{,b}`.

#### Round two on the 0.17 line: why the pilot lost, and the simulator made to see as the twin (2026-10-21..26)

Branch `pilot-why`. The question left on 2026-10-20 — why pilots that
pass the bench lose to the stick on the twin — was taken apart one
difference at a time, each measured on the twin: the stick against the
pilot of the same build, four tours of casa_ingombra and casa_arredata,
daemon-v0.17.6 + set v8, two twins at a time (`rl-runs/twin-work/P8/`).

- **Bookings 7.8 cm too near** (`9d59dfb`): kinematics measures a hit's
  range from the sensor, not from the trunk axis its doc names
  (pollen-robotics/microduck#430), so what the books held sat nearer the
  body than it was. Against frozen main, stick to stick (AB7): 24/24 on
  both sides in both houses; casa_ingombra mean 112 → 119 s, casa_arredata
  101 → 102 — within noise.
- **Bookings from the frame's pose, the stick beside a rim** (`fd38eb0`):
  a frame carries the odometry pose it was judged at, and a thing seen is
  booked from there, not from the body up to a curving step later (a bag
  had been booked 0.55 rad round, into the passage beside it); within
  0.5 m of a booked rim the stick walks the legs, where the pilot had
  dithered by casa_arredata's stairwell (+28 s to the bath, +33 s home).
  AB8: 24/24 both arms; pilot 115 s against the stick's 114 in
  casa_ingombra, 100 against 100 in casa_arredata.
- **The handover** (`52ac48a`, `95c66c9`, `e34bedd`): the stick takes the
  legs from a pilot that gets no nearer the goal by 0.15 m in 12 s, and
  keeps them until the body is 0.3 m from where it took over (30 s at
  most) — before that the pilot got them back on the spot every 30 s
  against casa_ingombra's basket and spent the bath's budget; and
  nothing seen ahead is booked within 1.5 s of a turn in place.
- **The simulator** (`1f0d284`, `5f8120b`, `3324b0c`, `9546216`,
  `19a8a6e`): a turn in place coasts on after its stop as set v8 does
  (23° asked, about 56° walked; `rl_calib` fits the coast); a tip-over
  gets up after 4 s with its pose lost a while, instead of ending the
  journey; the sensor's rows sit where the walk carries the head; steps
  turn by side and back-offs as measured; the sensor's frame noise is at
  most 0.02 m. The twin's houses are on the bench (`--families
  twin:<house>`: the twin's tour, each journey starting where the twin's
  last one stopped, its things put down since unmapped) and its other
  houses in training (`twinlike:<house>`); the gate also refuses more
  timeouts, tip-overs or seconds a journey than the stick.
- **Pilot v8l-s3** (`rl-runs/calib-v8l`: v8j's recipe from v8d, seeds 2–4,
  the bench's best): AB13 23/24 and 24/24; with the hold (AB14) 48/48
  arrived, no fall, casa_ingombra mean 110 s against the stick's 112,
  casa_arredata 96 against 101 — paired by journey 2.2 and 4.4 s faster.
  Bench (420 journeys, seeds 200000): pilot 98.6 % arrived, 6 tip-overs,
  1.47 bumps a journey; the stick 98.3 %, 18 and 3.08. The first pilot on
  the 0.17 line not slower than the stick.
- **A rule that won on the twin and lost on the bench** (`6433012`,
  reverted in `fca0e14`): the stick taking the legs within 0.5 m of a
  thing booked on the journey, where the pilot had turned, stepped back
  and waited about ten times a journey before casa_ingombra's bag and
  basket. AB15: 48/48, the pilot 5.2 and 6.2 s a journey faster than the
  stick, the time at the bag 23 → 12 s. On the bench (1,400 journeys,
  seeds 500000) 19 journeys worse against 9 better, arrivals 98.2 →
  97.5 %, tip-overs 40 → 60, bumps 2.80 → 3.69: the stick held beside a
  box too low for the walking sensor and pushed it to the timeout (seed
  500140, 147 bumps). A release when the stick got no nearer brought the
  arrivals back (98.0 %) but not the tip-overs (55). Reverted: nothing
  ships with a regression on the bench, and the twin's three seconds are
  for training to win.
- **The sensor against the twin's truth** (`TOF_TRUTH_LOG` in the twin's
  body server, `examples/tof_vs_twin`, `3b32539`): the twin's sensor walks
  pitched −8.5° and stands at −11°, 0.233–0.238 m up. The simulator's
  beams coming down over a low thing went on to the floor: walking within
  0.2 m of casa_ingombra's bag it returned 4 hits a frame against the
  twin's 10 at the twin's own poses; they meet the top now, 10 against 10.
  The calibration's pitch is −6° (`calib-v8m`): the floor zones match the
  twin walking at −3/−6° and standing at −9°.
- **Retraining on the corrected simulator**: v8m (200 updates from s3,
  seeds 2–4) beat s3 nowhere on the bench's twin houses (casa_ingombra s3
  104 s against the stick's 107; casa_arredata 83 against 88). v8n (600
  updates, seed 2): 98.3 % on the gate's bench against s3's 99.0 % on the
  same calibration, 5 tip-overs against 12 — not better; seeds 3 and 4
  were not run, because finalize's shield test fails on that calibration
  (next).
- **A gap the corrected sensor shows up** (`rl-runs/calib-v8n`): at the
  calibration's −6° the reckless-back brain falls into the stairwell of
  seed 300002 (at −3° it does not; at 600 seeds both give 6–7 falls in
  12,600 walks). The trace: the duck turns from the rim, the stick takes
  the legs 0.34 m from it and walks the route along the hole's side; the
  drops on the books sit 12–42 cm inside the hole, because a drop is
  booked at `range_m`, where the floor was expected, and not at the rim,
  which lies between the last floor the column saw (`edge_min_m`) and
  that — one row of the sensor nearer. The route keeps 0.20 m off a
  booked drop, so the body's edge can overhang the rim. `main` books the
  same way (the branch did not touch it); on the twin's A/B the rims come
  from the oracle, so the A/B does not see it, exploring from nothing
  would. To fix in a round of its own: book the rim between
  `edge_min_m` and `range_m`, then the bench's stairwell family, 600
  reckless seeds on both calibrations, and exploring from nothing on the
  twin.
- **The shields' test at 600 seeds** (`rl_eval --reckless`, 12,600 walks):
  7 hole falls after the revert, 6 before, three seeds common — the stick
  clipping the corner of a rim nothing had booked, a known limit (the
  route keeps 0.20 m off a booked drop, not off an unbooked one). finalize's
  60 seeds pass.
- **People walking between the rooms**: `casa_gente` and the movers that
  step aside or back (see the changelog, 2026-10-26). On the bench the
  pilot v8l-s3 arrives 89 % of the time with 2.2 contacts with people a
  journey, the stick 84 % and 8.0; the losses are the duck bumping someone
  who waits in a door — the behaviour the next round trains. The twin
  (AB16, the stick against pilot v8l-s3 on the same build, four tours):
  the stick 23/24, mean 110 s, 41 contact episodes with people (touches
  less than 2 s apart counted once); the pilot 23/24, mean 119 s, 33
  episodes; no fall. Each missed one journey to the time budget — the
  stick the bath, 741 touches in the passage by the stairwell, where the
  person waiting for it had no room to step aside; the pilot the kitchen,
  pushed about by two people in the corridor's west end. Where the people
  cross the way (kitchen, bedroom) the pilot is 30–50 s slower than the
  stick, past the stairwell 54 s faster: it meets people with more waits
  and back-offs, the stick with more pushing. The bench had said 89 %
  against 84 % and 2.2 against 8.0 contacts a journey: the twin agrees on
  the order and on the stick's pushing, and is kinder on arrivals.
- **Against frozen main** (`dcafd35`, AB17: main's stick on one twin, the
  branch's pilot v8l-s3 on the other, four tours of each A/B house, the
  same books): no fall on either side, 24/24 arrived everywhere. The
  pilot mean 113 s against main's stick 117 on casa_ingombra, 96 against
  100 on casa_arredata; paired by journey 4.2 and 3.9 s faster, faster in
  14 and 18 journeys of 24; the pose at arrival 0.17 m off the truth on
  both sides. Per goal the pilot gives up a little where the way is clear
  (home: 102 against 90 s on casa_ingombra) and gains where it is not
  (office 91 against 108, bedroom 129 against 140). That is the number
  the user asked for — a pilot trained on a stick that does not do worse
  than the stick — and what shipped v8l-s3 as the 0.17 velstand pilot
  (`quack-rl/pilots/v8l-s3-velstand/`).

## Known limits

- **A drop is booked where the floor was expected, not at the rim**
  (2026-10-26). The sensor bounds a hole's edge between the last floor a
  column saw (`edge_min_m`) and where it expected the next (`range_m`),
  and the books take the latter: one row of the sensor inside the hole,
  12–42 cm in the stairwell of the bench's seed 300002, where the route's
  0.20 m off the booked drops lets the body's edge overhang the rim and a
  reckless brain falls on the corrected calibration (−6°). `main` books the
  same way; on the twin's A/B the rims come from the oracle, so it does
  not show there; exploring from nothing would show it. To fix in a round
  of its own (the passage beside casa_arredata's stairwell is 0.49 m: a
  rim booked nearer may shut it).
- **A rim booked where the pose had it.** About 1 in 50 drops lands 20–35 cm
  off the true rim (the pose's error when it was seen). In the open that
  costs nothing; in a passage beside a hole it narrows the passage for the
  planner, and house2's goal beyond the stairwell (g4) was missed in all
  three rounds — by `main`'s build too, on the same book. A passage the duck
  has walked stays open (the lanes), but not one it has only looked at.
  Counted again on 2026-10-18, the four explorations of the band's
  confirmation run: one book (main, the apartment) has 6 rim points 13–28
  cm into the passage east of the stairwell, the other three none beyond
  16 cm, and every journey arrived. Not fixed: moving a booked rim on later
  sightings would let the same pose error take a true hole off the books.
- **Fixed after the release (2026-09-25): a resumed session could break
  the map.** casa_arredata's second session took its walls from 3.8 to 17
  cm off the truth. The cause was not the pose it came home with but a
  bug in maploc: when a boot finds the duck elsewhere, the saved map's last
  (empty) submap is re-anchored there, and the odometry edge into it was
  left saying where it had been — 3.7 m away in a replayed case — until
  the first loop closure let the optimizer satisfy it and the pose jumped
  1.7 m. Replayed, eleven recorded sessions of the three houses: wall
  error 6.9 -> 4.6 cm on average, the worst trajectory error 1.95 -> 0.16 m;
  mapped again on the twin with the fix, casa_arredata's four sessions
  improved the map one after the other (3.5 cm of wall error, rigidly
  fitted) and every journey that started arrived, the bathroom included.
  The tables above are the release's, measured before the fix.
- **Coming home in a regular house can take long, or fail.** The valley test
  refuses a pose a long plain wall cannot pin down: no wrong pose was
  believed, but in casa_arredata (a generated, very regular house, its
  bathroom half mapped) two boots of seventeen stood down after 16 minutes.
  Since 2026-09-29 the shadow map gives the windows a seed: 23 of 24
  wakes on the twin's bench confirmed right, none wrong, medians 87–123 s
  against 126–192 s before. The apartment's duck woken east of the
  stairwell, once slow (the windows refuted a right seed for minutes),
  measured again on 2026-10-18 (velstand, the map of the band's
  confirmation run, four spots by three headings): 12 of 12 right, none
  wrong, median 108 s, 78–144 s but one: 289 s in the office facing
  north, boxed in five times before a window refuted a first fit.
- **Some loop closures measure the heading wrong** — a few degrees, and
  the live pose drifts with it: phantom holes north of casa_arredata's
  stairwell (x13) came from a pose 35 cm off after a closure 5° wrong, and
  maploc's covariance did not flag it (σ 0.08 m at 0.35 m off). Why is
  still unknown — nothing a closure carries tells a wrong one — but since
  2026-10-01 a closure turns the heading by 4° at most (was 26°): on the
  twin the heading's median error went from 0.97–1.26° to 0.63–0.92°,
  map and journeys unchanged. In the journeys of the band's confirmation
  run (2026-10-18, both arms, both houses) the heading's error is 0.9–1.8°
  (median), 3.3–4.5° (95th percentile).
- **Slower than `main`.** A journey's median is 106–111 s against `main`'s
  66–101 s; on open floor (casa_libera) 1.7 times as long. It is the price of
  walking the planned route (the string pulled 0.6 m at most, none beside a
  drop) — and `main` arrived in 63 % of the journeys against 90 %, and fell
  twice in casa_arredata the day before.
- **"How far along" errs low**: 7–22 points under the truth after a
  session, and high in a new map's first minutes, when the walls it knows are
  one room's. At the end of the band's confirmation run (2026-10-18) the
  houses declared themselves 78–92 % done; what is left unknown is mostly
  under furniture, and the band covers only 0.7–0.8 m² of it, so counting
  the band would add about two points: left as it is.
- **Exploring lingers in corridors** (half of each session in house2), going
  back over known floor to close loops.
- **Twin only.** None of this has run on a physical duck yet; the gait, the
  depth sensor and the floor are MuJoCo's. With an assistant that also runs
  a house's automation (Arkimede), "explore the house" went to the house's
  lights until the duck was named in the sentence.
