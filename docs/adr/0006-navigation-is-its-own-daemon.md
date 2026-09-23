# ADR 0006: The navigation is its own daemon

- Status: accepted
- Date: 2026-09-22
- Inputs: ADR 0001 (separate repo), ADR 0005 (consume maploc),
  `docs/todo-map.md` (three weeks of measurements on the MuJoCo twin),
  the user's decision of 2026-09-22

## Context

ADR 0005 put the map client, the places registry and the cliff guard in
`quack-places`, "no voice dependencies, a future repo of its own". Since
then that future arrived from the other side: the explorer, the passage
law, the ground books, the homecoming and the paper twin grew inside
`quacksat-core` until nine thousand of its eleven thousand lines were
navigation and two thousand were the voice satellite the repo is named
after.

Two things made the split urgent rather than tidy. The navigation is
worth having without a microphone — a ROS bridge, an agent, a shell
script all want "where am I" and "go to the kitchen", and none of them
want a wake word. And the satellite is worth having without the
navigation: a duck that listens and answers is a product on its own,
and it should not carry a costmap planner to do it.

The user asked the question that settled the shape: if the two are
separate repos, how does an installed quacksat know that quack-nav is
installed? A Cargo feature answers at compile time, which is no answer
for somebody who installed a binary.

## Decision

### 1. Three crates, two repos

- `quack-duck` — robotd's lane (NDJSON JSON-RPC over the unix socket),
  the gait's limits, the body's own commands and the helpers every tool
  needs. Both sides send these.
- `quack-nav` — the map client, the cliff guard, the costmap planner,
  the places registry, the explorer, the homecoming, the navigation
  tools and the paper twin. It ships `quack-navd`.
- `quacksat-core` and its backends — audio, wake word, VAD, playback,
  the thinking cue, and the body's own tools.

`quack-duck` and `quack-nav` live in the navigation's repo: a
navigation library that ships its own robot lane is ordinary, and it
keeps the satellite free of a dependency it would otherwise inherit.

### 2. The navigation answers on a socket

`quack-navd` listens on `/run/quack-nav/nav.sock` and speaks robotd's own
wire: NDJSON, JSON-RPC 2.0, one connection per caller. Two methods:
`nav.catalog` returns the tool catalog (JSON Schema, the shape ADR 0004
defined), `nav.call` executes one of them. The daemon owns the map
lane, the guard, the registry, the explore job and the homecoming, and
reads its own `/etc/robot/quack-nav.toml` — `[map]`, `[gait]` and
`[homecoming]` moved there with the code.

(2026-09-23: first written as `/run/quack-nav.sock`, which the
unprivileged unit cannot create under `ProtectSystem=strict`; the socket
lives in its `RuntimeDirectory`, mode 0660, group `robot`, as robotd's
and tofd's do.)

### 3. The satellite probes, and works without it

At startup quacksat asks the socket for its catalog. If a daemon
answers, its tools are announced beside the satellite's own and every
call for one of those names is proxied over the lane. If none answers,
the satellite says so once and runs as a voice assistant whose duck
cannot be sent anywhere. `[nav] socket` is all the satellite's config
keeps of the navigation, and its systemd unit `Wants` the daemon rather
than requiring it.

### 4. One driver at a time, still

The explore job refuses a manual step while it drives, as before; the
refusal now travels back over the lane, unchanged, to whoever asked.

## Consequences

- The two can be installed, updated and versioned apart. What binds
  them is a wire, and a wire can be spoken by anything.
- The navigation is publishable on its own — which is the point: it is
  three weeks of measurements that nobody else on this robot has.
- One more process on a 1 GB board. Its unit is fenced like the
  satellite's (`Nice=5`, `CPUWeight=70`, 256/320 MB), and robotd's
  50 Hz loop still wins every contest for a core.
- One more hop for a navigation tool call: a unix socket round trip
  against an in-process call. A journey is a background job that
  answers at once, so the hop is paid on the answer, not on the walk.
- The satellite's tests no longer cover the navigation, and the
  navigation's no longer need a microphone. 105 tests, both sides
  green; the paper bench is unchanged by the split (guarded 22/30,
  blind 25/30).
