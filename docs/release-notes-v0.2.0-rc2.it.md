# quack-nav v0.2.0-rc2 — note di rilascio

2026-10-03. Copia inglese: [release-notes-v0.2.0-rc2.md](release-notes-v0.2.0-rc2.md).
L'elenco completo delle modifiche è in [CHANGELOG.it.md](../CHANGELOG.it.md); le
note della candidata precedente sono in [release-notes-v0.2.0-rc1.it.md](release-notes-v0.2.0-rc1.it.md).

## Cos'è

Una **seconda release candidate, ancora validata solo sui gemelli** — il
gemello MuJoCo del Microduck (ora il robotd rilasciato di Pollen
daemon-v0.15.0 e il corpo di `microduck_rl`) e il gemello di carta — **non
ancora su una papera vera**. La papera fisica arriva a dicembre 2026. Fino a
una prima sessione su di lei, ogni numero qui sotto è del gemello, non una
promessa.

La rc1 ha fatto sì che la papera mappi, torni a casa e navighi. La rc2
riguarda le ore in mezzo, quando la papera sta ferma, viene presa in braccio
o guidata a mano — e ciò che serve da lei a una pagina di controllo.

## Cosa è cambiato dalla rc1

- **La sosta, e ritrovarsi prima di un lavoro.** Una papera lasciata ferma
  per un minuto senza niente da fare ora *si riposa*: la posa la porta
  l'odometria, niente viene corretto, e ogni due minuti una finestra (la
  testa che spazza una volta) viene giudicata contro la mappa. Se la mappa
  non è d'accordo due volte, o la papera viene portata, seduta o cade, la
  posa diventa `untrusted`. La papera non se ne va in giro a cercarsi: il
  prossimo `go_to` o `map_explore` prima cammina e guarda finché non sa
  dov'è (`relocalizing`), poi parte — o fallisce senza aver camminato verso
  la meta.
- **La correzione dello scivolamento.** La posa di una papera ferma non
  cammina più lungo un muro lungo: in una sosta le correzioni si sommavano
  una sull'altra nella direzione che il muro fissa appena; ora sono
  ancorate a dove la sosta è cominciata.
- **I luoghi appartengono alla loro mappa.** Un luogo con nome è legato
  alla mappa su cui è stato insegnato, così un'accensione non rende più
  stantio ogni luogo ("vai in cucina" rifiutato dopo ogni avvio). Un luogo
  è `usable`, `pending` (la papera non si è ancora ritrovata su quella
  mappa), `other_map` o `stale` (la sua mappa non c'è più). `places.json`
  passa alla versione 2 e si migra da solo.
- **robotd 0.15.0.** I crate di Pollen sono fissati a daemon-v0.15.0 (API
  37, solo aggiunte); quack-navd si comporta come prima su una scheda alla
  0.14.4.
- **Il contratto di controllo.** Ciò che usa un client che gestisce
  quack-navd — per primo [quack-control](https://github.com/andreagenovese/quack-control),
  la pagina web locale che mostra la mappa e guida la papera — scritto in
  [control-contract.it.md](control-contract.it.md): i due socket, il flusso
  della mappa, i luoghi da una vista della mappa (`at`, un luogo insegnato
  in un punto toccato), `nav.knobs` (il file d'ambiente delle manopole che
  l'unit legge a ogni avvio) e `nav.restart`. Il visore del gemello ora
  serve la telecamera della testa con la chiamata di mediad, così la
  pagina legge il gemello e la papera allo stesso modo.
- **Le guardie delle mosse e il movimento autonomo.** `robot.move` da un
  chiamante (il "vai avanti" dell'agente vocale, la scheda Avanzate di
  quack-control) tiene sempre accesa la guardia del dirupo del sensore di
  profondità: un buco entro 0,40 m davanti la ferma, con o senza mappa.
  `robot.map_step` su una posa incerta giudica col solo sensore. Quando la
  papera si muove da sola (la ricerca del ritorno a casa, la sua
  esplorazione, una rilocalizzazione) lo dice (`self_started`), e lo STOP
  dell'utente ferma tutto entro un tick e tiene finché non si chiede un
  lavoro. Guidare la papera a mano — anche col teleop di Pollen — non le
  fa perdere la posizione.
- **Una mappa completa resta congelata, le mappe ruotate si contano
  giuste.** Una casa dichiarata completa è congelata appena viene caricata
  o adottata, così un'accensione in `stop_and_scan` non disegna più sulla
  mappa su cui naviga. Una mappa iniziata dove stava la papera, ruotata
  rispetto alla casa, non conta più come casa l'ignoto oltre i muri ("55 %,
  bloccata" è diventato 83 %).
- **Il README si apre con una GIF** di un `go_to` sul gemello MuJoCo.

## Risultati principali

Sul gemello MuJoCo (casa_grande salvo dove detto); fonti in
[todo-map.it.md](todo-map.it.md) §2d, le sue voci dal 2026-10-01 al
2026-10-03.

- **Tornare a casa, alla maniera della navigazione.** Il banco dei
  risvegli avviato come fa la navigazione (`WAKE_MODE=localize`; 14 punti
  di partenza su casa_grande e casa_arredata, poi gli stessi girati di
  180°): **28 risvegli su 28 confermati giusti**, nessuno sbagliato,
  **mediana 87 s** (75–141 s), 0,01–0,15 m dalla verità, 0 cadute.
- **Lo scivolamento da ferma.** La registrazione in cui una sosta di
  quattordici minuti ha portato la posa 1,56 m lungo un muro, rigiocata: la
  posa resta entro **9,4 cm** da dove si è fermata (prima **1,62 m**).
  Quaranta sessioni rigiocate: ATE come prima (rumore).
- **La sosta.** Una sosta di trenta minuti dopo un `go_to`, con la papera
  del gemello che in piedi gira e scivola da sola: errore della posa
  **medio 10,0 cm, peggiore 17,2** (da sveglia, dieci minuti nello stesso
  punto: medio 12,0, peggiore 18,2); un `go_to` l'ha svegliata 0,13 s dopo
  la chiamata. quack-navd il 2,0–2,35 % di un core del Mac a riposo contro
  il 2,72 % da sveglio.
- **Spostata mentre si riposava.** Portata a 3,3 m in un'altra stanza e
  girata di 86°: non fidata subito, trovata in 70 s, arrivata a 0,02 m.
  Dopo **~16 h** ferma, la posa a 1,38 m e non fidata, un `go_to` verso la
  cucina **si è rilocalizzato in 97 s** (6 passi) ed **è arrivato a
  0,21 m** dalla cucina vera.
- **La mappa** (x26, il protocollo final-house da zero, entrambe le case):
  muri sulla verità **98 %**, pavimento noto 94–96 %, **0 dislivelli
  fantasma**.
- **Una vita in stop_and_scan** (avvio sulla mappa completa salvata,
  "esplora da capo" dalla cucina, `complete`, poi due riaccensioni
  altrove): **9/9 `go_to` arrivati** (0,09–0,30 m dalla meta), **0
  cadute**, l'esplorazione ha dichiarato la casa **completa da sola
  all'86 %**. Con la mappa completa congelata, due accensioni hanno tenuto
  495 sottomappe per 16 minuti e due `go_to`.
- **Le guardie.** Di fronte a un buco da 1,15 m, un `robot.move` si è
  fermato col tronco a 0,56 m dal bordo; nove chiamate su pavimento libero,
  nessuno stop falso. Uno STOP durante la ricerca all'avvio risponde in
  0,15 s con 3 cm di inerzia (prima 7–8 s e fino a 0,33 m).
- **La soglia del gemello di carta** (CI, semi fissi): identica byte per
  byte alla rc1 — esplorazione 40 giri, 0 cadute, copertura media 53,2 %;
  `go_to` 30/30. 182 test passano (1 ignorato).

## Compilare, installare, far girare

Il binario per la scheda è allegato a questa release (`quack-navd-aarch64-linux`,
con il suo sha256), compilato dalla CI con `scripts/cross-build.sh`.

Segui il [README](../README.it.md#farlo-girare). Per la scheda (Radxa Zero
3, aarch64, Debian 13): [Compilare per la papera](../README.it.md#compilare-per-la-papera),
poi `scripts/install-on-duck.sh microduck@<papera>` lo installa o lo
aggiorna con la sua unità systemd ([Installare sulla papera](../README.it.md#installare-sulla-papera);
l'account dell'immagine della scheda è `microduck` dal #340 di Pollen) —
provato in un container con systemd, non ancora su una scheda. Il gemello
MuJoCo è in [scripts/twin/README.it.md](../scripts/twin/README.it.md).

## Aggiornare dalla rc1

- **robotd**: fissato a daemon-v0.15.0 (API 37). Una scheda ancora alla
  0.14.4 funziona allo stesso modo.
- **L'unità**: rilanciate `scripts/install-on-duck.sh` — l'unità nuova
  legge `/var/lib/quack-nav/knobs.env` (`EnvironmentFile=-`; il nuovo
  `knobs_env` della configurazione nomina lo stesso file).
- **Luoghi**: `places.json` viene riscritto alla versione 2 al primo uso
  (un file di versione 1 si migra: i luoghi correnti aspettano la prima
  mappa salvata su cui la papera è confermata). La rc1 rifiuta un file di
  versione 2: tenetene una copia se potreste tornare indietro.
  `robot.list_places` aggiunge `state`, `map`, `live_map` e `at`; il
  vecchio flag `stale` ora vuol dire anche `other_map`.
  `robot.remember_place` accetta `x`, `y` ed è rifiutato mentre la mappa
  viva è sconosciuta.
- **`robot.move`** (chi chiama): una mossa in avanti può finire prima o
  essere rifiutata davanti a un dislivello — `"done": false`, `stopped`,
  `walked_s`; ogni risposta porta `cliff_guard`, e `stopped_own` quando ha
  fermato il movimento autonomo della papera.
- **`robot.map_step`**: su una posa persa o `untrusted` valgono solo i
  controlli del sensore (`checks`, `clearance` null); rifiutato mentre
  corre un lavoro chiesto dall'utente.
- **`robot.go_to` / `robot.map_explore`**: su una posa non fidata la
  risposta porta `relocalizing: true` e il lavoro parte dopo la conferma
  della posa; `{"stop": true}` ferma anche il movimento autonomo della
  papera e tiene finché non si chiede un lavoro.
- **Campi di stato**: `robot.map_status` e `map.frame` aggiungono
  `resting`, `rest_watch` e `untrusted`; `explore.state` può essere
  `relocalizing` o `searching`; `explore` aggiunge `self_started` e
  `stopped_by_user`.
- **`robot.map_load` / `robot.map_adopt`** congelano una casa dichiarata
  completa e rispondono `frozen`; `robot.map_explore` su di essa è
  rifiutato come prima, una sessione sotto un altro `save_as` la scongela.
- **API Rust**: `places::Place::generation` è diventato `map`;
  `generation()` e `observe()` del registro non ci sono più.

## Limiti noti

- **Uno STOP senza niente in corso tiene comunque fermo il ritorno a
  casa**: non avvia niente da sé finché non si chiede un lavoro.
- **Il bastone ripete un passo tornato subito** (rifiutato, robotd perso,
  il fermo dell'utente): lo legge come uno stallo, tre come un urto, e
  mette a registro un ostacolo al naso.
- **Una posa non fidata resta tale fino a un lavoro**, anche quando la
  papera viene rimessa dov'era: il prossimo `go_to` cerca prima.
- **I gradini al posto dei buchi non sono provati**: un gradino in
  discesa o in salita non è mai stato provato sul gemello.
- **Gli ostacoli temporanei vanno rimisurati** sul percorso attuale dei
  viaggi, e cosa succede quando uno viene tolto di nuovo.
- **robotd non ha uno stop per i dislivelli**: il teleop di Pollen guida
  tramite robotd, quindi scavalca la guardia del dirupo di quack-nav e può
  cadere da una scala (chiesto a monte,
  [study/upstream-asks.it.md](study/upstream-asks.it.md) §8).
- **La papera del gemello deriva da ferma**: la policy di stand del
  simulatore la fa girare di circa 0,1°/s e scivolare (8,5 cm in nove
  minuti) con un comando nullo; se lo faccia la papera vera non è misurato,
  e i numeri della sosta qui sopra lo includono.
- **quacksat non parla da sé**: gli arrivi si dicono solo se chiesti, e il
  movimento autonomo della papera non viene annunciato (quack-navd non ha
  un canale per spingerlo).
- **La CPU sull'RK3566 non è misurata**, come il passo delle celle e
  l'estensione della mappa sull'hardware.
- Ancora aperti dalla rc1: la posa può fermarsi ~1,5 s nelle virate
  veloci, i risvegli lenti a est della tromba delle scale
  dell'appartamento, la covarianza non è ancora un allarme. Vedi
  [todo-map.it.md](todo-map.it.md).

## Cosa serve per la v0.2.0 finale

Una prima sessione sulla papera vera: lo stack che gira sull'RK3566 sopra il
robotd rilasciato, con le guardie accese e qualcuno accanto, e i suoi numeri
scritti accanto a quelli del gemello. La checklist di quella sessione:
[first-duck-session.it.md](first-duck-session.it.md).
