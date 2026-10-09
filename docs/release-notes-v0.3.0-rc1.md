# quack-nav v0.3.0-rc1 — release notes

2026-10-09. Italian copy: [release-notes-v0.3.0-rc1.it.md](release-notes-v0.3.0-rc1.it.md).
The full list of changes is in [CHANGELOG.md](../CHANGELOG.md); the previous
candidate's notes are in [release-notes-v0.2.0-rc2.md](release-notes-v0.2.0-rc2.md).

## What this is

A **third release candidate, validated on the twins only** — the MuJoCo
twin of the Microduck (Pollen's released daemon-v0.16.1 and
`microduck_rl`'s body, with both of Pollen's walking policies) and the
paper twin — **not yet on a real duck**. The physical duck arrives in
December 2026; until a first session on it, every number below is the
twin's.

rc2 made the duck rest, get carried and be driven by hand. rc3 is about
the duck as Pollen ships it — its default walk, its newest daemon — and
about stepping better: a stick that meets things the map does not have,
and a neural navigation model that can take its place.

## Before installing

1. **Say which walk the duck runs**, in `/etc/robot/quack-nav.toml`:

   ```toml
   [gait]
   profile = "velstand"   # Pollen's default; "alpha" for alpha_walking + alpha_stand
   ```

   Left out, it means velstand. A duck on alpha whose config has no
   `[gait]` section must add `profile = "alpha"` (the installer keeps an
   existing config as it is).
2. Pollen's **daemon-v0.16.1** is the daemon validated.

## What changed since rc2

- **Both walks.** velstand (Pollen's default since policy set v5: one
  network that walks and stands) did not work before: robotd labels a
  standing velstand duck "walk", so quack-nav never saw it still, and since
  0.16.1 robotd's idle glancing took the head. Now the applied twist
  decides, the duck counts as still 0.6 s after it (its body goes on that
  long) and stands are that much longer. Measured on the twin: the same
  journeys as alpha, the pose closer (6–7 cm median), about 10 % slower.
- **The pilot, a neural navigation model, one per walk.** An MLP
  (351 inputs, 9 moves) chooses each step of a journey in place of the
  stick's rules, never the route, with hard shields around it. The
  package installs `pilots/alpha/` and `pilots/velstand/` (`pilot.json`
  for quack-navd, `pilot.onnx` for anything else) in
  `/var/lib/quack-nav/pilots/`; off until
  `QK_RL_POLICY=/var/lib/quack-nav/pilots` (knobs.env), then the pilot of
  the walk. Bench: 97.1 % against the stick's 93.3 % (alpha), 95.2 % against
  94.8 % (velstand); on the twin with velstand, 12/12 with no fall.
- **The stick, on a journey**: a nose against something for two steps is a
  stall (apartment's coffee table, where the body pushed for minutes and
  the pose slid 0.3 m); what the sensor keeps seeing within 0.30 m goes on
  the books before the bump; a turn in place stops short by the coast it
  learns (232 dithers to 5, journeys 9–22 % faster). A walked lane yields to
  a booked drop within 0.20 m, which keeps the route off a stairwell's
  corner.
- **The hole guard by a wall.** A hole set against a wall read as a box's
  edge; a brain choosing at random stepped in once on the bench. Fixed in
  the guard only; 0 holes in 7,560 random journeys since.
- **daemon-v0.16.1** (API 41): the pin moved; the head sweep now takes the
  head back from robotd's idle glancing.

## Measured against main

Four rounds a house on the MuJoCo twin, the same books, two twins at a
time:

| | main | v0.3.0-rc1 |
|---|---|---|
| journeys, velstand | 48/48 + casa_ingombra 11/12 | 48/48 + 12/12 |
| journeys, alpha | 48/48 + 11/12 | 48/48 + 12/12 |
| exploration, velstand (three houses) | coverage, no phantom, 34/34 after | the same |
| falls into a hole | 0 | 0 |

Details: [results.md](results.md), "v0.3.0-rc1: the verifications against
main".

## Known limits

- **The twins only.** Calibrate the pilot on the duck's own traces
  before trusting it (docs/rl-pilot.md, "Calibration on the duck").
- **velstand explores slower** in a house's first session (its stands are
  longer): about one session more to finish a house.
- **Things that move** (people, pets) are booked only within 0.30 m and
  stay on the books until a "no way" clears them: the next work, after
  this release.
- robotd's own teleop still has no drop protection
  (docs/study/upstream-asks.md §8).
