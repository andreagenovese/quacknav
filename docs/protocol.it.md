# Il protocollo

Il filo di quack-navd: NDJSON, JSON-RPC 2.0, sul suo socket Unix (`socket` nella configurazione,
`/run/quack-nav/nav.sock` sulla papera). I tipi sono quelli del crate `quack-nav-proto`; il design
è [`design/service.it.md`](design/service.it.md); i tool dietro `nav.call` sono
[`control-contract.it.md`](control-contract.it.md).

Un client manda per primo `nav.hello` e registra cosa riceve. Una differenza di versione si
registra e si serve, mai si rifiuta: un client più vecchio non chiama ciò che non conosce; un
metodo che questo daemon non ha risponde `METHOD_NOT_FOUND` (−32601), un campo sconosciuto
`INVALID_PARAMS` (−32602) che lo nomina.

## Metodi

| metodo | da | parametri | risultato |
|---|---|---|---|
| `nav.hello` | v1 | `{client?, api_version?}` — chi chiede e la versione contro cui è compilato, per il log del daemon | `{api_version, daemon_version, revision, methods, events}` |
| `nav.subscribe` | v2 | `{topics?}` — tutti se assente | `{topics}` concessi; un argomento sconosciuto rifiutato per nome. Da lì la connessione porta le notifiche `nav.event`; una volta per connessione |
| `nav.claim` | v3 | `{holder, ttl_s?, token?}` — un nome per le persone, 60 s di base e 300 al più, il token quando si rinnova da un'altra connessione | `{token, holder, ttl_s}`; −32010 quando un'altra connessione lo tiene |
| `nav.release` | v3 | `{token?}` — la connessione di chi lo tiene può ometterlo | `{released, holder?}` |
| `nav.jobs` | v4 | `{steps: [{tool, args}…], on_failure?}` per partire (1–32 passi, `stop` di base o `continue`); `{}` per lo stato; `{stop: true}` per finirla | `{queued, on_failure}`; `{running, steps, done}`; `{stopping}` |
| `nav.act` | v5 | `{skill, at?}` — una skill che `robot.skills` nomina; `at` `"here"` (predefinito), il nome di un posto o `{x, y}` | `{queued, started}`; la fine su `queue` |
| `nav.catalog` | prima di v1 | — | i tool, ciascuno `{name, description, parameters}` |
| `nav.call` | prima di v1 | `{name, args}` | la risposta del tool; un errore del tool è −32000 col suo messaggio |
| `nav.knobs` | prima di v1 | vedi [`knobs.it.md`](knobs.it.md) | |
| `nav.restart` | prima di v1 | — | `{restarting, reason?, hint?}` |

## Eventi (v2)

Dopo `nav.subscribe` il daemon scrive `{"jsonrpc": "2.0", "method": "nav.event", "params":
{topic, seq, t, data}}` su quella connessione, tra le risposte a qualunque altra cosa vi si
chieda. `seq` conta da 1 per connessione senza buchi; `t` sono i secondi monotoni del daemon. Al
meglio e in ordine: un iscritto indietro di più di 256 eventi viene tolto e la sua connessione
chiusa. Interrogare `robot.map_status` resta corretto.

| argomento | quando | dati |
|---|---|---|
| `job` | lo stato di un lavoro cambia; per un viaggio, appena la sua meta è nota (una partenza senza meta è detta esplorazione tre secondi dopo) | `{state, kind, goal, reason, self_started}` — `kind` `journey`, `explore` o `own` (il moto proprio della papera) |
| `job.progress` | una volta al secondo mentre un lavoro gira | `{legs, refusals, target_distance_m, route_m}` |
| `pose` | la posa della mappa diventa fidata o non fidata | `{tracking, untrusted, pose}` |
| `fall` | il corpo si siede o cade mentre un lavoro gira | `{pose}` |
| `question` | l'esploratore chiede cos'è un posto | `{pose}` |
| `lease` | il prestito è stato preso o è finito (v3) | `{state, holder, why}` |
| `queue` | una coda è partita, un passo è finito, la coda è finita (v4) | `{state, …}` (sotto) |

## Il prestito (v3)

Senza prestito chiunque può guidare, come prima. Una connessione che lo chiede tiene il moto della
papera: `robot.go_to`, `robot.map_explore`, `robot.map_step` e `robot.move` da qualunque altra
connessione rispondono −32010 nominando chi lo tiene. Le letture passano sempre. **Uno stop passa
sempre** — `robot.go_to` o `robot.map_explore` con `{"stop": true}` — e chiude un prestito tenuto
da altri. Un prestito finisce con `nav.release`, allo scadere del ttl, e quando la connessione di
chi lo tiene si chiude; l'argomento `lease` dice `{state, holder, why}` (una fine per ttl si dice
alla prima chiamata che la trova).

## La coda (v4)

`nav.jobs` esegue i suoi passi in ordine sulla scheda. Un passo è un tool coi suoi argomenti come
li prende `nav.call`; un passo che avvia un lavoro (`robot.go_to`, `robot.map_explore`) finisce
quando finisce il lavoro — `done` è un successo, `failed` o `stopped` un fallimento col motivo del
lavoro — e ogni altro passo finisce con la sua risposta. La fine di ogni passo si dice
sull'argomento `queue` (`{state: "step", index, tool, ok, reason}`), e quella della coda (`{state:
"done" | "failed" | "stopped", steps}`). Al primo fallimento la coda si ferma salvo che
`on_failure` sia `continue`. Uno stop — `nav.jobs {"stop": true}` o il `robot.go_to {"stop": true}`
dell'utente — chiude il passo in corso e la coda. Una coda alla volta. Muove la papera, quindi un
prestito tenuto da un'altra connessione la rifiuta, e i suoi passi girano col prestito della
connessione che l'ha chiesta.

## Fare (v5)

`nav.act {skill, at?}` esegue una delle skill di `robotd` come una coda sua: prima una camminata
fino a `at` quando c'è, poi `robot.do`, sorvegliato fino alla fine sull'etichetta della policy del
flusso di stato — comincia quando l'etichetta lascia `walk`/`stand` e finisce quando ci torna. Il
passo della skill risponde `{skill, ran_s, label, body}` (`body` lo stato del registro della
camminata: `free` o `carrying`), oppure fallisce col motivo: robotd l'ha rifiutata, non è mai
cominciata, non è finita nel suo tempo (la durata della tabella più 6 s, o 12 s più 6 s quando la
tabella non ne dà), o il corpo è caduto o è stato tenuto. Le skill si leggono da `robot.skills` a
run time; una sconosciuta è rifiutata nominando quelle che la papera ha. Il catalogo porta lo
stesso come due tool per un agente che conosce solo `nav.call`: `robot.skills` (niente si muove) e
`robot.act {skill, at?}` (avvia la coda, risponde subito); `robot.act` può anche essere un passo
di `nav.jobs`. Fare muove la papera: rispetta il prestito, e uno stop lo chiude.

## Un client in Rust

`quack-nav-client` (in questo workspace) è un client bloccante su questi tipi: `Client::connect`,
`hello` (un daemon di prima di `nav.hello` risponde come una v0, i suoi tool funzionano ancora),
`catalog`, `call(tool, args)`, `claim`/`release` (v3), `jobs`/`jobs_state`/`jobs_stop` (v4), `act` (v5), e `subscribe(socket, topics)` — gli eventi su una connessione loro,
come un iteratore. `cargo run -p quack-nav-client --example watch -- <socket> [argomento…]` li
stampa.

## Codici d'errore

| codice | significato |
|---|---|
| −32700 | la riga non era JSON |
| −32601 | nessun metodo, o nessun tool, con quel nome |
| −32600 | un secondo `nav.subscribe` sulla stessa connessione |
| −32602 | un parametro che questo metodo non prende, nominato |
| −32010 | un'altra connessione tiene il prestito sul moto della papera; il messaggio la nomina |
| −32000 | il tool ha risposto con un errore: il suo messaggio dice perché |

## Storia

- **v5** (2026-11-02) — fare: `nav.act`; i tool `robot.skills` e `robot.act`.
- **v4** (2026-11-02) — una coda di lavori: `nav.jobs`, l'argomento `queue`.
- **v3** (2026-11-02) — un proprietario alla volta: `nav.claim`, `nav.release`, il codice
  −32010, l'argomento `lease`.
- **v2** (2026-11-02) — eventi: `nav.subscribe`, `nav.event`, gli argomenti `job`,
  `job.progress`, `pose`, `fall`, `question`.
- **v1** (2026-11-02) — `nav.hello`. La versione sul filo, i metodi e gli eventi che una build
  serve. `nav.catalog`, `nav.call`, `nav.knobs` e `nav.restart` com'erano.
