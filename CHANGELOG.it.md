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

### Aggiunto

- **Un pacchetto d'installazione a ogni release.** La CI impacchetta
  `quack-nav-<versione>-aarch64-linux.tar.gz` (con il suo `.sha256`)
  accanto al binario nudo, e un tag `v*` li allega entrambi: il binario,
  l'unità, l'utente di servizio, la configurazione d'esempio,
  `install-on-duck.sh` e un `README-install.it.md` passo per passo (e
  `.md`), così la papera si installa da un download, senza repository né
  compilazione (README, "Installare da una release").
  `scripts/package.sh <versione> <binario> <cartella>` lo impacchetta in
  locale. `install-on-duck.sh` gira dal pacchetto o da una copia del
  repository, accetta `--dry-run` (stampa ogni comando, non si collega) e
  non fallisce più con il bash 3.2 di macOS quando `SSH_OPTS` è vuoto.

### Modificato

- **In un viaggio, il naso contro qualcosa che la mappa non ha è uno
  stallo.** Due passi avanti di fila con il sensore di profondità che vede
  qualcosa nella corsia del corpo entro 0.15 m (nella maggior parte dei
  frame degli ultimi 0.6 s): lo stick gira di almeno 20° e registra ciò che
  ha visto, a meno che un muro della mappa sia entro 0.15 m o un drop sul
  libro entro 0.5 m. Sotto il tavolino di apartment il corpo spingeva sul
  bordo per minuti mentre la posa scivolava di 0.3 m (gemello MuJoCo). Il
  log del passo porta l'ostacolo nella corsia (`ahead`) e l'odometria.
- **Una corsia percorsa cede a un drop sul libro entro 0.20 m** (era
  0.12): il percorso del bagno di casa_arredata passava a 9–12 cm
  dall'angolo nord-est non registrato del vano scala, dove la papera del
  gemello è caduta due volte; ora 16 cm, nessuna tappa dei giri salvati
  più lunga o chiusa. `QK_LANE_YIELD_M` per misurare; `route_on_map`
  riporta il buco vero più vicino (`TRUTH_HOLES`) e stampa il percorso
  (`ROUTE_POINTS=1`). Vedi docs/results.it.md, "Sporgenze e angoli dei
  vani scala".

## [0.2.0-rc2] - 2026-10-03

Una seconda release candidate, ancora validata solo sui gemelli. Note di
rilascio: [docs/release-notes-v0.2.0-rc2.it.md](docs/release-notes-v0.2.0-rc2.it.md).

### Aggiunto

- **`robot.move` ha una guardia del dirupo, sempre accesa.** La
  mossa a tempo con cui guidano l'agente vocale ("vai avanti") e la scheda
  Avanzate di quack-control non aveva nessuna guardia: sul bordo di una
  scala ci cadeva. Ora, prima del primo comando e a ogni tick, si leggono
  i frame più recenti del sensore di profondità (anche quelli in cammino,
  la regola della gamba cieca: un buco vero, non il piede di un muro, due
  frame d'accordo) nella corsia della mossa (0,17 m di semilarghezza): un
  buco col bordo vicino entro 0,40 m chiude la mossa con uno zero
  esplicito, qualunque cosa dica la posa e con o senza mappa. La risposta
  lo dice: `{"done": false, "stopped": "a drop ahead (depth sensor): …",
  "walked_s": …, "cliff_guard": "on"}`; una mossa completa risponde
  `"done": true` come prima, con `cliff_guard`. Solo in avanti — il
  sensore guarda avanti e in basso: retromarcia e passi laterali sono
  `"not covered"`, una rotazione sul posto `"not judged"`, senza guardia o
  senza sensore `"off"`. La guardia è solo dei chiamanti (nav.call da un
  client: il front end vocale, quack-control, qualunque client del
  socket): le gambe dell'esploratore, il giro dei bordi, i viaggi e gli
  impulsi e le rotazioni del ritorno a casa si muovono con
  `tools::internal_move`, senza guardia come prima, con le loro guardie
  tarate per avvicinarsi a un bordo — con la guardia sempre accesa anche
  lì, un'esplorazione su casa_arredata (x25) si è vista fermare le gambe
  17 volte al buco del corridoio e si è sigillata. Sul gemello
  (casa_grande), di fronte al buco del soggiorno da 1,15 m: fermata dopo
  1,71 s col tronco a 0,56 m dal bordo, e ogni altra chiamata in avanti
  rifiutata sul bordo; nove chiamate su 4 m di pavimento libero, nessuno
  stop falso.
- **`robot.map_step` su una posa incerta giudica col solo sensore.** Con
  la posa persa o `untrusted` (spostata mentre si riposava) i controlli
  della mappa — il muro davanti, i lati del passaggio, lo scostarsi dal
  muro, il centrarsi, lo spazio libero nella risposta — sarebbero giudicati
  in un posto dove la papera può non essere: restano fuori, valgono solo i
  controlli del sensore di profondità (la guardia del dirupo, le cose in
  mezzo, "fermati prima" dove non ha guardato), e il nuovo `checks` della
  risposta dice `"position uncertain: checks from the sensor only"`
  (altrimenti `"map and sensor"`), `clearance` null, con un suggerimento.
- **Guidare la papera in giro a mano non le fa perdere la posizione**,
  verificato: il giudizio della sosta sul movimento è l'etichetta di passo
  di robotd, che il camminare di qualunque client imposta — il teleop di
  Pollen tramite robotd, senza che quack-nav lo sappia, quanto
  `robot.move` — così una guida sveglia la sosta come "motion" e la posa
  segue l'odometria, fidata; solo un movimento che l'andatura non spiega
  (odometria spostata oltre 5 cm o 0,15 rad senza camminare: un
  trasporto; una seduta; una caduta) la rende `untrusted`. Sul gemello: a
  riposo, guidata 16 s a 0,3 m/s tramite robotd a 50 Hz come `padd`
  (1,17 m netti, l'andatura che sbanda) — svegliata come "motion", mai non
  fidata, lo spostamento della posa della mappa entro 4 mm da quello vero,
  le finestre successive integrate. Il
  teleop di Pollen scavalca la guardia del dirupo di quack-nav, e robotd
  non ne ha una: detto nella nota di sicurezza del README e chiesto a monte
  ([docs/study/upstream-asks.it.md §8](docs/study/upstream-asks.it.md)).
- **Il movimento autonomo della papera è detto, e STOP lo ferma per
  davvero.** Quando la papera cammina da sola — la ricerca guarda-e-cammina
  del ritorno a casa all'avvio (nuovo `explore.state` `searching`),
  l'esplorazione che il ritorno a casa avvia (nessuna conferma, "starting a
  fresh map and exploring instead"; una sessione progressiva; dopo aver
  adottato la mappa salvata), la rilocalizzazione prima di un lavoro —
  `robot.map_status` lo dice (`explore.self_started`, il motivo, un
  suggerimento) e quack-navd lo scrive una volta nel log come avviso. Lo
  stop dell'utente (`robot.go_to`/`robot.map_explore` `{"stop": true}`, lo
  STOP di quack-control) ora copre tutto questo, compresa la ricerca
  all'avvio, e agisce prima del lucchetto del robot: la gamba in corso
  finisce entro un tick e la sosta è accorciata (sul gemello lo STOP ha
  risposto in 0,15 s e la papera ha proseguito 3 cm per inerzia; prima
  aspettava 7–8 s la fine del passo, 0,18–0,33 m camminati). Poi tiene —
  `explore.stopped_by_user`, "stopped by the user; not searching again
  until asked" — finché non si chiede un lavoro: il ritorno a casa non
  avvia più nulla. Un `robot.move` o `robot.map_step` mentre la papera si
  muove da sola ferma quel movimento e obbedisce (`stopped_own` nella
  risposta); dopo uno STOP funzionano subito. `robot.map_step` ora è
  rifiutato, come `robot.move`, mentre corre un lavoro chiesto
  dall'utente.
- **La sosta: le lunghe pause da ferma** (maploc). Un minuto ferma senza
  un lavoro che guidi il corpo (né `go_to`, né esplorazione, né giro dei
  bordi, né ritorno a casa), il mapper si riposa: nessuna finestra viene
  integrata o corregge la posa, la porta solo l'odometria, e una
  *guardia* — una finestra, la testa che spazza una volta (6 s) — viene
  giudicata contro la mappa all'inizio della sosta e ogni due minuti,
  senza applicare niente. L'accordo tiene la sosta; una posa che il
  match del tracciamento sposterebbe la sveglia perché la correggano le
  finestre; un residuo oltre 0,06 m, o 0,01 m oltre il migliore della
  sosta, la prima volta le sveglia anch'esso, e dopo, due volte di fila,
  rende la posa dubbia: persa, come la rende il watchdog, e cercata entro
  un metro. Un lavoro, un movimento o una spinta che nessuno ha comandato
  chiudono la sosta al primo tick (un `go_to` sul gemello: 0,12 s dalla
  chiamata). `robot.map_status` e `map.frame` portano `resting` e
  `rest_watch` (l'ultimo verdetto, il suo residuo e la sua età);
  quack-navd scrive nel log ogni sosta, guardia e risveglio.
  `MAPLOC_REST=0` tiene sveglia ogni sosta. La papera del gemello, in
  piedi, gira e scivola da sola (la sua rete in piedi, non quack-nav): in
  soste di trenta minuti ha girato fino a 139° ed è scivolata di
  12-22 cm, e la posa è rimasta in media a 5,5-7,9 cm dalla verità;
  quattro lunghe soste registrate, rigiocate a riposo e da sveglia, pari
  (errore medio 0,057 contro 0,088 m in una, 0,046 contro 0,037 in
  un'altra). quack-navd spende il 2,0-2,35 % di un core del Mac a riposo
  contro il 2,72 % fermo da sveglio; il replay, il 46 % di CPU in meno.
  Quaranta sessioni rigiocate, ATE come prima (media 0,0920 -> 0,0913 m).
  La papera può essere spostata ovunque mentre si riposa: due guardie
  contraddette dalla mappa, una spinta oltre un urtino (5 cm, 0,15 rad di
  odometria senza che nessuno la faccia camminare), una seduta o una caduta
  rendono la posa `untrusted` — cercata su tutta la mappa come a un avvio,
  mai ripresa non verificata, e solo quando un lavoro lo chiede: un
  `go_to` o un `map_explore` risponde allora `relocalizing: true`, la
  ricerca cammina-e-guarda del ritorno a casa trova la posa
  (`quack-nav/src/relocate.rs`, `explore.state` "relocalizing"), e il
  lavoro parte quando maploc la conferma — o fallisce senza aver camminato
  verso la meta. `untrusted` viaggia in `map.frame` e `robot.map_status`.
  Sul gemello: portata a 3,3 m in un'altra stanza, trovata in 70 s e
  arrivata a 0,02 m; un trasporto silenzioso di 0,35 m, trovata in 71 s,
  arrivata a 0,15 m; dopo ~16 h ferma, la posa a 1,38 m e non fidata, un
  `go_to` verso la cucina si è rilocalizzato in 97 s ed è arrivato a
  0,21 m. Il banco dei risvegli in localize (`WAKE_MODE=localize`, nuovo):
  28 risvegli su 28 confermati giusti, nessuno sbagliato, mediana 87 s.
  Dettagli in [docs/todo-map.it.md](docs/todo-map.it.md).
- **Luoghi da una vista della mappa**: `robot.list_places` dà di ogni luogo
  il suo `at` (l'ancora a cui va `go_to`, in metri della mappa);
  `robot.remember_place` con `x` e `y` dà un nome a un punto di pavimento
  mappato invece che a dove sta l'anatra. Ciò che serve alla pagina di
  quack-control per segnare i luoghi e nominare un punto toccato; nient'altro
  cambia.
- **Il contratto di controllo** ([docs/control-contract.it.md](docs/control-contract.it.md)):
  che cosa offre quack-navd a un client che lo gestisce — per prima la
  pagina web locale di quack-control. `nav.knobs` elenca le manopole che
  quack-navd legge e scrive il loro file d'ambiente,
  `/var/lib/quack-nav/knobs.env` (config `knobs_env`), che ora l'unit legge
  a ogni avvio (`EnvironmentFile=-`); `nav.restart` salva la sessione ed
  esce perché systemd riavvii il demone (`Restart=always`), e fuori da
  systemd lo dice. Sono metodi, non strumenti: un agente non li vede mai.
  `scripts/knobs.py` scrive anche `quack-nav/src/knobs.json`, la lista
  leggibile da una macchina (controllata da `--check`). Il gemello legge
  anche `$STATE/knobs.env`, e `scripts/twin/twin.sh restart-navd` riavvia
  solo quack-navd.
- **La telecamera del gemello**: con `VIEWER=on` il visore serve la
  telecamera della testa su `$STATE/media.sock` con la chiamata di mediad,
  `media.frame` (640×360 UYVY, ruotata di 90° come il montaggio vero), così
  quack-control legge il gemello e la papera con un solo adattatore
  (`scripts/twin/viewer/eye.py`).
- **Una GIF di un `go_to` sul gemello MuJoCo** in cima al README
  (`docs/media/go_to.gif`: casa_grande, dalla camera in fondo alla cucina,
  riprodotta a 6×).
- **Lo studio dell'app della mappa** ([docs/study/map-app.it.md](docs/study/map-app.it.md)):
  un'app alla robot aspirapolvere sul canale di Pollen; deciso il
  2026-10-01: prima una pagina locale sulla rete di casa (quack-control),
  il resto voci TODO datate.

### Modificato

- **La mappa ombra funziona in localize** (maploc): il suo mapper prendeva
  la configurazione della mappa viva, lì congelata, e non inchiostrava
  niente; ora non è mai congelato e non si riposa mai.
- **Una rilocalizzazione non inchiostra più la mappa congelata** (maploc,
  localize). La finestra che confermava una posa veniva inchiostrata
  mentre il mapper era ancora perso, così ogni avvio e ogni recupero
  aggiungeva una submap alla casa salvata (627 -> 628 su casa_grande del
  gemello), e una sosta cominciata dopo giudicava le sue finestre contro
  il proprio inchiostro. Trenta sessioni rigiocate su mappa congelata: ATE
  medio 0,1183 -> 0,1169 m, 4 meglio e 4 peggio di oltre 5 mm.
- **Fissato a daemon-v0.15.0** (API 37; prima daemon-v0.14.4, API 34):
  `duck-ipc-proto` e `kinematics` di Pollen a quel tag. Le API 35–37 sono
  aggiunte — `robot.state` porta le `velocities` misurate dei servo
  (rad/s) e `currents_ma` (corrente presente, mA), facoltative sul filo,
  e `update.status` due campi di `updaterd`; `kinematics` non cambia.
  Per la scheda: `quack-navd` si comporta come prima, contro robotd 0.15.0
  o su una scheda ancora alla 0.14.4 (lì i nuovi blocchi mancano e ancora
  nessuno li legge). robotd 0.15.0 li pubblica di default (`[control]
  publish_velocity_and_load`), circa il 10–12 % di byte in più per frame
  di stato. Il gemello lo usa (`scripts/twin/README.it.md`).
- **`robot.map_step` è rifiutato mentre corre un lavoro chiesto
  dall'utente**, come lo era già `robot.move` (e come documentato).
- **Il ritorno a casa non chiede più di esplorare una casa completa**
  dopo un'adozione.
- **Il banco dei risvegli** (`wake_bench.py`) può avviare la papera come
  fa la navigazione: `WAKE_MODE=localize`, la mappa congelata, niente
  esplorazione.
- **Insegnare un luogo è rifiutato** mentre la mappa viva è sconosciuta
  (il ritorno a casa non si è ancora assestato) e sulla mappa di ricerca
  del ritorno a casa.

### Rimosso

- La generazione del registro dei luoghi e la sua regola delle meno
  submap (un luogo diventava stantio ogni volta che robotd riportava meno
  submap di quante mai viste), sostituite dalla discendenza della mappa
  qui sotto; con esse `places::Place::generation` e `generation()` e
  `observe()` del registro.

### Corretto

- **Una casa dichiarata completa viene congelata dopo un'accensione,
  qualunque cosa dica `resume_explore`.** Il ritorno a casa congelava una
  casa completa solo con `resume_explore` acceso: in `stop_and_scan` senza,
  la papera tornava a casa su casa_grande e continuava a disegnare sulla
  mappa su cui navigava (627 → 630 sottomappe). Ora `robot.map_load` e
  `robot.map_adopt` congelano una casa completa appena la caricano, come
  `localize` congela ogni mappa (la posa cercata come quando si mappa,
  niente disegnato); la risposta porta `frozen`. Una sessione di
  `map_explore` sotto un altro `save_as` prima la scongela; `fresh` avvia
  una mappa nuova come prima. Sul gemello, due accensioni sulla mappa
  completa: congelata al caricamento, 495 sottomappe per 16 minuti e due
  go_to.
- **Una mappa ruotata rispetto alla casa non conta più l'ignoto oltre i
  muri.** La percentuale della casa, `left_m2` e il pezzo ignoto più
  grande della fine sessione contavano l'ignoto dentro il riquadro dei
  muri sugli assi della mappa; una mappa iniziata dove stava la papera è
  ruotata come la papera, e il riquadro prendeva gli angoli della griglia
  oltre i muri, uniti alla casa da ogni varco. "Esplora da capo" dalla
  cucina (una mappa a ~200° dalla casa) diceva 55 %, restano 37,9 m² e
  "bloccata" su un pezzo di 15,4 m². Ora il riquadro è ruotato sui muri
  (la rotazione che allinea meglio le celle di muro; gli assi della mappa
  restano finché le allineano almeno al 90 %), uno ruotato perde una
  cella per lato: la stessa mappa dice 83 %, 9,5 m², 2,2 m² (la mappa
  allineata della casa: 80 %, 11,6, 2,7), e le due ruotate di 15–290°
  sulla carta 82–83 %. Le mappe allineate non cambiano di una cella (le
  19 di x24 e x26, il cancello del gemello di carta).
- **La posa di una papera ferma non scivola più lungo un muro.** A una
  sosta, la correzione del tracciamento di ogni finestra ferma prendeva
  come prior la risposta della finestra prima, e correzioni di un
  centimetro si sommavano lungo una direzione che la scena fissa appena:
  su casa_grande (modo localize, dopo un `go_to`) la papera del gemello è
  rimasta quattordici minuti davanti a un solo muro lungo e la sua posa sulla
  mappa ci ha camminato lungo 1,56 m in 132 correzioni, ognuna a
  migliorare il residuo della sua finestra, finché ogni `go_to` falliva
  "no way to … on the map". Ora la sosta di maploc tiene il prior delle
  sue finestre, in posizione, dove la sosta è cominciata (quello
  dell'angolo resta della posa, e corregge la deriva d'angolo
  dell'odometria): rigiocata, la stessa registrazione resta entro 9 cm da
  dove si è fermata (prima 1,62 m). Quaranta sessioni rigiocate di dieci
  corse: ATE RMS medio 0,0913 -> 0,0920 m, 14 meglio e 14 peggio di oltre
  5 mm — rumore. Il `FROZEN=1` di `trajectory` ora congela anche la mappa
  che costruisce un caricamento `MAP_LOAD_AT_S`, e il suo `CORR_LOG` porta
  il condizionamento di ogni correzione.
- **I luoghi con nome sopravvivono a un'accensione.** Il registro rendeva
  stantio ogni luogo ogni volta che la mappa riportava meno submap di
  quante mai viste, e ogni avvio col ritorno a casa parte su una mappa
  nuova prima di caricare quella salvata: qualunque chiamata nel frattempo
  ("vai in cucina") li perdeva tutti — un "places: the map was reset" per
  sessione su casa_grande, `cucina` e `soggiorno` stantii dopo due riavvii
  sul gemello. Ora un luogo appartiene alla mappa su cui è stato insegnato
  (una discendenza che il registro tiene col nome della mappa salvata):
  `pending` finché la papera non è confermata su quella mappa, `usable`
  poi; `other_map` mentre è viva un'altra mappa salvata; `stale` solo se
  la sua mappa non c'è più (salvata sopra da una nuova, o cancellata senza
  salvarla). `robot.list_places` aggiunge `state`, `map` e `live_map`
  (`stale` resta, e vuol dire "non su questa mappa"), `robot.where_am_i`
  `pending_places`; `robot.go_to` dice perché un luogo con nome ora non si
  raggiunge. Un `places.json` di versione 1 si legge e si migra
  ([README-places](quack-nav/README-places.it.md#a-quale-mappa-appartiene-un-luogo)).
- docs/knobs.md e `nav.knobs` danno un default che nel codice è una
  costante col suo valore (`QK_DROP_INFLATE` 0.05, non `DROP_INFLATE_DEFAULT`),
  una manopola senza commento sopra la lettura prende la frase del suo file
  che la nomina, e le manopole dell'oracolo del gemello (`QK_ORACLE_*`)
  restano fuori da `nav.knobs`.
- Gli esempi di installazione entrano via ssh come `microduck`, l'account
  dell'immagine della scheda (#340 di Pollen), non `radxa`.
- Il testo della release su GitHub: la CI rende assoluti i link relativi
  delle note di rilascio, verso i file al tag (`scripts/ci/release_body.py`);
  sulla pagina della release non portavano da nessuna parte (corretti a mano
  sulla v0.2.0-rc1).

### Modifiche incompatibili per chi integra

- **robotd**: fissato a daemon-v0.15.0 (API 37). quack-navd gira ancora
  su una scheda alla 0.14.4.
- **`robot.move`** (chi chiama tramite `nav.call`): una mossa in avanti
  ora può finire prima o essere rifiutata davanti a un dislivello —
  `"done": false` con `stopped` e `walked_s`; ogni risposta porta
  `cliff_guard`, e `stopped_own` quando ha fermato il movimento autonomo
  della papera.
- **`robot.map_step`**: su una posa persa o `untrusted` i controlli della
  mappa restano fuori (`checks` dice quali valgono, `clearance` è null);
  rifiutato mentre corre un lavoro chiesto dall'utente; `stopped_own` come
  `robot.move`.
- **`robot.go_to` / `robot.map_explore`** su una posa non fidata
  rispondono `relocalizing: true` e partono solo quando la posa è
  confermata; uno stop ora ferma anche il movimento autonomo della papera
  e tiene finché non si chiede un lavoro.
- **`robot.map_status` e `map.frame`**: nuovi campi `resting`,
  `rest_watch`, `untrusted` (in `map.frame` `resting` e `untrusted`
  mancano quando sono falsi); `explore.state` ha due valori nuovi,
  `relocalizing` e `searching`; `explore` aggiunge `self_started` e
  `stopped_by_user`. Un client che elenca gli stati deve accettarli.
- **Luoghi**: `places.json` ora è alla versione 2; un file di versione 1
  si migra alla prima lettura, e la v0.2.0-rc1 rifiuta un file di
  versione 2 — tenetene una copia prima di aggiornare se potreste tornare
  indietro. `robot.list_places` aggiunge `state` (`usable`, `pending`,
  `other_map`, `stale`), `map`, `live_map` e `at`; il vecchio flag
  `stale` resta, ora vero anche per `other_map`. `robot.where_am_i`
  aggiunge `pending_places`. `robot.remember_place` accetta `x`, `y`, ed è
  rifiutato su una mappa viva sconosciuta.
- **`robot.map_load` / `robot.map_adopt`** congelano una casa dichiarata
  completa appena la caricano, e la risposta porta `frozen`;
  `robot.map_explore` su una casa così è rifiutato come prima, una
  sessione sotto un altro `save_as` la scongela.
- **Metodi nuovi** `nav.knobs` e `nav.restart` (non in `nav.catalog`);
  l'unit legge `/var/lib/quack-nav/knobs.env` (`EnvironmentFile=-`, il
  nuovo `knobs_env` della configurazione) e tiene `Restart=always` —
  rilanciate `scripts/install-on-duck.sh` per avere l'unit nuova.
- **API Rust**: `places::Place::generation` è `map` (una discendenza);
  `generation()` e `observe()` del registro non ci sono più.

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
