# quack-nav

Navigation for the [Microduck](https://pollen-robotics.com/microduck/):
where the duck is, and how it gets somewhere else.

A daemon (`quack-navd`) and the library under it. It consumes robotd's
on-board `maploc` (upstream PR 127) for the map and the pose, and adds
everything above it: a cliff guard that judges the 8×8 depth sensor's
downward beams against the floor, a costmap planner, a registry of
places people taught it, an explorer that maps a house on its own, a
homecoming that recognises the house at boot — and a *paper twin* that
runs all of it against a kinematic model of the duck, thousands of
times an hour, so a rule is measured before it is believed.

Independent project, not affiliated with Pollen Robotics. Apache-2.0.
Split out of [quacksat](https://github.com/andreagenovese/quacksat) on
2026-09-22 (ADR 0006), with the history of every measurement that made
it.

## What it is for

A duck that knows where it is can be told where to go. `quack-navd`
answers for that on a unix socket, in robotd's own dialect — NDJSON,
JSON-RPC 2.0 — so anything can drive it: a voice satellite, an agent,
a ROS bridge, a shell script.

```sh
# the catalog it announces
printf '{"jsonrpc":"2.0","id":1,"method":"nav.catalog","params":{}}\n' | nc -U /run/quack-nav/nav.sock

# where am I?
printf '{"jsonrpc":"2.0","id":2,"method":"nav.call","params":{"name":"robot.where_am_i","args":{}}}\n' | nc -U /run/quack-nav/nav.sock

# map the house, then go to the kitchen
… {"name":"robot.map_explore","args":{}}
… {"name":"robot.go_to","args":{"place":"kitchen"}}
```

Twelve tools: `robot.where_am_i`, `robot.remember_place`,
`robot.forget_place`, `robot.list_places`, `robot.map_status`,
`robot.map_step`, `robot.map_explore`, `robot.go_to`, `robot.map_save`,
`robot.map_list`, `robot.map_load`, `robot.map_match` — each with a
JSON-Schema parameter list, ready to be projected onto OpenAI tools or
MCP by whoever hosts them.

## The crates

- **`quack-duck`** — robotd's lane (a JSON-RPC client over its unix
  socket), the gait's limits, the body's own commands. A client of the
  duck, nothing more; the satellite uses it too.
- **`quack-nav`** — the map client, the cliff guard, the planner, the
  places registry, the explorer, the homecoming, the tools, the paper
  twin, and `quack-navd`.

## What has been measured

Three weeks on the MuJoCo twin, written down as it happened in
[`docs/todo-map.md`](docs/todo-map.md) (Italian copy beside it) and
[`docs/study/baseline-twin.md`](docs/study/baseline-twin.md). The
shape of it:

- **Blind journeys on a saved map**: six goals round a flat, 6/6, about
  eight minutes, no fall — run after run since 2026-09-16.
- **The boot**: the duck wakes up, recognises the house it saved and
  confirms its pose in 70–330 s depending on the room.
- **The stairwell**: the passage beside a hole, 0.54 m wide, walked
  with the guards on when the pose is within 10 cm — and why it is the
  pose, not the rules, that decides (the scan matcher lags 8–10 cm
  along a corridor; measured, not guessed).
- **Things on the floor**: a 7 cm cube beside a blind leg is seen and
  gone round; under ~9 cm the sensor's own floor threshold loses it
  while walking.

## The paper twin

`quack-nav/examples/paper_twin.rs` is the explorer run against a
kinematic model of the duck in a flat of boxes: the real planner, the
real guards, the real job — with the gait, the depth sensor and the
pose's drift modelled from what the MuJoCo twin measured. Thirty trials
take a minute:

```sh
cargo run --release --example paper_twin -- \
    quack-nav/examples/apartment.world.json /tmp/out --runs 30 --goto -2.64,-2.12 --books
```

`--known` freezes the world as the map (the journey bench), `--bias
dx,dy` offsets the map's frame from the world (what a real pose error
does to a passage).

## Running it

```sh
cargo build --release
target/release/quack-navd /etc/robot/quack-nav.toml
```

```toml
socket = "/run/quack-nav/nav.sock"
robotd_socket = "/run/robotd.sock"

[map]
enabled = true
tof_socket = "/run/tofd/tof.sock"
places_path = "/var/lib/quack-nav/places.json"

[homecoming]
enabled = true          # recognise the house at boot, and take its map back

[maploc]
enabled = true          # host the mapper here, against the released robotd
mode = "stop_and_scan"  # or "localize" once the house is mapped
map_path = "/var/lib/quack-nav/maploc.session"
```

With `[maploc]` on, `quack-navd` runs Pollen's `maploc` itself (the
`maploc` crate in this workspace): it reads `robot.state` and tofd's
depth stream, pans the head at stops, and serves the map on
`/run/quack-nav/map.sock` in robotd's `robot.map*` dialect. Nothing in
robotd changes — daemon-v0.14.4 publishes everything the mapper needs.
With it off, the map comes from a robotd that hosts maploc itself.

`quack-nav/systemd/quack-navd.service` and `quack-nav/systemd/sysusers.d/`
install it as an unprivileged service beside robotd.

## Status

Measured on the MuJoCo twin (`microduck_rl` + robotd); the physical duck
arrives in December 2026. Two ways to run it:

- **Released robotd** (daemon-v0.14.4) with `[maploc] enabled`: the
  mapper in `quack-navd`. On the twin, 2026-09-23: explore, recognise the
  saved house at boot (78 s), walk to the kitchen (37 s).
- **A robotd that hosts maploc** — upstream PR 127, still open, plus the
  map library of `docs/study/upstream-asks.md` §5, which lives on a fork
  of `pollen-robotics/microduck` — with `[maploc]` off.
