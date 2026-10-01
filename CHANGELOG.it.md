# Changelog

Le modifiche rilevanti di quack-nav. Il formato segue
[Keep a Changelog](https://keepachangelog.com/it-IT/1.1.0/); le versioni
seguono il [Semantic Versioning](https://semver.org/lang/it/) (prima della
1.0 una versione minore può rompere qualcosa). Copia inglese:
[CHANGELOG.md](CHANGELOG.md).

Ogni numero qui sotto è stato misurato sul gemello MuJoCo o su quello di
carta, mai su una papera fisica; i dettagli sono in
[docs/results.it.md](docs/results.it.md) e
[docs/todo-map.it.md](docs/todo-map.it.md).

## [Unreleased]

### Corretto

- Il testo della release su GitHub: la CI rende assoluti i link relativi
  delle note di rilascio, verso i file al tag (`scripts/ci/release_body.py`);
  sulla pagina della release non portavano da nessuna parte (corretti a mano
  sulla v0.2.0-rc1).

## [0.2.0-rc1] - 2026-10-01

Una release candidate: tutto lo stack validato sui gemelli. Note di
rilascio: [docs/release-notes-v0.2.0-rc1.it.md](docs/release-notes-v0.2.0-rc1.it.md).

### Aggiunto

- **Il mapper ospitato in quack-navd** (ADR 0007, 2d495a9). `maploc`,
  derivato dalla PR upstream 127 di Pollen, è un crate del workspace; con
  `[maploc] enabled` `quack-navd` lo fa girare sopra il robotd *rilasciato*
  di Pollen (fissato a daemon-v0.14.4, API 34), muove la testa alle soste
  e serve la mappa su `/run/quack-nav/map.sock` nel dialetto `robot.map*`
  di robotd. Il thread del worker gira a priorità più bassa del demone.
- **La posa a 20 Hz** (e92aa14): un `map.pose` leggero ogni 50 ms tra i
  `map.frame` a 1 Hz; l'errore di direzione dal vivo, campionato, è sceso da
  17° RMS a circa 2°.
- **L'incertezza della posa** (735b564): una covarianza 3×3 (x, y, yaw),
  riportata da `robot.map_status` come `pose_uncertainty`. Per ora non
  cambia nessuna decisione.
- **Ritorno a casa e rilocalizzazione**: la posa salvata portata dal moto
  stesso del risveglio (5b4a041); la ricerca all'avvio chiede la conferma a
  ogni quarto di giro della scansione (708a791); una perdita cerca vicino
  alla posa portata dall'odometria (baf1124); il watchdog prova un match
  locale prima di chiamare una finestra una contraddizione (89c5abe).
- **La regola di adozione** (3b5d4df): una mappa salvata si adotta con
  sovrapposizione ≥ 0,50 e margine ≤ 0,50, chiedendo ogni 60 s, tre risposte
  concordi. Su 27 risvegli rigiocati: passano 626 risposte giuste (prima
  565) e nessuna sbagliata (prima 9).
- **La mappa ombra** (10f5a22, 2247634, 9c37473): una papera ripartita
  persa su una mappa salvata tiene una mappa nuova del suo cammino e ogni
  30 s chiede dove si incastra. Sul banco dei risvegli del gemello 23
  risvegli su 24 confermati giusti, nessuno sbagliato, mediane 87–123 s
  (prima 126–192 s). `MAPLOC_SHADOW=0` la spegne.
- **L'esplorazione una carica alla volta** (ADR 0008, 2f905c4): una sessione
  per carica, la mappa salvata col suo nome, il progresso a libro;
  `robot.map_status` riporta `house.percent_mapped`; `map_explore complete`
  chiude la mappa, `map_explore fresh` chiede `confirmed` prima di una
  mappa nuova. Una mappa finita è congelata e la papera ci naviga sopra.
- **La regola di fine sessione** (fec3863): una sessione senza frontiere a
  portata e senza pezzi di ignoto da 4,5 m² o più a contatto col pavimento
  noto trova la casa mappata (le mappe arredate tengono sempre celle di
  frontiera lungo muri e mobili).
- **I viaggi sul loro anello** (1adc36b, `explore/navigate.rs`): budget,
  posa, arrivo e il bastone (f98b42b: virate chiuse sul yaw
  dell'odometria, una sosta dopo ogni virata e ogni 0,4 m), predefinito da
  9e84781; lo usano sia `robot.go_to` sia gli spostamenti dell'esplorazione
  verso le frontiere (67cde18).
- **Gemelli**: la verità dei muri di house2 (763170e); le case generate
  casa_libera, casa_arredata e **casa_grande** (d60e067, 9016e8c: 9 × 7 m,
  sette stanze, un corridoio che gira di 90°, due buchi, mai usata per
  tarare nulla); script e visore del gemello in `scripts/twin/`.
- **Strumenti di misura**: ATE/RPE (`traj_metrics.py`), il banco di replay
  deterministico (`maploc/examples/trajectory.rs`), `wake_bench.py`,
  `maploc/examples/wake_match.rs`, `quack-nav/examples/drop_replay.rs`,
  `map_vs_truth.py`, `room_fit.py`, `LOOP_LOG`; le registrazioni `.mdlg`
  timbrano gli orologi di robotd e tofd, così un replay gira come dal vivo
  (f38b341).
- **CI** (429ba16): test unitari, rotte golden, test di proprietà, una
  regressione su replay e la soglia del gemello di carta su semi fissi a
  ogni push; da a5c83e8 anche `scripts/knobs.py --check`.
- **`docs/knobs.it.md`**: ogni variabile d'ambiente che il codice legge,
  generata dal codice.
- **Il binario della scheda dalla CI**: un job `aarch64` compila in cross
  `quack-navd` a ogni push (artifact `quack-navd-aarch64-linux` con il suo
  sha256, simboli glibc controllati contro il minimo 2.31); un tag `v*` lo
  allega alla release su GitHub, come pre-release se il tag contiene `-rc`.
- **Compilazione cross per la scheda** (`scripts/cross-build.sh`):
  `cargo zigbuild` per `aarch64-unknown-linux-gnu` con la glibc minima
  fissata a 2.31, senza Docker; il binario richiede glibc 2.30 ed è stato
  fatto girare in un container Debian 13 arm64. README, "Compilare per la
  papera".
- **Installare sulla papera** (`scripts/install-on-duck.sh <utente@host>`):
  binario, unità, utente di servizio e — solo se manca — la configurazione
  (`quack-nav/quack-nav.example.toml`, nuovo) via ssh, un vecchio
  `/var/lib/quacksat/places.json` copiato, il servizio abilitato e
  riavviato; rilanciato, aggiorna. README, "Installare sulla papera", con i
  comandi a mano, i controlli, l'aggiornamento e la disinstallazione;
  provato in un container Debian 13 arm64 con systemd, non ancora su una
  scheda.

### Modificato

- **Interruttori rinominati: `QUACKSAT_*` → `QK_*`, senza alias** (81d466f,
  a5c83e8). I vecchi nomi non si leggono più.
- **Da 169 a 122 interruttori** (b7e11ee, 9975422, 76295c5, b1e776e): gli
  interruttori degli esperimenti conclusi sono stati tolti, ciascuno al suo
  valore predefinito misurato; `QK_*` da 72 a 35, `MAPLOC_*` da 26 a 18. I
  nomi tolti sono elencati in cima a [docs/todo-map.it.md](docs/todo-map.it.md).
- **Le chiusure d'anello correggono la direzione di 4° al massimo**
  (c6aae96, `MAPLOC_LOOP_CAP_YAW` predefinito da 0,45 a 0,07 rad). Venti
  sessioni rigiocate: ATE RMS medio da 0,0946 a 0,0891 m. Sul gemello
  l'errore mediano di direzione è passato da 0,97–1,26° a 0,63–0,92°; mappa
  e viaggi invariati.
- **Il socket del demone è `/run/quack-nav/nav.sock`** (era
  `/run/quack-nav.sock`; 7334989, ADR 0006): sotto systemd sta nella
  `RuntimeDirectory` dell'unità, dato al gruppo `robot` a 0660.
- **`places_path` ha come predefinito `/var/lib/quack-nav/places.json`**,
  senza ripiego su `/var/lib/quacksat/` (a5c83e8).
- **Fissato a daemon-v0.14.4** (990d165; la 0.1.0 usava daemon-v0.10.0).
- La rotta percorsa con lo spago tirato al massimo 0,6 m, mai accanto a un
  dislivello (3f2d00b); la virata sul posto oltre la zona morta della gait
  (b8cebfe).
- Il visore del gemello raggiunge il demone con `QUACK_NAV_SOCKET`.

### Rimosso

- **Il vecchio percorso delle gambe dell'esploratore** (9975422):
  `walk_leg`, le gambe guardate, il sigillo e l'allargamento, con 29
  interruttori. Girava solo con `QK_EXPLORE_NAV=0`.
- **Il vecchio percorso dei viaggi** (1fffbac): ogni viaggio usa
  `navigate.rs`.
- Esperimenti misurati e non adottati: la traversata, la memoria del bordo,
  la costmap a strati e i muri assottigliati (e26fa4a); il planner locale
  DWA (a2708e9); Regulated Pure Pursuit (e5e7aaa); il filtro a particelle
  di maploc all'avvio e il giudice lungo il raggio (b1e776e).
- API pubblica che nessuno chiamava (76295c5, b1e776e, 8fa4f28):
  `maploc::Mapper::boot_search`, `ExploreHandle::forget_ground`,
  `Grid::unknown_around`, `Control::request_method`, i limiti di testa e
  sguardo e `HOLD_STRAIGHT_MAX` in `quack_duck::body`, `frontier::waypoint`.
- `QUACK_NAV_MCP` / `QUACK_NAV_MCP_TOKEN` del visore del gemello (per poco
  `QUACKSAT_MCP` / `QUACKSAT_TOKEN`), sostituiti da `QUACK_NAV_SOCKET`
  (solo script del gemello).

### Corretto

- Il riancoraggio di una sessione ripresa porta con sé l'arco
  dell'odometria che vi entra (8d5ffeb): rigiocato, errore dei muri da 6,9 a
  4,6 cm in media, peggior errore di traiettoria da 1,95 a 0,16 m.
- La posa di una rilocalizzazione non viene trascinata via dal
  congelamento causato dal suo stesso salto (7e2825b): 20 sessioni
  rigiocate, ATE medio da 0,1045 a 0,0975 m.
- Un sensore coperto non è un buco (953285b): spariti 18 buchi fantasma sul
  letto dell'appartamento.
- Due valli che si incrociano si risolvono a vicenda (54a49a9).
- Nessuna mappa salvata mentre la papera è persa; nessun avvio riprende su
  una posa che niente ha potuto giudicare (41e5ca3); dopo una caduta il
  lavoro aspetta la posa (dfa044c).
- "Esplorazione completa" è definitiva, e una mappa finita resta finita dopo
  un `go_to` (8643067, 0954d3d); dopo `fresh`, `map_explore` riporta la
  mappa nuova (4aa11ba).
- La ricerca all'avvio del ritorno a casa non salva mai sopra la casa
  (e8d9482).
- Gli errori all'avvio nominano il loro percorso: un socket che non si
  riesce a creare (quello di navigazione, quello della mappa), un file di
  configurazione illeggibile o che non si interpreta, e gli avvisi non
  fatali (robotd irraggiungibile, il registro dei luoghi, la sessione
  salvata) dicono quale file e cosa fare — lanciato a mano senza
  `/run/quack-nav/`, il demone diceva solo `No such file or directory`.

### Modifiche incompatibili per chi integra

- Ambiente: ogni interruttore `QUACKSAT_*` si legge solo come `QK_*`; gli
  interruttori tolti vengono ignorati in silenzio (elenco in
  docs/todo-map.it.md).
- Socket e percorsi: `nav.sock` spostato in `/run/quack-nav/nav.sock`;
  `places.json` si legge solo da `places_path` (predefinito
  `/var/lib/quack-nav/places.json`) — un file vecchio va spostato a mano.
- Configurazione: `[map] explore_turn` si carica ancora ma è ignorato.
- API Rust: `Mapper::boot_search` e le altre voci qui sopra non ci sono
  più; `explore::Job::new`, `to_goal`, `start` e `start_goto` hanno perso
  la mano di virata (`Job::new(known, max_s, ask, now)`).
- Log: la riga di stato di maploc non porta più il campo `boot`.
- Visore del gemello: `QUACK_NAV_MCP*` sostituito da `QUACK_NAV_SOCKET`.

## [0.1.0] - 2026-09-22

La navigazione separata da quacksat (ADR 0006, 747b233) con la sua storia:
`quack-duck` (la corsia di robotd) e `quack-nav` (il client della mappa del
`maploc` di robotd, la guardia del dirupo, il planner su mappa dei costi,
il registro dei posti, l'esploratore, il ritorno a casa, i dodici strumenti
`robot.*`, il gemello di carta e `quack-navd` su `/run/quack-nav.sock`),
sopra daemon-v0.10.0. 60 test.
