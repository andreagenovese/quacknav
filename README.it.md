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
quacksat ora è solo il front end vocale: inoltra i comandi a voce
dell'utente agli strumenti di quack-nav e non fa navigazione.

> **Un esperimento — da prendere con le pinze.** Tutto quello che c'è qui
> ha girato solo sul gemello MuJoCo e su quello di carta, mai su una
> papera fisica. Un robot che cammina vicino alle scale può cadere e
> rompersi: se lo provi su hardware vero, tienilo lontano dai dislivelli e
> stagli accanto. I numeri in [docs/results.it.md](docs/results.it.md)
> sono quello che ha misurato il gemello, non una promessa di cosa farà
> una casa vera.

Release attuale: **v0.2.0-rc1**, una release candidate validata sui gemelli —
[note di rilascio](docs/release-notes-v0.2.0-rc1.it.md), [changelog](CHANGELOG.it.md).

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
- **Il boot**: la papera si sveglia dove l'hanno portata, riconosce la
  casa che ha salvato e conferma la posa. Al banco dei risvegli
  (2026-09-30, 12 partenze sparse in due case, poi le stesse girate di
  180°): 23 su 24 giuste, nessuna sbagliata, nessuna caduta; mediane di
  87–105 s, e 105–123 s girate (174–192 s e 126–135 s prima della mappa
  ombra).
- **Una casa mai usata per mettere a punto nulla** (casa_grande,
  2026-09-30: sette ambienti, un corridoio che gira di 90°, due buche):
  16/16 viaggi, 8/8 risvegli (mediana 84 s), nessuna caduta, muri al 99 %
  sulla verità.
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

Rust 1.89 o più recente; la prima compilazione scarica da GitHub i crate
`duck-ipc-proto` e `kinematics` di Pollen (tag daemon-v0.14.4).

```sh
git clone https://github.com/andreagenovese/quacknav.git && cd quacknav
cargo build --release
target/release/quack-navd /etc/robot/quack-nav.toml
```

`quack-nav/quack-nav.example.toml` (i commenti lì sono in inglese):

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
lo installano come servizio non privilegiato accanto a robotd
([Installare sulla papera](#installare-sulla-papera)); l'unità
esegue `/usr/local/bin/quack-navd /etc/robot/quack-nav.toml` e crea
`/run/quack-nav/` per i socket. Lanciato a mano fuori da quell'unità,
`/run/quack-nav/` deve esistere ed essere scrivibile — `[maploc] socket` ha
come predefinito `/run/quack-nav/map.sock` anche quando `socket` è altrove —
altrimenti il demone si ferma subito, nominando il percorso e la via d'uscita:

```text
Error: cannot bind the map socket /run/quack-nav/map.sock: No such file or directory (os error 2) — its directory /run/quack-nav does not exist (systemd's RuntimeDirectory= creates it; by hand: mkdir -p /run/quack-nav or set `[maploc] socket`)
```

Un file di configurazione che non si legge o non si interpreta viene
nominato allo stesso modo. Parte
anche senza robotd e tofd e li aspetta (intanto gli strumenti rispondono "no
map yet"); senza la papera, il gemello MuJoCo fa le veci di entrambi ([scripts/twin/README.it.md](scripts/twin/README.it.md)).

I test e la soglia del gemello di carta, come li fa girare la CI
(`.github/workflows/ci.yml`):

```sh
cargo test --workspace --release --features maploc/kinematics
python3 scripts/knobs.py --check
cargo build --release -p quack-nav --example paper_twin
mkdir -p /tmp/paper-twin
python3 scripts/ci/paper_twin_gate.py target/release/examples/paper_twin \
    quack-nav/examples/apartment.world.json /tmp/paper-twin
```

### Compilare per la papera

La scheda della papera è una Radxa Zero 3 (RK3566, aarch64) con Armbian
26.2.x e l'userland di Debian 13 (Trixie), glibc 2.41. Un Mac con Apple
silicon ha la stessa CPU ma non lo stesso sistema operativo, quindi
`quack-navd` si compila in cross per `aarch64-unknown-linux-gnu` con
[cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild): `zig cc` fa
da linker e porta con sé gli stub della glibc, senza Docker. La glibc minima
è fissata a 2.31 — la stessa del `cargo board` di microduck — così il
binario si carica sulla scheda qualunque glibc abbia la macchina che
compila.

```sh
# una volta, su un Mac (il rustup di Homebrew è keg-only e non tocca il suo `rust`)
brew install rustup zig cargo-zigbuild
/opt/homebrew/opt/rustup/bin/rustup toolchain install stable --profile minimal \
    --target aarch64-unknown-linux-gnu
# a ogni compilazione
scripts/cross-build.sh
```

Lo script trova la toolchain di rustup, esegue
`cargo zigbuild --release -p quack-nav --bin quack-navd --target aarch64-unknown-linux-gnu.2.31`
e controlla il risultato:

```text
target/aarch64-unknown-linux-gnu/release/quack-navd: ELF 64-bit LSB pie executable, ARM aarch64, version 1 (SYSV), dynamically linked, interpreter /lib/ld-linux-aarch64.so.1, for GNU/Linux 2.0.0, stripped
glibc required: GLIBC_2.30
```

Il linker di zig stampa un avviso innocuo (`ignoring deprecated linker
optimization setting '1'`). Il binario è stato fatto girare in un container
`debian:trixie` arm64 (glibc 2.41): parte, crea i due socket e aspetta
robotd e tofd. Su Linux lo stesso script funziona (`rustup target add
aarch64-unknown-linux-gnu`, zig dalla distribuzione o `pip install
ziglang`, `cargo install cargo-zigbuild`); in alternativa
[`cross`](https://github.com/cross-rs/cross) con Docker o Podman
(`cross build --release -p quack-nav --bin quack-navd --target
aarch64-unknown-linux-gnu`), oppure un semplice `cargo build --release -p
quack-nav --bin quack-navd` su una qualunque macchina Linux aarch64, scheda
compresa (lento lì: quattro core Cortex-A55).

### Installare sulla papera

La scheda dev'essere già preparata da microduck: robotd e tofd in
funzione, e il gruppo `robot` a cui appartengono i loro socket. Ecco
tutto ciò che quack-nav aggiunge:

| sulla papera | da questo repo |
|---|---|
| `/usr/local/bin/quack-navd` | il binario compilato in cross |
| `/etc/systemd/system/quack-navd.service` | `quack-nav/systemd/quack-navd.service` |
| `/etc/sysusers.d/quack-nav.conf` (utente `quacknav`) | `quack-nav/systemd/sysusers.d/quack-nav.conf` |
| `/etc/robot/quack-nav.toml` | `quack-nav/quack-nav.example.toml` (la configurazione qui sopra) |
| `/run/quack-nav/{nav,map}.sock` | creati dall'unità (`RuntimeDirectory=`), modo 0660, gruppo `robot` |
| `/var/lib/quack-nav/` (luoghi, sessioni, `maps/`) | creata dall'unità (`StateDirectory=`), di `quacknav` |

L'unità esegue il demone come `quacknav` con `robot` come gruppo
supplementare (raggiunge i socket 0660 di robotd e di tofd, e dà i suoi
due a `robot`), a nice 5, sotto i 320 MB, con il filesystem in sola
lettura tranne la sua directory di stato.

Un solo comando dalla macchina di sviluppo, dopo `scripts/cross-build.sh`:

```sh
scripts/install-on-duck.sh radxa@192.168.1.42
```

Copia i quattro file con `scp`, poi con `sudo` sulla papera installa il
binario, l'unità e l'utente, installa la configurazione solo se
`/etc/robot/quack-nav.toml` non c'è (una già modificata resta), copia un
vecchio `/var/lib/quacksat/places.json` se `/var/lib/quack-nav/` non ne ha
uno, e abilita e riavvia il servizio, stampando ogni comando che esegue.
Rilanciato, è l'aggiornamento. `SSH_OPTS="-p 2222"` passa opzioni a ssh e
scp. A mano, lo stesso:

```sh
# sulla macchina di sviluppo
scp target/aarch64-unknown-linux-gnu/release/quack-navd \
    quack-nav/systemd/quack-navd.service quack-nav/systemd/sysusers.d/quack-nav.conf \
    quack-nav/quack-nav.example.toml radxa@192.168.1.42:/tmp/
# sulla papera
sudo install -m 755 /tmp/quack-navd /usr/local/bin/quack-navd
sudo install -m 644 /tmp/quack-navd.service /etc/systemd/system/quack-navd.service
sudo install -m 644 /tmp/quack-nav.conf /etc/sysusers.d/quack-nav.conf
sudo systemd-sysusers /etc/sysusers.d/quack-nav.conf
sudo install -D -m 644 /tmp/quack-nav.example.toml /etc/robot/quack-nav.toml   # solo la prima volta
sudo systemctl daemon-reload && sudo systemctl enable --now quack-navd
```

Per controllarlo:

```sh
systemctl status quack-navd
journalctl -u quack-navd -f
# da un utente del gruppo `robot` (o con sudo)
printf '{"jsonrpc":"2.0","id":1,"method":"nav.call","params":{"name":"robot.where_am_i","args":{}}}\n' \
    | nc -U -q1 /run/quack-nav/nav.sock
```

**Aggiornare**: di nuovo `scripts/install-on-duck.sh`, oppure a mano
`sudo systemctl stop quack-navd`, `sudo install -m 755 /tmp/quack-navd
/usr/local/bin/quack-navd`, `sudo systemctl start quack-navd` (fermarlo
salva prima la sessione di mappatura). **Un vecchio registro dei luoghi**:
la 0.1.0 lo teneva in `/var/lib/quacksat/places.json`, che non si legge
più — `sudo install -D -o quacknav -g quacknav -m 644
/var/lib/quacksat/places.json /var/lib/quack-nav/places.json`, poi un
riavvio. **Disinstallare**:

```sh
sudo systemctl disable --now quack-navd
sudo rm /usr/local/bin/quack-navd /etc/systemd/system/quack-navd.service /etc/sysusers.d/quack-nav.conf
sudo systemctl daemon-reload
# restano apposta: la configurazione, /etc/robot/quack-nav.toml, e le mappe
# e i luoghi, /var/lib/quack-nav/ — toglili (e `sudo userdel quacknav`)
# solo per dimenticare la casa
```

Lo script, l'unità e la disinstallazione sono stati provati in un container
Debian 13 arm64 avviato con systemd, con sshd, un utente `radxa` con sudo e
un gruppo `robot` (`systemd-analyze verify` passa; i socket nascono 0660
`quacknav:robot`; `nc -U` riceve una risposta da un utente di `robot`).
Non ancora su una scheda vera.

## Debito tecnico, e dove va

Detto chiaramente, così nessuno deve scoprirlo da sé: questo è un prototipo
misurato con rigore, non uno stack di navigazione all'altezza degli standard
del settore.

- **L'esploratore è un accumulo di regole.** Ognuna — la legge dei passaggi
  accanto a un drop, prima via dal bordo, i punti da evitare, gambe cieche e
  guardate, corsie, pavimento fidato — viene da una
  caduta o da uno stallo misurati sul gemello, e i perché sono nel codice e
  negli ADR. Insieme sono difficili da ragionare, e le loro soglie sono state
  tarate su tre case simulate (due generate): possono essere adattate al gemello.
- **Il codice lo mostra.** `explore/mod.rs` è di circa 2.000
  righe; 35 interruttori `QK_*` nell'ambiente (e 18 `MAPLOC_*`, tutti elencati in [`docs/knobs.it.md`](docs/knobs.it.md), generato dal codice); le gambe sono
  `serde_json::Value`; i recuperi decidono sui *messaggi* di errore
  (`why.contains("° right")` nella ricerca del ritorno a casa), che una frase riformulata rompe.
- **La localizzazione è fatta di soglie, non di confidenza.** Lo standard
  (AMCL, SLAM Toolbox, Cartographer) porta una covarianza; qui una posa è
  fidata o no. Il test della valle di maploc è un sostituto empirico
  dell'analisi di degenerazione di uno scan matcher.
- **La pianificazione non è a strati.** Nav2 ha un pianificatore globale, un
  controllore locale, una costmap a strati (ostacoli, inflazione, zone vietate)
  e i recuperi in un behavior tree. Qui: Dijkstra, un filo teso, gambe a
  stop-and-go, e recuperi sparsi nell'esploratore. Il libro dei drop è uno
  strato di costmap in tutto tranne che nel nome.
- **Test.** 153 test che passano e 1 ignorato (2026-10-01). Il gemello di carta gira in CI
  come soglia su semi fissi (esplorazione 40 × 1200 s, `go_to` 30;
  `.github/workflows/ci.yml`, `scripts/ci/paper_twin_gate.py`); oltre a
  quello, il comportamento si verifica con giri di ore, non deterministici,
  sul gemello MuJoCo.
- **Solo simulazione.** Il sensore, il pavimento e il passo veri sposteranno
  molti dei numeri.

Una parte è della papera: un sensore a tempo di volo 8×8 con 45° di campo, un
passo che non ruota sotto una certa velocità, la mappatura solo da fermi,
niente ROS a bordo — Nav2 così com'è qui non girerebbe. La direzione è tenere
il comportamento e dargli le forme del settore:

1. errori tipizzati al posto delle stringhe confrontate;
2. l'esploratore come macchina a stati (o behavior tree) di parti piccole e
   testate;
3. drop, corsie e punti da evitare come strati di costmap;
4. la confidenza della posa come misura (la matrice dell'informazione della
   scan match), non un sì o un no;
5. il gemello di carta in CI, con i criteri di release di `docs/results.md`
   come soglia (la soglia gira; le sue barre sono numeri a semi fissi, non
   quei criteri);
6. la papera fisica.

## Stato

Misurato sul gemello MuJoCo (`microduck_rl` + robotd); la papera fisica
arriva a dicembre 2026. Due modi di farlo girare:

- **robotd ufficiale** (daemon-v0.14.4) con `[maploc] enabled`: il
  mapper sta in `quack-navd`. È la configurazione della preview; i numeri
  sono in [`docs/results.it.md`](docs/results.it.md).
- **Un robotd che ospita maploc** — la PR 127 upstream, ancora aperta,
  più la libreria di mappe di `docs/study/upstream-asks.md` §5, che vive su
  un fork di `pollen-robotics/microduck` — con `[maploc]` spento.

daemon-v0.15.0 (API 37) è validato sul gemello solo sul branch
`microduck-015` (quattro sessioni per casa, nessuna regressione); `main`
resta fissato a daemon-v0.14.4.
