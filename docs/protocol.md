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
| `nav.catalog` | before v1 | — | the tools, each `{name, description, parameters}` |
| `nav.call` | before v1 | `{name, args}` | the tool's answer; a tool's own error is −32000 with its message |
| `nav.knobs` | before v1 | see [`knobs.md`](knobs.md) | |
| `nav.restart` | before v1 | — | `{restarting, reason?, hint?}` |

## Error codes

| code | meaning |
|---|---|
| −32700 | the line was not JSON |
| −32601 | no such method, or no such tool |
| −32602 | a param this method does not take, named |
| −32000 | the tool answered with an error: its message says why |

## History

- **v1** (2026-11-02) — `nav.hello`. The version on the wire, the methods and events a build
  serves. `nav.catalog`, `nav.call`, `nav.knobs` and `nav.restart` as they were.
