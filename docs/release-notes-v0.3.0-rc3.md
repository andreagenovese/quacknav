# quack-nav v0.3.0-rc3 — release notes

2026-10-27. Italian copy: [release-notes-v0.3.0-rc3.it.md](release-notes-v0.3.0-rc3.it.md).
The full list of changes is in [CHANGELOG.md](../CHANGELOG.md); the previous
candidate's notes are in [release-notes-v0.3.0-rc2.md](release-notes-v0.3.0-rc2.md).

## What this is

A **fifth release candidate, validated on the twins only** — the MuJoCo
twin of the Microduck (Pollen's daemons v0.16.1 and v0.17.6 and
`microduck_rl`'s body) and the paper twin — **not yet on a real duck**.
The physical duck arrives in December 2026; until a first session on it,
every number below is the twin's.

rc2 was about what the 2D map does not have. rc3 is about Pollen's new
daemon line and about not falling: the daemon 0.17 line with its own
walk numbers and its own pilot, a look at the floor before a step the
map cannot vouch for, and a turn in place that stops where it was told
to. It also brings people walking through the house on the twin and the
bench alike, and a bug in how far away things were booked.

## Before installing

- **Nothing new to set.** The config of rc2 works as it is. A new
  optional key `daemon` (`"auto"`, the default: robotd says which daemon
  it is at start; `"0.16"` or `"0.17"` to name it) picks the walk's
  numbers for Pollen's line ([README, The daemon](../README.md#the-daemon)).
- **Built against daemon-v0.17.6**; the same build runs on every daemon
  from v0.14.4.
- The maps and the places saved by rc2 load as they are.
- The install package carries one more pilot, `pilots/0.17/velstand/`,
  which quack-navd takes on the 0.17 line when `QK_RL_POLICY` names the
  pilots' directory. Unset, the stick drives, as before.

## What changed since rc2

- **The daemon 0.17 line.** From daemon-v0.16.2 the duck runs policy set
  v8: velstand walks slower forward (0.095 m/s at vx 0.3) and turns about
  twice as fast (a turn in place at 1.0 is about 65°/s). One file per
  line holds velstand's numbers; `daemon = "auto"` picks the line. On
  that line the exploration's turns in place stop short by the learned
  coast too.
- **A look at the floor before a step onto what the map does not know.**
  The depth sensor cannot see the floor nearer than 0.4–0.6 m, and a
  stairwell against a wall hides a second way. Every reckless fall on the
  bench was that: a turn taken too far, then straight steps into a
  stairwell no frame had seen. Before a step whose stride runs onto cells
  the map holds as neither floor nor wall, or the first after a turn in
  place of 0.5 rad or more, quack-navd now pitches the head at the floor
  0.30 m ahead for 0.45 s and hands it back; what it sees goes through
  the guards and on the books. `QK_LOOK_DOWN=0` switches it off.
- **A turn in place stops where it was told to.** A timed move ended
  without a stop and robotd held the last command to its deadman, 500 ms:
  a third of a right angle at set v8's rate, on every turn. The explorer
  now sends a stop at the end of every turn in place (`QK_TURN_STOP=0`
  for the old way). The turns land 12–16° past what was asked instead of
  43–44°, and the coast the next turns stop short by learns the gait's
  own, not the hold.
- **Pilot v8l-s3 for velstand on the 0.17 line**, trained on the twin's
  traces and houses with the simulator's sensor made to see as the
  twin's: the first pilot on that line not slower than the stick. The
  handover between the pilot and the stick is firmer: the stick takes the
  legs from a pilot that gets no nearer and keeps them until the body is
  0.3 m from where it took over; within 0.5 m of a booked rim the stick
  walks the legs.
- **Things seen were booked 7.8 cm nearer the body than they are.**
  kinematics measures a hit's range from the sensor, not from the trunk
  axis its doc names (pollen-robotics/microduck#430); the books now use
  the trunk's distance, and a thing seen is booked from the pose of the
  frame that saw it.
- **People walking between the rooms**, on the twin and on the bench
  alike: `casa_gente` (four people on paths between the rooms, a cat),
  movers that wait for the duck within 0.35 m and step aside, the same
  rule with the same numbers in the twin and in the simulator.
- The simulator's houses are the twin's (`--families twin:<house>`), a
  tip-over gets up after 4 s on the 0.17 line, the sensor's rows follow
  the head's pitch, and the simulator holds a spin's last command to the
  deadman unless stopped, as robotd does.
- The twin: `twin.sh down` cleans the sockets a lost pid file left; a map
  loaded at boot is placed once the pose is confirmed; `explore_ab.sh`
  starts each house from an empty state.

## Measured against the version before

Each change against the `main` before it, on the MuJoCo twin with
velstand on daemon-v0.17.6, four rounds a house, two twins at a time
(the houses: casa_ingombra, casa_arredata; 24 journeys an arm and house):

| | before | after |
|---|---|---|
| the `daemon` key and the line's numbers, run on daemon-v0.16.1: reach, falls, wake-ups | `main` | no regression |
| pilot v8l-s3 against frozen `main`'s stick: journeys | 48/48 | 48/48, 4.2 and 3.9 s a journey faster, no fall |
| the look at the floor: journeys | 48/48 | 48/48, no fall, 6.2 and 2.7 s a journey more (4.4 and 3.4 looks a journey) |
| the look at the floor: houses explored from nothing, floor mapped after two sessions | 63 / 69 % | 76 / 78 %, no fall |
| the look at the floor: reckless brains on the bench, 25 200 journeys | 13 falls into a hole | 0 |
| the turn's stop: journeys | 48/48 | 48/48, no fall, 2.0 s slower in the cluttered house, 5.9 s faster in the furnished one |
| the turn's stop: where a turn lands past what was asked (median) | 43–44° | 12–16° |
| people in the house (bench, casa_gente, 192 journeys): arrivals, contacts with people a journey | stick 84 %, 8.0 | pilot 89 %, 2.2 |

Details: [results.md](results.md), "Round two on the 0.17 line", "The
sensor's blind zone", "The turn in place's overshoot".

## Known limits

- **The twins only.** The deadman, the sensor's rows and the walk's
  numbers are measured on the twin; the duck's own are the first
  session's job ([first-duck-session.md](first-duck-session.md)).
- **A walking leg's hold to the deadman is not modelled**: about 5 cm at
  set v8's pace after a leg's last chunk, inside the speed fitted from
  the traces. And a turn asked through `robot.move` (quack-control's
  manual move) still goes on to the deadman, about 33° at 1.0: only the
  explorer's own turns send the stop.
- **The look covers the stride ahead, not the house.** The sensor's blind
  zone under 0.4–0.6 m stays; a hole beside the body, or one reached
  sideways or backwards, is on the books or nowhere.
- **The learned coast lives in the process**: at every restart a turn's
  coast starts again from the line's prior (17°) and converges in a few
  turns. Keeping it, and the walk's other numbers, beside the map is the
  next work (the todo's gait ledger).
- **A rim is booked where the pose had it**: about 1 in 50 drops lands
  20–35 cm from the true rim ([results.md](results.md), "Known limits").
- robotd's own teleop still has no drop protection
  ([study/upstream-asks.md §8](study/upstream-asks.md)).
