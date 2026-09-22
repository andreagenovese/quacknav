# quack-nav: the places

Names for where the Microduck stands.

robotd's on-board `maploc` (Pollen Robotics, upstream PR 127) gives the
duck a 2D map of the house and a pose in it. It knows geometry, not
rooms. The places are the layer above: a *place* is a name somebody
attached to a pose ("this is the kitchen"), recognized later by
distance. Four tools expose it — `where_am_i`, `remember_place`,
`forget_place`, `list_places` — beside the eight that map the house and
walk it (`map_status`, `map_step`, `map_explore`, `go_to`, `map_save`,
`map_list`, `map_load`, `map_match`), all of them agent-neutral, ready
to be projected onto OpenAI tools, MCP, or anything else by whoever
hosts them.

This is the `quack-nav` crate of the [quack-nav](../README.md)
workspace (ADR 0005, ADR 0006): no voice in it. It depends on the
duck's IPC types, its head geometry (`kinematics`, pure Rust), the
robot lane (`quack-duck`) and serde, nothing else, and `quack-navd`
hosts it on a socket of its own.

## What is inside

| module | what it does |
|---|---|
| `map` | `robot.map` client: subscribes, keeps the newest `map.frame` (pose, tracking flag, trinary grid), reconnects on loss, turns itself off on a robotd that predates the map API, and bumps an *epoch* when the map frame was evidently reset |
| `places` | the registry: JSON file, several anchors per name, case-insensitive matching, a persisted *generation* that goes stale on a map reset (the lane's epoch, or fewer submaps than ever seen — a wipe while the host was down) |
| `tools` | the twelve tools as a catalog (JSON Schema) plus an executor on a `Robot` (robotd lane + map lane + registry + cliff guard): the places, the map in numbers with the clearance in four directions and a hint for a mapping tour, the explorer's jobs, the saved maps |
| `cliff` | the cliff guard: tofd's raw frames reprojected through Pollen's head geometry (`kinematics`); a downward beam that returns nothing, or 1.5× too long, where the floor should be is a drop — stairs, a hole — that the 2D map cannot show. Judged over 3 s in the body frame and kept for 8, so a head sweep accumulates a view |
| `frontier` | where the known floor meets the unknown: frontier groups, and a costed planner on the grid (known floor cheap, unknown dear, walls inflated, walked lanes always open) to the cheapest reachable one and to any goal — what "map everything" and `go_to` loop over |
| `passage` | threading a narrow passage: two side boundaries and the steering that keeps the body between them |
| `explore` | the jobs that drive: map a house, walk to a goal on a map already made, and the rules that keep a leg off the stairs |
| `homecoming` | waking up in a house the duck has mapped before: load the newest saved map, confirm the pose, or explore and ask again |
| `config` | the `[map]` section (`enabled`, `places_path`, `cliff_guard`, `tof_socket`, `explore_max_s`, `ask_phrase`, `explore_turn`), `[homecoming]`, and the daemon's own file (`NavdConfig`) |

Wire shape pinned to upstream API v17 (`MAP_API_VERSION`); the types are
a local mirror until the `duck-ipc-proto` release that carries them.

## Hosting it

The usual way is the daemon: `quack-navd` answers `nav.catalog` and
`nav.call` on its unix socket (see the [workspace README](../README.md)).
In process, the same crate:

```rust
let mut robot = quack_nav::tools::Robot::connect(&config.map, &config.robotd_socket, config.gait.clone());
// splice the catalog into your own …
let mut tools = my_tools();
tools.extend(quack_nav::tools::catalog());
// … and route the names it claims back to it
if quack_nav::tools::handles(name) {
    return quack_nav::tools::execute(name, &args, &mut robot);
}
```

`where_am_i` answers `known: false` with a reason while the pose is
untrusted (seated, searching, no map yet); teaching is refused there.
Otherwise it names the nearest place with `at_place` and `distance_m`:
the nearest, not necessarily the one the duck is in.

A name is matched without regard to case, never translated: `cucina`
and `kitchen` are two places. A model that hosts the tools may translate
on its own (qwen3:8b taught `kitchen` when told "questa è la cucina",
and asked for `kitchen` again on "vieni in cucina", on the twin,
2026-09-22) — consistent, but its choice, not the registry's.

## Watching a robot

```sh
cargo run -p quack-nav --example map_watch -- /run/robotd.sock 5
```

prints one line per frame and the grid as text (`#` wall, `.` free,
`D` the duck). Works against the MuJoCo twin as well as a robot.

## Not here

The transports (WebSocket bridge, MCP server, OpenAI projection) live in
[quacksat](https://github.com/andreagenovese/quacksat), which reaches
the tools over `quack-navd`'s socket. Object recognition is not what a
ToF map can do: names come from people.

Italian copy: `README-places.it.md`. License: Apache-2.0, as the workspace.
