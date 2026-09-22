# quack-nav: i luoghi

Nomi per il posto in cui si trova il Microduck.

Il `maploc` a bordo di robotd (Pollen Robotics, PR upstream 127) dà
all'anatra una mappa 2D della casa e una posa al suo interno. Conosce la
geometria, non le stanze. I luoghi sono lo strato sopra: un *luogo* è un
nome che qualcuno ha attaccato a una posa ("questa è la cucina"),
riconosciuto poi per distanza. Lo espongono quattro strumenti —
`where_am_i`, `remember_place`, `forget_place`, `list_places` — accanto
agli otto che mappano la casa e la percorrono (`map_status`, `map_step`,
`map_explore`, `go_to`, `map_save`, `map_list`, `map_load`,
`map_match`), tutti neutri rispetto all'agente, pronti per essere
proiettati su tool OpenAI, MCP o altro da chi li ospita.

Questo è il crate `quack-nav` del workspace [quack-nav](../README.it.md)
(ADR 0005, ADR 0006): niente voce dentro. Dipende dai tipi IPC
dell'anatra, dalla sua geometria della testa (`kinematics`, Rust puro),
dalla lane del robot (`quack-duck`) e da serde, nient'altro, e
`quack-navd` lo ospita su un socket suo.

## Cosa c'è dentro

| modulo | cosa fa |
|---|---|
| `map` | client `robot.map`: si sottoscrive, tiene l'ultimo `map.frame` (posa, flag di tracking, griglia ternaria), si riconnette in caso di perdita, si spegne da solo su un robotd precedente all'API della mappa, e incrementa un'*epoca* quando il frame della mappa è stato evidentemente azzerato |
| `places` | il registro: file JSON, più ancore per nome, confronto senza distinzione di maiuscole, una *generazione* persistita che diventa stantia a un reset della mappa (l'epoca della lane, o meno submap di quante mai viste — un wipe mentre l'host era spento) |
| `tools` | i dodici strumenti come catalogo (JSON Schema) più un esecutore su un `Robot` (lane robotd + lane mappa + registro + guardiano del vuoto): i luoghi, la mappa in numeri con lo spazio libero nelle quattro direzioni e un suggerimento per un giro di mappatura, i lavori dell'esploratore, le mappe salvate |
| `cliff` | il guardiano del vuoto: i frame grezzi di tofd riproiettati con la geometria della testa di Pollen (`kinematics`); un raggio verso il basso che non torna, o torna 1,5× troppo lungo, dove dovrebbe esserci il pavimento è un dislivello — scale, una buca — che la mappa 2D non può mostrare. Giudicato sugli ultimi 3 s nel frame corpo e tenuto per 8, così uno sweep della testa accumula una vista |
| `frontier` | dove il pavimento noto incontra l'ignoto: gruppi di frontiera, e un pianificatore a costi sulla griglia (pavimento noto economico, ignoto caro, muri gonfiati, corsie camminate sempre aperte) verso la più economica raggiungibile e verso qualsiasi meta — ciò su cui girano "mappa tutto" e `go_to` |
| `passage` | infilare un passaggio stretto: due confini laterali e la sterzata che tiene il corpo fra loro |
| `explore` | i lavori che guidano: mappare una casa, camminare verso una meta su una mappa già fatta, e le regole che tengono una gamba lontana dalle scale |
| `homecoming` | svegliarsi in una casa già mappata: caricare l'ultima mappa salvata, confermare la posa, oppure esplorare e richiedere |
| `config` | la sezione `[map]` (`enabled`, `places_path`, `cliff_guard`, `tof_socket`, `explore_max_s`, `ask_phrase`, `explore_turn`), `[homecoming]`, e il file del demone (`NavdConfig`) |

Forma del filo fissata all'API upstream v17 (`MAP_API_VERSION`); i tipi
sono una copia locale finché non esce la release di `duck-ipc-proto` che
li porta.

## Ospitarlo

Di solito lo si usa attraverso il demone: `quack-navd` risponde a
`nav.catalog` e `nav.call` sul suo socket unix (vedi il
[README del workspace](../README.it.md)). Nello stesso processo, lo
stesso crate:

```rust
let mut robot = quack_nav::tools::Robot::connect(&config.map, &config.robotd_socket, config.gait.clone());
// innesta il catalogo nel tuo …
let mut tools = my_tools();
tools.extend(quack_nav::tools::catalog());
// … e instrada a lui i nomi che dichiara suoi
if quack_nav::tools::handles(name) {
    return quack_nav::tools::execute(name, &args, &mut robot);
}
```

`where_am_i` risponde `known: false` con un motivo finché la posa non è
fidata (seduto, in ricerca, senza mappa); l'insegnamento viene
rifiutato. Altrimenti nomina il luogo più vicino con `at_place` e
`distance_m`: il più vicino, non per forza quello in cui si trova
l'anatra.

Un nome si confronta senza badare alle maiuscole, mai tradotto:
`cucina` e `kitchen` sono due luoghi. Un modello che ospita gli
strumenti può tradurre di suo (qwen3:8b ha insegnato `kitchen` a
"questa è la cucina", e ha chiesto di nuovo `kitchen` a "vieni in
cucina", sul gemello, 2026-09-22) — coerente, ma è una scelta sua, non
del registro.

## Guardare un robot

```sh
cargo run -p quack-nav --example map_watch -- /run/robotd.sock 5
```

stampa una riga per frame e la griglia in testo (`#` muro, `.` libero,
`D` l'anatra). Funziona sia contro il gemello MuJoCo sia contro un robot.

## Cosa non c'è

I trasporti (bridge WebSocket, server MCP, proiezione OpenAI) vivono in
[quacksat](https://github.com/andreagenovese/quacksat), che raggiunge
gli strumenti attraverso il socket di `quack-navd`. Il riconoscimento
degli oggetti non è ciò che una mappa ToF può fare: i nomi vengono dalle
persone.

Copia inglese canonica: `README-places.md`. Licenza: Apache-2.0, come il
workspace.
