# Studio: un'app della mappa per l'anatra, sul canale di Pollen

Scritto il 2026-10-01. Uno studio, non una decisione: qui non è costruito
nulla. Chiede che cosa serva a un'app come quella di un robot aspirapolvere
— la mappa costruita dall'anatra con la sua posa dal vivo, "vai qui" con un
tocco, stanze e luoghi per nome, il controllo dell'esplorazione, e una
conversazione con l'agente dell'anatra (quacksat) per testo e più avanti a
voce — date due decisioni che l'utente ha già preso:

1. **Funziona anche fuori casa** (da remoto), non solo sulla LAN.
2. **Passa dal canale di Pollen**, non da un nostro server esposto
   sull'anatra.

Fonti: `pollen-robotics/microduck` a `origin/main` 9060e81 (2026-10-01) e
al tag `daemon-v0.15.0` (API 37); le sue issue e PR lette con `gh`;
`quacksat` a ca9b782; questo repo a `main`. Progetto indipendente, nessuna
affiliazione con Pollen.

## Decisione 2026-10-01

L'utente, letto questo studio: **per ora solo la gestione locale** — una
pagina semplice e funzionale in un browser sulla rete di casa. È un
**piano di controllo a sé, `quack-control`**, in un repository separato:
un piccolo demone sull'anatra che gestisce quack-nav ora, quacksat e i
demoni futuri poi (un adattatore per tipo), e fatto per incontrare più
avanti il canale di Pollen. Mappa e posa dal vivo, tocca-e-vai, stop,
luoghi, avvio, stop e completamento dell'esplorazione; una vista avanzata
con tutti gli strumenti del catalogo e le manopole. HTTP in chiaro sulla
LAN con un token facoltativo; niente esce di casa. quack-nav tiene solo la
sua parte, il **contratto di controllo** che qualsiasi client può usare
([../control-contract.it.md](../control-contract.it.md)): i socket,
`nav.catalog`/`nav.call`, il flusso della mappa, `nav.knobs` e
`nav.restart`, le coordinate dei luoghi e l'insegnamento in un punto.

Tutto il resto di questo studio è **rimandato, invariato come piano**, ed
elencato come voci datate in [`../todo-map.it.md`](../todo-map.it.md) §5:
l'accesso remoto sul canale di Pollen (§1, §4 — `quack-linkd` come producer
a sé sul rendezvous, la richiesta upstream di rotte di estensione, il token
HF e i suoi scope §5, le domande per Pollen §8), la chat con quacksat (§3 —
`chat.sock`, il lock di turno condiviso, la voce dall'app), l'app
Tauri/mobile (§6, fasi 2 e 6 del §7), le stanze come aree (§7 fase 3), le
zone vietate e i muri virtuali (§7 fase 4), e la gestione delle mappe —
rinomina, cancella, esporta (§7 fase 5). Il piano di controllo locale non
si allontana da quel piano: parla le stesse righe `nav.call` e tiene lo
stesso tipo di lista ammessa (§5) che terrebbe un adattatore, e il
`quack-linkd` dell'opzione A può diventarne una delle facce.

## In un paragrafo

Pollen ha due strade remote — una sessione WebRTC (`mediad`, video più un
datachannel `control`) e la **corsia di controllo del rendezvous**
(JSON-RPC dentro buste `peer`, portate via HTTP `POST` e SSE dallo Space
Hugging Face `reachy_mini_central`). Entrambe raggiungono il robot
attraverso **una sola tabella di instradamento, `mediad/src/route.rs`, che
è un `match` esaustivo su `duck_ipc_proto::Call`**: un metodo che non è una
variante di `Call` riceve `METHOD_NOT_FOUND` prima di essere instradato
ovunque (`mediad/src/session.rs`, `request.as_call()`). Quindi **oggi un
demone di terzi come quack-navd non può esporre una rotta attraverso
mediad, per progetto**. Ciò che *funziona* oggi, senza alcuna modifica da
parte di Pollen, è che un nostro piccolo adattatore sull'anatra si registri
sullo stesso rendezvous **come producer a sé**, con un proprio token
Hugging Face, e risponda alle stesse buste `peer {rpc}` a cui risponde
`mediad::relay`. Raccomandato: quell'adattatore ora (opzione A), fatto in
modo da ridursi a un manifesto quando Pollen accetterà un meccanismo di
rotte di estensione (opzione C), che proponiamo upstream. L'app è codice
web (TypeScript, la mappa su canvas) impacchettato con Tauri 2, che è ciò
con cui è costruita l'app di Pollen.

## 1. I canali remoti di Pollen, uno per uno

### 1.1 La sessione WebRTC (`mediad`) — rilasciata

- **Cosa porta.** Una sessione `webrtcsink`: una traccia video H.264 e un
  datachannel affidabile e ordinato `control` con JSON-RPC 2.0 NDJSON, lo
  stesso filo di ogni socket unix (`docs/design/remote-webrtc.md` §2, §5).
  Il canale inaffidabile `teleop` e la traccia audio sono progettati, **non
  costruiti** (§2 "la prima versione apre solo `control`", §12). Una
  sessione media alla volta (§12).
- **Instradamento.** `mediad::session::handle` legge la riga, risponde da
  sé a `media.video` e `media.stream`, poi `request.as_call()`; un metodo
  sconosciuto riceve l'errore di parsing ed è "rifiutato per nome anziché
  inoltrato" (test `an_unknown_method_names_itself`). Le chiamate permesse
  vanno a uno di cinque socket: updater, robot, config, pad, tof
  (`mediad/src/upstream.rs`, `Sockets`). Un sesto non c'è, né una
  configurazione per aggiungerlo.
- **Autenticazione.** Nessuna sul robot, di proposito: "chiunque sulla
  stessa rete ha il robot e la sua telecamera … Non va bene in una casa"
  (§4). Le sessioni remote le autentica invece il rendezvous (§1.3).
- **Console LAN.** `mediad` serve `http://<robot>:8080/` con una pagina in
  un solo file incorporata (`docs/design/webrtc-console.md`, arrivata il
  2026-08-25); segnalazione su `ws://<robot>:8443`. Non estendibile da
  fuori: la pagina è `include_str!` nel binario.
- **NAT.** STUN `stun.l.google.com:19302` su entrambi i lati; il robot
  offre un relay TURN Cloudflare coniato per account tramite
  `fastrtc-turn-service.hf.space`, 10 GB al mese su un account gratuito,
  "circa un gigabyte l'ora" di video relayato (`remote-access-design.md`
  §6).
- **Costo.** `rtpgccbwe` vale ~40% di un core; una sessione ~25% di un core
  in tutto (`remote-webrtc.md` §0).

### 1.2 La corsia di controllo del rendezvous — rilasciata (in 0.15.0)

- **Cosa porta.** Righe JSON-RPC dentro buste `peer`: il rendezvous
  "inoltra alla lettera ogni chiave di una busta `peer` tranne `type` e
  `sessionId`", quindi `{type: peer, sessionId, rpc: {…}}` è una chiamata,
  "senza modificare un servizio da cui dipende anche la flotta dei mini"
  (intestazione di `mediad/src/relay.rs`; `remote-access-design.md` §3.8,
  aggiunto dalla PR #323 "a server-side agent drives a duck over the
  rendezvous lane", fusa il 2026-09-23). Niente ICE, niente DTLS, niente
  TURN: funziona da un data centre come da un telefono in 4G. `relay.rs`
  tratta `rpc` *prima* di qualunque SDP, quindi una sessione non ha bisogno
  di negoziazione per portare chiamate.
- **Stessa tabella.** `open_control` esegue `session::run` invariato: ciò
  che un peer via ponte può chiamare è esattamente ciò che può un peer LAN
  (§3.6), quindi la regola del `METHOD_NOT_FOUND` vale anche qui.
- **Limiti.** Il rendezvous permette **1200 richieste ogni 60 s per peer**;
  superarle costa un `429` sull'intero peer, compreso il lease del robot.
  `relay::Budget` tiene le notifiche a **400 al minuto** e non limita mai le
  risposte. "Niente pixel" (§3.8). La latenza non è misurata da nessuna
  parte che abbiamo trovato: due salti HTTP attraverso il proxy di uno
  Space, più SSE.
- **Autenticazione.** OAuth Hugging Face su entrambi i lati: il robot tiene
  un token da device flow (`/etc/robot/hf-token`, 30 giorni, refresh che
  ruota, tutti gli scope HF — §2.4 dice che vanno ristretti prima di
  spedire); un client tiene un proprio token HF; il servizio mostra a un
  client solo i producer dello stesso account (`/api/robot-status`). "Una
  sessione che arriva da lì è stata autorizzata due volte"
  (`remote-webrtc.md` §4). **I peer sono indicizzati per token**: una
  seconda connessione con lo stesso token scalza la prima (§3.7), quindi
  nessuno tranne `mediad` può usare il token del robot.
- **Identità.** `meta.hardware_id` di un producer è la chiave di sfratto
  (stesso utente + stesso id sfratta il più vecchio), `meta.kind =
  microduck` permette ai client di distinguere le famiglie,
  `meta.simulated` segna un'anatra MuJoCo (§3.7).
- **Chi può usarla.** L'issue #329 (2026-09-25, chiusa il 2026-09-30) ha
  chiesto se un *client* di terzi (Microduck Studio, un'app iOS) è
  benvenuto sulla corsia. Pierre Rouanet: "Sì, va bene usare il rendezvous
  purché resti ragionevole … non posso garantire che l'API non cambi", ha
  chiesto un `User-Agent` e che l'app dica di non essere quella ufficiale.
  **Nessuno ha chiesto di un *producer* di terzi.**
- **Salute.** `robotctl health` stampa una riga `central`: se il servizio
  elenca il robot, con quale account, ultimo heartbeat
  (`robotctl/src/main.rs`, `central_line`).

### 1.3 `media.stream` — rilasciato

`media.stream {url}` fa sì che il robot chiami **verso l'esterno** un
`wss://` e ci spinga frame H.264 (o JPEG) (`remote-access-design.md` §5.3).
Un'istruzione sulla corsia, il carico in uscita. Per programmi, non per una
persona che guarda.

### 1.4 BLE (`btd`, `duckctl`) e `configd` — rilasciati, solo a portata LAN

`btd` è il canale permanente del telefono per la configurazione (wifi,
aggiornamenti, account, nome) con la sua tabella esaustiva
`btd/src/route.rs`; lo strato BLE dell'app è in Rust (`duck-ble`). Dieci
metri di radio: irrilevante per il remoto, e la sua tabella ha la stessa
proprietà "solo `proto::Call`". `configd` possiede wifi, identità,
alimentazione e accoppiamento del pad; è un socket dietro `mediad` e `btd`,
non un canale.

### 1.5 Le app di Pollen

- **L'app ufficiale per telefono** è costruita:
  `pollen-robotics/microduck-app` (privata — `gh` non la risolve),
  **Tauri 2, React, il protocollo in Rust** attraverso `duck-ipc-proto` e
  `duck-ble`, circa trecento righe di CSS e nessun kit di UI
  (`docs/design/mobile-app.md`, 2026-09-17). BLE per le impostazioni,
  WebRTC per `drive`. Rifiuta i robot sotto l'API 31. Pierre sulla #329:
  "stiamo sviluppando anche noi un'app".
- **`microduck-console`**, uno Space Docker con `hf_oauth: true`, che serve
  lo stesso `index.html` servito dal robot, in remoto attraverso il
  rendezvous (`remote-access-design.md` §5).
- **`spaces/policy-playground`** e `spaces/shared/{rendezvous,wire,control}.py`
  — le metà client in Python; la roadmap dice "l'SDK è quelli,
  impacchettati" (`docs/project/roadmap.md`, M5).
  `spaces/policy-playground/web/src/rendezvous.ts` è un client browser
  dello stesso servizio in TypeScript, il riferimento più vicino per il
  trasporto della nostra app.
- **Estendibilità:** nessuna. I documenti di design dicono che un metodo
  nuovo è "una modifica di una riga a `route.rs`" — una modifica nel repo
  di Pollen a un enum chiuso. L'unica menzione di codice di terzi sulla
  scheda è in `architecture.md` ("se codice di terzi o dell'utente girerà
  mai sulla scheda"), a proposito dei permessi dei socket.

## 2. Che cosa offre già quack-navd all'app

quack-navd risponde su due socket unix, entrambi JSON-RPC NDJSON, modo
0660, gruppo `robot` (`quack-nav/src/sockets.rs`):

- **Il socket nav** (`/run/quack-nav/nav.sock`): `nav.catalog` e
  `nav.call {name, args}` (`quack-nav/src/bin/quack-navd.rs`). Una risposta
  per richiesta, **nessun push**, ogni chiamata sotto un solo
  `Mutex<Robot>`.
- **Il socket della mappa** (`[maploc] socket`), il dialetto `robot.map*`
  di robotd (`quack-nav/src/mapd/server.rs`): `robot.map` si abbona a un
  `map.frame` al secondo e a un `map.pose` ogni 50 ms; la libreria
  (`map_save`, `map_list`, `map_load`, `map_match`, `map_adopt`) e
  `map_wipe`. **Questo è già uno stream push.**

| all'app serve | oggi | mancanza |
|---|---|---|
| la mappa | `map.frame`: origine, `cell_m` 0.05, `rows × cols`, base64 di un byte per cella (0 ignoto, 1 libero, 2 muro), `seq`, `frozen` (`quack-nav/src/map.rs`, `MapFrame`) | dimensione per un collegamento remoto (sotto) |
| posa e incertezza | `map.pose` a 20 Hz con `tracking`, `seated`, `pose_sigma`; `robot.map_status.pose_uncertainty` | nessuna |
| tocca-e-vai | `robot.go_to {x, y}` in metri di mappa, oppure `{place}`; lavoro in background; `stop` | nessuna per un punto |
| il percorso in corso | `robot.map_status.explore.route`, `goal`, `target`, `target_distance_m`, `state`, `reason` | solo interrogando |
| luoghi | `remember_place` **solo alla posa dell'anatra**, `forget_place`, `list_places` (nome, numero di ancore, raggio, distanza, stale) | niente coordinate nell'elenco; niente nome a un punto toccato; niente rinomina |
| esplorazione | `robot.map_explore` avvio / `stop` / `complete` / `fresh` + `confirmed`; `progress`, `house.percent_mapped`, `house.done` | nessuna |
| "dove siamo?" durante l'esplorazione | `nav.take_question` (interrogato da quacksat) | un push sarebbe meglio |
| mappe | `map_save`, `map_list` (nome, byte, saved_at), `map_load`, `map_match`, `map_adopt`, `map_wipe` | rinomina, cancella, esporta |
| dislivelli (scale) | `cliff.drops` in coordinate del corpo in `map_status`; i registri sono su disco per mappa | non sul socket in coordinate di mappa |
| rientro (homecoming) | visibile come stato dell'esplorazione e suggerimenti | nessuno stato esplicito |
| stanze come aree, zone vietate | — | mancano del tutto |

**Il tocca-e-vai verso un punto funziona già**: `go_to` accetta `x`, `y`.
Ciò che manca per una prima app è poco e soprattutto in lettura: luoghi con
coordinate, dare il nome a un punto toccato, rinominare, i dislivelli in
coordinate di mappa, e non bloccare le letture dietro l'unico mutex (un
`robot.map_step` lo tiene per tutta la camminata e la sosta).

**Dimensione e frequenza su un collegamento remoto.** casa_grande è
9 × 7 m (`todo-map.md`, 2026-09-30); con un metro di margine fanno circa
220 × 180 = 40 000 celle: **40 KB grezzi, ~53 KB in JSON base64 per frame,
ogni secondo — ~0,4 Mbit/s**. Una casa di 15 × 12 m fa ~100 KB a frame. Tre
valori per cella stanno in 2 bit (×4) e una griglia di occupazione si
comprime bene con deflate, quindi l'ordine probabile è di pochi KB a frame
— *da misurare, non misurato*. Conta di più la frequenza: la mappa cambia
solo durante la mappatura, e per nulla una volta `frozen`. La corsia
vuole **la mappa solo quando cambia (per `seq`), compressa; la posa a
1–2 Hz; lo stato quando cambia** — al massimo circa 200 post al minuto,
dentro il limite di 1200 e vicino al budget di 400 notifiche di `mediad`.

## 3. quacksat, e cosa servirebbe per una chat nell'app

- **Struttura** (`README.md`, `Cargo.toml` del workspace): `quacksat-core`
  (cattura del microfono, parola di attivazione, VAD, riproduzione, client
  di robotd, allowlist degli strumenti, `nav_client.rs`) e tre backend
  intercambiabili: `wyoming` (Home Assistant Assist fa STT, intent e TTS),
  `agent` (audio del microfono su WebSocket verso un bridge —
  `bridge/bridge.py` — che fa STT → LLM → TTS, protocollo in
  `docs/agent-protocol.md`), `direct` (l'anatra stessa chiama tre endpoint
  in dialetto OpenAI; la configurazione d'esempio punta l'LLM a un Ollama
  locale, `qwen3:8b`, `quacksat.example.toml`).
- **Come raggiunge quack-navd:** `NavLane` sonda `nav.catalog` sul socket
  nav all'avvio, innesta il catalogo negli strumenti dell'agente, esegue
  con `nav.call` e interroga `nav.take_question`
  (`quacksat-core/src/nav_client.rs`). **Nessun canale remoto di Pollen**
  in quacksat: parla con i socket unix di robotd e di quack-navd, ed esce
  verso i suoi endpoint LLM/STT/TTS o verso il bridge.
- **I suoi punti d'ingresso:** nessuno per una conversazione. quacksat non
  apre alcun socket proprio (lo fanno solo i test). Il turno del backend
  `direct` è `run_turn(utterance: &[i16], history, …)` — audio in ingresso,
  la cronologia un `Vec` dentro il ciclo audio
  (`backends/direct/src/lib.rs`). Il server MCP opzionale
  (`backends/direct/src/mcp.rs`, token bearer obbligatorio, TCP) espone
  **gli strumenti del robot, non la conversazione** — ed è proprio il tipo
  di server sull'anatra che l'utente ha escluso per il remoto. La
  conversazione di `agent` vive nel bridge; quella di `wyoming` in Home
  Assistant, la cui app chatta già per testo.
- **Audio:** un microfono, nessuna cancellazione d'eco, accesso ALSA
  esclusivo di quacksat (`docs/adr/0003-audio-access.md`). `mediad` non ha
  codice audio (`git grep alsasrc|opus` non trova nulla) — la traccia audio
  WebRTC è una riga di design (`remote-webrtc.md` §2) — e se esistesse
  contenderebbe a quacksat lo stesso dispositivo di cattura e arriverebbe in
  `mediad`, non nell'agente.

**Quindi una chat richiede un ingresso testuale in quacksat**: un socket
unix (`/run/quacksat/chat.sock`) con `chat.say {text, speak?}` a cui
rispondono notifiche in streaming `chat.delta`, `chat.tool {name, args,
result}` e `chat.done`, e `chat.subscribe` per rispecchiare ogni turno —
anche quelli a voce — nell'app. Per `direct` significa dividere `run_turn`
in "testo in ingresso" e "audio in ingresso" su una cronologia condivisa;
per `agent`, un evento `text.utterance` e eventi di testo della risposta
nel protocollo dell'agente (il bridge ha già la trascrizione). Le chiamate
agli strumenti arrivano all'app mentre accadono, così "vado in cucina" può
disegnare il percorso sulla mappa dagli argomenti di `robot.go_to` e da
`map_status.explore.route`.

**Chi risponde quando parlano entrambi.** Una conversazione, un turno alla
volta: un lucchetto di turno condiviso dalla parola di attivazione e dal
socket della chat. Un turno di testo che arriva mentre l'anatra ascolta o
parla aspetta (con un limite) e lo si dice; un'attivazione durante un turno
di testo aspetta allo stesso modo. **Una risposta torna da dove è venuto il
suo turno** — l'altoparlante dell'anatra per la voce, l'app per il testo —
e viene detta anche dall'anatra solo se l'app lo chiede (`speak: true`).
Tutto è rispecchiato nella trascrizione dell'app.

**La voce dall'app.** Non via WebRTC: la traccia audio non esiste, e il
microfono è di quacksat. Due strade praticabili, entrambe fase 2: (a)
**riconoscimento vocale sul telefono o sul desktop** (quello della
piattaforma) e invio di testo — nessun audio attraversa la corsia, il che
si addice al suo limite di frequenza; (b) un enunciato compresso in Opus
(pochi KB al secondo, ~15 KB per cinque secondi) inviato come una chiamata
allo STT di quacksat, che tiene un solo riconoscitore e le sue impostazioni
di lingua ma richiede un decoder Opus sull'anatra. Risposte lette dal TTS
del dispositivo, o restituite come audio solo se lo si vuole. L'MVP è
**solo testo**.

## 4. Opzioni di architettura

```
 app ──HTTP POST/SSE──► rendezvous (Space HF, di Pollen) ──SSE/POST──► anatra
                                                                      │
   A:  quack-linkd (nostro, producer a sé, token proprio) ── nav.sock, socket mappa, chat.sock
   B/C: mediad::relay (di Pollen) ── route.rs ── /run/robot/ext.d/… ── nav.sock, chat.sock
```

### A. Un nostro demone adattatore, registrato come producer a sé — funziona oggi

`quack-linkd`: un processo non privilegiato sull'anatra che si connette
**verso l'esterno** al rendezvous con un token HF **suo** (un login da
device flow tutto suo — mai `/etc/robot/hf-token`, che scalzerebbe il peer
di `mediad`), si registra come producer con `meta.kind = "quack-nav"`, un
`hardware_id` derivato dal seriale del robot più un suffisso (così non
sfratta né viene sfrattato da `mediad`), `meta.robot` che nomina l'anatra a
cui appartiene, `simulated` sul gemello, e un `User-Agent` come da #329.
Risponde alle buste `peer {rpc}` con **una propria allowlist esaustiva** sui
metodi nav, mappa e chat, e spinge posa, stato ed eventi della chat dentro
un proprio budget. `mediad::relay` e `spaces/shared/wire.py` (Apache-2.0)
sono il riferimento per ogni stranezza: lo stream prima dell'invio,
l'heartbeat via `POST`, il controllo dello split-brain, il timeout di
lettura di 60 s.

- **Pro:** nessuna modifica da Pollen; un budget proprio di 1200 al minuto,
  separato da quello del robot; quack-navd resta fuori dalla rete;
  costruibile e collaudabile ora, sul gemello.
- **Contro:** un secondo producer per anatra nell'elenco del proprietario;
  la console di Pollen filtra su `kind`, ma i client del mini no, e
  `ReachyCentralConsumer` ripiega sull'unico producer visibile
  (`remote-access-design.md` §5.1) — quindi un'app del mini su un account
  il cui unico producer è il nostro adattatore proverebbe a pilotarlo; un
  secondo token sulla scheda (stesso problema di scope larghi del §2.4);
  dipende dal fatto che a Pollen vada bene un producer di terzi, cosa che
  nessuno ha chiesto; niente video nella stessa sessione (l'app apre una
  seconda sessione WebRTC verso `mediad` se mai servisse la telecamera).

### B. La corsia di Pollen inoltra a quack-navd per noi — serve Pollen, metodo per metodo

Un ramo `nav.*` in `route.rs` e un sesto socket in `upstream.rs`. Pulito per
l'utente (un producer, una sessione, video e mappa insieme) e impossibile
senza fare dei metodi di quack-nav varianti di `duck_ipc_proto::Call` —
l'API di Pollen che possiede la nostra. Non è una richiesta realistica.

### C. Un meccanismo di rotte di estensione upstream — lo stato finale pulito

Ciò che vuole B, generalizzato in modo che Pollen possieda il meccanismo e
non i nostri metodi. Bozza della richiesta:

**Che cosa vediamo.** `route.rs` e `btd/src/route.rs` sono esaustivi su
`proto::Call`, che è la garanzia giusta per i metodi di Pollen e non lascia
alcun modo a un servizio sul robot che non sia di Pollen di essere
raggiunto via WebRTC o via la corsia — quindi un terzo deve far girare un
producer suo (opzione A), con un secondo token e una seconda voce
nell'elenco.

**Modifica proposta.** Una directory di drop-in, di proprietà di root
(`/etc/robot/ext.d/<nome>.toml`): un **prefisso** di metodo (`nav.`,
`chat.`), un percorso di socket, i trasporti che può raggiungere
(`webrtc`, `lane`; BLE mai), e un insieme `mutating` dichiarato. `mediad`
risponde a ogni metodo sotto un prefisso registrato inoltrando la riga alla
lettera a quel socket su una corsia propria — la stessa regola "non leggere
mai una risposta" — e rifiuta i prefissi sconosciuti come oggi. Il match
esaustivo resta esattamente com'è per `proto::Call`; la tabella delle
estensioni è una seconda tabella guidata dai dati il cui contenuto è stato
installato da un amministratore con root, che è la stessa fiducia
dell'installare il demone. `robotctl health` elenca le estensioni;
`only_these_mutating_calls_are_reachable_over_webrtc` acquista un
corrispettivo che nomina i prefissi di estensione. Facoltativamente `hello`
le riporta, così l'app ufficiale può mostrare un pannello di terzi.

**Come verificarlo.** Un socket di estensione finto nell'imbracatura di
test di `mediad` (`fake_daemon`): una chiamata sotto il suo prefisso lo
raggiunge, una sotto un prefisso non registrato è rifiutata per nome, e un
prefisso registrato non può oscurare un metodo di `proto::Call`.

- **Pro:** un producer, un token, video e mappa in una sessione, l'app
  ufficiale potrebbe ospitarla; Pollen tiene la politica.
- **Contro:** il tempo di Pollen e un principio che hanno difeso
  (`remote-webrtc.md` §5, "il match esaustivo"); non a breve.

### Non considerate oltre

Un nostro server raggiunto dall'anatra che chiama verso l'esterno, o una
porta sull'anatra — entrambe fuori dalla decisione dell'utente. Un tunnel
dentro una `proto::Call` esistente (abusando di un campo stringa) —
disonesto e fragile.

### Raccomandazione

**A ora, progettata per diventare C.** L'adattatore parla JSON-RPC semplice
con metodi `nav.*` e `chat.*`, le stesse righe che consegnerebbe a un socket
sotto C; quando C arriva, la tabella dell'adattatore diventa un manifesto e
l'app cambia producer, nient'altro. Prima di costruire A, **chiedere a
Pollen** la domanda della #329 per un producer (sotto). Se dicono di no, A
gira comunque in sviluppo e sulla nostra anatra, e C è l'unica strada verso
gli utenti.

## 5. Sicurezza

- **Chi può raggiungere l'anatra da fuori:** i client entrati con l'account
  Hugging Face del proprietario, e nessun altro — il rendezvous abbina
  account con account. Dentro l'account non c'è un'identità per persona
  sulla busta, quindi **l'account è l'ambito di controllo**. Condividere in
  sola lettura con la famiglia su un altro account non è possibile su
  questo servizio.
- **Ambiti dentro l'adattatore** (la sua allowlist, esaustiva come quella
  di Pollen): *lettura* (mappa, posa, stato, luoghi, elenco mappe,
  trascrizione); *controllo* (`go_to`, `stop`, avvio/arresto/completamento
  esplorazione, modifica luoghi, caricamento mappa, `chat.say`);
  *distruttivo* (`map_wipe`, esplorazione `fresh`, cancellazione mappa) solo
  con un `confirmed` esplicito, come già chiede `map_explore`.
  `robot.map_step` e `robot.move` restano fuori dalla corsia: sono strumenti
  a frequenza da joystick. Un interruttore di configurazione rende il lato
  remoto di sola lettura.
- **L'agente è una via di controllo.** Un `chat.say` può far chiamare
  all'agente `robot.go_to`; gira sotto l'allowlist di quacksat, come un
  turno a voce, quindi la chat sta nell'ambito *controllo*, non *lettura*.
- **Nessuno guarda.** Nell'MVP di A non c'è video: un'anatra mandata
  dall'altra parte della casa dall'ufficio non è sorvegliata. Le guardie
  sono le stesse di casa — la guardia dei dislivelli, i registri dei
  dislivelli, `max_s` — più un rifiuto quando `tracking` è falso, la posa
  incerta o l'anatra seduta, e un push all'app per una caduta o un rifiuto.
- **Riservatezza.** La mappa è la planimetria di una casa; la trascrizione è
  il parlato della famiglia. Entrambe restano sull'anatra; l'app le tiene
  solo in memoria; il rendezvous vede le buste in chiaro (è TLS verso lo
  Space, non da capo a capo). Da dire chiaramente all'utente.
- **Due conducenti.** `remote-webrtc.md` §9: nulla arbitra tra un pad e un
  peer remoto. quack-navd rifiuta già un secondo lavoro mentre uno è in
  corso (`not_exploring`); l'app mostra chi ha avviato quello in corso.
- **Token.** Il token dell'adattatore è una credenziale bearer in un file,
  con tutti gli scope HF se ottenuto come quello di `mediad`. Meglio
  un'app OAuth con solo `openid profile`, se il device flow di HF ne
  permette una registrata da noi.

## 6. Tecnologia dell'app

- **Vincolo:** la corsia è HTTP semplice — `fetch` per `POST /send`, e SSE
  letto con `fetch` (EventSource non può impostare l'header
  `Authorization`, e il server sta togliendo `?token=`,
  `remote-access-design.md` §5). Per l'MVP non serve WebRTC. Un browser ha
  bisogno che il rendezvous permetta la sua origine (CORS) — deve farlo per
  l'origine di `microduck-console`; se per qualunque origine è **da
  verificare**. Un'app Tauri invia le richieste da Rust e il CORS non la
  tocca.
- **Raccomandazione: codice web (TypeScript, la mappa su `<canvas>`)
  dentro Tauri 2**, prima desktop, poi iOS e Android dallo stesso codice. È
  la scelta di Pollen per la loro app, permette al client di usare i tipi
  del filo in Rust (i nostri, e `duck-ipc-proto` dove tocca Pollen), e lo
  stesso codice web può essere servito come Space Hugging Face con
  `hf_oauth` per un client remoto senza installazione, esattamente come
  `microduck-console`. Flutter o React Native costerebbero entrambe le
  cose. Una PWA da sola perde il comportamento in background e le notifiche
  che un'app per telefono vuole.
- **Accesso:** OAuth HF con PKCE, `openid profile`, come da #329 e dalla
  console (in stile `ASWebAuthenticationSession` su mobile, un redirect su
  loopback su desktop, il client id iniettato dallo Space sul web).
- **Sul gemello:** l'anatra MuJoCo si registra come un robot
  (`configd --simulated`, `meta.simulated`); `quack-linkd` gira sul Mac
  accanto a quack-navd, con un token suo, e l'app lo trova nello stesso
  elenco. Per lavorare offline, un finto rendezvous su loopback con i suoi
  tre endpoint (`/events`, `/send`, `/api/robot-status`), come i test di
  `mediad::relay` girano contro un servizio finto. Il viewer
  (`VIEWER=on`) resta la verità con cui confrontare il disegno dell'app.

## 7. Piano a fasi

| fase | l'app | quack-navd / quacksat | quack-linkd |
|---|---|---|---|
| **0. Chiedere** | — | — | Pollen: un producer di terzi sul rendezvous, e la proposta di rotte di estensione (§4 C) |
| **1. MVP** | accesso, scelta dell'anatra, mappa dal vivo + posa + incertezza, percorso del lavoro in corso, tocco → `go_to {x,y}`, stop, luoghi come segnaposto (tocco su un segnaposto → vai), nome a un punto toccato, avvio/arresto/completamento dell'esplorazione con avanzamento, la domanda "dove siamo?" a cui si risponde scrivendo | `list_places` con le coordinate delle ancore; `remember_place {x, y}`; `rename_place`; letture (`map_status`, `list_places`) non bloccate da un `map_step` in corso; dislivelli in coordinate di mappa | registrazione, heartbeat, allowlist, mappa quando cambia (2 bit + deflate), posa a 1–2 Hz, stato quando cambia, `User-Agent` |
| **2. Chat** | un riquadro di conversazione: testo in ingresso, risposta in streaming, chiamate agli strumenti disegnate sulla mappa; voce con STT sul dispositivo | quacksat: il socket della chat, il lucchetto di turno, `chat.subscribe`; protocollo dell'agente: `text.utterance` | rotte `chat.*` nell'ambito controllo |
| **3. Stanze** | disegnare una stanza come poligono, o accettarne una suggerita; "vai in cucina" va dentro | i luoghi diventano aree: poligono + un punto obiettivo; `where_am_i` per contenimento; stanze suggerite dalle porte (più avanti) | — |
| **4. Zone vietate** | disegnare muri virtuali e aree proibite | uno strato di costo nel pianificatore e nell'esploratore, salvato per mappa con i luoghi e i registri dei dislivelli; dislivelli mostrati come divieti automatici | — |
| **5. Mappe** | elencare, rinominare, cancellare, cambiare piano, esportare | `map_rename`, `map_delete`, esportazione; la mappa mostrata allineata alla casa | — |
| **6. Mobile** | build iOS e Android della stessa app; notifiche (arrivata, caduta, serve un nome) | — | push degli eventi che meritano una notifica |
| **più avanti** | la telecamera, via una sessione WebRTC verso `mediad`; l'opzione C se Pollen la accetta | — | si riduce a un manifesto sotto C |

La chat arriva in fase 2, non nell'MVP: richiede lavoro in un secondo repo
(quacksat), una modifica al protocollo dell'agente e una decisione sul
lucchetto di turno; la mappa è utile anche senza, e la voce in casa esiste
già.

## 8. Domande aperte

**Per l'utente.**

1. Va bene un secondo producer per anatra nell'elenco Hugging Face del
   proprietario (opzione A), sapendo che la console di Pollen lo filtra e i
   client del mini forse no?
2. Controllo remoto per default, o remoto in sola lettura con il controllo
   solo su richiesta?
3. La chat in fase 2 come proposto, o nell'MVP?
4. La voce dall'app: il riconoscitore della piattaforma (semplice, lingua
   per dispositivo) o lo STT di quacksat con un upload Opus (un solo
   riconoscitore)?
5. Quale backend è il riferimento per la chat: solo `direct` (Ollama), o
   anche `agent`? (Chi usa `wyoming` chatta in Home Assistant.)

**Per Pollen** (un'issue nello stile della #329).

1. È benvenuto un **producer** di terzi su `reachy_mini_central`, con
   `kind: quack-nav`, un token suo, un `hardware_id` che non può collidere,
   e un `User-Agent`?
2. Accettereste un meccanismo di rotte di estensione in `mediad` (§4 C), se
   lo scrivessimo noi?
3. Il rendezvous permette il CORS da origini diverse dai vostri Space?
4. C'è un limite di dimensione del corpo su `POST /send` (un frame di mappa
   è di decine di KB)?
5. Un login da device flow può usare un'app OAuth con scope più stretti del
   client first-party di HF?

**Da misurare.** La latenza della corsia (andata e ritorno di una chiamata
da un telefono in 4G); la dimensione di un frame di mappa dopo
l'impacchettamento e deflate su casa_grande e su una casa più grande; i
post al minuto dell'adattatore durante una sessione di mappatura.
