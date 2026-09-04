# TODO — Mappa della casa e localizzazione

Stato: deciso, non iniziato (ADR 0005). Prerequisito: quacksat parla
(fatto) e il `maploc` di Pollen entra in robotd (PR upstream 127, aperta).
Principio: **mappatura e localizzazione girano a bordo, dentro robotd**;
quacksat consuma la mappa via IPC come client non privilegiato (modello
padd) e possiede solo lo strato sopra: luoghi con nome, strumenti per
l'agente e, più avanti, la semantica.

Sostituisce la versione del 2026-08-31 di questo file, che assumeva uno
SLAM visivo fuori bordo su server GPU. Quella pista è retrocessa a fase
finale opzionale; il perché è nell'ADR 0005.

## Cosa fornisce (o fornirà) upstream

- Sottocrate `maploc/` ospitato da robotd (PR 127): SLAM 2D a submap sul
  ToF 8×8 più odometria di contatto, chiusura dei loop, pose graph,
  rilocalizzazione MCL, pianificatore A* e follower (gli ultimi due
  portati ma dormienti). `[maploc]` in robotd.toml, spento di default;
  sessione persistita e ripristinata al boot; sweep della testa opzionale
  a ogni fermata.
- IPC (API v17 nella PR): sottoscrizione `robot.map` → notifiche
  `map.frame` a ~1 Hz con posa nel frame mappa, flag `tracking`,
  origine/passo della griglia, griglia ternaria (ignota/libera/muro) in
  base64, `n_submaps`, `n_loops`, `windows`, `still`, `seated`.
  `robot.map_wipe` azzera la sessione.
- Già in main: `kinematics::tof::Reprojector` (filtro pavimento, raggi
  consapevoli della posa della testa) e l'IK dello sguardo `robot.look`
  (PR 126).
- Validato sul gemello MuJoCo (PR 202): posa tracciata entro ~6 cm dalla
  verità, odometria grezza in deriva fino a 0,35 m.

## 0. Seguire upstream
- [ ] Seguire le PR 127 e 202 fino al merge; annotare la versione API
      finale e ogni modifica a `MapFrame`/`MapStreamResult`.
- [x] Studio dettagliato del flusso dati di maploc (cosa entra in robotd,
      come viene elaborato, cosa esce) → `docs/study/maploc-dataflow.md`
      + `.mermaid` (2026-09-04).
- [x] Riprodurre il bench offline sul Mac: esempi `evaluate`/`replay` di
      `maploc` sulle registrazioni `.mdlg` committate (2026-09-04: compila
      in secondi, Rust puro; tutte e quattro le righe della tabella della
      PR 202 riprodotte al millimetro; giro pulito 0,031 m di errore medio
      sui muri, 8 submap, 0 loop; il giro specchiato riprodotto con
      `MAPLOC_MIRROR_COLS=1` chiude 3 loop).
- [x] Provare il percorso del gemello MuJoCo (`scripts/duck-sim` +
      `sim-maploc/`) sul Mac: l'unico modo, prima di dicembre, di vedere
      una mappa costruirsi dal vivo (2026-09-04: funziona — `robotd --sim`
      e `tofd --sim` veri + corpo MuJoCo, giro stop-and-scan di 245 s,
      59 finestre, 7 submap, posa tracciata a ~4 cm dalla verità al
      ritorno, 0,035 m di errore medio sui muri; lo stream `map.frame` dal
      vivo letto da un semplice client socket).

## 1. quacksat consuma la mappa
- [x] Sottoscrizione `robot.map` in quacksat-core (client robotd):
      decodificare `map.frame`, tenere l'ultimo frame, esporre posa +
      tracking + griglia. Controllare la versione API; un robotd senza
      `robot.map` risponde METHOD_NOT_FOUND e la funzione resta spenta in
      silenzio (2026-09-04: `quacksat-core/src/map.rs`, config `[map]`,
      cablato nel binario, esempio `map_watch`; entrambi i percorsi
      verificati dal vivo contro il gemello MuJoCo e un `robotd --fake`
      di main).
- [ ] Supporto `robotd --fake`: verificare se il finto serve `robot.map`;
      altrimenti una fixture che riproduce una sequenza di frame
      registrata.
- [x] Rilevare "il frame mappa è cambiato" (wipe, ripristino fallito,
      rilocalizzazione dopo un reset di sessione) e invalidare tutto ciò
      che vi è ancorato (2026-09-04: `MapStatus::epoch`, incrementato su
      una regressione di `seq` o sul conteggio delle submap che scende a
      zero — conservativo, perché il filo non porta un id di sessione;
      anche un riavvio che ripristina la sessione lo incrementa).

- [x] Indipendenza operativa dal satellite vocale: la lane mappa, il
      registro e i quattro strumenti vivono nel crate `quack-places`
      (2026-09-04), senza dipendenza da quacksat-core — ospitabile da un
      demone proprio o spostabile in un repository a sé senza modifiche.
      I trasporti (bridge, server MCP) restano in quacksat; estrarre il
      server MCP in un crate condiviso è l'unico passo che manca a un
      demone autonomo. La unit systemd recinta le risorse (Nice,
      CPUWeight, limiti di memoria) così il satellite non costa mai a
      robotd il suo loop.

## 2. Luoghi e `where_am_i` (senza telecamera, senza server)
- [x] Registro dei luoghi: pose con nome nel frame mappa, indicizzate
      per identità della sessione di mappa, salvate nella directory di
      stato di quacksat. Insegnate a voce ("questa è la cucina") o
      dall'agente (2026-09-04: `quacksat-core/src/places.rs`, JSON in
      `[map] places_path`, più ancore per nome, una generazione
      persistita che diventa stantia a un reset della mappa — l'epoca
      della lane mappa o un numero di submap sotto il massimo visto dal
      registro).
- [x] Strumenti per l'agente: `where_am_i()` → luogo più vicino +
      distanza + confidenza del tracking; `list_places()`,
      `remember_place(name)`, `forget_place(name)`. Esporli nella
      allowlist del bridge, nel server MCP lato anatra e nel backend
      `direct` (2026-09-04: nell'unico catalogo servito da ogni percorso —
      bridge via session.start, MCP lato anatra, direct; gli strumenti
      agiscono su un `tools::Robot` che possiede la lane robotd, la lane
      mappa e il registro. Verificato dal vivo via MCP contro il gemello
      MuJoCo: insegna, riconosce, si allontana, wipe → stantio,
      reinsegna).
- [x] Passeggiata di mappatura come comportamento guidato: l'agente (o
      l'utente) conduce un giro stop-and-scan; quacksat riferisce a
      parole il progresso di `windows`/`n_submaps`. Richiede
      `[maploc] enabled = true` sul robot (2026-09-04: `robot.map_status`
      in quack-places — numeri più un suggerimento — e `robot.map_step`
      in quacksat-core — una camminata a tempo poi una sosta di 6 s di
      default, che riferisce `new_windows`; l'agente concatena le tappe
      e racconta. La sosta sta dentro lo strumento così la fermezza è
      garantita a prescindere dalla latenza dell'LLM; una tappa sta nei
      30 s di timeout del bridge. Verificato dal vivo sul gemello
      MuJoCo, che ha insegnato due cose: la policy di cammino non fa
      passi sotto circa 0,25 m/s comandati, quindi il tetto del movimento
      è passato da 0,2 al valore del gamepad di Pollen, 0,3; e un passo
      alla cieca contro un muro non ancora inchiostrato ha ribaltato
      l'anatra — ora `map_status`/`map_step` riferiscono lo spazio libero
      nelle quattro direzioni dalla griglia, e `map_step` rifiuta di
      camminare contro un muro mappato o in spazio non mappato a un palmo
      dal becco. L'anatra si è rialzata da sola e maploc si è
      rilocalizzato).
- [x] Gestire con onestà `seated`/`tracking = false` nelle risposte
      ("non sono sicura di dove sono, devo alzarmi e guardarmi intorno")
      (2026-09-04: `where_am_i` risponde `known: false` con il motivo;
      l'insegnamento viene rifiutato. Dettaglio visto sul gemello: prima
      di `robot.enable` robotd riferisce `seated = false` anche con
      l'anatra seduta — il flag viene dal controller, che esiste solo
      dopo l'enable — quindi la posa di una mappa nuova è "fidata" al
      boot; il flag driving di `robot.state` potrà filtrarla in seguito).

## 3. `go_to` (serve un RPC di goal upstream)
- [ ] Seguire upstream per un RPC tipo `robot.goto` (pianificatore e
      follower esistono nel crate, non sono cablati). Se entro dicembre
      non compare nulla, proporlo come PR sul repo Pollen con l'anatra in
      mano.
- [ ] Strumento `go_to(place)` sopra di esso: pianifica, segue, riferisce
      arrivo o fallimento; l'evitamento ToF di M9 è compito di upstream,
      non nostro.
- [ ] `look_at` tramite il `robot.look` esistente.

## 4. Più avanti, opzionale: semantica dalla telecamera (fuori bordo)
- [ ] Solo se luoghi + `where_am_i` si rivelano insufficienti: frame da
      mediad (`get_frame` o WebRTC), un server locale che etichetta ciò
      che l'anatra vede e arricchisce il registro dei luoghi ("cucina:
      forno, frigo"). Il video di casa non lascia mai il server locale.
- [ ] Scene graph a vocabolario aperto e `where_is(object)` /
      `describe_surroundings()` restano in questa fase.

## Rischi e domande aperte
- La PR 127 è senza review e in conflitto con main: la forma dell'IPC
  può ancora cambiare. Costruire contro una versione API fissata,
  aspettarsi un bump.
- La rilocalizzazione al boot non è ancora cablata in robotd: le
  etichette dei luoghi sopravvivono solo quanto la sessione salvata. Il
  registro va indicizzato per sessione e deve tollerare un reset.
- La mappatura stop-and-scan è lavoro deliberato: qualcuno deve portare
  l'anatra in giro con delle pause. Progettare il giro guidato, non darlo
  per scontato.
- Passo delle celle, estensione della mappa e costo CPU sull'RK3566 vanno
  misurati sull'hardware (dicembre); maploc è "la cosa più affamata di
  CPU che il robot possa fare", e la wake word di quacksat condivide gli
  stessi quattro core.
- Più anatre: una mappa per robot per ora; una mappa condivisa è un
  problema di upstream, se mai arriverà.
- Nessun file di licenza su `microduck_maploc_rs`; il codice dentro il
  repo Pollen è Apache-2.0. Lo consumiamo via IPC, non lo incorporiamo.
