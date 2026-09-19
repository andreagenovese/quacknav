# Baseline on the twin — 2026-09-17

What the duck does on the MuJoCo twin at this commit, so a later change
can be measured against it: run the same commands, compare the numbers.
A regression is a fall, a goal missed, or a time well outside the range
below (these are single runs; twins of the same run vary by ±20 %).

Conditions: house `house2` (the human drive of 2026-09-16, robotd fixed),
its ground book at 51 drops (`private/drives/runs/house2/ground-51.json`),
robotd from the worktree `maploc-study` in `localize` mode with
`MAPLOC_RAY_JUDGE=1`; quacksat with its defaults — on a frozen map a
journey walks blind (`QK_NO_GUARDS`, `QK_FOLLOW_ROUTE`, `QK_FAST` unset),
the route pulled and kept; the boot search looks before it walks and
scans the horizon when nothing is ahead.

## Boot: the pose confirmed on the saved map

`MICRODUCK_START="x,y,yaw" private/drives/boottest.sh <label>` — or all
five with `private/drives/queue-spawn.sh`. Time from "loaded the newest
map" to "the pose is confirmed"; pose error is posetrack's at five
minutes.

| spawn (world x, y, yaw) | boot | steps | refusals | pose at 5 min |
|---|---|---|---|---|
| corridor (0.05, 0, 0) — default | 78–79 s | 8 | 0 | 3–5 cm / 0.2–0.8° |
| living room (−2.5, −1.5, 0) | 83 s | 8 | 0 | 3 cm / 1.3° |
| bathroom (2.5, −2.2, 1.57) | 125 s | 12 | 1 | 11 cm / 1.5° |
| office (2.5, 0.0, 3.14) | 128 s | 10 | 0 | 11 cm / 2.3° |
| bedroom (2.0, 2.5, −1.57), back to the wall | 154 s | 11 | 0 | 7 cm / 0.1° |
| kitchen (−2.5, 2.0, 3.14), island and stools | 375 s | 11 | 0 | 8 cm / 0.7° |

No fall in any. The floor is maploc's gates (a metre of chord, a lead of
three windows, half a metre to confirm; six-second stands): about 65 s.
The kitchen is the one to watch: the scan finds no metre of floor and
the search stands boxed in five times before the second budget's scan
finds the door.

## Journey: six goals round the house, blind, on the frozen map

`GOALS="1.50,2.50 2.50,0.17 0.90,-2.40 -2.64,-2.12 -2.30,2.10 -0.30,1.50"
MAPLOC_MODE=localize private/drives/abgoto.sh <label>` (boot at the
default spawn, then `speed_test.py`; "truly" is the twin's distance to
the goal when the job says it arrived).

| leg | house1tour (night, 39 drops) | house2tour (morning, 51 drops) |
|---|---|---|
| boot | 116 s | 79 s |
| bedroom (1.5, 2.5) | 23 s, 0.15 m | 47 s, 0.10 m |
| study (2.5, 0.17) | 104 s, 0.13 m | 93 s, 0.14 m |
| bathroom (0.9, −2.4) | 99 s, 0.20 m | 130 s, 0.05 m |
| living room (−2.64, −2.12), past the stairwell | 66 s, 0.09 m | 70 s, 0.12 m |
| kitchen (−2.3, 2.1) | 100 s, 0.08 m | 103 s, 0.17 m |
| corridor (−0.3, 1.5) | 35 s, 0.07 m | 29 s, 0.12 m |
| journeys | 427 s, 6/6 | 472 s, 6/6 |
| pose at 5 and 10 min | 9 / 8 cm, 0.6 / 1.0° | 8 / 8 cm, 0.6 / 0.6° |
| nearest the truth came to the west rim | on it (book without the rim) | 20 cm |

No refusal, no stall, no drop struck, no fall; the book unchanged after
a blind tour (a blind trail strikes nothing and lays no lane).

## 2026-09-18, the modes apart (commit b288348)

`explore.rs` became `explore/` — mod.rs (the loop), journey.rs,
mapping.rs, guarded.rs, recover.rs, gait.rs, books.rs, mode.rs — and a
policy per mode: mapping and the blind journey keep this baseline's
behaviour, the guarded journey alone carries the passage-law
experiments of the 17th/18th. Blind six-goal tour after: house7tour
604 s (40, 108, 139, 87, 192, 38), house8tour 512 s (43, 94, 115, 78,
152, 30), both 6/6, no fall, pose 8–14 cm — the kitchen leg the one
that varies (100–192 s). Before the separation the leak had cost
693 s (house6tour). The guarded journey through the stairwell's side:
rim7's exact configuration replayed five times, 0/5 — one in five in
every configuration tried (`queue-rim7.sh`, `paper30.sh`).

## Known failing, unchanged today

A journey WITH the guards through the stairwell's side (rim2, rim3:
legs of 421 s and 447 s, the goal 2.4 m away) — the guards-vs-stairwell
knot of 2026-09-16. It is what a new house with no books would meet.

## Where the runs are

`private/drives/runs/<label>/` keeps each run's pose track and logs
(spawn-*, look*, scan*, house1tour, house2tour, rim2, rim3);
`private/drives/daytable.py` tabulates a set of them.

## Where the negatives still are (2026-09-19)

Holding: the boot from the corridor (65–79 s, six spawns, no fall); the
blind journeys on the frozen map with its books (eleven six-goal tours,
66 of 66 legs, no fall, 472–604 s); the modes apart. Still negative,
by severity:

1. **The books and the pose.** A book is worth the pose it was written
   with: at 17 cm of pose error a stand booked thirty phantoms and
   sealed the corridor (rim1); there is NO pose-quality signal (the
   still window's residual does not correlate with the truth error,
   `agreecheck.py`). The gravest open risk for the real duck.
2. **The stairwell's mouth, guards on.** One time in three the duck
   cannot turn beside the hole; the seal comes after 3.5 min
   (trust1/3, retreat8, study1). Mitigated (3 of 5 round trips, from
   1 in 5), not solved.
3. **The go-round.** Works end to end, does not fit the budget (a
   failed mouth plus 10 m; rim14, retreat8, study1). Easy: a budget
   sized to the route, or a quicker seal.
4. **The boot in furnished rooms.** Kitchen 375 s (no metre of floor
   between island and stools), bedroom 154 s; "0.19 m all round" for a
   look after a bump.
5. **Lost in localize.** Lost mid-journey, relocalised 77° off (lane1,
   2026-09-16); not seen since, no rule for it.
6. **Low obstacles.** The cube and the ball are dragged along: neither
   the boot nor the journeys see them.
7. **Fresh exploration** not re-measured on MuJoCo after this week's
   changes (explmap1: 46 min, the south corridor at 22 min); the rim
   calibration (booked 10–16 cm short) never done.
8. **The left-turn alignment** stops 8–13° short, inside the 11°
   tolerance (`alignprobe.py`).
9. **The blind journeys' variance**: the study and the bathroom walk
   2.3–2.6× the straight line; the kitchen 100–192 s. Time, not safety.
10. **The bench**: the paper twin never maps the living room (32 % in
    900 s), so it cannot measure the seal or the go-round; a `--known`
    world is missing.
11. **Upstream**: PR 202 closed; the worktree's fixes (frame/head
    pairing, frozen localize) have no PR to attach to.
12. **The drive mode** is a private script and a recorder, not a
    feature.

Safety lives in 1, 5 and 6; the rest is time or tooling.
