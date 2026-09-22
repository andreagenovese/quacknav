# quack-places

Nomi per il posto in cui si trova il Microduck.

Il `maploc` a bordo di robotd (Pollen Robotics, PR upstream 127) dà
all'anatra una mappa 2D della casa e una posa al suo interno. Conosce la
geometria, non le stanze. Questo crate aggiunge lo strato sopra: un
*luogo* è un nome che qualcuno ha attaccato a una posa ("questa è la
cucina"), riconosciuto poi per distanza. Gli strumenti che lo espongono
— `where_am_i`, `remember_place`, `forget_place`, `list_places` — sono
neutri rispetto all'agente, pronti per essere proiettati su tool OpenAI,
MCP o altro da chi li ospita.

Parte di [quacksat](../README.it.md) (ADR 0005), ma volutamente estraneo
alla voce: dipende dai tipi IPC dell'anatra, dalla sua geometria della testa (`kinematics`, Rust puro) e da serde, nient'altro,
così può essere ospitato dal satellite vocale oggi e da un demone
proprio domani — o spostarsi in un repository a sé — senza cambiare.

## Cosa c'è dentro

| modulo | cosa fa |
|---|---|
| `map` | client `robot.map`: si sottoscrive, tiene l'ultimo `map.frame` (posa, flag di tracking, griglia ternaria), si riconnette in caso di perdita, si spegne da solo su un robotd precedente all'API della mappa, e incrementa un'*epoca* quando il frame della mappa è stato evidentemente azzerato |
| `places` | il registro: file JSON, più ancore per nome, confronto senza distinzione di maiuscole, una *generazione* persistita che diventa stantia a un reset della mappa (l'epoca della lane, o meno submap di quante mai viste — un wipe mentre l'host era spento) |
| `tools` | gli strumenti come frammento di catalogo (JSON Schema) più un esecutore su un contesto `Places` (lane mappa + registro): i quattro strumenti dei luoghi e `map_status`, la mappa in numeri, lo spazio libero nelle quattro direzioni dalla griglia, più un suggerimento per un giro di mappatura |
| `cliff` | il guardiano del vuoto: i frame grezzi di tofd riproiettati con la geometria della testa di Pollen (`kinematics`); un raggio verso il basso che non torna, o torna 1,5× troppo lungo, dove dovrebbe esserci il pavimento è un dislivello — scale, una buca — che la mappa 2D non può mostrare. Tenuto 3 s nel frame corpo così uno sweep della testa accumula una vista |
| `frontier` | dove il pavimento noto incontra l'ignoto: gruppi di frontiera, percorsi in ampiezza con i muri gonfiati verso la più vicina raggiungibile, un waypoint per tappa — ciò su cui gira "mappa tutto" |
| `config` | la sezione `[map]` che un host incorpora: `enabled`, `places_path`, `cliff_guard`, `tof_socket`, `explore_max_s`, `ask_phrase`, `explore_turn` |

Forma del filo fissata all'API upstream v17 (`MAP_API_VERSION`); i tipi
sono una copia locale finché non esce la release di `duck-ipc-proto` che
li porta.

## Ospitarlo

```rust
let places = quack_places::Places::connect(&config.map, &config.robotd_socket);
// innesta il frammento nel tuo catalogo …
let mut tools = my_tools();
tools.extend(quack_places::tools::catalog());
// … e instrada a lui i nomi che dichiara suoi
if quack_places::tools::handles(name) {
    return quack_places::tools::execute(name, &args, &mut places);
}
```

`where_am_i` risponde `known: false` con un motivo finché la posa non è
fidata (seduto, in ricerca, senza mappa); l'insegnamento viene rifiutato.

## Guardare un robot

```sh
cargo run -p quack-places --example map_watch -- /run/robotd.sock 5
```

stampa una riga per frame e la griglia in testo (`#` muro, `.` libero,
`D` l'anatra). Funziona sia contro il gemello MuJoCo sia contro un robot.

## Cosa non c'è

I trasporti (bridge WebSocket, server MCP, proiezione OpenAI) vivono in
quacksat, e così `robot.map_step` (cammina, poi sta fermo): guida il
corpo, che è la lane robotd di quacksat. La navigazione (`go_to`) aspetta un RPC di goal upstream. Il
riconoscimento degli oggetti non è ciò che una mappa ToF può fare: i
nomi vengono dalle persone.

Copia inglese canonica: `README.md`. Licenza: Apache-2.0, come il
workspace.
