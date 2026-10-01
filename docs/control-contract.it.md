# Il contratto di controllo

Che cosa offre quack-navd a un programma che lo gestisce — una vista della
mappa, un piano di controllo, uno script. Scritto il 2026-10-01 per
quack-control (la pagina web locale, un repository a sé, non ancora
pubblicato: la decisione è in [study/map-app.it.md](study/map-app.it.md),
"Decisione 2026-10-01"), e per qualsiasi altro client: tutto qui sono i
socket del demone, niente è di quack-control. Copia inglese:
[control-contract.md](control-contract.md).

## I due socket

| socket | percorso di default (chiave di config) | che cosa serve |
|---|---|---|
| nav | `/run/quack-nav/nav.sock` (`socket`) | gli strumenti (`nav.catalog`, `nav.call`), le manopole (`nav.knobs`), `nav.restart` |
| mappa | `/run/quack-nav/map.sock` (`[maploc] socket`) | la mappa dal vivo (`robot.map`: `map.frame`, `map.pose`) e la libreria delle mappe, nel dialetto `robot.map*` di robotd |

Sono entrambi socket unix, modo 0660, gruppo `robot`
(`quack-nav/src/sockets.rs`): un client gira come un utente in `robot` —
che è anche il diritto di guidare l'anatra, quindi un client che serve la
rete porta con sé quel diritto. Con `[maploc] enabled = false` la mappa
arriva invece dal socket di robotd (`NavdConfig::map_socket`).

Il protocollo è JSON-RPC 2.0, un oggetto JSON per riga (NDJSON) in
entrambe le direzioni, un thread per chiamante. Una richiesta con un `id`
riceve esattamente una riga di risposta. Errori: `-32700` non è JSON,
`-32601` metodo o strumento inesistente, `-32000` uno strumento ha
rifiutato — il suo `message` è il motivo, scritto per essere mostrato a una
persona.

## nav.catalog e nav.call

```json
{"jsonrpc":"2.0","id":1,"method":"nav.catalog","params":{}}
{"jsonrpc":"2.0","id":2,"method":"nav.call","params":{"name":"robot.go_to","args":{"x":1.2,"y":-0.4}}}
```

`nav.catalog` risponde con una lista di `{name, description, parameters}`,
i parametri in JSON Schema — la stessa lista che riceve un agente, quindi
un client può costruirci un modulo per strumento. `nav.call` ne esegue uno,
e il suo `result` è il JSON dello strumento. Le chiamate prendono un solo
lock: girano una alla volta, e una chiamata aspetta quella prima. Quasi
tutte rispondono in millisecondi; `robot.map_explore` `{"complete": true}`
aspetta fino a due minuti che una sessione in corso salvi.
`robot.map_explore` e `robot.go_to` avviano un lavoro in background e
rispondono subito.

Che cosa usa una vista della mappa:

| chiamata | che cosa dà o fa |
|---|---|
| `robot.map_status` | `pose` (x, y, yaw), `pose_uncertainty` (`xy_m`, `xy_minor_m`, `along_deg`, `yaw_deg`, una sigma; null quando è persa), `tracking`, `seated`, `mode`; `house` (`map`, `percent_mapped`, `sessions`, `done`); `explore` — il lavoro: `state` (`idle`, `running`, `done`, `stopped`, `failed`), `reason`, `route` e `route_raw` (`[[x, y], …]`), `aim`, `goal` (`[x, y]`), `local` (`[[x, y, r], …]`: r ≥ 0,10 m un dislivello nei registri, meno un ostacolo), `progress`, `question_pending` |
| `robot.list_places` | di ogni luogo `name`, `radius_m`, `state` (`usable`, `pending` — la papera non si è ancora ritrovata sulla mappa del luogo, `other_map` — è viva un'altra mappa salvata, `stale` — la sua mappa non c'è più; vedi [README-places](../quack-nav/README-places.it.md#a-quale-mappa-appartiene-un-luogo)), `stale` (il vecchio flag: `other_map` o `stale`), `map` (la mappa salvata a cui appartiene, o null), `distance_m` (solo luoghi usabili), e `at` — `{x, y}`, l'ancora a cui va `go_to`; `live_map`, il nome salvato della mappa viva (null se ignota o mai salvata) |
| `robot.go_to` | `{"x", "y"}` un punto, `{"place"}` un nome, `{"stop": true}` ferma qualunque lavoro sia in corso |
| `robot.map_explore` | `{}` avvia una sessione, `{"stop": true}` la ferma, `{"complete": true}` chiude la mappa così com'è; `fresh` sostituisce la mappa e chiede `confirmed` |
| `robot.remember_place` | `{"name"}` dove sta l'anatra; `{"name", "x", "y"}` un punto di pavimento mappato |
| `robot.forget_place` | `{"name"}` |

Tutte le coordinate sono della mappa dal vivo, in metri.

## La mappa dal vivo: robot.map sul socket della mappa

```json
{"jsonrpc":"2.0","id":1,"method":"robot.map","params":{}}
```

La risposta `{"accepted", "enabled", "mode"}`, poi notifiche sulla stessa
connessione finché il chiamante non se ne va:

- `map.frame`, una al secondo: `seq`, la posa (`x`, `y`, `yaw`,
  `tracking`, `still`, `seated`, `frozen`, `pose_sigma`), la griglia
  (`x_min`, `y_min`, `cell_m`, `rows`, `cols`, e `cells`: base64, un byte
  per cella, 0 ignota, 1 libera, 2 muro, per righe, riga 0 a `y_min`), e
  `n_submaps`, `n_loops`, `windows`. Un salto di decine di submap è
  un'altra mappa (caricata, adottata, cancellata): una scia disegnata sulla
  vecchia non vuol dire niente sulla nuova.
- `map.pose`, ogni 50 ms tra un frame e l'altro: `seq` (il frame a cui
  appartiene), `x`, `y`, `yaw`, `tracking`, `seated`, `pose_sigma`.

`scripts/twin/viewer/maploc_overlay.py` le legge entrambe.

## Le manopole: nav.knobs e knobs.env

Quasi tutta la taratura sono variabili d'ambiente
([knobs.it.md](knobs.it.md)), lette dall'ambiente del processo, che niente
da fuori può cambiare. Quindi:

- l'unit legge `/var/lib/quack-nav/knobs.env` a ogni avvio
  (`EnvironmentFile=-`, facoltativo) — la chiave di config `knobs_env`
  nomina lo stesso file per il demone;
- `nav.knobs` lo legge e lo scrive, come utente del demone (la directory di
  stato è di `quacknav`, non del client):

```json
{"jsonrpc":"2.0","id":1,"method":"nav.knobs","params":{}}
{"jsonrpc":"2.0","id":2,"method":"nav.knobs","params":{"set":{"QK_CLIFF_MARGIN_M":"0.3","QK_TRAIL":null}}}
{"jsonrpc":"2.0","id":3,"method":"nav.knobs","params":{"reset_all":true}}
```

La risposta è `{"env_file", "restart_needed", "knobs": […]}`, ogni manopola
come la tiene `quack-nav/src/knobs.json` — `name`, `group` (`QK`,
`MAPLOC`), `type` (`number`; `switch` `0`/`1`; `choice` con `options`;
`flag`, `1` accesa e assente spenta; `text`), `default` (null quando il
codice non ne ha), `read_as`, `where`, `doc` —
più `saved` (il valore del file, o null: il default) e `running` (quello di
questo processo). `restart_needed` dice che i due differiscono da qualche
parte. `null` in `set` toglie un valore. Un valore che non rispetta il tipo
della sua manopola è rifiutato e non si scrive niente; le righe del file
che non sono manopole (un `RUST_LOG`, un commento) restano. La lista è
generata dal codice da `scripts/knobs.py` — solo le manopole lette dai
sorgenti di quack-navd, non quelle dei banchi né l'oracolo del gemello
(`QK_ORACLE_*`, che sostituiscono la mappa o la posa con la verità del
simulatore) — e il CI controlla che sia aggiornata. Un `default` che nel
codice è una costante è dato col suo valore; uno che dipende dal modo si
legge `0.12 (guarded: 0.20)`.

## nav.restart

```json
{"jsonrpc":"2.0","id":1,"method":"nav.restart","params":{}}
```

Sotto systemd: `{"restarting": true}`, poi il demone se ne va come con
SIGTERM — il lavoro in corso fermato, la sessione di mappatura salvata, i
socket rimossi — e il `Restart=always` dell'unit lo riavvia circa 5 s dopo,
rileggendo il file. La homecoming gira come all'avvio: l'anatra si alza,
trova la mappa e se stessa sopra. I client perdono entrambi i socket per
quei secondi e si riconnettono. Altrove (il gemello, una shell) risponde
`{"restarting": false, "reason": …}` e resta: sul gemello lo fa
`scripts/twin/twin.sh restart-navd`, che legge `$STATE/knobs.env` allo
stesso modo.

`nav.knobs` e `nav.restart` sono metodi, non strumenti: non sono in
`nav.catalog`, quindi un agente che innesta il catalogo nei suoi strumenti
(quacksat) non li vede mai.

## Stabilità

Additivo dentro una linea di release: campi e metodi si aggiungono, non si
rinominano né si tolgono, senza una riga nel [CHANGELOG](../CHANGELOG.it.md).
Un client dovrebbe ignorare i campi che non conosce e trattare un `at`
mancante o un metodo mancante (`-32601`) come un quack-navd più vecchio.
