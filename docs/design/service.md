# quack-navd as a service

Status: design, first cut · Date: 2026-11-02 · Owner: andreagenovese · Roadmap: N4, N5

This page owns the contract between `quack-navd` and whatever sits above it — the voice
satellite, the control page, an agent, a script. The tools themselves (what `robot.go_to` does,
what `robot.map_status` reports) stay in [`control-contract.md`](../control-contract.md); this is
the shape around them: how a client learns what it is talking to, hears what happened without
asking, keeps the duck to itself while it uses it, hands the duck a list of things to do, and
asks it to *do* something rather than walk.

It follows `robotd`'s own shape on purpose (Pollen's `docs/design/robotd-design.md`,
`duck-ipc-proto`): a typed, versioned protocol crate, one hello, a version skew that is logged
and served rather than refused, NDJSON JSON-RPC 2.0 on a local socket.

## 1. What is there today

One Unix socket, NDJSON, JSON-RPC 2.0, four methods: `nav.catalog` (the tools with their JSON
schemas), `nav.call` (`{name, args}`, one tool), `nav.knobs`, `nav.restart`. No version on the
wire; nothing pushed — the satellite and the control page poll `robot.map_status` to learn that a
journey arrived; anyone may drive at any time, the last call wins; one call is one job. Each client
carries its own copy of the wire (quacksat's `nav_client.rs`, quack-control's bridge).

Everything below is additive. `nav.catalog` and `nav.call` stay, with today's answers, for as long
as the daemon exists: an agent that only knows them keeps working.

## 2. The version

`API_VERSION`, an integer in the protocol crate, bumped by every change a client could notice: a
new method, a new member, a new event. Never for a fix that keeps the wire.

`nav.hello` → `{api_version, daemon_version, revision, methods, events}`. A client sends it first
and logs what it got. **A skew is logged and served, never refused** (robotd's rule): an older
client simply does not call what it does not know; a newer client calling a method this daemon
lacks gets `METHOD_NOT_FOUND` (−32601) and an unknown member gets `INVALID_PARAMS` (−32602) naming
it — only a genuinely missing route or parameter refuses. The protocol page carries a short
history: `vN — what came`.

## 3. Events, pushed

`nav.subscribe {topics?}` turns the connection into a stream: the answer is the topics granted, and
from then on the daemon writes notifications `{"method": "nav.event", "params": {topic, seq, t,
data}}` on that connection, interleaved with answers to anything else the client still asks on
it. `seq` is per connection and gapless, so a client can tell it missed something; `t` is the
daemon's monotonic seconds.

The first topics, each from a fact the daemon already has:

| topic | when | data |
|---|---|---|
| `job` | a job's state changes — started, running, done, stopped, failed, refused | `{state, kind, goal, reason, progress}` |
| `job.progress` | at most once a second while a job runs | `{progress, route_m, legs}` |
| `pose` | the map's pose becomes trusted, or stops being | `{tracking, untrusted, pose}` |
| `fall` | the body is down or seated during a job | `{pose}` |
| `question` | the explorer asks the person what a place is | `{phrase, pose}` |

Delivery is best effort and in order. A subscriber that does not read is not waited for: past a
bounded queue its connection is closed and the fact logged — the daemon never stalls a job on a
slow listener. Polling `robot.map_status` stays correct; events only make it unnecessary.

## 4. One owner at a time

Today the last caller wins. With a voice, a control page and an agent on the same duck that is a
journey cut in half by someone else's `go_to`. The answer is an explicit, short **lease**:

- `nav.claim {holder, ttl_s}` → `{token, holder, until}`. `holder` is a name for people ("quacksat",
  "control page", "the agent"); `ttl_s` at most 300. Renewed by claiming again with the token.
- While a lease is held, the tools that **move the duck** (`robot.go_to`, `robot.map_explore`,
  `robot.map_step`, `robot.move`, `nav.act`, `nav.jobs`) from anyone else are refused with
  `LEASED` (−32010) and the holder's name. Reads are never refused.
- **A stop is never refused.** `robot.go_to {"stop": true}` from anyone ends the lease's job and
  the lease with it; safety does not wait for ownership.
- `nav.release {token}` ends it; so does the ttl, and so does the holder's connection closing.
- No lease, no change: anyone may drive, as today. A client that never claims behaves exactly as
  it does now.

The lease is the navigation's, not the motors'. `robotd` stays the only owner of the joints and
its deadman stays the backstop; the lease only says whose *jobs* quack-navd runs.

## 5. A queue of jobs

`nav.jobs {steps: [{tool, args}…], on_failure: "stop"|"continue"}` → `{queue}`. The daemon runs the
steps in order, each to its end (a journey to its arrival, a skill to its finish), and reports
each on the `job` topic with its index. `on_failure: "stop"` (the default) ends the queue at the
first step that fails or is refused, and says which and why. `nav.jobs {}` answers the queue's
state; a stop ends it. One queue at a time; it holds the lease implicitly while it runs if the
caller holds one.

This is the "go to the bath, pick up the socks, take them to the laundry, come back to me" list:
the agent plans it, the duck runs it on the board, step by step, and the agent hears each step
end. The planning stays the agent's; the running stays the duck's.

## 6. Doing, not only walking — `nav.act` (N5)

`nav.act {skill, at?}`: one of the skills the daemon has — read from `robotd`'s `robot.skills` at
run time (table entries and built-ins), never a list coded here — carried out as one job:

1. `at` absent or `"here"`: as the duck stands. `at` a named place or `{x, y}`: walk there first
   (a journey, with everything a journey has).
2. Square up: the last stand turns the body toward the target heading the place gives, if any.
3. `robotd`'s `robot.do {skill}`; wait for the body to finish (the policy label back to `walk` or
   `stand`, or the skill's own end).
4. Answer `{done, skill, at, ok, reason}`: `ok` false with the reason when the daemon refused the
   skill, the body fell, or the skill's end was not seen within its time.

The skills are Pollen's policies, run by `robotd`; quack-navd never trains, ships or replaces one.
Whether a pick *held* the thing is the body's to tell (the mouth's gap, `ledger::BodyWatch`) and
is reported, not guessed. Aiming at a thing the camera saw is N7's, not this.

## 7. The protocol crate and the client crate

- `quack-nav-proto`: `API_VERSION`, method names, the params and results of `nav.hello`,
  `nav.subscribe`, `nav.event`, `nav.claim`, `nav.release`, `nav.jobs`, `nav.act`, the error codes.
  No I/O, no dependencies beyond `serde`. The daemon and every client build against it, so a
  member renamed is a compile error on both sides rather than a silence on the wire.
- `quack-nav-client`: a blocking client over the socket — `connect`, `hello`, `call(tool, args)`,
  `catalog`, `subscribe(topics) -> events`, `claim`/`release`, `jobs`, `act`. quacksat's
  `nav_client.rs` and quack-control's bridge move onto it; that is N4's test.

## 8. Order of work

1. The proto crate, `nav.hello`, `docs/protocol.md` with its version history. (v1)
2. Events: `nav.subscribe` and the five topics. (v2)
3. The client crate; quacksat and quack-control on it.
4. The lease. (v3)
5. The queue. (v4)
6. `nav.act` over `robot.do`, `robot.skills` read at run time. (v5)

Each step its own branch, its tests, the twin's A/B where it touches what the duck does (4–6),
docs EN+IT, merged on the user's go.

## 9. Not here

The transport beyond the local socket (an authenticated TCP lane for a client on the LAN) waits
for a client that needs it. The camera, the object ledger and the seek job are N7. Several ducks
are not a case.
