# quack-places

Names for where the Microduck stands.

robotd's on-board `maploc` (Pollen Robotics, upstream PR 127) gives the
duck a 2D map of the house and a pose in it. It knows geometry, not
rooms. This crate adds the layer above: a *place* is a name somebody
attached to a pose ("this is the kitchen"), recognized later by
distance. The tools that expose it — `where_am_i`, `remember_place`,
`forget_place`, `list_places` — are agent-neutral, ready to be projected
onto OpenAI tools, MCP, or anything else by whoever hosts them.

Part of [quacksat](../README.md) (ADR 0005), but deliberately not about
voice: it depends on the duck's IPC types, its head geometry (`kinematics`, pure Rust) and serde, nothing else, so it
can be hosted by the voice satellite today and by a daemon of its own
tomorrow — or move to a repository of its own — without changing.

## What is inside

| module | what it does |
|---|---|
| `map` | `robot.map` client: subscribes, keeps the newest `map.frame` (pose, tracking flag, trinary grid), reconnects on loss, turns itself off on a robotd that predates the map API, and bumps an *epoch* when the map frame was evidently reset |
| `places` | the registry: JSON file, several anchors per name, case-insensitive matching, a persisted *generation* that goes stale on a map reset (the lane's epoch, or fewer submaps than ever seen — a wipe while the host was down) |
| `tools` | the tools as a catalog fragment (JSON Schema) plus an executor on a `Places` context (map lane + registry): the four place tools and `map_status`, the map in numbers, the clearance in four directions from the grid, plus a hint for a mapping tour |
| `cliff` | the cliff guard: tofd's raw frames reprojected through Pollen's head geometry (`kinematics`); a downward beam that returns nothing, or 1.5× too long, where the floor should be is a drop — stairs, a hole — that the 2D map cannot show. Kept for 3 s in the body frame so a head sweep accumulates a view |
| `frontier` | where the known floor meets the unknown: frontier groups, wall-inflated breadth-first paths to the nearest reachable one, a waypoint per leg — what "map everything" loops over |
| `config` | the `[map]` section a host embeds: `enabled`, `places_path`, `cliff_guard`, `tof_socket`, `explore_max_s`, `ask_phrase`, `explore_turn` |

Wire shape pinned to upstream API v17 (`MAP_API_VERSION`); the types are
a local mirror until the `duck-ipc-proto` release that carries them.

## Hosting it

```rust
let places = quack_places::Places::connect(&config.map, &config.robotd_socket);
// splice the fragment into your catalog …
let mut tools = my_tools();
tools.extend(quack_places::tools::catalog());
// … and route the names it claims back to it
if quack_places::tools::handles(name) {
    return quack_places::tools::execute(name, &args, &mut places);
}
```

`where_am_i` answers `known: false` with a reason while the pose is
untrusted (seated, searching, no map yet); teaching is refused there.

## Watching a robot

```sh
cargo run -p quack-places --example map_watch -- /run/robotd.sock 5
```

prints one line per frame and the grid as text (`#` wall, `.` free,
`D` the duck). Works against the MuJoCo twin as well as a robot.

## Not here

The transports (WebSocket bridge, MCP server, OpenAI projection) live in
quacksat, and so does `robot.map_step` (walk, then stand): it drives the
body, which is quacksat's robotd lane. Navigation (`go_to`) waits for a goal RPC upstream. Object
recognition is not what a ToF map can do: names come from people.

Italian copy: `README.it.md`. License: Apache-2.0, as the workspace.
