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
| `job` | lo stato di un lavoro cambia; per un viaggio, appena la sua meta è nota (una partenza senza meta è detta esplorazione un secondo dopo) | `{state, kind, goal, reason, self_started}` — `kind` `journey`, `explore` o `own` (il moto proprio della papera) |
| `job.progress` | una volta al secondo mentre un lavoro gira | `{legs, refusals, target_distance_m, route_m}` |
| `pose` | la posa della mappa diventa fidata o non fidata | `{tracking, untrusted, pose}` |
| `fall` | il corpo si siede o cade mentre un lavoro gira | `{pose}` |
| `question` | l'esploratore chiede cos'è un posto | `{pose}` |

## Codici d'errore

| codice | significato |
|---|---|
| −32700 | la riga non era JSON |
| −32601 | nessun metodo, o nessun tool, con quel nome |
| −32600 | un secondo `nav.subscribe` sulla stessa connessione |
| −32602 | un parametro che questo metodo non prende, nominato |
| −32000 | il tool ha risposto con un errore: il suo messaggio dice perché |

## Storia

- **v2** (2026-11-02) — eventi: `nav.subscribe`, `nav.event`, gli argomenti `job`,
  `job.progress`, `pose`, `fall`, `question`.
- **v1** (2026-11-02) — `nav.hello`. La versione sul filo, i metodi e gli eventi che una build
  serve. `nav.catalog`, `nav.call`, `nav.knobs` e `nav.restart` com'erano.
