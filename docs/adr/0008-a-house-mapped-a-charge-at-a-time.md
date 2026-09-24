# ADR 0008 — A house mapped a charge at a time

Date: 2026-09-24. Status: accepted.

## Context

A Microduck's battery does not last the ninety minutes a house takes to
explore, and on the MuJoCo twin one long exploration was worse than
several short ones anyway: the longer a job ran, the more of it went into
the same stuck corner (30 minutes on one spot beside casa_arredata's
stairwell), and the less of the house it reached (house2's bathroom 15 %
after 90 minutes). A user, meanwhile, wants three things the explorer did
not offer: to know how far along the map is, to say "that is enough", and
to start again from nothing when the house has changed.

## Decision

**Every exploration is a session of a progressive one.** `robot.map_explore`
saves the map under a name when the session ends — its budget, a battery
under `battery_min_pct` (read from `robot.health`), or nothing left to
explore — and writes the progress beside the drop book (`<name>.progress`:
sessions, minutes, share mapped, done). The mapper refuses to save while
the duck does not know where it is, and a session so refused is not
counted.

**The next charge goes on where the last one stopped.** With
`[homecoming] resume_explore`, a boot on a map still being explored comes
home on it and explores on: the frontiers left are where the last session
stopped. A boot that cannot confirm its pose never starts a fresh map — it
searches on, then stands down — so the saved map is never replaced by one
the duck could not find itself on.

**Done is a verdict, the duck's or the user's.** A session that ends with
no frontier left, or with only unreachable ones and under 2 m² of unknown
floor within their reach, finds the house done. The user may say so
earlier: `complete: true` stops the session, saves, declares the map done
with the share it has, and freezes it. A done map is not explored again;
`robot.map_explore` says so. Only `fresh: true, confirmed: true` starts a
new map — the first call without `confirmed` answers what would be lost —
and the saved map stays in the library until the new map's first session
saves over it.

**A done map is navigated.** The homecoming freezes it at boot
(`quack.map_freeze`, mapd, at run time — maploc's `localize` without a
restart), and journeys on it are hybrid: a leg walks blind where the map
knows the floor, and is judged by the guard where it crosses a cell the
map has not seen.

**How far along** is `robot.map_status` `house.percent_mapped`, live from
the map in hand: known floor over known floor plus the unknown a frontier
reaches inside the map's walls (walled-in pockets — a sofa's inside — are
not left to explore). On the twin it runs 7–12 points under the truth once
a session is done, and over it in the first minutes of a new map, when the
walls it knows are only those of one room.

## And what the sessions taught

Measured on the twin while this was built, each now a rule:

- *Off the rim first.* Both falls of 2026-09-23/24 were a duck standing
  still 3–12 cm from a rim, every move refused that near, while the
  standing gait crept it in. A drop nearer than a turn in place is allowed
  (0.15 m) is left before anything else.
- *A spot stuck on again and again is a no-go* for the rest of the job —
  and the no-go spots go first when the duck is sealed in.
- *A refusal beside the body is not the frontier's*: counted against a
  frontier two rooms away, it cost house2 its bathroom for two sessions.
- *A pose on a map from an earlier run is believed only where the scan
  pins it down* (maploc's valley test): a long plain wall matched itself
  1.7 m along, and a boot confirmed it.
- *After a fall, the pose first*: the job waits and the duck looks around
  until a pose is confirmed; no boot resumes on a pose nothing could judge.

## Consequences

Exploring a house is several short sessions, each safe to cut short. The
duck's own number for how far along it is errs on the low side after a
session and on the high side in a new map's first minutes. A map the user
closed early is navigated as it is: its unknown parts are walked with the
guard on, not explored.
