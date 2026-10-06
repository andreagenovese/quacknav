# The pilot: a learned policy for the stick's legs

Branch `rl-nav`, experimental. Italian copy: [rl-pilot.it.md](rl-pilot.it.md).

The journey (`go_to`) and the exploration's travel walk the planner's
route with the **stick** (`quack-nav/src/explore/stick.rs`): turn in place
when the route is off the nose, step curving onto it, book what the nose
pushed against after three steps that did not move the body. The **pilot**
is a small network that picks the leg's move instead, from what the duck
sees now: the route ahead, the depth sensor's last 1.4 s, the map and the
books around the body. It was trained on hundreds of thousands of
simulated journeys through generated houses with what the map does not
know on the way — things put down since, pets and feet crossing, doorways
half closed, passages beside a hole, low furniture, a map a little off.

Nothing else changes: the route is the planner's, the books are the
books, the stands for the mapper are the stick's, and the **shields** sit
over the pilot (below). Unset, quack-navd drives with the stick exactly as
before (the paper twin's gate gives the same numbers to the decimal).

## On the duck

| knob | what |
|---|---|
| `QK_RL_POLICY=/var/lib/quack-nav/pilot.json` | the pilot file; unset, the stick. A file that does not load (another observation version, a broken file) is said in the log and the stick drives |
| `QK_RL_TRACE=/var/lib/quack-nav/rl-traces` | every leg and stand recorded for the calibration (below), one JSONL file per start of quack-navd |

Both go in `/var/lib/quack-nav/knobs.env` (quack-control's knobs page
writes it) and take effect at the next `systemctl restart quack-navd`.
The network is a 327 → 256 → 256 → 9 MLP evaluated in plain Rust
(`quack_nav::rlnav::Pilot`), about 150 k multiply-adds a leg: well under a
millisecond on the board's Cortex-A55. The same network is exported as
ONNX (`pilot.onnx`) for anyone who wants to look at it with other tools;
quack-navd reads the JSON.

## What it reads, what it does

The observation (`quack_nav::rlnav::observe`, one function for the
simulator and the duck, version 2, 327 values):

- the route 0.2, 0.4, 0.7 and 1.0 m ahead, and the goal, in the body's frame;
- the depth sensor's memory in 12 bearing sectors across ±1.2 rad (the
  walking frames and the stand's sweep): the nearest obstacle and the
  nearest drop per sector, in the last 0.7 s and in the 0.7 s before (what
  moves shows as a change);
- the map around the body, 16 × 16 cells of 10 cm from 0.4 m behind to
  1.2 m ahead, with the books on it (free 0, unknown 0.5, wall or drop 1);
- its own last move, how many in a row the shields refused, how many legs
  did not move the body, how far the last one did.

The moves (9): five steps (vx 0.3 for 0.6 s, yaw −0.7 … +0.7: the stick's
own), a turn in place either way (0.4 rad, closed on odometry like the
stick's), a back-off (vx −0.3 with yaw +0.7: the only backing the gait does
from a standstill), and a wait (a 0.6 s stand, for what moves to pass).

## The shields

The pilot proposes, the shields dispose — on the duck and in training
alike, so the network learned with them:

1. **The hole guard** (the stick's own): a forward step with a true hole
   in its lane is not walked; the body turns from it and the rim goes on
   the books.
2. **No blind back-off**: backing only onto floor the map knows, or a wall
   (a bump, not a fall), off the books' drops. The first pilot backed into
   an unbooked stairwell, turning and backing by turns beside it.
3. **No step across a drop**: the step played through the gait model must
   not cross a drop on the books, nor the map's unknown within 0.35 m of
   one (a hole is never mapped as floor). Unknown far from any drop is
   floor nothing looked at — refused there, a patch across a corridor held
   the pilot for good.
4. **No pushing on**: after two legs that did not move the body, no step
   while something is ahead (the sensor's lane within 0.25 m, or the map's
   wall at the nose) — on the bench, a body scuffing along a wall slid
   sideways into an unbooked hole the wall hid from the sensor.
5. **The stick takes over**: after two refused moves in a row, the stick
   flies that leg. A deterministic pilot that asks again for what was
   refused would stand for good.

**Reckless brains** check them: a "pilot" that only ever backs, one that
only steps straight, one that picks at random, journey after journey in the
generated houses. None may fall (`rl_eval --reckless random|back|straight`;
`finalize.py` and `gate.py` refuse a pilot otherwise). Before shields 2-4,
the straight one fell 8 times in 40 beside the stairwells; after, none in
2,100 journeys.

## The training ground (`quack-rl`)

- **Scenarios** (`scenarios.rs`): seven families — `clutter`, `doorway`,
  `stairwell`, `corners`, `movers`, `low`, `mixed` — at four curriculum
  levels; each a piece of a house, a start, a goal 2-6 m away, and what the
  map does not know. The map is drawn as a mapper draws (furniture inked
  as a band, its inside unknown; patches of floor never seen), up to
  0.2 m off the world.
- **The body** (`body.rs`) is the paper twin's model, its numbers in a
  [`Calib`](../quack-rl/src/calib.rs): forward speed per vx, the yaw per
  unit, the veer of a straight step, the short pulse's random gain, the
  dead zone of the turn in place and its rate each way, the back-off;
  odometry and the map's pose drifting, the map's pose corrected at the
  stands and published every 50 ms (`map.pose`); the depth sensor at
  15 Hz, 8 × 8, with range noise, bias, dropouts, low things lost while
  walking, phantom drops; a bump slides along a face.
- **The loop is quack-navd's own.** Every training journey runs
  `Job::to_goal` — the route, the books, the stands, the shields — on the
  simulated body; the brain on the stick's legs answers from the learner
  over a pipe (`rl_env`). What the network learns on is what it flies in.
- **The expert** (`expert.rs`) sees the truth: the world's distance field,
  dearer near the rims and the furniture, and the movers. It drives first.

**Training** (`scripts/rl/train.py`): behaviour cloning with DAgger (the
expert drives, then the pilot drives more and more while the expert labels
what it met), then PPO on the reward — progress down the true way, time,
bumps, the shields' refusals, the rim's nearness; +3 arrived, −10 fell —
with the expert's labels kept as a fading auxiliary loss. 256 journeys at
once, about 30,000 legs a second on a 12-core Mac.

**Benches** (`rl_eval`): generated scenarios never used in training, the
stick, the expert and the pilot through quack-navd's own loop; the
checkpoint is chosen on seeds from 100000 and reported on seeds from
200000 (`finalize.py`, `report.md`).

## Calibration on the duck

What the simulator assumes comes from MuJoCo; the duck will differ. The
tool fits the simulator to the duck's own traces, retrains the pilot on
it, and lets it fly only if it beats the stick on that simulator:

1. **Record.** On the duck, `QK_RL_TRACE=/var/lib/quack-nav/rl-traces`
   and drive journeys as usual (`go_to` between marks: the stick's legs
   are enough; with a pilot loaded, its back-offs and waits are traced
   too). Twenty minutes of journeys give hundreds of legs.
2. **Fetch** `scp 'microduck@<duck>:/var/lib/quack-nav/rl-traces/*.jsonl' traces/`.
3. **Calibrate**: `scripts/rl/calibrate.sh calib-out quack-rl/pilots/v3-r6 traces/*.jsonl`.
   - `rl_calib` measures, number by number, against the prior: forward
     speed, the straight step's veer, the pulse's gain and spread, the
     turns in place each way, the back-off; the depth sensor's rate, its
     range bias and noise against the map at the stands, its dropouts, its
     phantom drops on known floor; odometry's drift where it stands out of
     the map pose's noise. What the traces cannot tell keeps its prior,
     and `calib.md` says so. It then replays every recorded leg through
     the gait model with the prior and with the fit and reports both errors.
   - the pilot trains on (PPO, 150 updates) in the simulator with the
     fitted numbers, varied narrowly around them;
   - `finalize.py` benches the new pilot, the old one and the stick on the
     calibrated simulator, and the reckless brains;
   - `gate.py`: the new pilot flies only with no fall, at least the
     stick's arrivals, and no more than 2 points under the old pilot. It
     prints the `scp`/`install` lines; otherwise it says what flies
     meanwhile (the old pilot if it passes, else the stick).

Checked on synthetic traces (a simulator with deliberately wrong numbers
standing in for the duck): the fit recovered the forward speed (0.098
against 0.095), the veer (0.040 against 0.040), the pulse's gain and
spread (1.12 / 0.30 against 1.1 / 0.3), the turns each way (0.70 / 1.15
against 0.70 / 1.15 rad/s), the back-off (0.063 against 0.06), the
sensor's rate (12.0 Hz) and its phantoms (0.008 against 0.01); the range
noise and dropouts come out as upper bounds (0.045 against 0.035, 0.076
against 0.06: the map's cells and the stand's pose add their own);
odometry's drift was below the map pose's noise, so the prior was kept and
the report says so. The replay's yaw error halved (0.102 → 0.055 rad).

## Results

See [Results](#results-1) below, filled from `rl-runs/*/report.md`.

## Limits

- Measured on simulators only: the generated houses, the paper twin's
  model, the MuJoCo twin. The duck has not run it.
- The walking policy is Pollen's and unchanged: stepping over things is
  not the pilot's to learn (it would mean retraining the gait with the
  terrain in its observation).
- Dynamic obstacles exist in the generated houses, not on the MuJoCo twin.
- The pilot is a reactive policy with 1.4 s of sensor memory: it does not
  remember a thing it saw and turned away from a minute ago; the books do
  (bumps and drops), the planner routes round them.
