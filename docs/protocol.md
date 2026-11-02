# The protocol

quack-navd's wire: NDJSON, JSON-RPC 2.0, on its Unix socket (`socket` in the config,
`/run/quack-nav/nav.sock` on the duck). The types are the `quack-nav-proto` crate's; the design
is [`design/service.md`](design/service.md); the tools behind `nav.call` are
[`control-contract.md`](control-contract.md).

A client sends `nav.hello` first and logs what it gets. A version skew is logged and served,
never refused: an older client does not call what it does not know; a method this daemon lacks
answers `METHOD_NOT_FOUND` (−32601), an unknown member `INVALID_PARAMS` (−32602) naming it.

## Methods

| method | since | params | result |
|---|---|---|---|
| `nav.hello` | v1 | `{client?, api_version?}` — who asks and the version it was built against, for the daemon's log | `{api_version, daemon_version, revision, methods, events}` |
| `nav.subscribe` | v2 | `{topics?}` — all when absent | `{topics}` granted; an unknown topic refused by name. From then on the connection carries `nav.event` notifications; once per connection |
| `nav.claim` | v3 | `{holder, ttl_s?, token?}` — a name for people, 60 s by default and 300 at most, the token when renewing from another connection | `{token, holder, ttl_s}`; −32010 when another connection holds it |
| `nav.release` | v3 | `{token?}` — the holder's own connection may leave it out | `{released, holder?}` |
| `nav.catalog` | before v1 | — | the tools, each `{name, description, parameters}` |
| `nav.call` | before v1 | `{name, args}` | the tool's answer; a tool's own error is −32000 with its message |
| `nav.knobs` | before v1 | see [`knobs.md`](knobs.md) | |
| `nav.restart` | before v1 | — | `{restarting, reason?, hint?}` |

## Events (v2)

After `nav.subscribe` the daemon writes `{"jsonrpc": "2.0", "method": "nav.event", "params":
{topic, seq, t, data}}` on that connection, between the answers to anything else asked on it.
`seq` counts from 1 per connection without gaps; `t` is the daemon's monotonic seconds. Best
effort and in order: a subscriber more than 256 events behind is dropped and its connection
closed. Polling `robot.map_status` stays correct.

| topic | when | data |
|---|---|---|
| `job` | a job's state changes; for a journey, once its goal is known (a start with no goal is said as an exploration three seconds on) | `{state, kind, goal, reason, self_started}` — `kind` `journey`, `explore` or `own` (the duck's own motion) |
| `job.progress` | once a second while a job runs | `{legs, refusals, target_distance_m, route_m}` |
| `pose` | the map's pose becomes trusted or untrusted | `{tracking, untrusted, pose}` |
| `fall` | the body sits or falls while a job runs | `{pose}` |
| `question` | the explorer asks what a place is | `{pose}` |
| `lease` | the lease was taken or ended (v3) | `{state, holder, why}` |

## The lease (v3)

Without a lease anyone may drive, as before. A connection that claims holds the duck's motion:
`robot.go_to`, `robot.map_explore`, `robot.map_step` and `robot.move` from any other connection
answer −32010 naming the holder. Reads always pass. **A stop always passes** — `robot.go_to` or
`robot.map_explore` with `{"stop": true}` — and ends a lease held by someone else. A lease ends at
`nav.release`, at its ttl, and when the holder's connection closes; the `lease` topic says
`{state, holder, why}` (an end by the ttl is said at the next call that finds it).

## A client in Rust

`quack-nav-client` (in this workspace) is a blocking client on these types: `Client::connect`,
`hello` (a daemon from before `nav.hello` answers as a v0, its tools still work), `catalog`,
`call(tool, args)`, and `subscribe(socket, topics)` — the events on a connection of their own, as
an iterator. `cargo run -p quack-nav-client --example watch -- <socket> [topic…]` prints them.

## Error codes

| code | meaning |
|---|---|
| −32700 | the line was not JSON |
| −32601 | no such method, or no such tool |
| −32600 | a second `nav.subscribe` on the same connection |
| −32602 | a param this method does not take, named |
| −32010 | another connection holds the lease on the duck's motion; the message names it |
| −32000 | the tool answered with an error: its message says why |

## History

- **v3** (2026-11-02) — one owner at a time: `nav.claim`, `nav.release`, the code −32010, the
  `lease` topic.
- **v2** (2026-11-02) — events: `nav.subscribe`, `nav.event`, the topics `job`, `job.progress`,
  `pose`, `fall`, `question`.
- **v1** (2026-11-02) — `nav.hello`. The version on the wire, the methods and events a build
  serves. `nav.catalog`, `nav.call`, `nav.knobs` and `nav.restart` as they were.
