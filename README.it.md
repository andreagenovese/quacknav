# quack-nav

Navigazione per il [Microduck](https://pollen-robotics.com/microduck/):
dove si trova la papera, e come arriva da un'altra parte.

Un demone (`quack-navd`) e la libreria che ci sta sotto. Ospita da sé il
mapper — `maploc`, in questo workspace, derivato da quello di Pollen (PR
upstream 127) — sopra il robotd **rilasciato** di Pollen, e aggiunge tutto
quello che ci sta sopra: una guardia del dirupo che
giudica i raggi verso il basso del sensore 8×8 contro il pavimento, un
planner su mappa dei costi, un registro dei posti che le persone le
hanno insegnato, un esploratore che mappa una casa da solo, un
homecoming che riconosce la casa all'avvio — e un *gemello di carta*
che fa girare tutto questo contro un modello cinematico della papera,
migliaia di volte all'ora, così una regola si misura prima di crederci.

Progetto indipendente, non affiliato a Pollen Robotics. Apache-2.0.
Separato da [quacksat](https://github.com/andreagenovese/quacksat) il
2026-09-22 (ADR 0006), con la storia di ogni misura che l'ha fatto.

## A cosa serve

Una papera che sa dov'è la si può mandare da qualche parte.
`quack-navd` risponde per questo su un socket unix, nel dialetto di
robotd — NDJSON, JSON-RPC 2.0 — così può guidarla qualunque cosa: un
satellite vocale, un agente, un bridge ROS, uno script.

```sh
# il catalogo che annuncia
printf '{"jsonrpc":"2.0","id":1,"method":"nav.catalog","params":{}}\n' | nc -U /run/quack-nav/nav.sock

# dove sono?
printf '{"jsonrpc":"2.0","id":2,"method":"nav.call","params":{"name":"robot.where_am_i","args":{}}}\n' | nc -U /run/quack-nav/nav.sock

# mappa la casa, poi vai in cucina
… {"name":"robot.map_explore","args":{}}
… {"name":"robot.go_to","args":{"place":"cucina"}}
```

Dodici tool: `robot.where_am_i`, `robot.remember_place`,
`robot.forget_place`, `robot.list_places`, `robot.map_status`,
`robot.map_step`, `robot.map_explore`, `robot.go_to`, `robot.map_save`,
`robot.map_list`, `robot.map_load`, `robot.map_match` — ognuno con i
parametri in JSON Schema, pronti da proiettare sui tool OpenAI o su MCP
da chi li ospita.

## Come si usa

Cosa può chiedere un utente alla papera, con un satellite vocale o con
qualunque cosa parli il socket:

- **"Esplora la casa."** L'esplorazione è progressiva: una sessione per
  carica (un tempo, o la batteria sotto il 25 %), ognuna riprende da dove si
  era fermata la precedente e salva la mappa alla fine. Dopo la carica
  successiva la papera ritrova la mappa, ritrova se stessa su di essa e
  continua (`[homecoming] resume_explore`), finché non resta niente di grande
  — allora la casa è completa.
- **"A che punto è la mappa?"** `robot.map_status` → `house.percent_mapped`,
  le sessioni fatte, completa o no.
- **"Esplorazione completata."** L'utente chiude la mappa com'è
  (`map_explore complete`): salvata, dichiarata completa, congelata.
- **Una mappa nuova.** `map_explore fresh` risponde cosa si perderebbe e
  aspetta `confirmed`; la mappa salvata viene sostituita solo quando salva la
  prima sessione della nuova.
- **Andare nei posti.** Su una mappa finita la papera non esplora più, anche
  dopo un riavvio: torna a casa, congela la mappa e naviga — alla cieca dove
  la mappa conosce il pavimento, con la guardia dove non lo conosce.
  `robot.go_to` verso un posto con nome o un punto; `robot.remember_place`
  dà un nome a dove si trova.

Il progetto è l'ADR 0008.

## I crate

- **`quack-duck`** — la lane di robotd (un client JSON-RPC sul suo
  socket unix), i limiti della gait, i comandi del corpo. Un client
  della papera, niente di più; lo usa anche il satellite.
- **`quack-nav`** — il client della mappa, la guardia del dirupo, il
  planner, il registro dei posti, l'esploratore, l'homecoming, i tool,
  il gemello di carta e `quack-navd`.

## Cosa è stato misurato

**[`docs/results.it.md`](docs/results.it.md)** ha i numeri della release, i
criteri con cui si giudicano e i limiti noti. In breve, sul gemello MuJoCo con
tre case: nessuna caduta in 11 sessioni di esplorazione e 51 viaggi; il 90 %
dei viaggi arrivati (la build di `main`, stesse mappe: 63 %); ogni posa
confermata entro 20 cm dalla verità; 5 criteri di release su 7, contando a
metà quelli parziali.

Prima di questo, tre settimane sul gemello, scritte mentre accadevano in
[`docs/todo-map.it.md`](docs/todo-map.it.md) e
[`docs/study/baseline-twin.it.md`](docs/study/baseline-twin.it.md). In
breve:

- **Viaggi ciechi su mappa salvata**: sei goal per un appartamento,
  6/6, circa otto minuti, nessuna caduta — corsa dopo corsa dal
  2026-09-16.
- **Il boot**: la papera si sveglia, riconosce la casa che ha salvato e
  conferma la posa in 70–330 s a seconda della stanza.
- **La tromba delle scale**: il passaggio accanto a un buco, largo
  0,54 m, camminato con le guardie accese quando la posa sta entro
  10 cm — e perché a decidere è la posa, non le regole (lo scan matcher
  resta indietro di 8–10 cm lungo un corridoio: misurato, non supposto).
- **Cose per terra**: un cubetto di 7 cm accanto a una gamba cieca
  viene visto e aggirato; sotto i ~9 cm la soglia del pavimento del
  sensore lo perde mentre cammina.

## Il gemello di carta

`quack-nav/examples/paper_twin.rs` è l'esploratore fatto girare contro
un modello cinematico della papera in un appartamento di scatole: il
planner vero, le guardie vere, il lavoro vero — con la gait, il sensore
di profondità e la deriva della posa modellati su ciò che il gemello
MuJoCo ha misurato. Trenta prove durano un minuto:

```sh
cargo run --release --example paper_twin -- \
    quack-nav/examples/apartment.world.json /tmp/out --runs 30 --goto -2.64,-2.12 --books
```

`--known` congela il mondo come mappa (la bench dei viaggi), `--bias
dx,dy` sposta il frame della mappa dal mondo (quel che un errore di posa
vero fa a un passaggio).

## Farlo girare

```sh
cargo build --release
target/release/quack-navd /etc/robot/quack-nav.toml
```

```toml
socket = "/run/quack-nav/nav.sock"
robotd_socket = "/run/robotd.sock"

[map]
enabled = true
tof_socket = "/run/tofd/tof.sock"
places_path = "/var/lib/quack-nav/places.json"

[homecoming]
enabled = true          # riconosce la casa all'avvio e si riprende la sua mappa
resume_explore = true   # esplora ancora dopo ogni carica finché la casa è completa

[maploc]
enabled = true          # ospita qui il mapper, con il robotd ufficiale
mode = "stop_and_scan"  # oppure "localize" quando la casa è mappata
map_path = "/var/lib/quack-nav/maploc.session"
```

Con `[maploc]` acceso, `quack-navd` fa girare da sé il `maploc` di Pollen
(il crate `maploc` di questo workspace): legge `robot.state` e lo stream
di profondità di tofd, muove la testa a ogni sosta e serve la mappa su
`/run/quack-nav/map.sock` nel dialetto `robot.map*` di robotd. In robotd
non cambia niente — la daemon-v0.14.4 pubblica tutto ciò che serve al
mapper. Spento, la mappa arriva da un robotd che ospita maploc da sé.

`quack-nav/systemd/quack-navd.service` e `quack-nav/systemd/sysusers.d/`
lo installano come servizio non privilegiato accanto a robotd.

## Stato

Misurato sul gemello MuJoCo (`microduck_rl` + robotd); la papera fisica
arriva a dicembre 2026. Due modi di farlo girare:

- **robotd ufficiale** (daemon-v0.14.4) con `[maploc] enabled`: il
  mapper sta in `quack-navd`. È la configurazione della preview; i numeri
  sono in [`docs/results.it.md`](docs/results.it.md).
- **Un robotd che ospita maploc** — la PR 127 upstream, ancora aperta,
  più la libreria di mappe di `docs/study/upstream-asks.md` §5, che vive su
  un fork di `pollen-robotics/microduck` — con `[maploc]` spento.
