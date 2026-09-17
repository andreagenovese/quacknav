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

## Known failing, unchanged today

A journey WITH the guards through the stairwell's side (rim2, rim3:
legs of 421 s and 447 s, the goal 2.4 m away) — the guards-vs-stairwell
knot of 2026-09-16. It is what a new house with no books would meet.

## Where the runs are

`private/drives/runs/<label>/` keeps each run's pose track and logs
(spawn-*, look*, scan*, house1tour, house2tour, rim2, rim3);
`private/drives/daytable.py` tabulates a set of them.
