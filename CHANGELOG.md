# Changelog

All notable changes to quack-nav. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[Semantic Versioning](https://semver.org/) (pre-1.0: a minor version may
break things). Italian copy: [CHANGELOG.it.md](CHANGELOG.it.md).

Every number below was measured on the MuJoCo or paper twins, never on a
physical duck; the details are in [docs/results.md](docs/results.md) and
[docs/todo-map.md](docs/todo-map.md).

## [Unreleased]

### Changed

- **A hole booked lately follows the map's correction of its pose.** A
  drop booked on a pose the map corrected a moment later stayed where
  that pose had put it — 0.2–0.3 m off its rim, in the 0.49 m corridor
  passage beside the stairwell, where it sealed the bath. Every hole
  booked is now held in the odometry's frame as well, and for 60 s the
  map's corrections of the pose move it along. Shields and bench the
  same as `main` (the simulator's pose hardly jumps). On the twin, over
  the explorations of e24, e24b and e25 against frozen `main`: a phantom
  in the passage in 0 of 4 explorations with the anchors against 1 of 5
  without (main's: two drops at (2.10, 0.18) and (1.97, 0.19), the bath
  lost at 0.10), the anchors moving 79–103 booked points an
  exploration, falls even (one each in e25, at the same corridor spot).
  Few events: the measure to keep is the phantom count over the next
  explorations.

- **The turn in place's rate from turns big enough to reach it.** A
  turn's time is 0.40 s of start plus its size at 1.43 rad/s (255 turns
  of the stick's on the twin, AB26), so the rate over a short turn's own
  time is mostly its start — 0.31–0.80 rad/s at a quarter turn against
  1.05–1.27 at a half or more — and a house of short turns had learned
  0.75 (S12, daemon 0.18.3) where the next learned 1.05. The ledger now
  samples the rate from turns of 0.6 rad or more. Shields and bench
  against frozen `main` the same to the journey (they do not run the
  ledger).
- **One exploration A/B at a time on the twins.** Both arms of
  `scripts/twin/houses/explore_ab.sh` use the same state directories and
  ports; a second A/B started beside a first ran into its state (e24: a
  loop left running after a stop explored one house beside the other's
  restart, and the second's map was saved into the first's run). A lock
  with the holder's pid now refuses a second A/B, takes over a stale
  lock, and is released on exit.

- **A move ends where it says.** A timed move left its last command in
  robotd, which holds it to its deadman half a second on — about 5 cm of
  walk at set v8's pace, a third of a right angle of turn — walked into
  the stand after a mapping step and past the end of a `robot.move`.
  `robot.move` now sends one explicit zero after its time (`"hold":
  true` keeps the last command instead, for a move another follows at
  once; the reply says `held`), and `robot.map_step` sends one at the
  walk's end when a stand follows, so the stand begins where the walk
  said; a step with no stand — the explorer's pulses — leaves the gait
  running. The explorer's turns had this since 2026-10-27. Measured on
  the twin, four moves of each kind by odometry at 20 Hz until the body
  was still: a 2 s walk at vx 0.3 went on 1.2 cm past its time with the
  zero (1.5 at most) against 5.1 cm held (5.6 at most), and was still
  0.9 s after its time against 1.3; a 1.5 s turn at 0.8 rad/s went on
  7° past its time (10 at most) against 35° held (38 at most), still
  0.7 s after against 1.2. Twin A/B against frozen `main` (AB26, four
  tours of each house, both ledgers empty): 48/48 arrived on both arms,
  0 falls, the times the same within noise (paired medians +0.0 and
  −0.5 s).
- **A hole's rim goes on the books where the rim is.** A drop was
  booked at `range_m`, the floor distance of the first beam that found
  no floor: a point inside the hole by the sensor's row spacing —
  0.03–0.17 m on the simulator's hole ahead at every pitch
  (`examples/drop_edge`), 12–42 cm on the bench's stairwells — so the
  route's margin was measured from inside the hole. The rim lies
  between that beam and the last one below it that still met the floor
  (`edge_min_m`); `Drop::rim_m` is the middle of that span (within
  ±0.07 m of the rim either way, the example says; 0.10 m short of
  `range_m` when no beam below saw the floor), and the books take a
  disc there *beside* the beam's own, when the two are 5 cm or more
  apart — not instead of it: booked instead, the rays that give no
  reach (oblique ones, obstacle edges) lost the hole's inside, and one
  seed of the shields (a hole's corner in the mixed family, calib v8l,
  the reckless-straight brain) walked in where `main` had timed out;
  booked beside, the books are a superset of main's and that seed
  arrives. The guards keep `edge_min_m` as their nearer bound.
  `QK_RIM_BOOK=0` books as before. Shields (600 seeds, three reckless
  brains, calibs v8l and v8m, 12 600 journeys each) against frozen
  `main`: hole falls 0 and 0 against main's 0 and 1 (the same corner
  seed, at the other calib), tip-overs the same within noise (637 and
  613 against 627 and 611), routes refused fewer (14 and 12 against 16
  and 14), the body farther from the stairwells' rims (closest approach
  0.04–0.07 m against 0.02–0.05); one tip-over of the branch's where
  main arrives (mixed seed 300224, the reckless-back brain, 24 bumps by
  a mover beside a hole after the rim's disc routed it the other way),
  and the bench the same (98.6/98.6 % against 98.6/98.8, the stick's
  closest approach to a rim 0.097 m against 0.074).
  Exploration A/B on the twin (e23, both houses from nothing, three
  sessions, four rounds of journeys): 0 falls on either arm; maps and
  journeys the same within the exploration's own noise, each arm
  losing one house's bath (main's casa_arredata, the branch's
  casa_ingombra) to phantom drops in the 0.49 m corridor passage beside
  the stairwell — a pose error at booking, which the old booking makes
  as well (three phantoms there on main's arm, one on the branch's);
  two more explorations of casa_ingombra with the rim's disc reached
  the bath (0.92 and 0.94 of its floor, 6/6 journeys each, 0 falls),
  and on the twin the old booking's discs already sat a median 4.5 cm
  inside the rim, so the disc's cost and gain are both small there; the
  gain is the bench's deep rims.
- **The trim's sample is a run of straight steps, and its ring four
  times the rest.** Since the settle was learned, casa_ingombra's trim
  had landed at −0.08…−0.11 in three runs of four, casa_arredata's at
  −0.01…−0.03. Measured on the twin with the body's state stream
  recorded beside the trace (2026-10-30): no dead zone — the walk
  answers the applied yaw linearly, 1.0 rad/s per unit down to ±0.05;
  the stop's own yaw small (−0.005…−0.04 rad within a second) and the
  stick's pulses back-to-back, so the shorter stand was not it. The
  cause is noise: a straight 0.6 s step's yaw by odometry spreads
  0.15–0.17 rad/s about a veer of 0.01, and the median of 32 such
  samples wanders ±0.035 for noise alone — the −0.1 values were that
  wander with its slow feedback. Now a trim sample is a run of 3–8
  consecutive straight steps (their yaw over their time: the gait's sway
  averages out, the veer does not), ended by a curving step, a turn, a
  step that does not count or a gap of 1.5 s, and the trim keeps the
  last 128 samples (standard error about 0.017). `robot.gait` reports
  `keep` and the run pending. The bench's perturbed walk (veer 0.12
  rad/s): the trim learned −0.12 (the exact −0.136) from run samples of
  −0.05…−0.15, arrivals 98.2 % against main's 97.9 on the same walk.
  Twin A/B against frozen `main` (AB25, four tours of each house, both
  ledgers empty): 48/48 arrived on both arms, 0 falls, the times the
  same within noise (paired medians +1.0 and +0.0 s); the trim learned
  −0.037 in casa_ingombra and −0.034 in casa_arredata from 29 and 77
  run samples spreading 0.04–0.05 (single steps: 0.15), where main's
  ended at −0.052 — the two houses agree now.

### Added

- **The satellites on the client crate; N4 done.** quacksat's
  navigation lane and quack-control's bridge now run on
  `quack-nav-client` and say `nav.hello`, logging the protocol version;
  quack-control hears the events and asks for the status the moment
  one comes. The client gained an answer timeout of the caller's
  (`connect_with`), for calls that queue behind another. The roadmap's
  N4 test, on the twin: a journey's end reached the control page 0.01 s
  after quack-navd said it, four times of four, where the page's poll
  took 0.14–1.0 s.
- **Doing, not only walking: `nav.act`** (protocol v5, the roadmap's
  N5). The duck carries out one of `robotd`'s skills — Pollen's
  policies, read from `robot.skills` at run time, never a list coded
  here — as a job: a walk to `at` first when given, then `robot.do`,
  watched to its end on the state stream's policy label (it begins when
  the label leaves `walk`/`stand`, ends when it comes back). The step
  says how long it ran and the body's state after (`free` or
  `carrying`, from the gait ledger), or why not: refused, never began,
  did not end in its time, fell. The catalogue gains `robot.skills` and
  `robot.act` for agents that only know `nav.call`, and `robot.act` may
  be a step of `nav.jobs` — "go to the bath and pick it up". The client
  has `act`.
- **A queue of jobs: `nav.jobs`** (protocol v4). The agent plans "go to
  the bath, then home"; the duck runs it on the board, step by step,
  each to its end — a journey to its arrival — and says each step's end
  on the `queue` topic. The first failure stops the queue unless asked
  to go on; a stop ends the step under way and the queue. One queue at a
  time; it moves the duck, so it respects the lease. The runner is
  `quack_nav::queue`, pure and tested; the daemon runs the steps on the
  same path as `nav.call`. The client has `jobs`, `jobs_state` and
  `jobs_stop`.
- **One owner at a time: `nav.claim`** (protocol v3). Without a lease
  anyone may drive, as before. A connection that claims the duck holds
  its motion for up to 300 s, renewed by claiming again: `robot.go_to`,
  `robot.map_explore`, `robot.map_step` and `robot.move` from anyone
  else are refused (−32010) naming the holder. Reads always pass, and a
  stop always does — and ends the lease; safety does not wait for
  ownership. The lease ends at `nav.release`, at its ttl and when its
  connection closes, said on the `lease` topic. The client has `claim`
  and `release`. The lease is the navigation's, not the motors':
  `robotd` stays the only owner of the joints.
- **A client crate: `quack-nav-client`.** A blocking client on the
  protocol's types — connect, hello, the catalogue, a tool call, a
  subscription as an iterator of events — so a satellite, a control
  page or a script stops writing the wire by hand. A daemon from before
  `nav.hello` answers as a v0 and its tools still work. The `watch`
  example prints a daemon's events; on the twin it heard a journey from
  its start to `arrived at (0.60, 0.00)`.
- **Events, pushed: `nav.subscribe`** (protocol v2). A connection that
  subscribes hears `job` (a job started, got its goal, ended — done,
  stopped, failed — with its reason), `job.progress` once a second,
  `pose` (trusted or not), `fall` and `question`, as notifications
  between the answers to anything else it asks; `seq` without gaps, so a
  client can tell it missed one. The events come from one watcher that
  reads the job's status and the map's pose five times a second while
  someone listens; nothing in the navigation waits on a subscriber, and
  one more than 256 events behind is dropped. Polling `robot.map_status`
  stays correct. On the twin a short journey was heard from the pose
  found to `arrived at (0.60, 0.00)`, with its progress each second.
- **The protocol has a version: `nav.hello`.** The first step of the
  service in `robotd`'s style ([docs/design/service.md](docs/design/service.md),
  the roadmap's N4): a `quack-nav-proto` crate holds the version, the
  method names, the params and results and the error codes, and both
  the daemon and its clients build against it. `nav.hello` answers
  `{api_version, daemon_version, revision, methods, events}`; a client
  may say who it is and which version it was built against, and a skew
  is logged and served, never refused, as `robotd` does. Unknown members
  are refused by name. `nav.catalog` and `nav.call` are unchanged. The
  wire and its history: [docs/protocol.md](docs/protocol.md).

- **One walk per state of the body: free, or carrying.** A load in the
  beak changes the walk, and its numbers must neither overwrite the free
  walk's nor wait for them to be forgotten. The state is read from
  robotd, never guessed: the mouth is a servo, and a mouth commanded
  shut that sits open past its target by 12° for a second has something
  in it (back under 6° for a second, it is empty) — `joints[mouth] −
  targets[mouth]` on the state stream. The ledger's key gains
  `/carrying`; the carrying walk starts from the free walk's numbers as
  they stand, counts its own on half the samples (the fast lane: the
  body carries rarely and briefly), answers with the free walk's where
  it has none yet, and the walk switches as the state does — a duck
  that drops what it carried is on its free numbers at once. The twin's
  mouth never moves in MuJoCo and sits 5° past its closed target, under
  the threshold on purpose: the twin never leaves the free walk, and
  what the state does to the walk waits for a duck with a beak.
  `robot.gait` reports `state`, `mouth_gap_rad`, `walks` and `fast`
  ([docs/control-contract.md](docs/control-contract.md)). Twin A/B
  against frozen `main` (AB24, four tours of each house, both ledgers
  empty): 48/48 arrived on both arms, 0 falls, the times the same
  within noise (casa_ingombra paired median +0.0 s, casa_arredata
  −0.5), the body free throughout and one walk in the file.
- **The ledger learns the settle after a stop and the walk's left/right
  yaw gains.** The settle — how long the body goes on after its twist is
  gone, the coded 0.6 s of the velstand lines, which sets the mapper's
  still window and the length of every stand — is watched on the body's
  state stream: from the state whose applied twist is gone to the first
  whose odometry stands still, bounded 0–1.5 s. The twin learned 0.12 s
  (samples 0.05–0.42): the coded number was measured from the command
  going to zero, the ledger measures from the applied twist vanishing,
  which robotd smooths out on its own, and from there the body is still
  almost at once — every stand 0.48 s shorter. The gains: the stick's
  0.6 s pulses cannot give the yaw per unit as one number, but they give
  the two sides' ratio at the same commands (|vyaw| 0.3–0.6, the clamp
  left out): each side's yaw per unit of steering sent is its own
  number (16 samples before it counts: a pulse's yaw is the noisiest
  sample there is), each side's gain is the two sides' mean over its
  own, within 0.75–1.25, laid under every walking yaw command once both
  sides are learned. The trim is left out of the denominator (it
  cancels the veer, it does not steer: with it in, the bench's perturbed
  walk read its veer as 0.99 against 0.55 between sides where the walk's
  own are 0.75 and 0.84), and a walk that veers past 0.06 rad/s gives
  no side sample at all. The twin's sides came out 0.67–0.82 rad/s per
  unit to the left against 0.85–1.05 to the right (the right turns
  more, as the calibration had it): gains about 1.2 and 0.86, and the
  yaw per unit *asked* met between the sides (0.73 against 0.83 where
  main's were 0.71 against 0.86). Twin A/B against frozen `main`
  (AB23, four tours of each house, both ledgers empty): 48/48 arrived
  on both arms, 0 falls, the branch 9 s a journey faster (casa_ingombra
  107.9 s against 117.1, paired median −12.5 s; casa_arredata 91.8
  against 101.5, −8.5). Exploration A/B (e22, both houses from nothing,
  three sessions, two rounds of journeys on the frozen map): maps as
  full as main's (casa_ingombra 79 % against 80, casa_arredata done on
  the duck's own word after two sessions against main's three without),
  journeys 22/24 against main's 21/24 (the bedroom's goal fell in an
  unmapped pocket for both arms and was refused; main refused the
  office once too), 0 falls, −10 s paired medians in both houses. The
  bench the same within noise ([docs/results.md](docs/results.md)).
- **A turn in place's time budget on the rate the ledger measured; the
  pulses' yaw per unit measured and left.** The seconds a turn in place
  gets before it is given up as not turning were twice its time at
  0.5 rad/s, the slowest side ever measured; they are now twice its time
  at the rate this duck's own turns were measured at (the ledger's
  `turn_rate_rad_s`, 0.87–0.95 rad/s on the twin), the old floor until
  one is learned — the stick's, the explorer's and the homecoming
  search's turns alike. The yaw per unit was to be learned from the
  stick's 0.6 s pulses, nearly all the curving legs there are; measured
  on the twin it is not one number: it falls with the command (frozen
  main's 447 pulses in casa_ingombra: 0.99 rad/s per unit at |vyaw| 0.3,
  0.82 at 0.4, 0.76 at 0.7), and laid under the stick's steering gain
  (1 / (yaw per unit × pulse)) it closes a loop — a lower number, a
  higher gain, bigger commands, a lower number again — that ran the
  value to its bound (0.82, then 0.63) and doubled the stick's curving
  pulses (912 against 447): 48/48 arrived and 0 falls on both arms of
  that A/B (AB21), but 17 s a journey slower in casa_ingombra (paired
  median +8.5 s; casa_arredata +0.0). So the pulses give no sample, as
  before, and the number stays at the prior the calibration fitted on
  legs of a second or more; a gain curve by command size, or a stick
  that closes its heading on odometry, is the way ([docs/results.md](docs/results.md)).
  Twin A/B of the budget alone against frozen `main` (AB22, four tours
  of each house, both ledgers empty at the start): 48/48 arrived on both
  arms, 0 falls, the times the same within noise (casa_ingombra 119.2 s
  a journey against 115.8, paired median +1.5 s; casa_arredata 99.2
  against 99.5, paired +0.0); the rate learned 0.82 and 0.96 rad/s.
- **A roadmap, in the shape of Pollen's own.** [docs/roadmap.md](docs/roadmap.md):
  where quack-nav is, the milestones with the test that says "done" for
  each, the order of work, the decisions that shape it and what it does
  not do on purpose — quack-nav as a service one floor above `robotd`,
  for whatever sits above it. The README and the todo point to it.
- **The gait ledger: the duck learns its own walk and keeps it.** The
  walk's numbers the navigation plans with — forward speed, the yaw a
  unit of command turns, the trim that keeps a straight step straight,
  the turn in place's coast and rate — were coded priors per daemon line
  and profile, measured on the twin; a real duck, and every walking
  policy Pollen ships after the measurement, walks its own way, and the
  one number learned at run time (the turn's coast) was lost at every
  restart. Every leg and turn is now a measurement, as the calibration
  reads them off the traces: a free forward step on a pose the map
  vouches for at both ends gives the speed (by the map's pose) and, when
  straight, the trim that would cancel its veer (by odometry); a curving
  leg of 0.8 s or more the yaw per unit (a step on the heels of a turn
  in place gives no yaw: the turn's coast would read as a veer); a turn
  in place its rate; the map's yaw at the next leg the coast. The ledger keeps the last 32
  samples of each, lays their median over the prior once 5 are in (the trim 10),
  bounded about the prior (speed 0.5–1.6×, yaw per unit 0.5–1.8×, trim
  ±0.25 rad/s, coast 0–0.7 rad), and writes them beside the map
  (`/var/lib/quack-nav/gait-ledger.json`, `ledger_path`) per daemon line
  and profile, so the next run starts where this one got and a daemon of
  another line starts from its own priors. `robot.gait` reports it
  (`robot.map_status` carries it as `gait`) and `{"reset": true}`
  forgets it; `QK_GAIT_LEDGER=0` leaves the priors alone. The turn's
  rate is reported, not yet used (the turns are closed on odometry).
  On the bench, a walk the priors get wrong on purpose (speed +40 %, a
  veer of 0.12 rad/s, yaw per unit ×0.8): the stick arrives 98.2 % with
  the ledger against 98.6 % without, 1.3 s a journey faster, the same
  bumps — a journey is closed on the map's pose and odometry, so the
  numbers matter little there; the twin told the wrong line's priors
  (`daemon = "0.16"` on daemon-v0.17.6), exploring casa_arredata for 15
  minutes: the speed learned at 0.103–0.110 m/s (the twin's 0.107, the
  prior 0.115), 241 samples, 0 falls. Twin A/B
  against frozen `main` (AB20, four tours of each A/B house): 48/48
  arrived on both arms, 0 falls, the times the same within noise
  (casa_ingombra 111.5 s against 112.3, casa_arredata 101.0 against
  100.5, paired medians +0.1 and +0.0 s); the ledger learned 0.103–0.109
  m/s (prior 0.095), a trim of +0.02…+0.01 (prior −0.03), a turn at
  0.88–0.98 rad/s and a coast of 0.25–0.27 rad, and carried them from one
  house to the next. Exploration A/B (e21, both houses from nothing,
  three sessions of 30 minutes each, then two rounds of journeys on the
  frozen map): the maps as full as main's (casa_ingombra 70 % against
  74, casa_arredata 74 % against 71, the rooms within a few points),
  24/24 journeys arrived on both arms, 0 falls, the journeys no slower
  (paired medians −5.0 and −2.5 s); the ledger, empty at each house's
  start, learned 0.105–0.107 m/s, a trim of −0.02…−0.03, a turn at 0.93
  rad/s and a coast of 0.26–0.27 rad over 2 570–3 120 steps. On Pollen's
  newer stack (daemon-v0.17.9, promoted 2026-10-26, a daemon the 0.17
  line's numbers were never measured on; `daemon = auto`, nothing
  edited, the ledger empty): casa_arredata mapped from nothing in two
  sessions (81 %, the house declared done by the duck), 12/12 journeys,
  0 falls, the same with the ledger off on the same stack (78 %, 12/12,
  0 falls, paired median 0.0 s) — the walk did not change between
  0.17.6 and 0.17.9, and the duck ran on it as is. On daemon-v0.18.3
  (API v47, 2026-10-30; the same walk, the fall predictor armed only
  from standing), the same test: `daemon = auto` took it as the 0.17
  line, the house done in two sessions on both arms (78 % with the
  ledger, 77 % without), 12/12 journeys each, 0 falls, the ledger's arm
  5 s a journey faster (paired median), the ledger learning 0.099 m/s,
  a settle of 0.13 s, a turn at 0.75 rad/s and the sides 0.81/0.93
  ([docs/results.md](docs/results.md)).
  The simulator's veer now acts under every forward command, not only
  under |vyaw| < 0.1 (a trim past that turned the simulated body the
  other way); `rl_eval --ledger` opens the ledger in memory for a bench
  that learns in order (`--threads 1`), and `examples/ledger_check` walks
  a wrong calibration and prints what the ledger finds beside the truth.

## [0.3.0-rc3] - 2026-10-27

A fifth release candidate, validated on the MuJoCo and paper twins only.
Release notes: [docs/release-notes-v0.3.0-rc3.md](docs/release-notes-v0.3.0-rc3.md).

### Added

- **A look at the floor before a step onto what the map does not know.**
  The sensor cannot see the floor nearer than where its nearest row
  lands — 0.4–0.6 m by pitch, in a stand or walking alike — and a
  stairwell against a wall is hidden a second way within 0.4 m of its
  rim (the wall beyond answers at the range the floor was expected, and
  the guard calls the drop a wall's foot). Every reckless fall on the
  bench was that: a turn in place taken 30–66° past what was asked, then
  straight steps into an unbooked stairwell no frame had seen. Before a
  forward step whose stride (0.12–0.40 m ahead, a body wide) runs onto
  cells the map holds as neither floor nor wall, or that is the first
  after a turn in place of 0.5 rad or more — the map cannot vouch for
  that stride: the mapper marks every cell under a beam free up to the
  wall the beam hit, a stairwell against a wall included — quack-navd now
  pitches the head at the floor 0.30 m ahead (robotd's `robot.look`, a
  request: the daemon drops it as a notification) for 0.45 s and hands it
  back; the mapper's head sweep stands aside for the look
  (`ExploreHandle::lend_head`). The rows land from about 0.15 m and what
  they see goes through the guards and on the books. The guarded step,
  the stick's step and the pilot's, once per pose; `QK_LOOK_DOWN=0`
  switches it off. The simulator's sensor takes the head's pitch. Bench:
  arrivals 98.8 / 98.3 % (pilot / stick, from 98.8 / 98.6), 2.5–3.4 s a
  journey more; reckless brains, 600 seeds a family at both calibrations:
  0 falls into a hole in 25 200 journeys, from 13 ([docs/results.md](docs/results.md),
  2026-10-27). `rl_trace` takes `RUST_LOG` (`quack_nav=debug`) over its
  `quack_nav=info`.
- **The simulator's houses are the twin's** (`quack-rl`): `--families
  twin:<house>` walks the MuJoCo twin's tour from
  `scripts/twin/houses/<house>.truth.json` — each journey starting where
  the twin's last one stopped, its things put down since unmapped, its
  people and pets moving — and `twinlike:<house>` trains in the twin's
  other houses (`--focus`). A tip-over gets up after 4 s on the 0.17 line
  with its pose lost a while, instead of ending the journey; a turn in
  place coasts on after its stop as set v8 does (`turn_coast_rad`,
  fitted by `rl_calib`); the sensor's rows sit where the walk carries the
  head and where the calibration's pitch puts them (`tof_pitch_rad`);
  steps turn by side and back-offs as measured; the gate also refuses
  more timeouts, tip-overs or seconds a journey than the stick.
- **People walking between the rooms, on the twin and on the bench alike.**
  `casa_gente` (`scripts/twin/houses/gen.py`) is casa_arredata with four
  people walking paths between its rooms — through the kitchen's and the
  bedroom's doors, the living's and the office's, the bath's and the
  passage by the stairwell, the corridor — and casa_viva's cat. The twin's
  movers (`scripts/twin/viewer/movers.py`) and quack-rl's new
  `MoverKind::Patrol` walk the same rule with the same numbers: they wait
  while the duck is within 0.35 m ahead; waited 3 s, they step 0.5 m to
  the side where the house leaves room; where there is none (a door, the
  passage by the stairwell) they step back to the point of the path they
  came from and pause there. quack-rl reads a house's `movers` from its
  truth, so `twin:` and `twinlike:` casa_viva, casa_porta and casa_gente
  have their people on the bench and in training. On the bench's
  casa_gente (192 journeys) pilot v8l-s3 arrives 89 % of the time with
  2.2 contacts with people a journey, the stick 84 % and 8.0: what is
  lost is the duck bumping someone who waits in a door. casa_viva on the
  bench matches the twin (the stick about 0.7 contact episodes a journey
  against the twin's 0.9). The bench's seven families are unchanged to the
  journey. `rl_trace` records where the movers stood, and
  `scripts/rl/trace_gif.py` draws a trace as a GIF, the stick and the
  pilot side by side.

- **`daemon`: one key chooses Pollen's daemon** (README, "The daemon").
  `"auto"`, the default and what a config without the key gets, asks
  robotd at start (`hello`'s `daemon_version`) and takes the newest line
  when robotd says nothing within 30 s; `"0.16"` (up to daemon-v0.16.1,
  policy set v7) or `"0.17"` (daemon-v0.16.2 on, set v8) — or a release,
  `"0.17.6"` — names it. velstand's numbers are the line's, one file per
  line (`quack-duck/src/daemon/`); alpha's are the same on every line.
  The twin: `DAEMON`, auto by default.
- A pilot says the daemon line it was trained on (`meta.daemon`, 0.16 when
  it says none; `DAEMON=` for `calibrate.sh`); a velstand pilot flies only
  its own line, and `QK_RL_POLICY`'s directory may hold one per line
  (`0.17/velstand/pilot.json`).

- **Pilot v8l-s3 ships for velstand on the 0.17 line**
  (`quack-rl/pilots/v8l-s3-velstand/`, the install package's
  `pilots/0.17/velstand/`, which the loader takes before `velstand/` on
  that line): trained on the twin's traces and houses with the
  simulator's sensor made to see as the twin's. On the twin, four tours
  of casa_ingombra and casa_arredata: 48/48 against the stick on the same
  build (2.2 and 4.4 s a journey faster), 48/48 against frozen `main`'s
  stick (4.2 and 3.9 s faster), no fall; on the bench 98.6 % arrived
  against the stick's 98.3 %, 6 tip-overs against 18. The first pilot on
  the 0.17 line not slower than the stick ([docs/rl-pilot.md](docs/rl-pilot.md),
  [docs/results.md](docs/results.md)).

### Changed

- **The handover between the pilot and the stick.** The stick takes the
  legs from a pilot that gets no nearer the goal by 0.15 m in 12 s and
  keeps them until the body is 0.3 m from where it took over (30 s at
  most); within 0.5 m of a booked rim the stick walks the legs (on the
  twin the pilot had dithered in the passage by casa_arredata's
  stairwell, +28 s to the bath); nothing seen ahead is booked within
  1.5 s of a turn in place. Pilot v8l-s3 on the 0.17 line: 48/48 on the
  twin, 2.2 and 4.4 s a journey faster than the stick (docs/results.md,
  "Round two on the 0.17 line").
- Built against daemon-v0.17.6 (API 44; the additions since v0.16.1 are
  optional on the wire, so the same build runs on every daemon from
  v0.14.4). velstand on the 0.17 line (set v8, measured on the twin):
  0.095 m/s at vx 0.3, 1.1 rad/s per unit of yaw walking, a turn in
  place asked at 1.0 (about 65°/s), a trim of -0.03 for its veer to the
  left. On that line the exploration's turns in place stop short by the
  learned coast too (set v8 turns on further; without it the exploration
  turned back and forth). Twin A/B against main on daemon-v0.16.1:
  no regression in reach, falls or wake-ups (docs/results.md, "daemon
  0.17.6 and policy set v8").

### Fixed

- **A turn in place went on a third of a right angle past its close.**
  A timed move repeats its command every 50 ms until its time is up and
  ends without a stop; robotd holds the last command to its deadman,
  500 ms, and at set v8's 65°/s that is 33° of turn after the explorer
  has read the turn as done — whatever the turn asked. Measured on the
  MuJoCo twin (2026-10-27, turns of 20 / 45 / 90° closed on odometry as
  `stick_turn` closes them): 36 / 45 / 47° past the close with nothing
  sent, 3–6 / 5–15 / 8–21° with a stop sent at once; the odometry's lag
  at the close is 0–5°, so shorter chunks were not the cure. The coast
  the next turns stop short by had learned the hold as the gait's coast
  (33–40° in the A/B of the day); under 15° from a standstill the body
  turns and comes back, the turn never made. The explorer now sends a
  stop at the end of every turn in place (`Body::stop`; `QK_TURN_STOP=0`
  leaves the command, as before), and the coast learns what the gait
  itself coasts. The deadman is robotd's on the duck as on the twin. The
  simulator holds a spin's last command to the deadman too unless
  stopped, and splits the coast fitted from the traces (the hold and the
  gait's own in one) accordingly; a walking leg's hold is not modelled
  (the speed fitted from the traces covers it — to measure). Bench:
  arrivals 99.0 / 98.1 % (pilot / stick, from 98.8 / 98.3), journeys
  1.3 / 2.7 s shorter; reckless brains, 25 200 journeys: 2 hole falls,
  both one known chaos seed (300570, from 0), reckless timeouts down a
  sixth to a third. Twin A/B against frozen `main`, 48 journeys each:
  48/48 both, 0 falls, the branch 2.0 s slower in the cluttered house
  (median 1.5) and 5.9 s faster in the furnished one (median 3.1), the
  turns landing 12–16° past what was asked instead of 43–44°
  ([docs/results.md](docs/results.md)).
- **Things seen were booked 7.8 cm nearer the body than they are.**
  kinematics measures a hit's range from the sensor, not from the trunk
  axis its doc names (pollen-robotics/microduck#430); an obstacle now
  carries its distance from the trunk too, and the books use it. And a
  thing seen is booked from the pose of the frame that saw it, not from
  the body's pose up to a curving step later (a bag booked 0.55 rad
  round, into the passage beside it).
- The stick that took the legs from a pilot that got no nearer gave them back
  after 30 s even where it took them: now only 0.3 m from there
  (`STICK_AWAY_M`). On the twin the pilot had spent a whole journey's budget
  at casa_ingombra's basket by the office door, taking the legs back five
  times; with the fix 48/48 arrived.
- quack-rl's simulator turned in place only from |vyaw| 1.45, set v7's
  ask: on the 0.17 line (a turn asks 1.0) the simulated duck could not
  turn from a standstill and `rl_calib` found no turns. The thresholds
  follow the walk; `rl_calib` takes `--gait`/`--daemon` and starts from
  the line's prior (`calibrate.sh` passes `GAIT`/`DAEMON`).
- quack-rl's sensor saw a low thing only on its face: a beam that came down
  over it went on to the floor. Within 0.2 m of casa_ingombra's bag the
  simulator returned a third of the twin's hits walking (4 a frame against 10, at
  the twin's true poses from the body's new `TOF_TRUTH_LOG`); now it meets
  the top too and returns about what the twin does (10 against 10).
  `examples/tof_vs_twin` re-senses a twin trace in the simulator; `rl_trace`
  takes `--reckless KIND` (finalize's shield test, one seed) and `--log DIR`.
- Twin viewer: a map loaded at boot was drawn at the spawn, turned by
  its yaw; it is now placed once the pose is confirmed, as an exchanged
  map is (12 wakes: within 0.11 m and 4° of the true origin).
- Twin: `twin.sh` with the viewer off stopped on `TWIN_MOVERS` unset;
  `twin.sh down` now also stops what a lost pid file left on the state's
  sockets (a robotd left over owned the socket and the next one exited).
- Twin harness: `explore_ab.sh` starts each house from an empty state (a
  map left from the house before cost one arm a session).

## [0.3.0-rc2] - 2026-10-18

A fourth release candidate, validated on the MuJoCo and paper twins only.
Release notes: [docs/release-notes-v0.3.0-rc2.md](docs/release-notes-v0.3.0-rc2.md).

### Changed

- **Moving obstacles: four rules, on by default** (docs/results.md,
  "Moving obstacles"). Each is a knob (docs/knobs.md); set it to 0 for
  the v0.3.0-rc1 behaviour.
  - `QK_SEEN_AHEAD_M` 0.35 (0.30 before): something in the lane is
    booked a little sooner, before the bump.
  - `QK_BACK_STEP_S` 0.6: after a bump or a touch, a step back before
    the turn, only onto floor the map knows.
  - `QK_FORGET_FREE`: what a journey booked leaves the books when the
    sensor sees the floor where it stood; drops are never forgotten.
  - `QK_MOVER_AHEAD_M` 0.6: something in the lane coming nearer faster
    than the body walks is booked from this far.
  - Bench: the stick's tip-overs 18 → 13 (velstand) and 17 → 13 (alpha),
    bumps a journey 2.59 → 1.88 and 4.66 → 3.45, arrivals within noise.
    Twin (velstand): every house's journeys as `main`'s, no fall
    exploring (`main` one). The forgetting never fired on the twin.

### Added

- **The band: 2.5D at the duck's height** (`maploc::band`, README "Using
  it"). Per 5 cm cell, ten 2.5 cm slices from 0.05 to 0.30 m, filled by
  the depth sensor's returns and emptied by the beams that cross them,
  booked standing with the pose tracked, kept by submap and saved beside
  the map (`<map>.band`). It rides `map.frame` as `band`, the navigation
  reads it as wall (`QK_BAND=0`: not), and the twin's viewer draws it in
  violet. Returns within 10 cm of a wall in the submap's own 2D grid are
  left to the map (`BandConfig::wall_snap_m`), so the band does not
  thicken the walls. What it fixes: a coffee table's top, a bed, a tub
  read as free floor on the 2D map. On the twin against main: 48/48
  journeys both, no fall (docs/results.md). `examples/band` and `scripts/band/band_vs_truth.py`
  measure it on a recording against a twin scene.
- **A textured 3D house, off board (experimental)** (`scripts/map3d`,
  its README). `record.py` keeps the head camera's frames, the pose on
  the map and the depth sensor's ranges beside an exploring duck;
  `build.py` builds a 3D house with the camera's colours on a Mac or a PC
  (Depth Anything V2 Small, Apache-2.0, scaled to metres by the depth
  sensor; fused in a TSDF) and writes `house3d.bin` for quack-control's
  3D tab; `send.sh` (or `build.py --send user@host`) puts it on the duck.
  Twin, casa_arredata, 15 minutes exploring: walls and furniture 2.6 cm
  from the truth (median), 96 % within 10 cm.
- Twin: the head camera faces forward (`viewer/body_with_map.py`, only
  while the model's looks away from the depth sensor).
- Twin: people and pets that move (`TWIN_MOVERS`, `viewer/movers.py`),
  and two houses with them: casa_viva and casa_porta.
- `twin_ab.py --knobs ARM:K=V,...`: an arm with its own knobs.
- `quack-rl`: `passers`, a bench-only family (someone in the doorway
  when the duck gets there), outside the training draws.

### Fixed

- `maploc`'s `evaluate` panicked on a recording that ends in half a
  record; it now evaluates up to it.

## [0.3.0-rc1] - 2026-10-09

A third release candidate, validated on the MuJoCo and paper twins only.
Release notes: [docs/release-notes-v0.3.0-rc1.md](docs/release-notes-v0.3.0-rc1.md).

### Upgrading

- **Say which walk the duck runs.** `[gait] profile` in
  `/etc/robot/quack-nav.toml`: `"velstand"` (Pollen's default, and
  quack-nav's when the key is left out) or `"alpha"` (robotd on
  `alpha_walking` + `alpha_stand`). A duck on alpha with no `[gait]`
  section was alpha before and is velstand now: add `profile = "alpha"`.
- Pollen's **daemon-v0.16.1** (API 41) is the one validated.

### Added

- **The pilot, a neural navigation model, one per walk** (optional;
  docs/rl-pilot.md). An MLP (351 inputs, 9 moves) picks each step of a
  journey in place of the stick's rules, never the route; shields keep it
  off holes and blind back-offs, and the stick takes over when it
  hesitates. `pilot.json` (run by quack-navd in pure Rust) and `pilot.onnx`
  for each walk: v3-r7-mujoco (alpha) and v3-r7-velstand. The package
  installs them in `/var/lib/quack-nav/pilots/<walk>/`;
  `QK_RL_POLICY=/var/lib/quack-nav/pilots` flies the one of the profile,
  and a pilot never flies a walk it was not trained on. Bench (420
  journeys): 97.1 % arrived against the stick's 93.3 % (alpha), 95.2 %
  against 94.8 % (velstand); on the twin, velstand, 12/12. The `quack-rl`
  crate and `scripts/rl` train, calibrate (`calibrate.sh`, `GAIT=`) and
  gate it; `QK_RL_TRACE` records a duck's legs for the calibration.
- **`[gait] profile = "velstand" | "alpha"`**: the walk's numbers (speed,
  yaw per unit, turn in place, the settle after a stop) for the explorer's
  gait model and `[gait]`'s defaults. alpha's are the old constants.
- **An install package on every release.** CI packs
  `quack-nav-<version>-aarch64-linux.tar.gz` (with its `.sha256`) beside
  the bare binary, and a `v*` tag attaches both: the binary, the unit, the
  service account, the example config, the pilots, `install-on-duck.sh`
  and a step-by-step `README-install.md` (and `.it.md`), so the duck
  installs from a download, no checkout and no build. `install-on-duck.sh`
  runs from the package or from a checkout and takes `--dry-run`.
- Twin tools: `final_ab.sh` and `explore_ab.sh` (A/B against main, two
  twins at a time, per-arm stacks and walks), `gaitprobe.py`,
  `odoprobe.py`, `settleprobe.py`; `route_on_map` reports the nearest true
  hole.

### Changed

- **velstand works**: a standing velstand duck is labelled "walk" by
  robotd; the applied twist now decides, the duck counts as still 0.6 s
  after it, and every stand is that much longer. Before, the mapper never
  saw it still and the homecoming gave up.
- **On a journey, the stick**:
  - turns when the nose meets something within 0.15 m for two steps, and
    books it (unless a map wall or a booked drop is near);
  - books what the sensor keeps seeing in its lane within 0.30 m, where
    the map has free floor and no wall or booked drop is near;
  - stops a turn in place short by the coast it learns (35–44° on the twin:
    dithers 232 → 5, journeys 9–22 % faster).
- **A walked lane yields to a booked drop within 0.20 m** (was 0.12): the
  route keeps off a stairwell's unbooked corner.
- **The head sweep tries its own commands before standing aside**:
  robotd 0.16.1's idle glancing stops at any head command.
- Pinned to **daemon-v0.16.1** (API 41), was daemon-v0.15.0.

### Fixed

- **The cliff guard took a hole against a wall for a box's edge**: a drop
  with an obstacle at its bearing within 0.25 m either side was dismissed.
  Now only an obstacle no farther than the drop explains it (the guard's
  test; the books keep theirs). A brain choosing at random stepped into a
  stairwell once on the bench; 0 in 7,560 journeys since.
- The twin's exploration harness asks again while the duck stands up.

### Measured (MuJoCo twin, against main, four rounds a house)

Journeys 48/48 and casa_ingombra 12/12 with both walks (main 48/48,
11/12); exploration with velstand as main's (coverage, no phantom drop,
34/34 journeys after); no fall into a hole anywhere. docs/results.md.

## [0.2.0-rc2] - 2026-10-03

A second release candidate, still validated on the twins only. Release
notes: [docs/release-notes-v0.2.0-rc2.md](docs/release-notes-v0.2.0-rc2.md).

### Added

- **`robot.move` keeps a cliff guard, always on.** The timed move the
  voice agent ("go forward") and quack-control's Advanced tab drive with
  had no guard at all: at a stair's edge it walked off. Now, before the
  first command and at every tick, the depth sensor's newest frames
  (walking ones too, the blind leg's rule: a true hole, not a wall's foot,
  two frames agreeing) are read in the move's lane (0.17 m half-width):
  a hole whose near edge is within 0.40 m ends the move with one explicit
  zero, whatever the pose says and with or without a map. The reply says
  so: `{"done": false, "stopped": "a drop ahead (depth sensor): …",
  "walked_s": …, "cliff_guard": "on"}`; a complete move answers
  `"done": true` as before, with `cliff_guard`. Forward moves only — the
  sensor looks forward and down: backing up and sidesteps are `"not
  covered"`, a turn in place `"not judged"`, no guard or sensor
  `"off"`. The guard is the callers' alone (nav.call from a client: the
  voice front end, quack-control, any socket client): the explorer's legs,
  the rim tour, the journeys and the homecoming's pulses and turns move
  through `tools::internal_move`, unguarded as before, with their own
  guards tuned to approach a rim — given the always-on guard too, an
  exploration on casa_arredata (x25) had its legs stopped 17 times at the
  corridor's hole and sealed itself in. On the twin (casa_grande), facing
  the living room's hole from 1.15 m: stopped after 1.71 s with the trunk
  0.56 m short of the rim, and every further forward call refused at the
  edge; nine calls over 4 m of open floor, no false stop.
- **`robot.map_step` on an uncertain pose judges by the sensor alone.**
  With the pose lost or `untrusted` (moved while it rested) the map's
  checks — the wall ahead, the passage's sides, the wall-hug, the centring,
  the clearance in the reply — would be judged at a place the duck may not
  be: they are left out, only the depth sensor's checks apply (the drop
  guard, things in the way, "stand first" where it has not looked), and the
  reply's new `checks` says `"position uncertain: checks from the sensor
  only"` (otherwise `"map and sensor"`), `clearance` null, with a hint.
- **Driving the duck around by hand keeps its position**, checked: the
  rest's verdict on motion is robotd's step label, which any client's
  walking sets — Pollen's teleop through robotd, quack-nav never told,
  as much as `robot.move` — so a drive wakes the rest as "motion" and the
  pose follows odometry, trusted; only motion the gait does not explain
  (odometry moved past 5 cm or 0.15 rad with no walking: a carry; a sit;
  a fall) makes it `untrusted`. On the twin: resting, driven 16 s at
  0.3 m/s through robotd at 50 Hz like `padd` (1.17 m net, the gait
  veering) — woke as "motion", never untrusted, the map pose's
  displacement within 4 mm of the truth's, the next windows integrating. Pollen's teleop bypasses quack-nav's cliff
  guard, and robotd has none: said in the README's safety note and asked
  upstream ([docs/study/upstream-asks.md §8](docs/study/upstream-asks.md)).
- **The duck's own motion is said, and STOP stops it for good.** When
  the duck walks by itself — the homecoming's walk-and-look at boot (new
  `explore.state` `searching`), the exploring the homecoming starts
  (no confirmation, "starting a fresh map and exploring instead"; a
  progressive session; after adopting the saved map), the relocalization
  before a job — `robot.map_status` says so (`explore.self_started`, the
  reason, a hint) and quack-navd logs it once at warn. The user's stop
  (`robot.go_to`/`robot.map_explore` `{"stop": true}`, quack-control's
  STOP) now covers all of it, the boot's search included, and acts before
  the robot's lock: the leg in flight ends within a tick and the stand is
  cut short (on the twin the STOP answered in 0.15 s and the duck coasted
  3 cm; before, it waited 7–8 s for the step to end, 0.18–0.33 m walked).
  It then holds — `explore.stopped_by_user`, "stopped by the user; not
  searching again until asked" — until a job is asked for: the homecoming
  starts nothing more. A `robot.move` or `robot.map_step` while the duck
  moves on its own stops that motion and obeys (`stopped_own` in the
  reply); after a STOP they work at once. `robot.map_step` is now refused,
  like `robot.move`, while a job the user asked for runs.
- **Rest: long idle stands** (maploc). A minute still with no job driving
  the body (no `go_to`, exploration, rim tour, homecoming), the mapper
  rests: no window is integrated or corrects the pose, odometry alone
  carries it, and a *watch* — one window, the head swept once (6 s) —
  is judged against the map as the rest begins and every two minutes,
  nothing applied. Agreement keeps the rest; a pose the tracking's own
  match would move wakes it for the windows to correct; a residual past
  0.06 m, or 0.01 m past the rest's best, wakes them too the first time,
  and after that, twice in a row, makes the pose doubtful: lost, as the
  watchdog makes it, and searched for within a metre. A job, a move or a
  push nobody commanded ends the rest on its first tick (a `go_to` on the
  twin: 0.12 s from the call). `robot.map_status` and `map.frame` carry
  `resting` and `rest_watch` (the last verdict, its residual and age);
  quack-navd logs each rest, watch and wake. `MAPLOC_REST=0` keeps every
  stand awake. The twin's standing duck turns and slides by itself (its
  standing network, not quack-nav): over thirty-minute stands it turned
  up to 139° and slid 12-22 cm, and the pose stayed 5.5-7.9 cm from the
  truth on average; four recorded long stands replayed, rest against
  awake, even (the mean error 0.057 against 0.088 m on one, 0.046 against
  0.037 on another). quack-navd spends 2.0-2.35 % of a Mac core resting
  against 2.72 % standing awake; the replay, 46 % less CPU. Forty
  replayed sessions, ATE as before (mean 0.0920 -> 0.0913 m). The duck
  may be moved anywhere while it rests: two watches the map contradicts, a
  push past a nudge (5 cm, 0.15 rad of odometry with nobody walking it), a
  sit or a fall make the pose `untrusted` — searched over the whole map
  as at a boot, never resumed unverified, and only when a job asks: a
  `go_to` or `map_explore` then answers `relocalizing: true`, the
  homecoming's walk-and-look search finds the pose
  (`quack-nav/src/relocate.rs`, `explore.state` "relocalizing"), and the
  job starts once maploc confirms it — or fails with nothing walked
  toward the goal. `untrusted` rides `map.frame` and `robot.map_status`.
  On the twin: carried 3.3 m into another room, found in 70 s and arrived
  0.02 m off; a 0.35 m silent carry, found in 71 s, arrived 0.15 m off;
  after ~16 h standing, the pose 1.38 m off and untrusted, a `go_to` to the
  kitchen relocalized in 97 s and arrived 0.21 m off. The wake bench in
  localize (`WAKE_MODE=localize`, new): 28 of 28 wakes confirmed right,
  none wrong, median 87 s. Details in
  [docs/todo-map.md](docs/todo-map.md).
- **Places from a map view**: `robot.list_places` gives each place's `at`
  (the anchor `go_to` walks to, map metres); `robot.remember_place` with
  `x` and `y` names a point of mapped floor instead of where the duck
  stands. What quack-control's page needs to pin places and name a tapped
  point; nothing else changes.
- **The control contract** ([docs/control-contract.md](docs/control-contract.md)):
  what quack-navd offers a client that manages it — quack-control's local
  web page first. `nav.knobs` lists the knobs quack-navd reads and writes
  their env file, `/var/lib/quack-nav/knobs.env` (config `knobs_env`),
  which the unit now reads at every start (`EnvironmentFile=-`);
  `nav.restart` saves the session and exits for systemd to start the
  daemon again (`Restart=always`), and outside systemd says so. Both are
  methods, not tools: an agent never sees them. `scripts/knobs.py` also
  writes `quack-nav/src/knobs.json`, the machine-readable list (checked by
  `--check`). The twin reads `$STATE/knobs.env` too, and
  `scripts/twin/twin.sh restart-navd` restarts quack-navd alone.
- **The twin's camera**: with `VIEWER=on` the viewer serves the head camera
  on `$STATE/media.sock` with mediad's own call, `media.frame` (640×360
  UYVY, rotated 90° like the real mount), so quack-control reads the twin
  and the duck with one adapter (`scripts/twin/viewer/eye.py`).
- **A GIF of a `go_to` on the MuJoCo twin** at the top of the README
  (`docs/media/go_to.gif`: casa_grande, the far bedroom to the kitchen,
  played at 6×).
- **The map app study** ([docs/study/map-app.md](docs/study/map-app.md)):
  a robot-vacuum-style app over Pollen's channel; decided 2026-10-01: a
  local page on the home network first (quack-control), the rest dated
  TODO items.

### Changed

- **The shadow map works in localize** (maploc): its own mapper took the
  live map's configuration, frozen there, and inked nothing; it is now
  never frozen and never rests.
- **A relocalization no longer inks the frozen map** (maploc, localize).
  The window that confirmed a pose was inked while the mapper was still
  lost, so every boot and every recovery added a submap to the saved
  house (627 -> 628 on the twin's casa_grande), and a stand that began
  after it judged its windows against its own ink. Thirty replayed
  sessions on a frozen map: ATE mean 0.1183 -> 0.1169 m, 4 better and 4
  worse by more than 5 mm.
- **Pinned to daemon-v0.15.0** (API 37; was daemon-v0.14.4, API 34):
  Pollen's `duck-ipc-proto` and `kinematics` at that tag. API 35–37 are
  additive — `robot.state` gains the servos' measured `velocities` (rad/s)
  and `currents_ma` (present current, mA), optional on the wire, and
  `update.status` two fields of `updaterd`'s; `kinematics` is unchanged.
  For the board: `quack-navd` behaves the same, against robotd 0.15.0 or
  a board still on 0.14.4 (the new blocks are absent there and nothing
  reads them yet). robotd 0.15.0 publishes them by default (`[control]
  publish_velocity_and_load`), about 10–12 % more bytes per state frame.
  The twin runs it (`scripts/twin/README.md`).
- **`robot.map_step` is refused while a job the user asked for runs**, as
  `robot.move` already was (and as documented).
- **The homecoming no longer asks to explore a done house** after an
  adoption.
- **The wake bench** (`wake_bench.py`) can boot the duck as the
  navigation does: `WAKE_MODE=localize`, the map frozen, no exploring.
- **Teaching a place is refused** while the live map is unknown (the
  homecoming has not settled) and on the homecoming's search map.

### Removed

- The places registry's generation and its fewer-submaps rule (a place
  staled whenever robotd reported fewer submaps than ever seen), replaced
  by the map lineage below; with them `places::Place::generation` and the
  registry's `generation()` and `observe()`.

### Fixed

- **A house declared done is frozen after a power-on, whatever
  `resume_explore` says.** The homecoming froze a done house only when
  `resume_explore` was on: in `stop_and_scan` without it, the duck came
  home on casa_grande and went on inking the map it navigated (627 → 630
  submaps). `robot.map_load` and `robot.map_adopt` now freeze a done house
  as they load it, as `localize` freezes every map (the pose searched as
  when mapping, nothing inked); their answer carries `frozen`. A
  `map_explore` session under another `save_as` thaws it first; `fresh`
  starts a new map as before. On the twin, two boots on the done map:
  frozen at the load, 495 submaps throughout 16 minutes and two go_to.
- **A map turned from the house no longer counts the unknown beyond its
  walls.** The house's percentage, `left_m2` and the session end's
  largest unknown piece counted the unknown inside the walls' box on the
  map's own axes; a map started where the duck stood is turned as the
  duck was, and the box took in the grid's corners beyond the walls,
  joined to the house through any gap. "Esplora da capo" from the kitchen
  (a map ~200° from the house) read 55 %, 37.9 m² left and "stuck" on a
  15.4 m² piece. The box is now turned to the walls (the turn that lines
  the wall cells up best; the map's axes kept while they line them up
  within 90 %), a turned one trimmed a cell a side: the same map reads
  83 %, 9.5 m², 2.2 m² (the house's aligned map: 80 %, 11.6, 2.7), and
  both turned 15–290° on paper 82–83 %. Aligned maps are unchanged to the
  cell (the 19 of x24 and x26, the paper twin's gate).
- **A standing duck's pose no longer creeps along a wall.** At a stand,
  every still window's tracking correction took the last window's answer
  as its prior, so corrections of a centimetre added up along a direction
  the scene barely pins: on casa_grande (localize mode, after a `go_to`)
  the twin's duck stood fourteen minutes before one long wall and its map
  pose walked 1.56 m along it in 132 corrections, each improving its
  window's residual, until every `go_to` failed "no way to … on the map".
  maploc's stand now keeps its windows' prior, in position, where the stand
  began (the heading's stays the pose's own, which corrects odometry's yaw
  drift): replayed, the same recording stays within 9 cm of where it
  stopped (1.62 m before). Forty replayed sessions of ten runs: ATE RMS
  mean 0.0913 -> 0.0920 m, 14 better and 14 worse by more than 5 mm —
  noise. `trajectory`'s `FROZEN=1` now also freezes the map a
  `MAP_LOAD_AT_S` load builds, and its `CORR_LOG` carries each correction's
  conditioning.
- **Named places survive a power-on.** The registry made every place
  stale whenever the map reported fewer submaps than ever seen, and every
  boot with the homecoming starts on a fresh map before it loads the saved
  one: any tool call in between ("go to the kitchen") lost them all — one
  "places: the map was reset" per session on casa_grande, `cucina` and
  `soggiorno` stale after two restarts on the twin. A place now belongs to
  the map it was taught on (a lineage the registry keeps with the saved
  map's name): `pending` until the duck is confirmed on that map, `usable`
  then; `other_map` while another saved map is live; `stale` only when its
  map is gone (saved over by a new one, or wiped unsaved).
  `robot.list_places` adds `state`, `map` and `live_map` (`stale` stays,
  meaning "not on this map"), `robot.where_am_i` `pending_places`;
  `robot.go_to` says why a named place cannot be reached now. A version-1
  `places.json` is read and migrated ([README-places](quack-nav/README-places.md#which-map-a-place-belongs-to)).
- docs/knobs.md and `nav.knobs` give a default named by a constant as its
  value (`QK_DROP_INFLATE` 0.05, not `DROP_INFLATE_DEFAULT`), a knob with no
  comment above its read takes the sentence of its file that names it, and
  the twin's oracle knobs (`QK_ORACLE_*`) are left out of `nav.knobs`.
- The install examples ssh in as `microduck`, the board image's account
  (Pollen's #340), not `radxa`.
- The GitHub release's text: CI makes the release notes' relative links
  absolute to the files at the tag (`scripts/ci/release_body.py`); on the
  release page they led nowhere (fixed by hand on v0.2.0-rc1).

### Breaking changes for integrators

- **robotd**: pinned to daemon-v0.15.0 (API 37). quack-navd still runs
  against a board on 0.14.4.
- **`robot.move`** (callers through `nav.call`): a forward move can now end
  early or be refused at a drop — `"done": false` with `stopped` and
  `walked_s`; every reply carries `cliff_guard`, and `stopped_own` when it
  stopped the duck's own motion.
- **`robot.map_step`**: on a lost or `untrusted` pose the map's checks are
  left out (`checks` says which applied, `clearance` is null); refused
  while a job the user asked for runs; `stopped_own` as `robot.move`.
- **`robot.go_to` / `robot.map_explore`** on an untrusted pose answer
  `relocalizing: true` and start only once the pose is confirmed; a stop
  now also ends the duck's own motion and holds until a job is asked for.
- **`robot.map_status` and `map.frame`**: new fields `resting`,
  `rest_watch`, `untrusted` (in `map.frame` `resting` and `untrusted` are
  absent when false); `explore.state` has two new values, `relocalizing`
  and `searching`; `explore` gains `self_started` and `stopped_by_user`.
  A client that enumerates the states must accept them.
- **Places**: `places.json` is now version 2; a version-1 file is migrated
  on first read, and v0.2.0-rc1 refuses a version-2 file — keep a copy
  before upgrading if you may go back. `robot.list_places` adds `state`
  (`usable`, `pending`, `other_map`, `stale`), `map`, `live_map` and `at`;
  the old `stale` flag stays, now true for `other_map` as well.
  `robot.where_am_i` adds `pending_places`. `robot.remember_place` takes
  `x`, `y`, and is refused on an unknown live map.
- **`robot.map_load` / `robot.map_adopt`** freeze a house declared done as
  they load it, and their answer carries `frozen`; `robot.map_explore` on
  such a house is refused as before, a session under another `save_as`
  thaws it.
- **New methods** `nav.knobs` and `nav.restart` (not in `nav.catalog`);
  the unit reads `/var/lib/quack-nav/knobs.env` (`EnvironmentFile=-`, the
  config's new `knobs_env`) and keeps `Restart=always` — rerun
  `scripts/install-on-duck.sh` to get the new unit.
- **Rust API**: `places::Place::generation` is `map` (a lineage);
  the registry's `generation()` and `observe()` are gone.

## [0.2.0-rc1] - 2026-10-01

A release candidate: the whole stack validated on the twins. Release notes:
[docs/release-notes-v0.2.0-rc1.md](docs/release-notes-v0.2.0-rc1.md).

### Added

- **The mapper hosted in quack-navd** (ADR 0007, 2d495a9). `maploc`,
  derived from Pollen's upstream PR 127, is a workspace crate; with
  `[maploc] enabled` `quack-navd` runs it against Pollen's *released*
  robotd (pinned to daemon-v0.14.4, API 34), pans the head at stops and
  serves the map on `/run/quack-nav/map.sock` in robotd's `robot.map*`
  dialect. The worker thread runs at a lower priority than the daemon.
- **The pose at 20 Hz** (e92aa14): a light `map.pose` every 50 ms between
  the 1 Hz `map.frame`s; the live heading error as sampled went from 17°
  RMS to about 2°.
- **The pose's uncertainty** (735b564): a 3×3 covariance (x, y, yaw),
  reported by `robot.map_status` as `pose_uncertainty`. It changes no
  decision yet.
- **Homecoming and relocalization**: the saved pose carried by the
  wake-up's own motion (5b4a041); the boot search asks for confirmation at
  every quarter turn of its scan (708a791); a loss searches near the
  odometry-carried pose (baf1124); the watchdog tries a local match before
  calling a window a contradiction (89c5abe).
- **The adoption rule** (3b5d4df): a saved map is adopted on overlap ≥ 0.50
  and margin ≤ 0.50, asking every 60 s, three agreeing asks. On 27
  replayed wakes: 626 right answers pass (565 before) and none wrong (9
  before).
- **The shadow map** (10f5a22, 2247634, 9c37473): a duck resumed lost on a
  saved map keeps a fresh map of its walk and asks every 30 s where it fits.
  On the twin's wake bench 23 of 24 wakes confirmed right, none wrong,
  medians 87–123 s (126–192 s before). `MAPLOC_SHADOW=0` turns it off.
- **Exploration a charge at a time** (ADR 0008, 2f905c4): one session per
  charge, the map saved under its name, the progress on the books;
  `robot.map_status` reports `house.percent_mapped`; `map_explore complete`
  closes the map, `map_explore fresh` asks for `confirmed` before a new map.
  A finished map is frozen and the duck navigates on it.
- **The session-end rule** (fec3863): a session that has no frontier in
  reach and no unknown piece of 4.5 m² or more touching known floor finds
  the house mapped (furnished maps always keep frontier cells along walls).
- **Journeys on their own loop** (1adc36b, `explore/navigate.rs`): budget,
  pose, arrival and the stick (f98b42b: turns closed on odometry's yaw, a
  stand after every turn and every 0.4 m), the default since 9e84781;
  `robot.go_to` and the exploration's travel to its frontiers (67cde18) both
  use it.
- **Twins**: house2's wall truth (763170e); the generated houses
  casa_libera, casa_arredata and **casa_grande** (d60e067, 9016e8c: 9 × 7 m,
  seven rooms, a corridor turning 90°, two holes, never used to tune
  anything); the twin's scripts and viewer in `scripts/twin/`.
- **Measuring tools**: ATE/RPE (`traj_metrics.py`), the deterministic replay
  bench (`maploc/examples/trajectory.rs`), `wake_bench.py`,
  `maploc/examples/wake_match.rs`, `quack-nav/examples/drop_replay.rs`,
  `map_vs_truth.py`, `room_fit.py`, `LOOP_LOG`; `.mdlg` recordings stamp
  robotd's and tofd's clocks so a replay runs as live (f38b341).
- **CI** (429ba16): unit tests, golden routes, property tests, a replay
  regression and the paper twin gate on fixed seeds on every push; since
  a5c83e8 also `scripts/knobs.py --check`.
- **`docs/knobs.md`**: every environment variable the code reads, generated
  from the code.
- **The board's binary from CI**: an `aarch64` job cross-builds
  `quack-navd` on every push (artifact `quack-navd-aarch64-linux` with its
  sha256, glibc symbols checked against the 2.31 floor); a `v*` tag attaches
  it to the GitHub release, a prerelease when the tag has `-rc`.
- **Cross-building for the board** (`scripts/cross-build.sh`):
  `cargo zigbuild` for `aarch64-unknown-linux-gnu` with the glibc floor
  pinned at 2.31, no Docker; the binary needs glibc 2.30 and ran in a
  Debian 13 arm64 container. README, "Building for the duck".
- **Installing on the duck** (`scripts/install-on-duck.sh <user@host>`):
  binary, unit, service account and — only when absent — the config
  (`quack-nav/quack-nav.example.toml`, new) over ssh, an old
  `/var/lib/quacksat/places.json` copied across, the service enabled and
  restarted; rerun, it upgrades. README, "Installing on the duck", with the
  manual commands, checks, upgrade and uninstall; run in a systemd Debian 13
  arm64 container, not yet on a board.

### Changed

- **Knobs renamed: `QUACKSAT_*` → `QK_*`, with no alias** (81d466f,
  a5c83e8). The old names are not read any more.
- **169 → 122 knobs** (b7e11ee, 9975422, 76295c5, b1e776e): the knobs of
  concluded experiments were removed, each at its measured default;
  `QK_*` 72 → 35, `MAPLOC_*` 26 → 18. The removed names are listed at the
  top of [docs/todo-map.md](docs/todo-map.md).
- **Loop closures may correct the heading by 4° at most** (c6aae96,
  `MAPLOC_LOOP_CAP_YAW` default 0.45 → 0.07 rad). Twenty replayed sessions:
  ATE RMS mean 0.0946 → 0.0891 m. On the twin the heading's median error
  went from 0.97–1.26° to 0.63–0.92°; map and journeys unchanged.
- **The daemon's socket is `/run/quack-nav/nav.sock`** (was
  `/run/quack-nav.sock`; 7334989, ADR 0006): under systemd it lives in the
  unit's `RuntimeDirectory`, handed to the `robot` group at 0660.
- **`places_path` defaults to `/var/lib/quack-nav/places.json`**, with no
  fallback to `/var/lib/quacksat/` (a5c83e8).
- **Pinned to daemon-v0.14.4** (990d165; 0.1.0 used daemon-v0.10.0).
- The route walked with the string pulled 0.6 m at most, none beside a drop
  (3f2d00b); turning in place past the gait's dead zone (b8cebfe).
- The twin's viewer reaches the daemon over `QUACK_NAV_SOCKET`.

### Removed

- **The explorer's old leg path** (9975422): `walk_leg`, the guarded legs,
  the seal and the widening, with 29 knobs. It ran only with
  `QK_EXPLORE_NAV=0`.
- **The old journey path** (1fffbac): every journey runs `navigate.rs`.
- Experiments measured and not adopted: the traverse, the rim memory, the
  layered costmap and thinned walls (e26fa4a); the DWA local planner
  (a2708e9); Regulated Pure Pursuit (e5e7aaa); maploc's particle filter at
  boot and the ray judge (b1e776e).
- Public API nothing called (76295c5, b1e776e, 8fa4f28):
  `maploc::Mapper::boot_search`, `ExploreHandle::forget_ground`,
  `Grid::unknown_around`, `Control::request_method`, the head/look limits
  and `HOLD_STRAIGHT_MAX` in `quack_duck::body`, `frontier::waypoint`.
- The twin viewer's `QUACK_NAV_MCP` / `QUACK_NAV_MCP_TOKEN` (briefly
  `QUACKSAT_MCP` / `QUACKSAT_TOKEN`), replaced by `QUACK_NAV_SOCKET`
  (twin scripts only).

### Fixed

- A resumed session's re-anchor carries the odometry edge into it (8d5ffeb):
  replayed, wall error 6.9 → 4.6 cm on average, worst trajectory error
  1.95 → 0.16 m.
- A relocalization's pose is not carried off by the freeze its own jump
  caused (7e2825b): 20 replayed sessions, mean ATE 0.1045 → 0.0975 m.
- A covered sensor is not a hole (953285b): 18 phantom holes on the
  apartment's bed gone.
- Two valleys that cross resolve each other (54a49a9).
- No map saved while the duck is lost; no boot resumes on a pose nothing
  could judge (41e5ca3); after a fall the job waits for the pose (dfa044c).
- "Exploration complete" is final, and a finished map stays finished after
  a `go_to` (8643067, 0954d3d); after `fresh`, `map_explore` reports the new
  map (4aa11ba).
- The homecoming's boot search never saves over the house (e8d9482).
- Startup errors name their path: a socket that cannot be bound (the nav
  socket, the map socket), a config file that cannot be read or parsed, and
  the non-fatal warnings (robotd unreachable, the places registry, the saved
  session) say which file and what to do — run by hand without
  `/run/quack-nav/`, the daemon said only `No such file or directory`.

### Breaking changes for integrators

- Environment: every `QUACKSAT_*` knob is read only as `QK_*`; the removed
  knobs are ignored silently (list in docs/todo-map.md).
- Sockets and paths: `nav.sock` moved to `/run/quack-nav/nav.sock`;
  `places.json` is read only from `places_path` (default
  `/var/lib/quack-nav/places.json`) — move an old file by hand.
- Config: `[map] explore_turn` still loads but is ignored.
- Rust API: `Mapper::boot_search` and the other items above are gone;
  `explore::Job::new`, `to_goal`, `start` and `start_goto` lost the turning
  hand (`Job::new(known, max_s, ask, now)`).
- Logs: maploc's status line no longer carries the `boot` field.
- Twin viewer: `QUACK_NAV_MCP*` replaced by `QUACK_NAV_SOCKET`.

## [0.1.0] - 2026-09-22

The navigation split out of quacksat (ADR 0006, 747b233) with its history:
`quack-duck` (robotd's lane) and `quack-nav` (the map client of robotd's
`maploc`, the cliff guard, the costmap planner, the places registry, the
explorer, the homecoming, the twelve `robot.*` tools, the paper twin and
`quack-navd` on `/run/quack-nav.sock`), against daemon-v0.10.0. 60 tests.

