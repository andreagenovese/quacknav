# The control contract

What quack-navd offers a program that manages it — a map view, a control
plane, a script. Written 2026-10-01 for
quack-control (the local web page, a repository of its own, not published
yet: the decision is in
[study/map-app.md](study/map-app.md), "Decision 2026-10-01"), and for any
other client: everything here is the daemon's sockets, nothing is
quack-control's. Italian copy: [control-contract.it.md](control-contract.it.md).

## The two sockets

| socket | default path (config key) | what it serves |
|---|---|---|
| nav | `/run/quack-nav/nav.sock` (`socket`) | the tools (`nav.catalog`, `nav.call`), the knobs (`nav.knobs`), `nav.restart` |
| map | `/run/quack-nav/map.sock` (`[maploc] socket`) | the live map (`robot.map`: `map.frame`, `map.pose`) and the map library, in robotd's `robot.map*` dialect |

Both are unix sockets, mode 0660, group `robot` (`quack-nav/src/sockets.rs`):
a client runs as a user in `robot` — which is also the right to drive the
duck, so a client that serves the network carries that right with it. With
`[maploc] enabled = false` the map comes from robotd's own socket instead
(`NavdConfig::map_socket`).

The wire is JSON-RPC 2.0, one JSON object per line (NDJSON) each way, one
thread per caller. A request with an `id` gets exactly one line back.
Errors: `-32700` not JSON, `-32601` no such method or tool, `-32000` a tool
refused — its `message` is the reason, written to be shown to a person.

## nav.catalog and nav.call

```json
{"jsonrpc":"2.0","id":1,"method":"nav.catalog","params":{}}
{"jsonrpc":"2.0","id":2,"method":"nav.call","params":{"name":"robot.go_to","args":{"x":1.2,"y":-0.4}}}
```

`nav.catalog` answers a list of `{name, description, parameters}`, the
parameters as JSON Schema — the same list an agent gets, so a client can
build a form per tool from it. `nav.call` runs one, and its `result` is the
tool's JSON. The calls take one lock: they run one at a time, and a call
waits for the one before. Most answer in milliseconds; `robot.map_explore`
`{"complete": true}` waits up to two minutes for a running session to save.
`robot.map_explore` and `robot.go_to` start a job in the background and
answer at once.

What a map view uses:

| call | what it gives or does |
|---|---|
| `robot.map_status` | `pose` (x, y, yaw), `pose_uncertainty` (`xy_m`, `xy_minor_m`, `along_deg`, `yaw_deg`, one sigma; null while lost), `tracking`, `seated`, `mode`; `resting` (true during a long idle stand: no job for a minute and the body still — the pose is carried by odometry and a window is judged against the map every two minutes, the head sweeping once for it, nothing corrected; a job, a move or a push ends it at once) and `untrusted` (true when the duck may have been moved while it rested — a watch the map contradicted, a push past a nudge, a sit or a fall: `tracking` is false, nothing is searched until a job asks, and the next `go_to` or `map_explore` finds the pose first) and `rest_watch` (the last such judgement, null before any: `verdict` `agrees`, `unjudged`, `drifted` — the rest ended so the windows correct the pose — or `contradicts` — two in a row make the pose untrusted; `ago_s`, `residual_m`, `observed` of `beams`, `offset_m` and `offset_deg`, how far the map would have moved the pose); `house` (`map`, `percent_mapped`, `sessions`, `done`); `explore` — the job: `state` (`idle`, `relocalizing` — a job asked for on an untrusted pose, the duck walking and looking until it finds where it is, `goal` set, the reason "the duck may have been moved: finding where it is first" —, `running`, `done`, `stopped`, `failed`), `reason`, `route` and `route_raw` (`[[x, y], …]`), `aim`, `goal` (`[x, y]`), `local` (`[[x, y, r], …]`: r ≥ 0.10 m a drop on the books, smaller an obstacle), `progress`, `question_pending` |
| `robot.list_places` | each place's `name`, `radius_m`, `state` (`usable`, `pending` — the duck has not found itself on the place's map yet, `other_map` — another saved map is live, `stale` — its map is gone; see [README-places](../quack-nav/README-places.md#which-map-a-place-belongs-to)), `stale` (the old flag: `other_map` or `stale`), `map` (the saved map it belongs to, or null), `distance_m` (usable places only), and `at` — `{x, y}`, the anchor `go_to` walks to; `live_map`, the saved name of the live map (null when unknown or never saved) |
| `robot.go_to` | `{"x", "y"}` a point, `{"place"}` a name, `{"stop": true}` stops whatever job runs (a relocalization too). On an untrusted pose the answer carries `relocalizing: true` and the reason: the journey starts once the pose is confirmed, or the job fails (`explore.reason`) with nothing walked toward the goal |
| `robot.map_explore` | `{}` starts a session, `{"stop": true}` stops it, `{"complete": true}` closes the map as it is; `fresh` replaces the map and asks for `confirmed` |
| `robot.remember_place` | `{"name"}` where the duck stands; `{"name", "x", "y"}` a point of mapped floor |
| `robot.forget_place` | `{"name"}` |
| `robot.move` | `{"vx", "vy", "vyaw", "duration_s"}` (≤ 3 s): a timed move, no map guard, the depth sensor's cliff guard always on (forward moves). Answers `{"done": true, "walked_s", "cliff_guard"}`, or `{"done": false, "stopped": "a drop ahead (depth sensor): its edge … m away, …", "walked_s", "cliff_guard"}` when a hole within 0.40 m in its lane ended it (one explicit zero sent). `cliff_guard`: `on`, `not covered: backing up …`, `not covered: a sidestep …`, `not judged: a turn in place …`, `off: …` |
| `robot.map_step` | `{"vx", "vy", "vyaw", "walk_s", "stop_s"}`: a guarded step, then a stand. The reply: `walked_s`, `stood_s`, `new_windows`, `tracking`, `pose`, `clearance`, `cliff`, `steered`, `shortened`, `hint`, and `checks` — `"map and sensor"`, or `"position uncertain: checks from the sensor only"` when the pose was lost or `untrusted`: the map's walls, passages and steering were left out (judged at the believed pose they would be judged at the wrong place), `clearance` is null, and only the depth sensor's checks applied |

All coordinates are the live map's, in metres.

## The live map: robot.map on the map socket

```json
{"jsonrpc":"2.0","id":1,"method":"robot.map","params":{}}
```

The answer `{"accepted", "enabled", "mode"}`, then notifications on the
same line until the caller goes:

- `map.frame`, once a second: `seq`, the pose (`x`, `y`, `yaw`, `tracking`,
  `still`, `seated`, `frozen`, `pose_sigma`, `resting` and `untrusted` —
  absent when false — and `rest_watch`, as in `robot.map_status`), the grid (`x_min`, `y_min`,
  `cell_m`, `rows`, `cols`, and `cells`: base64, one byte a cell, 0 unknown,
  1 free, 2 wall, row-major, row 0 at `y_min`), and `n_submaps`, `n_loops`,
  `windows`. A jump of tens of submaps is another map (loaded, adopted,
  wiped): a trail drawn on the old one means nothing on it.
- `map.pose`, every 50 ms between frames: `seq` (the frame it belongs to),
  `x`, `y`, `yaw`, `tracking`, `seated`, `pose_sigma`.

`scripts/twin/viewer/maploc_overlay.py` is a reader of both.

## The knobs: nav.knobs and knobs.env

Most tuning is environment variables ([knobs.md](knobs.md)), read from the
process's environment, which nothing outside can change. So:

- the unit reads `/var/lib/quack-nav/knobs.env` at every start
  (`EnvironmentFile=-`, optional) — the config's `knobs_env` names the same
  file for the daemon;
- `nav.knobs` reads and writes it, as the daemon's own user (the state
  directory is `quacknav`'s, not the client's):

```json
{"jsonrpc":"2.0","id":1,"method":"nav.knobs","params":{}}
{"jsonrpc":"2.0","id":2,"method":"nav.knobs","params":{"set":{"QK_CLIFF_MARGIN_M":"0.3","QK_TRAIL":null}}}
{"jsonrpc":"2.0","id":3,"method":"nav.knobs","params":{"reset_all":true}}
```

The answer is `{"env_file", "restart_needed", "knobs": […]}`, each knob as
`quack-nav/src/knobs.json` has it — `name`, `group` (`QK`, `MAPLOC`),
`type` (`number`; `switch` `0`/`1`; `choice` with `options`; `flag`, `1` on
and unset off; `text`), `default` (null when the code has none), `read_as`, `where`, `doc` — plus `saved` (the file's value,
or null: the default) and `running` (this process's). `restart_needed` says
the two differ somewhere. `null` in `set` drops an override. A value that
does not fit its knob's type is refused and nothing is written; lines the
file has that are not knobs (a `RUST_LOG`, a comment) are kept. The list is
generated from the code by `scripts/knobs.py` — only the knobs quack-navd's
own sources read, not the benches', nor the twin's oracle (`QK_ORACLE_*`,
which swap the map or the pose for the simulator's truth) — and CI checks
it is current. A `default` named by a constant in the code is given as its
value; one that depends on the mode reads `0.12 (guarded: 0.20)`.

## nav.restart

```json
{"jsonrpc":"2.0","id":1,"method":"nav.restart","params":{}}
```

Under systemd: `{"restarting": true}`, then the daemon goes as on SIGTERM —
the running job stopped, the mapping session saved, the sockets removed —
and the unit's `Restart=always` starts it again about 5 s later with the env
file read anew. The homecoming runs as at boot: the duck stands, finds the
map and itself on it. Clients lose both sockets for those seconds and
reconnect. Anywhere else (the twin, a shell) it answers `{"restarting":
false, "reason": …}` and stays: on the twin `scripts/twin/twin.sh
restart-navd` does it, and reads `$STATE/knobs.env` the same way.

`nav.knobs` and `nav.restart` are methods, not tools: they are not in
`nav.catalog`, so an agent that splices the catalog into its tools
(quacksat) never sees them.

## Stability

Additive within a release line: fields and methods are added, not renamed
or removed, without a line in the [CHANGELOG](../CHANGELOG.md). A client
should ignore fields it does not know and treat a missing `at` or a missing
method (`-32601`) as an older quack-navd.
