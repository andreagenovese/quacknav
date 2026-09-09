# What quacksat would ask of Pollen's stack

Written 2026-09-09, from a fortnight of mapping work on the MuJoCo twin
(`pollen-robotics/microduck` PR 127 `maploc` and PR 202's simulator, plus
`microduck_rl`). Independent project, no affiliation; everything below is
a finding with the run that produced it, not a wish list. Nothing here has
been sent upstream yet.

Every number comes from the twin, not from hardware — the physical duck
arrives in December. Where a finding is likely to be a simulator artefact
rather than the robot's, it says so.

The first four sections are `maploc` correctness and are, we think, worth
upstream's time whatever quacksat does. The rest are smaller.

## 1. Loop closures fire on map noise, and walk the pose off

**What we see.** On the twin, whose odometry is nearly ground truth
(0.13 m and 3° drift over 41 m of a hand-driven tour), `maploc` closes
loops dozens of times per run at map-noise level: correction floor 0.04 m,
allowance 0.06 m plus 0.08 m per submap capped at 0.6 m, two witnesses that
can both come from the same stand. The closures walk the tracked pose
0.3–0.5 m away from the truth; the watchdog then calls the pose lost, and
the brute-force relocalize picks a wrong basin metres away.

**Evidence.** Bench matrix over five recordings (`maploc/examples/evaluate`
replays a `.mdlg` byte for byte). Tightening the allowance to 0.03 m per
submap with a 0.30 m cap removed every LOST event and improved the map
against the truth walls on 5 recordings out of 5.

**Proposed change.** Lower `max_correction_per_submap_m` to 0.03 and
`max_correction_cap_m` to 0.30 (`maploc/src/pipeline.rs`), or make them
configurable and default them there. Require the two witnesses of a closure
to come from different stands.

**How to check it.** `cargo run -p maploc --example evaluate -- <rec.mdlg>
sim-maploc/apartment.toml out/` and compare `map walls vs room` and the
LOST lines before and after.

## 2. Nothing corrects the pose against the map between closures

**What we see.** Between loop closures the tracked pose is dead reckoning:
the scan matcher is used for closures and for relocalizing, never to keep
the pose on the map it is building. MCL exists in the crate and is not
wired in. So the pose drifts until a closure yanks it, which is the
mechanism behind section 1.

**Evidence.** Our fork adds a scan-to-map correction at every still window
(`Mapper::tracking_correction`, gated by an agreement test against the last
search). On the twin: run 67 without it drifted to 0.38 m median and 0.73 m
late in the run; run 69 with it held 0.15 m median over ninety minutes,
never lost, with 0 falls.

**Proposed change.** Correct the tracked pose against the map on every
still window, with a residual improvement threshold and a cap, so it can
only ever tighten a pose. Ours is `TrackingConfig` in
`maploc/src/mapper.rs` (274 lines of the diff, defaults on).

## 3. Relocalize can be confidently wrong

**What we see.** After a LOST, the global search returns a pose metres away
from the truth with a residual of 0.000–0.005 — a perfect match to the
wrong room. A flat has repeated rectangles and, with no magnetometer, the
search covers rotation too, so aliasing is expected; what is missing is any
test that the winner is *unique*.

**Evidence.** Runs 56, 64, 65 and 68 on the twin each relocalized 3–6 m
wrong. Replaying those recordings with our gates, the errors drop from
3.1 m to 0.10 m and from 2.06 m to 0.07 m.

**Proposed change**, all four small and independent:
- a uniqueness ratio: accept the winner only when it beats the runner-up
  basin by a margin (`uniqueness_ratio 0.6`, `runner_up` in
  `maploc/src/relocalize.rs`);
- agreement: require consecutive searches to agree within about 0.3 m
  before acting on either (`relocalize_agree_windows 2`);
- a local search first: when the pose was merely lost, search around where
  the duck thinks it is (`hard_lost_search_radius_m 1.0`) before searching
  the whole map;
- give up honestly: after N windows with no confident answer, resume on
  odometry and say so (`Note::ResumedUnverified`) instead of committing to
  a wrong pose. A client can then stand still, sweep, and ask.

## 4. The live pipeline and the bench disagree

**What we see, and cannot explain.** The same recording that the bench
replays cleanly is a run in which the live daemon lost the pose. This is
the finding we would most like upstream to look at, because it means the
bench cannot vouch for the robot.

**Evidence.** Run 73 (recording `1788809590.mdlg`, 60 minutes, clean boot):
live, the pose error grew past 0.5 m at minute 50, windows were quarantined
from 22:24, tracking was declared lost at 22:28:23 and resumed unverified
0.8 m off. Replaying the same file: the tracked pose stays within 5–36 cm
of truth throughout and 15–17 cm during those last ten minutes, and is
never lost. Live and replay agree for the first forty minutes (1–16 cm
apart) and diverge after. The same happened on run 49 in an earlier
session with recording `1788627740.mdlg`.

**Where we would look.** Frame timing and the still gate (which windows the
live worker actually integrates), dropped depth frames under load, and
whether the search runs on a stale window. The bench consumes every record;
the daemon may not.

## 5. A map library, and relocalizing at boot

**What exists.** One session file (`map_path`), saved on shutdown and
autosaved, reloaded at boot — trusting the last saved pose. The IPC surface
is `robot.map` and `robot.map_wipe`.

**Why that is not enough.** A robot that lives in a house should wake up
and know which house, and where in it. Today, either it is switched on
exactly where it was switched off, or the map is worthless: the saved pose
is wrong and nothing checks it.

**Proposed change.**
- `robot.map_save {name}`, `robot.map_list`, `robot.map_load {name}` — a
  directory of named sessions rather than one file.
- Loading a session starts the mapper in the hard-lost state and searches,
  with the gates of section 3, instead of trusting `tracked`.
- Say in the map frame which session is loaded and whether the pose has
  been confirmed since boot, so a client can hold still, sweep and ask the
  user rather than driving on a guess.

**A caveat we would raise with it.** An 8×8 ToF at 2 m is a poor signature
of a room, and with no absolute heading the search is over three degrees of
freedom. Two cheap signals would carry most of the "which map" question
without touching the ToF: the **dock** (a robot that boots on its charger
knows exactly where it is — that alone solves the common case) and the
**Wi-Fi** neighbourhood, which `configd` already sees. Neither is reachable
from a robotd client today.

**Measured, 2026-09-09.** We built the smallest version of this and put it
on the bench: `Mapper::resumed_lost` starts a loaded session in the lost
state, and `MAP_SESSION=<file>` in `evaluate` replays a recording into a
saved map. Two lessons.

The first is a design lesson we would pass on: the two settings that make
a *kidnap* recoverable are wrong at *boot*. A local search radius around
"where the duck thinks it is" is anchored to the very pose that must not be
trusted, and giving up means falling back to it. On a resumed map the
search must be global and must never fall back.

The second is the honest result. Replaying into run 71's saved map (536
frozen submaps, the flat at 46 %): a recording that booted **on the dock**
relocalized after 125 s, and its pose then agreed with the true walls to
0.070 m median against the 0.036 m a fresh map gives — it finds itself, but
slowly and less well. Two recordings that booted **beside the stairwell**
relocalized within 24 s to poses that agree with the true walls to only
0.169 and 0.203 m, where a correct pose scores under 0.10: fast, confident,
and wrong. So with an 8×8 ToF and no absolute heading, boot relocalization
is not usable yet, which is why the dock and the Wi-Fi above matter more
than they look. A fairer test is still owed: our away-from-dock recordings
are short and spent beside one wall, so they show the sensor little.

**The fair test, and what it settled (2026-09-09).** We recorded one: the
duck switched on in the kitchen, 3.5 m from the dock, exploring for eight
minutes — a panorama, several metres of walking, more panoramas, 4000 cells
mapped, no pose jump. Replayed into run 71's saved map, the pose stays
4–5 m from the truth for the whole replay; not one window of 89 comes
within half a metre.

The `RELOC_DEBUG` line says why, and it is not what we expected. The search
*does* find the true place — at 38 s its winner is 0.6 m from it, explaining
231 of 231 beams with a mean residual of 0.0132 m. In the same search, a
basin on the other side of the flat scores 0.0132 m as well. The uniqueness
gate then refuses both, which is the right answer to an ambiguous question,
and the duck stays honestly lost.

So the obstacle is not the acceptance threshold, the beam count or the
gates: one still window of an 8×8 ToF in a flat of repeated rectangles is
simply not enough to name a place. **What would settle it is the shape the
user proposed**: look around, walk a few metres, look around again, and ask
which hypothesis survives both — the aliases are contradicted by the second
viewpoint, the truth is not. In code that means carrying the top few
candidates forward with odometry and scoring them at the next window,
rather than the single best (`last_search`) the agreement gate carries
today. `maploc` already contains an MCL module that is not wired in, and
this is what it is for.

**Built and measured (2026-09-09).** The search now returns every basin it
finds plausible, not only the winner, and a mapper resumed on a saved map
keeps them all: each hypothesis is carried forward by **raw odometry** (the
tracked pose is frozen while lost, on purpose, so it cannot be used) and
scored against every new window where odometry says that hypothesis would
be. Scoring rather than waiting for the search to propose it again matters:
the search returns a handful of basins out of many and the true one is not
always among them. A hypothesis is believed only once it has been confirmed
over a set distance of walking and leads the rest.

It did not rescue the case. Replaying the kitchen boot into run 71's map:
demanding no walking, it commits after 32 s and is 5 m wrong; demanding one
metre, it commits after 195 s and is 4.5 m wrong; demanding two, it never
commits and the duck stays honestly lost for the whole eight minutes. So a
second viewpoint a metre or two away does not separate the true place from
its alias in this flat — the aliases keep scoring as well as the truth.

Two things temper that. The map was 46 % of the flat, so half of every scan
falls where the map has no opinion and cannot contradict a wrong pose; a
complete map is a fairer test and we do not have one yet. And the failure
mode, with enough evidence demanded, is the safe one: the duck says it does
not know rather than walking off confidently into the wrong room. For a
robot with a charging dock, "ask to be put back on the dock, or tell me
where I am" is a reasonable thing to do — and it is what we would build on
the client side while this stays unsolved.

**Map against map: the answer (2026-09-09).** The user's own conclusion —
if the duck does not recognise the house, let it explore as it always has —
turns out to be more than a fallback, because after a few minutes of that
it no longer has a scan to match: it has a map. `maploc/examples/align_maps`
asks whether a fresh map fits inside a saved one, by turning the fresh
map's wall cells into a synthetic scan and searching the saved map with the
same coarse-to-fine machinery. Thousands of cells instead of a couple of
hundred beams:

| the fresh map | wall cells | where it landed | off by |
|---|---|---|---|
| run 70, booted at the dock | 1313 | (0.05, 0.00, 0.0°) | **5 cm, 0°** |
| eight minutes after booting in the kitchen | 659 | (−3.50, 0.95, 6.0°) | 0.83 m, 14° |

Against the 4–5 m that scan-to-map was wrong by, that is the difference
between a method that works and one that does not. Both are still *refused*
by the uniqueness gate, and rightly so as it stands: it is calibrated for
scan-to-map, where a good residual is 0.01 and the rival must be 0.6 times
worse. Matched map against map the residual floor is map noise, 0.07–0.09,
and the runner-up — the flat's 180° mirror image, both times — sits at 0.76
to 0.81 of the winner. So the acceptance rule needs its own calibration for
this use, and would be helped by evidence the score does not use today: a
candidate that lays the fresh map's *free* cells on top of the saved map's
walls is wrong, and saying so costs nothing.

**What we would build on this.** Boot: load the saved map, hold still and
search; if the pose is not confirmed within a minute, open a fresh map and
explore, which is what the duck does well. Then, every few minutes, ask
whether the fresh map fits inside a saved one — and when it does, adopt the
old map with the transform, keeping the places and the routes that hang off
it. Recognition becomes something the robot arrives at, not something it
must do before it may move.

**The floor as evidence, and what is still missing (2026-09-09).** A
candidate that lays the fresh map's floor on the saved map's walls is
wrong, and the wall residual cannot see it. Scoring each basin by walls
plus a penalty on floor-laid-on-wall puts the truth first in both pairs and
widens its lead: the kitchen map goes from 0.76 to 0.74 of its runner-up,
run 70's from 0.81 to 0.78, and in both the truth has the lowest
floor-on-wall of every basin (3.7 % against 5.3–7.7, and 5.1 against
7.2–7.8). Nearly free, and the right direction — but not enough for a gate
set at 0.6, and we cannot calibrate one honestly on two examples that are
both true. What that needs is a negative control: a map of a house the duck
has *not* been in, which we do not have and cannot fake by mirroring the
same flat.

So the acceptance rule we would build does not rest on a threshold at all.
It rests on the same thing that makes the whole design work: **the fresh
map keeps growing.** Ask every few minutes; require the winner to be the
same place, within a third of a metre, on two successive asks, with the
fresh map larger the second time. A wrong basin does not survive its own
map growing into the rooms next door; the right one gets better. It is the
multi-hypothesis idea again, at the scale where the evidence is actually
strong.

**Where the work stands.** The recognition piece is built and measured
(`maploc/examples/align_maps`). What it needs to become the design above is
the map library on the IPC — `robot.map_save`, `robot.map_list`,
`robot.map_load` — which is upstream's to add or ours to prototype across
five crates, and a client that at boot holds still, searches, gives up
after a minute, explores, and asks the recognition question as it goes.
Nothing in the client can be built before the RPCs exist.

## 6. Gait facts a follower needs, and cannot find written down

We measured these on the twin because our first models of them were wrong
and the duck walked into walls. If they hold on hardware, they belong in
the docs; if the simulator has them wrong, that is worth knowing too.

- **A turning arc barely slows down.** At `vx 0.3, vyaw 0.7` the body
  advances 0.110 m/s against 0.121 m/s straight — not the quarter our
  models assumed. A follower that reserves a quarter of the room for an arc
  ends against the wall.
- **There is no turn in place from a standstill.** `vx 0, vyaw ±0.7` moves
  the body 1–2° in six seconds. One second of walking first, then yaw only,
  turns about 30°/s with 15 cm of drift.
- **Backing needs a positive yaw to start.** From a standstill, `vx -0.3`
  with `vyaw -0.7` does not move the body at all; with `+0.7` it backs
  0.23 m in three seconds. Once stepping, either sign works, and straight
  backing works too — half a second of `+0.7` is enough to start it.
- **Timed turns are not repeatable.** The same command varies threefold
  with the step phase, and the body coasts 5–10° after the command stops.
  Heading control has to close on odometry, not on time.

## 7. Small things in the simulator

- **A spawn pose.** `sim-maploc/body_with_map.py` always places the duck at
  the origin. A `--start x,y,yaw` (we use a `MICRODUCK_START` environment
  variable locally) makes it possible to test a behaviour where it happens
  — beside the stairwell, in a doorway — instead of walking there first,
  which is most of a test's runtime. The map overlay then needs the same
  transform, or the map is drawn at the origin while the duck is elsewhere.
- **The ToF reads low furniture as a hole.** In the apartment scene, the
  floor rows that land on a bed or a low table report a drop where there is
  none: on one run five of them sealed a bedroom doorway for a quarter of
  an hour. Whether the real sensor does this on a duvet is exactly the kind
  of thing the simulator should be right about, because a client that
  believes it will refuse to walk. (Our client now tells a hole from an
  edge by asking whether an obstacle stands at the same bearing; on the
  twin that classifies three drops in four as furniture.)

## What we would send with it

The four `maploc` changes above are on a local branch against PR 202
(`maploc/{pipeline,mapper,relocalize,scan_matcher}.rs`, plus env knobs in
`evaluate.rs` for hypothesis testing and one log line in
`robotd/src/maploc.rs`): about 435 lines. The recordings behind every claim
are ordinary `.mdlg` files and can travel with the report.
