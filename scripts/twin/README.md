# The twin, on the released robotd

Everything needed to run quack-nav on the MuJoCo twin with Pollen's
**released** robotd (daemon-v0.14.4) and the mapper hosted in
`quack-navd` (`[maploc]`, see ADR 0007) — and to repeat the measurements
of 2026-09-23 that say it behaves like the robotd fork.

## Setting up

```sh
# Pollen's daemon at the release, robotd and tofd built for the simulator
git clone https://github.com/pollen-robotics/microduck && cd microduck
git checkout daemon-v0.14.4 && cargo build -p robotd -p tof

# the simulator (its README sets up the .venv)
git clone https://github.com/pollen-robotics/microduck_rl

# this repository
cargo build --release
```

```sh
export MICRODUCK=~/src/microduck          # at daemon-v0.14.4, built
export MICRODUCK_RL=~/src/microduck_rl    # with its .venv
export POLICY_DIR=~/policies              # the alpha set: alpha_walking.onnx, alpha_stand.onnx,
                                          # alpha_sitstand.onnx, alpha_ground_pick.onnx,
                                          # ball_kick_left.onnx, ball_kick_right.onnx, roulade.onnx
export VIEWER_DIR=...                     # optional: body_with_map.py + maploc_overlay.py
```

The numbers here were measured with the alpha set; Pollen publishes the
shipped policies on the Hugging Face Hub.

`VIEWER_DIR` draws the map, the route, the depth sensor's rays and the
guard's lane in the viewer (the robotd fork's `sim-maploc/`, with
`QUACK_NAV_SOCKET` support). Without it the plain body server runs and
the viewer shows the duck alone.

## Running

```sh
scripts/twin/twin.sh up        # simulator, tofd, robotd, quack-navd
scripts/twin/twin.sh enable    # the duck boots seated
python3 scripts/twin/call.py /tmp/quack-twin/nav.sock robot.map_explore '{}'
python3 scripts/twin/call.py /tmp/quack-twin/nav.sock robot.where_am_i
scripts/twin/twin.sh down
```

`STATE` (default `/tmp/quack-twin`) holds the sockets, the logs, the
session, the saved maps and a `.mdlg` recording of every run; keep it
short, a unix socket's path is at most 104 bytes on macOS. `MAPLOC_MODE`
(`stop_and_scan` or `localize`), `HOMECOMING` (`on`/`off`) and `WIPE`
(`on`/`off`) set up a boot on a saved house. To talk to it, point a
voice satellite's `[nav] socket` at `$STATE/nav.sock` and its
`robotd_socket` at `$STATE/robotd.sock`.

## Measuring

| script | what it answers |
|---|---|
| `probe.py <robotd.sock> <tof.sock>` | what a mapper outside robotd receives: rates, fields, the two clocks |
| `spin.py <robotd.sock>` | how fast the panorama's turn turns (22–24°/s on both robotd) |
| `headwatch.py <robotd.sock> <s> [sway]` | who has the head; with `sway`, a thinking pose the sweep must yield to |
| `segs.py <file.mdlg> [from] [to]` | a recording as runs of moving and still — how the slow start was found |
| `ab_round.sh <n>` | the same route on the fork and here, both scored against the true walls (`FORK_TWIN`) |
| `scan_walk.py <s> <robotd.sock> <port>` | that route (Peter Schade's, PR 202) |

`maploc`'s own bench replays any recording: `cargo run -p maploc
--release --features kinematics --example evaluate -- <rec.mdlg>
<truth.toml> <out>`.

Italian copy: `README.it.md`.
