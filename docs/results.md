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
(`MAPLOC_SETTLE=1` on the bench):

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
  (`MAPLOC_LOOP_SIGMA_YAW`). maploc's parameters are judged on 12
  sessions or more from now on. A strike of booked holes where the floor
  was later seen (`QK_FLOOR_STRIKE=1`, removed 2026-09-30): under an unseen 11–15 cm pose
  error it also struck true rim points.
- **daemon-v0.15.0** (API 37) is validated on the twin only on the branch
  `microduck-015`: four sessions per house, no regression. There a
  turning head was measured to cost the map next to nothing (3.2–3.3 cm
  of residual from 0.05 to 1 rad/s; 3.46 cm on the 1 % of frames above).
  `main` stays on daemon-v0.14.4.

## Known limits

- **A rim booked where the pose had it.** About 1 in 50 drops lands 20–35 cm
  off the true rim (the pose's error when it was seen). In the open that
  costs nothing; in a passage beside a hole it narrows the passage for the
  planner, and house2's goal beyond the stairwell (g4) was missed in all
  three rounds — by `main`'s build too, on the same book. A passage the duck
  has walked stays open (the lanes), but not one it has only looked at.
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
  against 126–192 s before. Still slow: the apartment's duck woken east of
  the stairwell, where the windows refute a right seed for minutes.
- **Some loop closures measure the heading wrong** — a few degrees, and
  the live pose drifts with it: phantom holes north of casa_arredata's
  stairwell (x13) came from a pose 35 cm off after a closure 5° wrong, and
  maploc's covariance did not flag it (σ 0.08 m at 0.35 m off). Why is
  being measured, with the truth sampled densely (`LOOP_LOG`,
  `POSEERR_DT=0.5`).
- **Slower than `main`.** A journey's median is 106–111 s against `main`'s
  66–101 s; on open floor (casa_libera) 1.7 times as long. It is the price of
  walking the planned route (the string pulled 0.6 m at most, none beside a
  drop) — and `main` arrived in 63 % of the journeys against 90 %, and fell
  twice in casa_arredata the day before.
- **"How far along" errs low**: 7–22 points under the truth after a
  session, and high in a new map's first minutes, when the walls it knows are
  one room's.
- **Exploring lingers in corridors** (half of each session in house2), going
  back over known floor to close loops.
- **Twin only.** None of this has run on a physical duck yet; the gait, the
  depth sensor and the floor are MuJoCo's. With an assistant that also runs
  a house's automation (Arkimede), "explore the house" went to the house's
  lights until the duck was named in the sentence.
