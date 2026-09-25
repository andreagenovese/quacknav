# quack-nav

Navigation for the [Microduck](https://pollen-robotics.com/microduck/):
where the duck is, and how it gets somewhere else.

A daemon (`quack-navd`) and the library under it. It hosts the mapper
itself — `maploc`, in this workspace, derived from Pollen's (upstream PR
127) — against Pollen's **released** robotd, and adds everything above it: a cliff guard that judges the 8×8 depth sensor's
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

## Using it

What a user can ask of the duck, through a voice satellite or anything
that speaks the socket:

- **"Explore the house."** Exploring is progressive: one session per charge
  (a time budget, or the battery under 25 %), each going on from where the
  last one stopped and saving the map at its end. After the next charge the
  duck finds the map, finds itself on it and explores on (`[homecoming]
  resume_explore`), until nothing large is left — then the house is done.
- **"How far along is the map?"** `robot.map_status` → `house.percent_mapped`,
  the sessions so far, done or not.
- **"Exploration complete."** The user closes the map as it is
  (`map_explore complete`): saved, declared done, frozen.
- **A new map.** `map_explore fresh` answers what would be lost and waits for
  `confirmed`; the saved map is replaced only when the new one's first
  session saves.
- **Going places.** On a finished map the duck explores no more, even after a
  restart: it comes home, freezes the map and navigates — blind where the map
  knows the floor, with the guard on where it does not. `robot.go_to` a named
  place or a point; `robot.remember_place` names where it stands.

The design is ADR 0008.

## The crates

- **`quack-duck`** — robotd's lane (a JSON-RPC client over its unix
  socket), the gait's limits, the body's own commands. A client of the
  duck, nothing more; the satellite uses it too.
- **`quack-nav`** — the map client, the cliff guard, the planner, the
  places registry, the explorer, the homecoming, the tools, the paper
  twin, and `quack-navd`.

## What has been measured

**[`docs/results.md`](docs/results.md)** has the release's numbers, the
criteria they are held to and the known limits. In short, on the MuJoCo twin
with three houses: no fall in 11 exploration sessions and 51 journeys; 90 %
of journeys arrived (`main`'s build, same maps: 63 %); every confirmed pose
within 20 cm of the truth; 5 of 7 release criteria met, counting partials as
half.

Before that, three weeks on the twin, written down as it happened in
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
resume_explore = true   # explore on after each charge until the house is done

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
  mapper in `quack-navd`. This is the preview's configuration; the numbers
  are in [`docs/results.md`](docs/results.md).
- **A robotd that hosts maploc** — upstream PR 127, still open, plus the
  map library of `docs/study/upstream-asks.md` §5, which lives on a fork
  of `pollen-robotics/microduck` — with `[maploc]` off.
