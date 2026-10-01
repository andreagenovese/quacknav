# Study: a map app for the duck, over Pollen's channel

Written 2026-10-01. A study, not a decision: nothing here is built. It
asks what an app like a robot vacuum's would take — the map the duck built
with its live pose, "go here" by a tap, rooms and places by name, explore
control, and a conversation with the duck's agent (quacksat) by text and
later by voice — given two decisions the user has already taken:

1. **It works from outside the home** (remote), not only on the LAN.
2. **It rides Pollen's channel**, not a server of ours exposed on the duck.

Sources: `pollen-robotics/microduck` at `origin/main` 9060e81
(2026-10-01) and tag `daemon-v0.15.0` (API 37); its issues and PRs read
with `gh`; `quacksat` at ca9b782; this repo at `main`. Independent project,
no affiliation with Pollen.

## In one paragraph

Pollen has two remote paths — a WebRTC session (`mediad`, video plus a
`control` datachannel) and the **rendezvous control lane** (JSON-RPC inside
`peer` envelopes, carried over HTTP `POST` and SSE by the Hugging Face Space
`reachy_mini_central`). Both reach the robot through **one routing table,
`mediad/src/route.rs`, which is an exhaustive `match` over
`duck_ipc_proto::Call`**: a method that is not a `Call` variant is answered
`METHOD_NOT_FOUND` before it is routed anywhere (`mediad/src/session.rs`,
`request.as_call()`). So **today a third-party daemon such as quack-navd
cannot expose a route through mediad, by design**. What *does* work today,
with no change at Pollen, is for a small adapter of ours on the duck to
register on the same rendezvous **as a producer of its own**, with its own
Hugging Face token, and answer the same `peer {rpc}` envelopes that
`mediad::relay` answers. Recommended: that adapter now (option A), shaped
so that it shrinks to a manifest when Pollen accepts an extension-route
mechanism (option C), which we propose upstream. The app is web code
(TypeScript, a canvas map) packaged with Tauri 2, which is what Pollen's own
app is built with.

## 1. Pollen's remote channels, one by one

### 1.1 The WebRTC session (`mediad`) — shipped

- **What it carries.** One `webrtcsink` session: an H.264 video track and a
  reliable, ordered datachannel `control` carrying NDJSON JSON-RPC 2.0, the
  same wire as every unix socket (`docs/design/remote-webrtc.md` §2, §5).
  The `teleop` unreliable channel and the audio track are designed, **not
  built** (§2 "the first version opens `control` only", §12). One media
  session at a time (§12).
- **Routing.** `mediad::session::handle` parses the line, answers
  `media.video` and `media.stream` itself, then `request.as_call()`; an
  unknown method returns the parse error and is "refused by name rather than
  forwarded" (test `an_unknown_method_names_itself`). Permitted calls go to
  one of five sockets: updater, robot, config, pad, tof
  (`mediad/src/upstream.rs`, `Sockets`). There is no sixth, and no
  configuration for one.
- **Auth.** None on the robot, deliberately: "anyone on the same network
  has the robot and its camera … Not fine in a home" (§4). Remote sessions
  are authenticated by the rendezvous instead (§1.3).
- **LAN console.** `mediad` serves `http://<robot>:8080/` with an embedded
  single-file page (`docs/design/webrtc-console.md`, landed 2026-08-25);
  signalling on `ws://<robot>:8443`. Not extensible from outside: the page
  is `include_str!`'d into the binary.
- **NAT.** STUN `stun.l.google.com:19302` on both ends; the robot offers a
  Cloudflare TURN relay minted per account through
  `fastrtc-turn-service.hf.space`, 10 GB/month on a free account, "roughly a
  gigabyte an hour" relayed video (`remote-access-design.md` §6).
- **Cost.** `rtpgccbwe` is ~40% of a core; a session is ~25% of a core in
  total (`remote-webrtc.md` §0).

### 1.2 The rendezvous control lane — shipped (in 0.15.0)

- **What it carries.** JSON-RPC lines inside `peer` envelopes: the
  rendezvous "relays every key of a `peer` envelope except `type` and
  `sessionId` verbatim", so `{type: peer, sessionId, rpc: {…}}` is a call,
  "with no change to a service the mini fleet also depends on"
  (`mediad/src/relay.rs` header; `remote-access-design.md` §3.8, added by
  PR #323 "a server-side agent drives a duck over the rendezvous lane",
  merged 2026-09-23). No ICE, no DTLS, no TURN: it works from a data centre
  and from a phone on 4G alike. `relay.rs` handles `rpc` *before* any SDP,
  so a session needs no negotiation to carry calls.
- **Same table.** `open_control` runs `session::run` unchanged: what a
  bridged peer may call is exactly what a LAN peer may (§3.6), so the
  `METHOD_NOT_FOUND` rule above applies here too.
- **Limits.** The rendezvous allows **1200 requests per 60 s per peer**;
  exceeding it earns a `429` on the whole peer, the robot's lease included.
  `relay::Budget` keeps notifications to **400 per minute** and never
  throttles replies. "No pixels" (§3.8). Latency is unmeasured anywhere we
  could find: two HTTP hops through a Space proxy, plus SSE.
- **Auth.** Hugging Face OAuth on both sides: the robot holds a device-flow
  token (`/etc/robot/hf-token`, 30 days, rotating refresh, every HF scope —
  §2.4 says that must narrow before shipping); a client holds its own HF
  token; the service lists to a client only the producers of the same
  account (`/api/robot-status`). "A session arriving through it has been
  authorised twice over" (`remote-webrtc.md` §4). **Peers are keyed by
  token**: a second connection on the same token supersedes the first (§3.7),
  so nothing but `mediad` may use the robot's token.
- **Identity.** A producer's `meta.hardware_id` is the eviction key (same
  user + same id evicts the older), `meta.kind = microduck` lets clients
  tell families apart, `meta.simulated` marks a MuJoCo duck (§3.7).
- **Who may use it.** Issue #329 (2026-09-25, closed 2026-09-30) asked
  whether a third-party *client* (Microduck Studio, an iOS app) is welcome
  on the lane. Pierre Rouanet: "Yes it's ok to use the rendezvous as long as
  it stays reasonable … I can not guarantee that the API will not change",
  asked for a `User-Agent`, and for the app to say it is not the official
  one. **Nobody has asked about a third-party *producer*.**
- **Health.** `robotctl health` prints a `central` line: whether the
  service lists the robot, under which account, last heartbeat
  (`robotctl/src/main.rs`, `central_line`).

### 1.3 `media.stream` — shipped

`media.stream {url}` makes the robot dial **out** to a `wss://` and push
H.264 (or JPEG) frames there (`remote-access-design.md` §5.3). An
instruction over the lane, payload outbound. For programs, not for a person
watching.

### 1.4 BLE (`btd`, `duckctl`) and `configd` — shipped, LAN-range only

`btd` is the phone's permanent channel for setup (wifi, update, account,
name) with its own exhaustive table `btd/src/route.rs`; the phone app's
BLE layer is Rust (`duck-ble`). Ten metres of radio: irrelevant for remote,
and its table has the same "only `proto::Call`" property. `configd` owns
wifi, identity, power and pad pairing; it is a socket behind `mediad` and
`btd`, not a channel.

### 1.5 Pollen's apps

- **The official phone app** is built: `pollen-robotics/microduck-app`
  (private — `gh` cannot resolve it), **Tauri 2, React, the protocol in
  Rust** through `duck-ipc-proto` and `duck-ble`, about three hundred lines
  of CSS and no UI kit (`docs/design/mobile-app.md`, 2026-09-17). BLE for
  settings, WebRTC for `drive`. It refuses robots below API 31. Pierre on
  #329: "we are also developing an app".
- **`microduck-console`**, a Docker Space with `hf_oauth: true`, serving
  the same `index.html` the robot serves, remote through the rendezvous
  (`remote-access-design.md` §5).
- **`spaces/policy-playground`** and `spaces/shared/{rendezvous,wire,control}.py`
  — the Python client halves; the roadmap says "the SDK is those, packaged"
  (`docs/project/roadmap.md`, M5). `spaces/policy-playground/web/src/rendezvous.ts`
  is a browser client of the same service in TypeScript, the nearest
  reference for our app's transport.
- **Extensibility:** none. The design docs say a new method is "a one-line
  change to `route.rs`" — a change in Pollen's repo to a closed enum. The
  only mention of third-party code on the board is `architecture.md`
  ("if third-party or user code ever runs on the board"), about socket
  permissions.

## 2. What quack-navd already offers the app

quack-navd answers on two unix sockets, both NDJSON JSON-RPC, mode 0660,
group `robot` (`quack-nav/src/sockets.rs`):

- **The nav socket** (`/run/quack-nav/nav.sock`): `nav.catalog` and
  `nav.call {name, args}` (`quack-nav/src/bin/quack-navd.rs`). One reply
  per request, **no push**, every call under one `Mutex<Robot>`.
- **The map socket** (`[maploc] socket`), robotd's `robot.map*` dialect
  (`quack-nav/src/mapd/server.rs`): `robot.map` subscribes to a
  `map.frame` every second and a `map.pose` every 50 ms; the library
  (`map_save`, `map_list`, `map_load`, `map_match`, `map_adopt`) and
  `map_wipe`. **This is already a push stream.**

| the app needs | today | gap |
|---|---|---|
| the map | `map.frame`: origin, `cell_m` 0.05, `rows × cols`, base64 one byte per cell (0 unknown, 1 free, 2 wall), `seq`, `frozen` (`quack-nav/src/map.rs`, `MapFrame`) | size for a remote link (below) |
| pose and uncertainty | `map.pose` 20 Hz with `tracking`, `seated`, `pose_sigma`; `robot.map_status.pose_uncertainty` | none |
| tap-to-go | `robot.go_to {x, y}` in map metres, or `{place}`; background job; `stop` | none for a point |
| the route being walked | `robot.map_status.explore.route`, `goal`, `target`, `target_distance_m`, `state`, `reason` | pushed only by polling |
| places | `remember_place` **at the duck's pose only**, `forget_place`, `list_places` (name, anchors count, radius, distance, stale) | no coordinates in the list; no naming of a tapped point; no rename |
| explore | `robot.map_explore` start / `stop` / `complete` / `fresh` + `confirmed`; `progress`, `house.percent_mapped`, `house.done` | none |
| "where are we?" while exploring | `nav.take_question` (polled by quacksat) | a push would be nicer |
| maps | `map_save`, `map_list` (name, bytes, saved_at), `map_load`, `map_match`, `map_adopt`, `map_wipe` | rename, delete, export |
| drops (stairs) | body-frame `cliff.drops` in `map_status`; the books are on disk per map | not on the socket in map coordinates |
| homecoming | visible as explore state and hints | no explicit status |
| rooms as areas, no-go zones | — | missing entirely |

**Tap-to-go to a point already works**: `go_to` takes `x`, `y`. What is
missing for a first app is small and mostly read-side: places with
coordinates, naming a tapped point, renaming, the drops in map coordinates,
and not blocking reads behind the one mutex (a `robot.map_step` holds it
for its whole walk and stand).

**Size and rate on a remote link.** casa_grande is 9 × 7 m
(`todo-map.md`, 2026-09-30); with a metre of margin that is about 220 × 180
= 40 000 cells: **40 KB raw, ~53 KB as base64 JSON per frame, every second
— ~0.4 Mbit/s**. A 15 × 12 m house is ~100 KB a frame. Three cell values
pack into 2 bits (×4) and an occupancy grid deflates well, so a few KB per
frame is the likely order — *to measure, not measured*. More important is
the rate: the map changes only while mapping, and not at all once `frozen`.
The lane wants **the map on change (by `seq`), compressed; the pose at
1–2 Hz; status on change** — about 200 posts a minute at most, inside the
1200 limit and near `mediad`'s own 400-notification budget.

## 3. quacksat, and what a chat in the app would need

- **Structure** (`README.md`, workspace `Cargo.toml`): `quacksat-core`
  (mic capture, wake word, VAD, playback, robotd client, tool allowlist,
  `nav_client.rs`) and three interchangeable backends: `wyoming` (Home
  Assistant Assist does STT, intent and TTS), `agent` (mic audio over a
  WebSocket to a bridge — `bridge/bridge.py` — that runs STT → LLM → TTS,
  protocol in `docs/agent-protocol.md`), `direct` (the duck itself calls
  three OpenAI-dialect endpoints; the example config points the LLM at a
  local Ollama, `qwen3:8b`, `quacksat.example.toml`).
- **How it reaches quack-navd:** `NavLane` probes `nav.catalog` on the nav
  socket at startup, splices the catalog into the agent's tools, executes
  with `nav.call`, and polls `nav.take_question` (`quacksat-core/src/nav_client.rs`).
  **No Pollen remote channel anywhere** in quacksat: it talks robotd's and
  quack-navd's unix sockets, and goes outbound to its LLM/STT/TTS endpoints
  or bridge.
- **Its own entry points:** none for a conversation. quacksat binds no
  socket of its own (only tests do). The `direct` backend's turn is
  `run_turn(utterance: &[i16], history, …)` — audio in, the history a
  `Vec` inside the audio loop (`backends/direct/src/lib.rs`). The optional
  MCP server (`backends/direct/src/mcp.rs`, bearer token mandatory, TCP)
  exposes **the robot tools, not the conversation** — and it is exactly the
  kind of server on the duck the user ruled out for remote. The `agent`
  conversation lives in the bridge; the `wyoming` one in Home Assistant,
  whose own app already chats by text.
- **Audio:** one mic, no echo cancellation, exclusive ALSA access by
  quacksat (`docs/adr/0003-audio-access.md`). `mediad` has no audio code at
  all (`git grep alsasrc|opus` finds nothing) — the WebRTC audio track is a
  design line (`remote-webrtc.md` §2) — and if it existed it would contend
  with quacksat for the same capture device and land in `mediad`, not in
  the agent.

**So a chat needs a text entry in quacksat**: a unix socket
(`/run/quacksat/chat.sock`) with `chat.say {text, speak?}` answered by
streamed `chat.delta`, `chat.tool {name, args, result}` and `chat.done`
notifications, and `chat.subscribe` to mirror every turn — voice ones too —
to the app. For `direct` that is `run_turn` split into "text in" and
"audio in" over one shared history; for `agent`, a `text.utterance` event
and reply-text events in the agent protocol (the bridge has the
transcript already). The tool calls arrive in the app as they happen, so
"I'm going to the kitchen" can draw the route on the map from
`robot.go_to`'s args and `map_status.explore.route`.

**Who answers when both speak.** One conversation, one turn at a time: a
turn lock shared by the wake word and the chat socket. A text turn that
arrives while the duck is listening or speaking waits (bounded) and is
told so; a wake during a text turn waits likewise. **A reply goes back
where its turn came from** — the duck's speaker for voice, the app for
text — and is spoken on the duck as well only when the app asks
(`speak: true`). Everything is mirrored to the app's transcript.

**Voice from the app.** Not through WebRTC: no audio track exists, and the
mic is quacksat's. Two workable ways, both phase 2: (a) **speech-to-text on
the phone or desktop** (the platform's recogniser) and send text — no audio
crosses the lane, which suits its rate limit; (b) an Opus-compressed
utterance (a few KB a second, ~15 KB for five seconds) sent as one call for
quacksat's own STT, which keeps one recogniser and its language settings
but needs an Opus decoder on the duck. Replies spoken by the device's TTS,
or returned as audio only if wanted. The MVP is **text only**.

## 4. Architecture options

```
 app ──HTTP POST/SSE──► rendezvous (HF Space, Pollen's) ──SSE/POST──► duck
                                                                     │
   A:  quack-linkd (ours, own producer, own token) ── nav.sock, map sock, chat.sock
   B/C: mediad::relay (Pollen's) ── route.rs ── /run/robot/ext.d/… ── nav.sock, chat.sock
```

### A. An adapter daemon of ours, registered as its own producer — works today

`quack-linkd`: an unprivileged process on the duck that connects
**outward** to the rendezvous with **its own** HF token (a device-flow login
of its own — never `/etc/robot/hf-token`, which would supersede `mediad`'s
peer), registers as a producer with `meta.kind = "quack-nav"`, a
`hardware_id` derived from the robot's serial plus a suffix (so it neither
evicts nor is evicted by `mediad`), `meta.robot` naming the duck it belongs
to, `simulated` on the twin, and a `User-Agent` per #329. It answers `peer
{rpc}` envelopes with **its own exhaustive allowlist** over nav, map and
chat methods, and pushes pose, status and chat events within its own budget.
`mediad::relay` and `spaces/shared/wire.py` (Apache-2.0) are the reference
for every quirk: stream before send, heartbeat by `POST`, the split-brain
poll, the 60 s read timeout.

- **For:** no change at Pollen; its own 1200-per-minute budget separate
  from the robot's; quack-navd stays off the network; buildable and testable
  now, against the twin.
- **Against:** a second producer per duck in the owner's listing; Pollen's
  console filters on `kind`, but the mini's clients do not, and
  `ReachyCentralConsumer` falls back to the only visible producer
  (`remote-access-design.md` §5.1) — so a mini app on an account whose only
  producer is our adapter would try to drive it; a second token on the
  board (same broad-scope problem as §2.4); depends on Pollen being content
  with a third-party producer, which nobody has asked; no video in the
  same session (the app opens a second, WebRTC session to `mediad` if a
  camera is ever wanted).

### B. Pollen's lane relays to quack-navd for us — needs Pollen, per method

A `nav.*` arm in `route.rs` and a sixth socket in `upstream.rs`. Clean for
the user (one producer, one session, video and map together) and
impossible without making quack-nav's methods variants of
`duck_ipc_proto::Call` — Pollen's API owning ours. Not a realistic ask.

### C. An extension-route mechanism upstream — the clean end state

What B wants, generalised so that Pollen owns the mechanism and not our
methods. Sketch of the ask:

**What we see.** `route.rs` and `btd/src/route.rs` are exhaustive over
`proto::Call`, which is the right guarantee for Pollen's methods and leaves
no way for an on-robot service that is not Pollen's to be reached over
WebRTC or the lane — so a third party must run a producer of its own
(option A), with a second token and a second listing entry.

**Proposed change.** A drop-in directory, root-owned
(`/etc/robot/ext.d/<name>.toml`): a method **prefix** (`nav.`, `chat.`),
a socket path, the transports it may reach (`webrtc`, `lane`; BLE never),
and a declared `mutating` set. `mediad` answers any method under a
registered prefix by forwarding the line verbatim to that socket on a lane
of its own — the same "never parse a reply" rule — and refuses unknown
prefixes as today. The exhaustive match stays exactly as it is for
`proto::Call`; the extension table is a second, data-driven table whose
contents an admin installed with root, which is the same trust as
installing the daemon. `robotctl health` lists the extensions;
`only_these_mutating_calls_are_reachable_over_webrtc` gains a
counterpart that names extension prefixes. Optionally `hello` reports
them, so the official app can show a third-party panel.

**How to check it.** A fake extension socket in `mediad`'s test harness
(`fake_daemon`): a call under its prefix reaches it, one under an
unregistered prefix is refused by name, and a registered prefix cannot
shadow a `proto::Call` method.

- **For:** one producer, one token, video and map in one session, the
  official app could host it; Pollen keeps the policy.
- **Against:** Pollen's time and a principle they have argued for
  (`remote-webrtc.md` §5, "the exhaustive match"); not soon.

### Not considered further

A server of ours reached by the duck dialling out, or a port on the duck —
both outside the user's decision. A tunnel inside an existing `proto::Call`
(misusing a string field) — dishonest and fragile.

### Recommendation

**A now, designed to become C.** The adapter speaks plain JSON-RPC with
`nav.*` and `chat.*` methods, the same lines it would hand a socket under
C; when C lands the adapter's routing table becomes a manifest and the
app changes producer, nothing else. Before building A, **ask Pollen** the
#329 question for a producer (below). If they say no, A still runs in
development and on our own duck, and C is the only road to users.

## 5. Security

- **Who may reach the duck from outside:** clients signed in to the
  owner's Hugging Face account, and nobody else — the rendezvous matches
  account to account. Within the account there is no per-person identity
  on the envelope, so **the account is the control scope**. Sharing
  read-only with family on another account is not possible on this
  service.
- **Scopes inside the adapter** (its allowlist, exhaustive like Pollen's):
  *read* (map, pose, status, places, maps list, transcript); *control*
  (`go_to`, `stop`, explore start/stop/complete, places edit, map load,
  `chat.say`); *destructive* (`map_wipe`, explore `fresh`, map delete) only
  with an explicit `confirmed`, as `map_explore` already asks.
  `robot.map_step` and `robot.move` stay off the lane: they are joystick-rate
  tools. A config switch makes the remote side read-only.
- **The agent is a control path.** A `chat.say` can make the agent call
  `robot.go_to`; it runs under quacksat's allowlist, as a voice turn does,
  so chat sits in the *control* scope, not *read*.
- **Nobody watching.** There is no video in A's MVP: a duck sent across
  the house from the office is unwatched. The guards are the same as at
  home — the cliff guard, the drop books, `max_s` — plus a refusal when
  `tracking` is false, the pose uncertain or the duck seated, and a push to
  the app on a fall or a refusal.
- **Privacy.** The map is the floor plan of a home; the transcript is the
  household's speech. Both stay on the duck; the app holds them in memory
  only; the rendezvous sees the envelopes in clear (it is TLS to the Space,
  not end to end). Worth saying to the user plainly.
- **Two drivers.** `remote-webrtc.md` §9: nothing arbitrates a pad and a
  remote peer. quack-navd already refuses a second job while one runs
  (`not_exploring`); the app shows who started the current job.
- **Tokens.** The adapter's token is a bearer credential in a file, with
  every HF scope if obtained like `mediad`'s. Prefer an OAuth app with
  `openid profile` only, if HF's device flow allows one we register.

## 6. App technology

- **Constraint:** the lane is plain HTTP — `fetch` for `POST /send`, and
  SSE read by `fetch` (EventSource cannot set the `Authorization` header,
  and the server is removing `?token=`, `remote-access-design.md` §5).
  No WebRTC needed for the MVP. A browser needs the rendezvous to allow its
  origin (CORS) — it must, for `microduck-console`'s origin; whether for
  any origin is **to check**. A Tauri app sends the requests from Rust and
  does not meet CORS.
- **Recommendation: web code (TypeScript, the map on a `<canvas>`) inside
  Tauri 2**, desktop first, then iOS and Android from the same code. It is
  Pollen's choice for their app, it lets the client use the Rust wire types
  (ours, and `duck-ipc-proto` where it touches Pollen's), and the same web
  code can be served as a Hugging Face Space with `hf_oauth` for a
  no-install remote client, exactly as `microduck-console` is. Flutter or
  React Native would cost both of those. A PWA alone loses the background
  and notification behaviour a phone app wants.
- **Sign-in:** HF OAuth with PKCE, `openid profile`, per #329 and the
  console (`ASWebAuthenticationSession`-style on mobile, a loopback
  redirect on desktop, the Space's injected client id on the web).
- **Against the twin:** the MuJoCo duck registers like a robot
  (`configd --simulated`, `meta.simulated`); `quack-linkd` runs on the Mac
  beside quack-navd, with its own token, and the app finds it in the same
  listing. For offline work a loopback fake of the rendezvous's three
  endpoints (`/events`, `/send`, `/api/robot-status`), as `mediad::relay`'s
  tests run against a fake service. The viewer (`VIEWER=on`) stays
  the truth to compare the app's drawing against.

## 7. Phased plan

| phase | the app | quack-navd / quacksat | quack-linkd |
|---|---|---|---|
| **0. Ask** | — | — | Pollen: a third-party producer on the rendezvous, and the extension-route proposal (§4 C) |
| **1. MVP** | sign in, pick the duck, live map + pose + uncertainty, route of the current job, tap → `go_to {x,y}`, stop, places as pins (tap a pin → go), name a tapped point, explore start/stop/complete with progress, the "where are we?" question answered by typing | `list_places` with anchor coordinates; `remember_place {x, y}`; `rename_place`; reads (`map_status`, `list_places`) not blocked by a running `map_step`; drops in map coordinates | register, heartbeat, allowlist, map on change (2-bit + deflate), pose 1–2 Hz, status on change, `User-Agent` |
| **2. Chat** | a conversation pane: text in, streamed reply, tool calls drawn on the map; voice by on-device STT | quacksat: the chat socket, the turn lock, `chat.subscribe`; agent protocol: `text.utterance` | `chat.*` routes in the control scope |
| **3. Rooms** | draw a room as a polygon, or accept a suggested one; "go to the kitchen" goes inside it | places become areas: polygon + a goal point; `where_am_i` by containment; room suggestion from doorways (later) | — |
| **4. No-go zones** | draw virtual walls and forbidden areas | a cost layer in the planner and the explorer, persisted per map with the places and the drop books; drops shown as automatic no-go | — |
| **5. Maps** | list, rename, delete, switch floors, export | `map_rename`, `map_delete`, export; the map display aligned to the house | — |
| **6. Mobile** | iOS and Android builds of the same app; notifications (arrived, fell, needs a name) | — | push of events worth a notification |
| **later** | the camera, via a WebRTC session to `mediad`; option C if Pollen takes it | — | shrinks to a manifest under C |

Chat lands in phase 2, not in the MVP: it needs work in a second repo
(quacksat), a change to the agent protocol, and a decision on the turn
lock; the map is useful without it, and the voice at home already exists.

## 8. Open questions

**For the user.**

1. Is a second producer per duck in the owner's Hugging Face listing
   acceptable (option A), knowing Pollen's console filters it out and the
   mini's clients may not?
2. Remote control by default, or read-only remote with control only on
   demand?
3. Chat in phase 2 as proposed, or in the MVP?
4. Voice from the app: the platform's recogniser (simple, language per
   device) or quacksat's own STT via an Opus upload (one recogniser)?
5. Which backend is the reference for the chat: `direct` (Ollama) only, or
   `agent` too? (`wyoming` users chat in Home Assistant.)

**For Pollen** (an issue in the style of #329).

1. Is a third-party **producer** welcome on `reachy_mini_central`, with
   `kind: quack-nav`, its own token, a `hardware_id` that cannot collide,
   and a `User-Agent`?
2. Would you take an extension-route mechanism in `mediad` (§4 C), if we
   wrote it?
3. Does the rendezvous allow CORS from origins other than your Spaces?
4. Is there a request-body size limit on `POST /send` (a map frame is tens
   of KB)?
5. Can a device-flow login use an OAuth app with narrower scopes than HF's
   first-party client?

**To measure.** Lane latency (round trip of a call from a phone on 4G);
map frame size after packing and deflate on casa_grande and on a larger
house; the adapter's posts per minute during a mapping session.
