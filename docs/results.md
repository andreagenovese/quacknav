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

## Known limits

- **A rim booked where the pose had it.** About 1 in 50 drops lands 20–35 cm
  off the true rim (the pose's error when it was seen). In the open that
  costs nothing; in a passage beside a hole it narrows the passage for the
  planner, and house2's goal beyond the stairwell (g4) was missed in all
  three rounds — by `main`'s build too, on the same book. A passage the duck
  has walked stays open (the lanes), but not one it has only looked at.
- **A resume on a slightly wrong pose writes into the map.** casa_arredata's
  second session came home 12 cm (and some degrees) off and mapped on: its
  walls went from 3.8 to 17 cm off the truth, and the next two boots could
  not find themselves on it. House2 took four sessions without harm. A
  consistency check before a resumed session inks anything is the next
  step; until then, a map can be redone from nothing (`fresh`).
- **Coming home in a regular house can take long, or fail.** The valley test
  refuses a pose a long plain wall cannot pin down: no wrong pose was
  believed, but in casa_arredata (a generated, very regular house, its
  bathroom half mapped) two boots of seventeen stood down after 16 minutes.
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
