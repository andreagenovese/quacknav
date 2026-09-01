# quacksat — pipeline HA vs agente diretto

Domanda: il satellite vocale sull'anatra deve parlare con Home Assistant
(pipeline Assist) o direttamente con un agente (Claude / [Arkimede](https://arkimede.ai/))?

## Le tre strade

| | **A — Pipeline HA (Assist)** | **B — Agente diretto, a cascata** | **C — Agente diretto, speech-to-speech** |
|---|---|---|---|
| Cosa fa quacksat | satellite Wyoming/ESPHome: wake word locale, stream mic → HA, riproduce TTS | client audio verso un endpoint self-hosted (WebSocket): stream mic → bridge, riproduce audio di ritorno | idem B, ma il bridge inoltra l'audio grezzo a un'API realtime |
| Chi fa STT | HA (Speech-to-Phrase / faster-whisper) | il bridge (faster-whisper streaming, o STT cloud) | il modello stesso (audio in → audio out) |
| Chi ragiona | HA intents locali (corsia veloce 0,17 s) → fallback Arkimede via shim OpenAI | Arkimede / Claude via API, con tool calling | il modello realtime (OpenAI Realtime, Gemini Live…) |
| Chi fa TTS | HA (Piper) | il bridge (Piper, Kokoro, ElevenLabs, …) | il modello stesso |
| Controllo domotico | nativo (intents HA), zero lavoro | via tool: agente → MCP → HA | via tool calling dell'API realtime → il bridge → HA |
| Controllo del robot | assente (o via automazioni HA → RPC) | naturale: l'agente ha tool "robot.move/head/skill" verso robotd | possibile, tool calling |
| Latenza percepita | media: turni netti, niente sovrapposizione | media/buona con streaming STT→LLM→TTS a pezzi | la migliore: ~300–600 ms, barge-in nativo |
| Conversazione | a turni, un comando/risposta | multi-turno con memoria, streaming, ma turn-taking da costruire (VAD) | full-duplex, interrompibile, naturale |
| Wake word | fornita (microWakeWord/openWakeWord) | da integrare nel client (stessi modelli) | idem B |
| Echo / rumore | a carico del satellite (mic singolo!) | idem | idem, ma il modello è più tollerante |
| Locale / privacy | 100% locale possibile (pipeline HA interamente locale) | locale possibile (Whisper+Piper) o cloud a scelta | solo cloud, audio grezzo fuori casa, costo a minuto |
| Con Claude? | sì, come conversation agent dietro una piattaforma come Arkimede | sì, è il caso tipico: STT → Claude (tool use) → TTS | no: Claude oggi non offre un'API speech-to-speech |
| Lavoro da fare | minimo: solo il satellite a bordo | satellite + bridge: VAD, turn-taking, streaming, tool | satellite + bridge sottile; il grosso lo fa l'API |
| Dipendenze | HA, add-on esistenti | un server self-hosted (es. Arkimede) | vendor cloud |
| Rischio principale | limiti della pipeline Assist: rigida, senza barge-in, TTS solo Piper | reinventare pezzi che HA offre gratis (timer, intents, entità) | lock-in, costi, privacy, lingua italiana da verificare |

## Come cambia il flusso

```
A   🦆 ─wake─► Wyoming ─► HA Assist ─► STT ─► intents │ Arkimede(shim) ─► TTS ─► 🦆
B   🦆 ─wake─► WebSocket ─► bridge ─► STT ─► Arkimede/Claude (tool: HA MCP, robotd) ─► TTS ─► 🦆
C   🦆 ─wake─► WebSocket ─► bridge ─► API realtime (audio↔audio, tool calling) ─► 🦆
```

In A l'agente è un *fallback* della pipeline; in B e C l'agente è il *centro*
e la domotica diventa uno dei suoi tool. Il satellite a bordo dell'anatra è
quasi identico nei tre casi: cambia solo chi c'è dall'altra parte del socket.

## Raccomandazione per quacksat

Progettare quacksat con un **backend intercambiabile**: il daemon a bordo
gestisce mic, speaker, wake word, VAD e i tool locali (starnazzi, testa,
intenti a robotd); il trasporto è un'astrazione con due implementazioni:

1. `wyoming` → strada A. Si fa per prima: valida l'hardware contro una
   catena HA già collaudata (es. Voice PE → HA → shim → agente → MCP → HA),
   quindi il rischio è solo sul lato robot. Fornisce subito wake word, una
   corsia veloce Speech-to-Phrase e controllo domotico nativo.
2. `agent-ws` → strada B, con una piattaforma agente (es. **Arkimede**) che
   già controlla HA via MCP: aggiungerle un endpoint audio streaming e i
   tool "robot". È il passo in cui l'anatra smette di essere un microfono
   che cammina e diventa un agente con un corpo.

La C resta un'opzione per chi vuole la conversazione più naturale possibile
accettando il cloud — utile come esperimento, poco coerente con un vincolo
di "elaborazione locale". Con Claude la via è comunque la B.

Il vero collo di bottiglia comune a tutte e tre è il **microfono singolo senza
echo cancellation** dell'anatra: è il primo test da fare sull'hardware,
prima di qualunque scelta di backend.
