# The twin, on the released robotd

Everything needed to run quack-nav on the MuJoCo twin with Pollen's
**released** robotd (daemon-v0.15.0) and the mapper hosted in
`quack-navd` (`[maploc]`, see ADR 0007) — and to repeat the measurements
of 2026-09-23 (on daemon-v0.14.4) that say it behaves like the robotd fork.

## Setting up

```sh
# Pollen's daemon at the release, robotd and tofd built for the simulator
git clone https://github.com/pollen-robotics/microduck && cd microduck
git checkout daemon-v0.15.0 && cargo build -p robotd -p tof

# the simulator (its README sets up the .venv)
git clone https://github.com/pollen-robotics/microduck_rl

# this repository
cargo build --release
```

```sh
export MICRODUCK=~/src/microduck          # at daemon-v0.15.0, built
export MICRODUCK_RL=~/src/microduck_rl    # with its .venv
export POLICY_DIR=~/policies              # the alpha set: alpha_walking.onnx, alpha_stand.onnx,
                                          # alpha_sitstand.onnx, alpha_ground_pick.onnx,
                                          # ball_kick_left.onnx, ball_kick_right.onnx, roulade.onnx
```

The numbers here were measured with the alpha set; Pollen publishes the
shipped policies on the Hugging Face Hub.

The viewer draws the map, the route, the depth sensor's rays and the
guard's lane (`viewer/`: PR 202's `sim-maploc` overlay, by Peter Schade,
with the fork's additions and `QUACK_NAV_SOCKET`); `VIEWER=off` runs the
plain body server and shows the duck alone, `VIEWER_DIR` points at
another copy.

The viewer is also the duck's camera (`viewer/eye.py`): there is no
`mediad` on the twin, so it answers mediad's `media.frame` on
`$STATE/media.sock` — the same JSON-RPC line, header (640x360, `UYVY`,
`rotate` 90 like the real mount) and raw frame as `/run/mediad/media.sock`
on the duck (daemon-v0.15.0) — rendered from the head camera of the model.
quack-control's camera window reads it with the same adapter as on the
duck. Nothing is rendered while nobody asks; a request costs the
simulation one `mj_copyData` between two steps (~15 µs) and the render
(~15 ms at 640x360, with the UYVY conversion ~17 ms) runs on the camera's
own thread with its own `mujoco.Renderer`. `VIEWER=off`: no camera.

## Running

```sh
scripts/twin/twin.sh up        # simulator, tofd, robotd, quack-navd
scripts/twin/twin.sh enable    # the duck boots seated
python3 scripts/twin/call.py /tmp/quack-twin/nav.sock robot.map_explore '{}'
python3 scripts/twin/call.py /tmp/quack-twin/nav.sock robot.where_am_i
scripts/twin/twin.sh down
```

The knobs (docs/knobs.md) as on the duck: `$STATE/knobs.env`, which
`nav.knobs` writes (quack-control's page, or by hand `NAME=value` lines),
is in quack-navd's environment when the file exists, and
`scripts/twin/twin.sh restart-navd` stops quack-navd (it saves the map),
starts it again with the file, and leaves the rest running — what
`nav.restart` asks of systemd on the duck, and answers here that it cannot.

`STATE` (default `/tmp/quack-twin`) holds the sockets, the logs, the
session, the saved maps and a `.mdlg` recording of every run; keep it
short, a unix socket's path is at most 104 bytes on macOS. `MAPLOC_MODE`
(`stop_and_scan` or `localize`), `HOMECOMING` (`on`/`off`) and `WIPE`
(`on`/`off`) set up a boot on a saved house; `RESUME=on` is progressive
exploration (home on a map still being explored, saved at the session's
end), `EXPLORE_S` the budget of the exploring the homecoming starts (720)
and `BOOT_SEARCH_S` how long it stands to confirm before it searches
(240). `PORT` is the simulator's port (7872), `SCENE` the MuJoCo scene
(default the apartment). `ASK_PHRASE` is what the
explorer asks at a nameless area (default "Qui dove siamo?"). To talk to it, point a
voice satellite's `[nav] socket` at `$STATE/nav.sock` and its
`robotd_socket` at `$STATE/robotd.sock`.

## Measuring

| script | what it answers |
|---|---|
| `probe.py <robotd.sock> <tof.sock>` | what a mapper outside robotd receives: rates, fields, the two clocks |
| `spin.py <robotd.sock>` | how fast the panorama's turn turns (22–24°/s on both robotd) |
| `headwatch.py <robotd.sock> <s> [sway]` | who has the head; with `sway`, a thinking pose the sweep must yield to |
| `turnprobe.py <robotd.sock> <port> <label> [reps]` | turning from a standstill: dead below ~1.2 rad/s, 30–60°/s above it |
| `segs.py <file.mdlg> [from] [to]` | a recording as runs of moving and still — how the slow start was found |
| `ab_round.sh <n>` | the same route on the fork and here, both scored against the true walls (`FORK_TWIN`) |
| `scan_walk.py <s> <robotd.sock> <port>` | that route (Peter Schade's, PR 202) |

`maploc`'s own bench replays any recording: `cargo run -p maploc
--release --features kinematics --example evaluate -- <rec.mdlg>
<truth.toml> <out>`. Beside it, same features:

- `trajectory -- <session.mdlg> <pose.tsv> <out.tsv>`: the replayed pose
  against the truth, for `traj_metrics.py`; `MAP_SESSION` /
  `MAP_LOAD_AT_S` replay into the saved map the session resumed on,
  `SAVE_SESSION` saves the map built; `CORR_LOG`, `ODOM_LOG`, `LOOP_LOG`
  (every closure with its heading error against the truth) and
  `TRACK_LOG` write per-event logs; `ODOM_SIGMA_XY/YAW` and
  `LOOP_SIGMA_XY/YAW` override the graph's sigmas.
- `wake_match -- <session.mdlg> <pose.tsv> <start_s> <map.session>...`:
  the homecoming's map-to-map question asked offline, every answer judged
  against the truth (`ASK_EVERY_S`, `ASK_FOR_S`).
- quack-nav's `drop_replay -- <session.mdlg> <pose.tsv> <truth.json>
  "x,y;..."` (`cargo run -p quack-nav --release --example drop_replay`):
  where a recording's drops land by the replayed pose and by the truth;
  `BOOK=` for the book mode.

`MAPLOC_SHADOW=0` (maploc's, live and on the bench) turns the shadow map
off; `MAPLOC_SHADOW_EVERY_S` (30) and `MAPLOC_SHADOW_ASKS` (2) set it.

## The test houses and the release protocol

`houses/` holds what `docs/results.md` was measured with:

| file | what it is |
|---|---|
| `gen.py <robot dir> <out> [house ...]` | writes the houses named (casa_libera and casa_arredata by default; casa_grande too, 9 x 7 m, a corridor turning 90°, nothing that blocks): the MuJoCo scenes (into microduck_rl's robot directory), maploc's truth (`.toml`), the paper twin's world (`.world.json`) and the holes, rooms and goals (`.truth.json`) |
| `final_house.py <name> <scene> <state> <port> <truth> <out> [session_s] [sessions] [rounds]` | the release protocol on one house: progressive exploration from nothing, "exploration complete" if the duck has not finished, three restarts with a go_to tour on the frozen map, and the same on `main`'s build (`AB_REPO`, a worktree of main with its release built). `ROUNDS_ONLY=1` starts at the tours, from the map and book the exploration left in `<out>` |
| `aggregate.py` | the tables of `docs/results.md` from the protocol's outputs |
| `modes_test.py` | resume, "how far along", complete, the frozen map after a restart, a fresh map replacing the old one only when it saves |
| `run_house.py`, `prog_house.py` | the one-exploration and the sessions-only versions |
| `poseerr.py <nav.sock> <port> <out.tsv>` | the map's pose against the simulator's truth every 5 s (`POSEERR_DT`; 0.5 for loop closures), `<out>.untracked` while the mapper vouches for none |
| `wake_bench.py <name> <scene> <state> <port> <truth> <out> <maps_dir> <book.json> [limit_s]` | wake-ups from spots across the house on a saved map (the duck put there with `MICRODUCK_START`), each judged right, wrong or never against the truth; `WAKE_TURN` turns every spawn, `WAKE_SPAWNS="x,y,yaw;..."` replaces them |
| `traj_metrics.py <pose.tsv>` | ATE and RPE from a sampler's file (live or `trajectory`'s), `--tum` for `evo` |
| `map_vs_truth.py <frame.json> <truth.toml> <truth.json> [book.json name]` | an explored map against the house, room by room: walls on, thickened or phantom, free inside walls, faces, floor, fit, book |
| `room_fit.py <frame.json> <truth.toml> <truth.json>` | the rigid misfit of the map and of each room against the true walls |
| `oracle_book.py <truth.json> <name> <book in> <book out>` | the oracle's drop book: the true holes' rims in place of the booked ones |
| `cut_mdlg.py <in.mdlg> <seconds> <out.mdlg> [pose.tsv out.truth.tsv]` | the first seconds of a recording as a fixture, with its truth rows |

They need `MICRODUCK`, `MICRODUCK_RL` and `POLICY_DIR` as `twin.sh` does,
`TWIN_WORK` for their outputs (default `/tmp/quack-twin-work`), `QN_REPO` for
the quack-nav checkout whose `quack-navd` they run, and `AB_REPO` for the
checkout a round compares against (`final_house.py`'s `main` rounds; skipped, and said so, when it is not a whole, built checkout). A session,
round or wake whose twin does not come up is skipped, and said so, not
waited on over an empty log (since 2026-10-01). A house takes
about four hours on the twin; three run side by side on a 12-core Mac with
`VIEWER=off` on two of them.

Italian copy: `README.it.md`.
