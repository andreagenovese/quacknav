# quacksat — HA pipeline vs direct agent

Question: should the voice satellite on the duck talk to Home Assistant
(Assist pipeline) or directly to an agent (Claude / [Arkimede](https://arkimede.ai/))?

## The three paths

| | **A — HA pipeline (Assist)** | **B — Direct agent, cascaded** | **C — Direct agent, speech-to-speech** |
|---|---|---|---|
| What quacksat does | Wyoming/ESPHome satellite: local wake word, mic stream → HA, plays TTS | audio client towards a self-hosted endpoint (WebSocket): mic stream → bridge, plays the returned audio | same as B, but the bridge forwards the raw audio to a realtime API |
| Who does STT | HA (Speech-to-Phrase / faster-whisper) | the bridge (streaming faster-whisper, or cloud STT) | the model itself (audio in → audio out) |
| Who reasons | local HA intents (0.17 s fast lane) → fallback to Arkimede via the OpenAI shim | Arkimede / Claude via API, with tool calling | the realtime model (OpenAI Realtime, Gemini Live…) |
| Who does TTS | HA (Piper) | the bridge (Piper, Kokoro, ElevenLabs, …) | the model itself |
| Home automation control | native (HA intents), zero work | via tools: agent → MCP → HA | via the realtime API's tool calling → the bridge → HA |
| Robot control | absent (or via HA automations → RPC) | natural: the agent has "robot.move/head/skill" tools towards robotd | possible, tool calling |
| Perceived latency | medium: clean turns, no overlap | medium/good with chunked streaming STT→LLM→TTS | the best: ~300–600 ms, native barge-in |
| Conversation | turn-based, one command/response | multi-turn with memory, streaming, but turn-taking to build yourself (VAD) | full-duplex, interruptible, natural |
| Wake word | provided (microWakeWord/openWakeWord) | integrated in the client (same models) | same as B |
| Echo / noise | the satellite's problem (single mic!) | same | same, but the model is more tolerant |
| Local / privacy | 100% local possible (fully local HA pipeline) | local possible (Whisper+Piper) or cloud at will | cloud only, raw audio leaves the house, per-minute cost |
| With Claude? | yes, as a conversation agent behind a platform like Arkimede | yes, it's the typical case: STT → Claude (tool use) → TTS | no: Claude does not offer a speech-to-speech API today |
| Work to do | minimal: only the on-board satellite | satellite + bridge: VAD, turn-taking, streaming, tools | satellite + thin bridge; the API does the heavy lifting |
| Dependencies | HA, existing add-ons | a self-hosted server (e.g. Arkimede) | cloud vendor |
| Main risk | limits of the Assist pipeline: rigid, no barge-in, Piper-only TTS | reinventing pieces HA provides for free (timers, intents, entities) | lock-in, costs, privacy, Italian language to verify |

## How the flow changes

```
A   🦆 ─wake─► Wyoming ─► HA Assist ─► STT ─► intents │ Arkimede(shim) ─► TTS ─► 🦆
B   🦆 ─wake─► WebSocket ─► bridge ─► STT ─► Arkimede/Claude (tool: HA MCP, robotd) ─► TTS ─► 🦆
C   🦆 ─wake─► WebSocket ─► bridge ─► API realtime (audio↔audio, tool calling) ─► 🦆
```

In A the agent is a *fallback* of the pipeline; in B and C the agent is the
*center* and home automation becomes one of its tools. The satellite on board
the duck is almost identical in all three cases: only what sits on the other
side of the socket changes.

## Recommendation for quacksat

Design quacksat with an **interchangeable backend**: the on-board daemon
handles mic, speaker, wake word, VAD and the local tools (quacks, head,
intents to robotd); the transport is an abstraction with two implementations:

1. `wyoming` → path A. Do it first: it validates the hardware against an
   already proven HA chain (e.g. Voice PE → HA → shim → agent → MCP → HA),
   so the risk is only on the robot side. It immediately provides the wake
   word, a Speech-to-Phrase fast lane and native home automation control.
2. `agent-ws` → path B, with an agent platform (e.g. **Arkimede**) that
   already controls HA via MCP: add an audio streaming endpoint and the
   "robot" tools to it. This is the step where the duck stops being a
   walking microphone and becomes an agent with a body.

C remains an option for whoever wants the most natural conversation possible
while accepting the cloud — useful as an experiment, poorly aligned with a
"local processing" constraint. With Claude the way is B anyway.

The real bottleneck common to all three is the duck's **single microphone
without echo cancellation**: it's the first test to run on the hardware,
before any backend choice.
