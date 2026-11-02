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
| `nav.catalog` | prima di v1 | — | i tool, ciascuno `{name, description, parameters}` |
| `nav.call` | prima di v1 | `{name, args}` | la risposta del tool; un errore del tool è −32000 col suo messaggio |
| `nav.knobs` | prima di v1 | vedi [`knobs.it.md`](knobs.it.md) | |
| `nav.restart` | prima di v1 | — | `{restarting, reason?, hint?}` |

## Codici d'errore

| codice | significato |
|---|---|
| −32700 | la riga non era JSON |
| −32601 | nessun metodo, o nessun tool, con quel nome |
| −32602 | un parametro che questo metodo non prende, nominato |
| −32000 | il tool ha risposto con un errore: il suo messaggio dice perché |

## Storia

- **v1** (2026-11-02) — `nav.hello`. La versione sul filo, i metodi e gli eventi che una build
  serve. `nav.catalog`, `nav.call`, `nav.knobs` e `nav.restart` com'erano.
