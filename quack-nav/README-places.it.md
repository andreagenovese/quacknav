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
| `places` | il registro: file JSON, più ancore per nome, confronto senza distinzione di maiuscole; ogni luogo appartiene alla mappa su cui è stato insegnato (vedi sotto) |
| `tools` | i dodici strumenti come catalogo (JSON Schema) più un esecutore su un `Robot` (lane robotd + lane mappa + registro + guardiano del vuoto): i luoghi, la mappa in numeri con lo spazio libero nelle quattro direzioni e un suggerimento per un giro di mappatura, i lavori dell'esploratore, le mappe salvate |
| `cliff` | il guardiano del vuoto: i frame grezzi di tofd riproiettati con la geometria della testa di Pollen (`kinematics`); un raggio verso il basso che non torna, o torna 1,5× troppo lungo, dove dovrebbe esserci il pavimento è un dislivello — scale, una buca — che la mappa 2D non può mostrare. Giudicato sugli ultimi 3 s nel frame corpo e tenuto per 8, così uno sweep della testa accumula una vista |
| `frontier` | dove il pavimento noto incontra l'ignoto: gruppi di frontiera, e un pianificatore a costi sulla griglia (pavimento noto economico, ignoto caro, muri gonfiati, corsie camminate sempre aperte) verso la più economica raggiungibile e verso qualsiasi meta — ciò su cui girano "mappa tutto" e `go_to` |
| `passage` | infilare un passaggio stretto: due confini laterali e la sterzata che tiene il corpo fra loro |
| `explore` | i lavori che guidano: mappare una casa, camminare verso una meta su una mappa già fatta, e le regole che tengono una gamba lontana dalle scale |
| `homecoming` | svegliarsi in una casa già mappata: caricare l'ultima mappa salvata, confermare la posa, oppure esplorare e richiedere |
| `mapd` | il mapper stesso, quando robotd non lo ospita: `maploc` alimentato da `robot.state` e dallo stream di tofd, la testa che scandisce a ogni sosta, la mappa e la sua libreria servite nel dialetto `robot.map*` di robotd su un socket suo (`[maploc]`) |
| `config` | la sezione `[map]` (`enabled`, `places_path`, `cliff_guard`, `tof_socket`, `explore_max_s`, `ask_phrase`, `explore_turn` — senza effetto dal 2026-09-30), `[homecoming]`, e il file del demone (`NavdConfig`) |

Forma del filo fissata all'API upstream v17 (`MAP_API_VERSION`); i tipi
sono una copia locale finché non esce la release di `duck-ipc-proto` che
li porta.

## A quale mappa appartiene un luogo

Un luogo è coordinate, e le coordinate hanno senso solo sulla mappa su cui
sono state insegnate. Così ogni luogo porta la *discendenza* di quella
mappa: un id che il registro conia ogni volta che una mappa parte da zero
(`robot.map_wipe`, un'esplorazione nuova, un reset visto dalla lane della
mappa che nessuno ha chiesto), tiene col nome della mappa quando la mappa
viva è salvata (`robot.map_save`, la fine di una sessione di esplorazione,
"esplorazione completata"), e riprende quando una mappa salvata è caricata
o adottata (`robot.map_load`, `robot.map_adopt`, il ritorno a casa). I file
della libreria non portano id, quindi il registro tiene i conti da sé, in
`places.json`. `robot.list_places` dà a ogni luogo uno `state`:

| state | quando | `stale` |
|---|---|---|
| `usable` | la sua mappa è quella viva e la papera ci ha avuto una posa fidata da quando è diventata viva | false |
| `pending` | non si sa ancora: all'avvio finché il ritorno a casa non ha caricato una mappa salvata e confermato la posa, o la mappa viva caricata ma non confermata. Mai riconosciuto, mai perso | false |
| `other_map` | appartiene a una mappa salvata che non è quella viva — un'altra casa, o la mappa viva è stata cancellata. Torna quando quella mappa è caricata o adottata | true |
| `stale` | la sua mappa non c'è più: una mappa partita da zero è stata salvata sopra col suo stesso nome, o è stato insegnato su una mappa viva cancellata o azzerata prima che qualcuno la salvasse | true |

Così un'accensione non costa più i luoghi: la mappa nuova con cui parte il
mapper, la ricerca del ritorno a casa e ogni chiamata nel frattempo li
lasciano `pending`, e tornano `usable` appena la posa è confermata sulla
mappa salvata. Fino al 2026-10-01 un'unica generazione diventava stantia
ogni volta che la mappa riportava meno submap di quante mai viste — cosa
che ogni avvio col ritorno a casa faceva, prima di caricare la mappa
salvata. Insegnare è rifiutato finché la mappa viva non è nota, e durante
la ricerca del ritorno a casa (una mappa che si butta quando la papera si
ritrova). Un `places.json` di versione 1 si legge: i suoi luoghi correnti
aspettano la prima mappa salvata, mai vista prima dal registro, su cui la
papera è confermata (o, senza ritorno a casa, la mappa lasciata all'ultima
esecuzione finché ci sono tutte le sue submap); quelli stantii restano
stantii — la versione 1 non distingue un reset vero da uno falso, e
reinsegnare costa meno di una camminata nella stanza sbagliata. Il file è
riscritto come versione 2.

## Ospitarlo

Di solito lo si usa attraverso il demone: `quack-navd` risponde a
`nav.catalog` e `nav.call` sul suo socket unix (vedi il
[README del workspace](../README.it.md)). Nello stesso processo, lo
stesso crate:

```rust
let mut robot = quack_nav::tools::Robot::connect(&config.map, &config.robotd_socket, config.map_socket(), config.gait.clone());
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

Una vista della mappa (la pagina di quack-control, dal 2026-10-01) può
dare un nome a un punto invece che a dove sta l'anatra: `remember_place`
con `x` e `y` in metri della mappa insegna lì, qualunque sia la posa — il
punto dev'essere pavimento che la mappa dal vivo conosce, non un muro né
l'inesplorato. `list_places` dà di ogni luogo il suo `at`, l'ancora a cui
va `go_to`, così una vista può segnarlo.

Un nome si confronta senza badare alle maiuscole, mai tradotto:
`cucina` e `kitchen` sono due luoghi. Un modello che ospita gli
strumenti può tradurre di suo (qwen3:8b ha insegnato `kitchen` a
"questa è la cucina", e ha chiesto di nuovo `kitchen` a "vieni in
cucina", sul gemello, 2026-09-22; la mattina dopo ha tenuto `cucina`
in tutti e due i casi) — una scelta sua, e non sempre la stessa, mai del
registro.

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
