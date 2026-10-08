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

![Un go_to sul gemello MuJoCo](docs/media/go_to.gif)

*Un `go_to` sul gemello MuJoCo, casa_grande, sulla mappa che la papera ha
esplorato da sola: 5,2 m in linea d'aria dalla camera in fondo alla
cucina, percorsi in 123 s e mostrati 6 volte più veloci. La linea
tratteggiata è il percorso fatto, la linea davanti alla papera la rotta
pianificata, le macchie rosse i dislivelli che ha registrato
(2026-10-01).*

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
>
> Ogni mossa che manda quack-nav — `robot.move` e `robot.map_step`
> compresi — si ferma davanti a un dislivello che il sensore di profondità
> vede (solo le mosse in avanti). **Il teleop di Pollen (il gamepad, la
> console) non passa da quack-nav, e robotd non ha una sua protezione dai
> dislivelli**: guidata così la papera scende da una scala come cammina sul
> pavimento (una richiesta a monte,
> [docs/study/upstream-asks.it.md §8](docs/study/upstream-asks.it.md)).
> Guidarla in giro a mano non le fa perdere la posizione: il camminare
> spiega il movimento; solo un trasporto, una seduta o una caduta le fanno
> cercare dov'è.

Release attuale: **v0.2.0-rc2**, una release candidate validata sui gemelli —
[note di rilascio](docs/release-notes-v0.2.0-rc2.it.md) (rc1: [note](docs/release-notes-v0.2.0-rc1.it.md)), [changelog](CHANGELOG.it.md).

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
  dopo un riavvio: torna a casa, congela la mappa (appena la carica, con o
  senza `resume_explore`) e naviga — alla cieca dove la mappa conosce il
  pavimento, con la guardia dove non lo conosce.
  `robot.go_to` verso un posto con nome o un punto; `robot.remember_place`
  dà un nome a dove si trova.
- **Quando si muove da sola.** Dopo un'accensione la papera può camminare
  per conto suo — cercando dov'è sulla mappa salvata, esplorando per
  riconoscere la casa, ritrovandosi prima di un lavoro dopo essere stata
  spostata. Permesso, ma detto: `robot.map_status` →
  `explore.self_started` e il motivo (`explore.state` `searching`
  all'avvio), un avviso nel log, un suggerimento. **STOP** —
  `robot.go_to {"stop": true}`, il pulsante di quack-control — la ferma
  subito e tiene: niente la rimette in moto da sola in quell'accensione
  finché non si chiede un lavoro. Un `robot.move` o `robot.map_step` mentre
  si muove da sola ferma quel movimento e obbedisce (`stopped_own` nella
  risposta); dopo uno STOP funzionano subito — su una posa persa o non
  fidata giudicati dal solo sensore di profondità, con la guardia del
  dirupo di `robot.move` accesa.

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
- **Lunghe soste da ferma** (2026-10-01): dopo un minuto senza un
  lavoro il mapper si riposa — l'odometria porta la posa, ogni due minuti
  una finestra lunga una spazzata della testa è giudicata contro la mappa,
  niente viene corretto — e un lavoro, un movimento o una spinta lo
  svegliano al primo tick (un `go_to`: 0,12 s). In soste di trenta minuti
  sul gemello, la cui papera in piedi gira e scivola da sola, la posa è
  rimasta in media a 5,5-7,9 cm dalla verità, più o meno quanto da
  sveglia, con meno CPU. Spostata mentre si riposa — una guardia
  contraddetta dalla mappa, una spinta, una seduta — la posa non è più
  fidata, e il prossimo `go_to` cammina e guarda finché non trova dov'è
  prima di partire (portata a 3,3 m in un'altra stanza: trovata in 70 s,
  arrivata a 2 cm).
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
`duck-ipc-proto` e `kinematics` di Pollen (tag daemon-v0.16.1).

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

[gait]
profile = "velstand"    # the walk robotd runs: "velstand" (Pollen's default since
                        # policy set v5) or "alpha" (alpha_walking + alpha_stand)
```

Con `[maploc]` acceso, `quack-navd` fa girare da sé il `maploc` di Pollen
(il crate `maploc` di questo workspace): legge `robot.state` e lo stream
di profondità di tofd, muove la testa a ogni sosta e serve la mappa su
`/run/quack-nav/map.sock` nel dialetto `robot.map*` di robotd. In robotd
non cambia niente — la release (dalla daemon-v0.14.4 in poi) pubblica
tutto ciò che serve al mapper. Spento, la mappa arriva da un robotd che ospita maploc da sé.

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

### Installare da una release

Niente copia del repository e niente compilazione: ogni release dalla
v0.2.0-rc2 in poi porta un pacchetto d'installazione,
`quack-nav-<versione>-aarch64-linux.tar.gz`, con il suo `.sha256` (quello
della v0.2.0-rc2 è stato aggiunto dopo, attorno al suo stesso binario; la
v0.2.0-rc1: solo il binario nudo). Contiene il binario per
la scheda, l'unità, l'utente di servizio, la configurazione d'esempio,
`install-on-duck.sh` e un `README-install.it.md` passo per passo (inglese:
`README-install.md`). La papera dev'essere già preparata da microduck
(robotd, tofd, il gruppo `robot`); al tuo computer servono ssh, scp, tar e
shasum.

```sh
V=0.2.0-rc2     # il tag della release senza la v
gh release download "v$V" --repo andreagenovese/quacknav \
    --pattern "quack-nav-$V-aarch64-linux.tar.gz*"
# oppure: curl -LO https://github.com/andreagenovese/quacknav/releases/download/v$V/quack-nav-$V-aarch64-linux.tar.gz
#         (e lo stesso URL con .sha256)
shasum -a 256 -c "quack-nav-$V-aarch64-linux.tar.gz.sha256"   # stampa OK
tar xzf "quack-nav-$V-aarch64-linux.tar.gz" && cd "quack-nav-$V"
./install-on-duck.sh --dry-run microduck@192.168.1.42   # facoltativo: stampa ogni comando, non si collega
./install-on-duck.sh microduck@192.168.1.42
```

Lo script trova i suoi file accanto a sé (`bin/quack-navd`, `systemd/`,
`quack-nav.example.toml`) e fa ciò che descrive
[Installare sulla papera](#installare-sulla-papera): binario, unità e
utente sostituiti, `/etc/robot/quack-nav.toml` installato solo se non c'è,
il servizio abilitato e riavviato.

**La configurazione**, sulla papera (`sudo nano /etc/robot/quack-nav.toml`,
poi `sudo systemctl restart quack-navd`). L'esempio va bene per una papera
standard; cosa guardare su una vera:

| chiave | nell'esempio | quando cambiarla |
|---|---|---|
| `robotd_socket` | `/run/robotd.sock` | robotd ascolta altrove |
| `[map] tof_socket` | `/run/tofd/tof.sock` | tofd ascolta altrove (lo leggono la guardia dei dislivelli e il mappatore) |
| `[maploc] mode` | `"stop_and_scan"` | la mappa cresce a ogni sosta; `"localize"` quando la casa è mappata (la mappa resta com'è salvata, la posa si corregge su di essa) |
| `[maploc] map_path` | `/var/lib/quack-nav/maploc.session` | la sessione di lavoro; le mappe con un nome sono in `maps/` accanto |
| `[map] places_path` | `/var/lib/quack-nav/places.json` | i luoghi con un nome |
| `[homecoming] enabled`, `resume_explore` | `true`, `true` | spenti: la papera non fa nulla da sé all'avvio, né riprende a esplorare dopo una ricarica |
| `socket` | `/run/quack-nav/nav.sock` | dove quacksat e quack-control trovano quack-navd |
| `[gait] profile` | `"velstand"` | `"alpha"` quando robotd cammina con `alpha_walking` + `alpha_stand` (vedi sotto) |

I percorsi devono restare sotto `/var/lib/quack-nav/` o `/run/quack-nav/`:
l'unità non lascia scrivere il demone altrove. Una chiave che il demone non
conosce lo ferma con un messaggio che la nomina (`journalctl -u
quack-navd`). Tutte le chiavi e i loro predefiniti:
`quack-nav/src/config.rs`.

**La camminata.** quack-nav deve sapere quale policy di camminata usa
robotd, e `[gait] profile` lo dice. Le papere di Pollen escono con
**velstand** (una sola rete che cammina e sta in piedi; dal set di policy
v5 in poi). **alpha** (`alpha_walking` + `alpha_stand`) è ancora nel set, e
ogni numero prima del 2026-10-08 è stato misurato con lei; per usarla,
sulla papera:

```sh
robotctl policy load walk alpha_walking.onnx
robotctl policy load stand alpha_stand.onnx
```

e qui `profile = "alpha"` (il default quando `[gait]` manca). Entrambe sono
validate sul gemello MuJoCo con daemon-v0.16.1: 48 viaggi su 48 con
velstand, 47 su 48 con alpha, la posa entro 6–7 cm (mediana) —
[docs/results.it.md](docs/results.it.md), "daemon 0.16.1 e velstand". Il
profilo porta i numeri della camminata (velocità, rotazione, quanto il
corpo impiega a fermarsi dopo uno stop); un profilo non cambia mai l'altro.

**Controllarlo**: `systemctl status quack-navd`, `journalctl -u quack-navd
-f`, e la chiamata `nc -U` sotto
[Installare sulla papera](#installare-sulla-papera). **Aggiornare**: il
pacchetto della release più nuova, verificato e scompattato, il suo
`./install-on-duck.sh` allo stesso modo; configurazione, mappe e luoghi
restano. **Disinstallare**: i comandi sotto
[Installare sulla papera](#installare-sulla-papera).

### Compilare per la papera

Non serve compilare: la CI compila `quack-navd` per la scheda a ogni push
(l'artifact `quack-navd-aarch64-linux` del job `aarch64`: il binario nudo e
il pacchetto d'installazione, ciascuno con il suo sha256, impacchettato da
`scripts/package.sh <versione> <binario> <cartella>`), e ogni tag `v*` li
allega alla
[release su GitHub](https://github.com/andreagenovese/quacknav/releases)
([Installare da una release](#installare-da-una-release); resta il binario
nudo `quack-navd-aarch64-linux`, da copiare in `/usr/local/bin/quack-navd`).
Per compilarlo da sé:

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
| `/var/lib/quack-nav/knobs.env` (facoltativo) | le manopole, scritte da `nav.knobs` ([docs/control-contract.it.md](docs/control-contract.it.md)), lette dall'unità a ogni avvio |

L'unità esegue il demone come `quacknav` con `robot` come gruppo
supplementare (raggiunge i socket 0660 di robotd e di tofd, e dà i suoi
due a `robot`), a nice 5, sotto i 320 MB, con il filesystem in sola
lettura tranne la sua directory di stato.

Un solo comando dalla macchina di sviluppo, dopo `scripts/cross-build.sh`:

```sh
scripts/install-on-duck.sh microduck@192.168.1.42
```

`microduck` è l'account dell'immagine della scheda (nei documenti di
Pollen e in `duckctl` dal 2026-10-01; le immagini più vecchie avevano
`radxa`).

Copia i quattro file con `scp`, poi con `sudo` sulla papera installa il
binario, l'unità e l'utente, installa la configurazione solo se
`/etc/robot/quack-nav.toml` non c'è (una già modificata resta), copia un
vecchio `/var/lib/quacksat/places.json` se `/var/lib/quack-nav/` non ne ha
uno, e abilita e riavvia il servizio, stampando ogni comando che esegue.
Rilanciato, è l'aggiornamento. `SSH_OPTS="-p 2222"` passa opzioni a ssh e
scp; `--dry-run` stampa ogni comando, compreso lo script che lancerebbe
sulla papera, e non si collega a niente; un secondo argomento installa un
altro binario (un percorso da dove lo si lancia). Da una copia del
repository prende i file dal repository, da un pacchetto di release
scompattato quelli accanto a sé (cerca `bin/quack-navd` vicino a sé). A
mano, lo stesso:

```sh
# sulla macchina di sviluppo
scp target/aarch64-unknown-linux-gnu/release/quack-navd \
    quack-nav/systemd/quack-navd.service quack-nav/systemd/sysusers.d/quack-nav.conf \
    quack-nav/quack-nav.example.toml microduck@192.168.1.42:/tmp/
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
`quacknav:robot`; `nc -U` riceve una risposta da un utente di `robot`), da
una copia del repository e, dal 2026-10-03, da un pacchetto scompattato
senza repository (installazione, aggiornamento, vecchi luoghi copiati,
disinstallazione). Non ancora su una scheda vera.

## Il controllo da un browser

Una pagina sulla rete di casa che mostra la mappa dal vivo e guida
l'anatra — tocca e vai, stop, luoghi, esplorazione, le manopole — è
**quack-control**, un repository a sé (deciso il 2026-10-01; vedi
[docs/study/map-app.it.md](docs/study/map-app.it.md), "Decisione
2026-10-01"). Gira sull'anatra accanto a quack-navd e parla con i suoi due
socket; questo repository tiene solo il contratto che usa, aperto a
qualsiasi altro client: [docs/control-contract.it.md](docs/control-contract.it.md)
— i socket, `nav.catalog` e `nav.call`, il flusso della mappa, `nav.knobs`
(il file d'ambiente delle manopole, `/var/lib/quack-nav/knobs.env`, che
l'unit legge a ogni avvio) e `nav.restart` (salva la sessione ed esce,
perché systemd riavvii il demone con quelle).

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
  righe; 35 interruttori `QK_*` nell'ambiente (e 19 `MAPLOC_*`, tutti elencati in [`docs/knobs.it.md`](docs/knobs.it.md), generato dal codice); le gambe sono
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
- **Test.** 180 test che passano e 1 ignorato (2026-10-02). Il gemello di carta gira in CI
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

- **robotd ufficiale** (daemon-v0.16.1) con `[maploc] enabled`: il
  mapper sta in `quack-navd`. È la configurazione della preview; i numeri
  di [`docs/results.it.md`](docs/results.it.md) sono stati misurati sulla
  daemon-v0.14.4.
- **Un robotd che ospita maploc** — la PR 127 upstream, ancora aperta,
  più la libreria di mappe di `docs/study/upstream-asks.md` §5, che vive su
  un fork di `pollen-robotics/microduck` — con `[maploc]` spento.

`main` è fissato a daemon-v0.16.1 (API 41) dal 2026-10-08, validato sul
gemello con entrambe le camminate (branch `microduck-016`: quattro giri
per casa, nessuna regressione con alpha; velstand altrettanto bene);
prima a daemon-v0.15.0 (API 37) dal 2026-10-01. Le sue aggiunte sono facoltative sul filo,
quindi lo stesso `quack-navd` gira anche su una scheda ancora alla
daemon-v0.14.4.
