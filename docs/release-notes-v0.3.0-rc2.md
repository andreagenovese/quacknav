# quack-nav v0.3.0-rc2 — release notes

2026-10-18. Italian copy: [release-notes-v0.3.0-rc2.it.md](release-notes-v0.3.0-rc2.it.md).
The full list of changes is in [CHANGELOG.md](../CHANGELOG.md); the previous
candidate's notes are in [release-notes-v0.3.0-rc1.md](release-notes-v0.3.0-rc1.md).

## What this is

A **fourth release candidate, validated on the twins only** — the MuJoCo
twin of the Microduck (Pollen's released daemon-v0.16.1 and
`microduck_rl`'s body) and the paper twin — **not yet on a real duck**.
The physical duck arrives in December 2026; until a first session on it,
every number below is the twin's.

rc1 was about the duck as Pollen ships it and about stepping better. rc2
is about what the 2D map does not have: things that move, and things the
duck walks under or into at its own height. It also brings a first,
experimental 3D house with the camera's colours, built off board.

## Before installing

Nothing new to set. The config, the walk profile (`[gait] profile`) and
daemon-v0.16.1 are as in rc1, and the maps saved by rc1 load as they are:
their band starts empty and fills as the duck stands and looks.

## What changed since rc1

- **Things that move: four rules, on by default.** Something in the lane
  is booked from 0.35 m (0.30 before), and from 0.6 m when it comes nearer
  faster than the body walks; after a bump or a touch the duck steps back
  before turning away, only onto floor the map knows; what a journey
  booked leaves the books when the sensor sees the floor where it stood
  (drops never do). Each is a knob ([knobs.md](knobs.md)); 0 gives rc1's
  behaviour.
- **The band: 2.5D at the duck's height.** Per 5 cm cell, ten slices from
  0.05 to 0.30 m, filled by the depth sensor's returns and emptied by the
  beams that cross them. The navigation reads it as wall: a coffee
  table's top, a bed's frame, a tub, which the 2D map read as free floor,
  are now in the way. It is saved beside the map (`<map>.band`) and rides
  `map.frame` as `band`. `QK_BAND=0` turns it off. Returns within 10 cm of
  a wall the 2D map already has are left to the map, so the band does not
  thicken the walls and narrow the passages.
- **A textured 3D house, off board (experimental).** `scripts/map3d`
  records the head camera beside an exploring duck, builds a 3D house with
  its colours on a Mac or a PC, and `send.sh` puts it on the duck, where
  quack-control v0.1.0-rc2's **3D** tab draws it. Nothing new runs on the
  duck. On the twin: walls and furniture 2.6 cm from the truth (median).
- The twin: people and pets that move (`TWIN_MOVERS`), two houses with
  them, and a head camera that faces forward.
- CI: actions on Node 24, tried on ubuntu-26.04.

## Measured against the version before

Each change against the `main` before it, on the MuJoCo twin with
velstand, four rounds a house, two twins at a time:

| | before | after |
|---|---|---|
| moving obstacles: journeys, apartment / casa_arredata / casa_ingombra | 24/24, 24/24, 12/12 | 24/24, 24/24, 12/12 |
| moving obstacles: casa_viva (three movers) / casa_porta (a person in a door) | 24/24, 24/24 | 24/24, 24/24 |
| moving obstacles: exploring from nothing, three houses | one fall | no fall |
| the band: journeys after exploring, apartment / casa_arredata | 24/24, 24/24 | 24/24, 24/24 |
| the band: exploration declared done (of four sessions) | casa_arredata at 4, the apartment never | both at 3 |

No fall in the band's runs on either side. Details: [results.md](results.md),
"Moving obstacles" and "The band".

## Known limits

- **The twins only.** The twin's depth sensor casts one ray per zone; the
  real one sees a zone about 5.6° wide, and how a wide zone grazing a wall
  lands in the band is unmeasured until the duck.
- **The 3D house is a preview.** The camera's axes and intrinsics must be
  checked on the duck; on the twin its textures are synthetic.
- **A person booked as a few points** never seals a door: the forgetting
  rule has not fired on the twin yet (its gain is the bench's).
- robotd's own teleop still has no drop protection
  ([study/upstream-asks.md §8](study/upstream-asks.md)).
