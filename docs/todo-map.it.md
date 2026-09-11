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
- [x] Audit delle perdite di posa sul twin → `docs/study/maploc-audit.it.md`
      (2026-09-06): l'odometria del twin è quasi verità, maploc traccia in
      dead reckoning e le sue chiusure di loop spostano la posa al livello
      del rumore di mappa finché il watchdog non grida al rapimento; una
      matrice di bench su cinque registrazioni e una correzione scan-to-map
      della posa (opt-in) sul branch `maploc-study` del worktree. Prossimo
      passo: ricompilare robotd con l'allowance stretto per le chiusure,
      rifare le due regressioni, segnalare a Pollen.
- [x] robotd ricompilato da `maploc-study` con l'allowance stretto per le
      chiusure (0.03 m per submap, tetto 0.30) e le due regressioni rifatte
      (2026-09-06, run 57): 30 min, 37 m, zero tracking lost, zero cadute,
      errore di posa rispetto alla verità mediano 0.12 m / massimo 0.28 (run
      49 su upstream: 0.3–0.5 m dal minuto 5, lost al 15); giro 7/9, zero
      lost, "ingresso" riconosciuto a 0.29 m. Le manovre sul posto coniano
      ancora submap nello stesso punto (86 submap, 189 chiusure nel giro di
      12 minuti, posa mediana 0.23 m): prossimo passo, legare la
      plausibilità di una chiusura alla distanza realmente percorsa fra le
      due submap, non all'indice di submap.
- [x] Esploratore: i tre difetti dietro il sud inesplorato (2026-09-06,
      diagnosticati sul paper twin con le decisioni dell'esploratore
      registrate per stanza del bersaglio): (1) "dritto se libero" guardava
      solo i muri mappati, quindi puntava attraverso la scala e la guardia
      del dislivello rifiutava; ora la corsia dritta evita anche ogni
      dislivello e ostacolo locale registrato. (2) Con il bersaglio dietro e
      senza spazio per un arco (il varco est contro il mobiletto: 84 rifiuti
      in un punto) l'esploratore non aveva una rotazione sul posto; ora un
      kick-then-spin chiuso sullo yaw, tre per punto prima che conti come
      rifiuto. (3) La corsia del dislivello (±0.30 m) non entrava in nessuno
      dei due passaggi accanto alla scala (0.44 e 0.54 m); ora ±0.22, e un
      dislivello registrato occupa 0.10 m nel costmap invece di 0.20. Trovati
      strada facendo: una tappa accettata che non muove il duck (fianco
      contro qualcosa sotto la portata minima del sensore) ora è un rifiuto,
      non una tappa ripetuta 270 volte; un dislivello registrato non viene
      mai dimenticato dal recupero "unseal"; nessuna retromarcia cieca con un
      dislivello a fianco o dietro; il guard del passo giudica un arco anche
      lungo il rumbo di partenza (la caduta del run 58: un arco il cui rumbo
      finale il sensore non aveva mai spazzato è finito nella scala); e ogni
      tappa, compresi i kick di head-for-space e della rotazione, viene prima
      simulata col modello dell'andatura contro i dislivelli registrati
      (margine 0.15 m). Paper twin, 30 semi: rifiuti mediani 184 → 41,
      copertura 29.6 → 32.7 %, cucina 39 → 60 %, urti 27 → 20, bagno
      raggiunto in 2 run e corridoio sud in 4 (mai prima), zero cadute.
      MuJoCo run 59: copertura 33 % (run 57: 26 %), 2 rifiuti in 28 min
      (87), zero cadute, un lost recuperato in 6 s; giro 6/9 con zero lost e
      zero cadute, i tre mancati alla porta della cucina al ritorno (la
      guida in linea retta dello script del giro, non l'esploratore).
      Ancora chiuso: il sud dietro la scala su MuJoCo.
- [x] Il passaggio accanto alla scala, secondo giro (2026-09-06 pomeriggio):
      il paper twin ha mostrato un dislivello fantasma registrato 17 cm
      fuori dal buco (il gestore del rifiuto registrava il dislivello *più
      vicino* a qualunque angolo, un avvistamento stantio di una sosta,
      alla posa corrente) — ora si registra quello nella corsia davanti; la
      regola grossolana "nessuna retromarcia cieca con un dislivello a
      fianco" lasciava un twin fermo 25 minuti nel passaggio ovest (muro
      davanti, buco a fianco, nessuna uscita) — ora la retromarcia viene
      simulata col modello dell'andatura contro i dislivelli registrati,
      come una tappa; e il "frontiere rimaste ma nessuna raggiungibile" del
      run 59 erano due punti del sensore alla porta est (montante, spigolo
      del mobiletto) che sigillavano un varco di 0.42 m da 2.4 m di
      distanza — gli ostacoli registrati a più di 1 m ora possono essere
      dimenticati quando sono loro, e non la mappa, a sigillare il resto
      (tre volte per run). Bloccare le celle di frontiera vicine ai
      dislivelli è stato provato e scartato: mandava il duck prima a sud e
      costava la cucina. Paper twin, 30 semi: copertura e rifiuti come
      prima (33 %, 42), corridoio sud oltre metà in 10 run (5), bagno in 3
      (2), zero cadute. MuJoCo run 60: 27 % in 30 min, 10 rifiuti, zero
      cadute, zero lost, posa mediana 0.10 m; l'unica tappa verso il
      passaggio ovest è stata rifiutata dalla guardia sul percorso (una
      tappa curva che piegava verso il bordo registrato) e il sud è rimasto
      chiuso. Punto di ritorno dello stato del mattino in
      `private/drives/savepoints/explorer-ok-2026-09-06/`.
- [x] Passaggio accanto a un dislivello (2026-09-06 pomeriggio, via
      dell'utente): con un dislivello registrato entro 1 m e il percorso
      pianificato che passa in un varco di 0.43–0.9 m fra un muro mappato e
      i dislivelli, l'esploratore gira sul posto fino all'asse del passaggio
      (direzione del percorso 0.4–1.0 m avanti, tenuta finché i dislivelli
      restano vicini), poi tappe dritte di 1.5 s con una centratura dolce
      fra muro e dislivelli (`steer: false` sul passo, così lo "scosta dal
      muro" del guard non piega la tappa nel buco, come faceva). Per il
      pianificatore un dislivello vale 0.17 m di raggio (più i 0.15 del
      costmap), così il passaggio est di 0.44 m non viene più pianificato;
      il margine della guardia sul percorso è 0.10 m più i 0.10 del
      dislivello (lo 0.20 dell'utente). La rotazione conta un giro a destra
      come un giro a sinistra per il resto del cerchio (l'andatura gira a
      sinistra qualunque sia il segno): chiusa sullo yaw nell'altro verso
      si fermava rivolta al muro sbagliato con il buco alle spalle. Provati
      e scartati: corsia del dislivello più stretta (0.18: il twin urta
      ovunque), ricerca dell'asse per spazio libero (una caduta), blocco
      delle frontiere vicino ai dislivelli. Paper twin, 30 semi: mediana
      33 % come prima, terzo quartile 41 (34), rifiuti 48 (43), urti 15
      (24), corridoio sud oltre metà in 21 run (10), bagno in 8 (4), zero
      cadute, ma 7 run sotto il 25 % (3): l'asse preso dal percorso che
      piega verso il buco nel passaggio, e un avvistamento stantio di
      dislivello prodotto dai frame sempre freschi del twin (artefatto del
      twin). MuJoCo run 63: 42 % in 30 min (record; 62: 34, 59: 33), bagno
      68 %, corridoio sud 94 %, soggiorno 53 %, zero cadute, zero lost,
      posa mediana 0.08 m.
- [x] Sera (2026-09-06): allineamento chiuso sullo yaw per l'ingresso nel
      passaggio, dopo una sonda della rotazione sul twin (i comandi a tempo
      variano del triplo, le rotazioni a destra funzionano, coda 5–10°);
      ogni dislivello visto dal sensore va nei registri dopo una sosta (da
      frame di ≤ 2 s); per il pianificatore un dislivello vale 0.12 m.
      maploc (`maploc-study`): test di unicità nella ricerca di
      relocalizzazione (secondo bacino), accordo fra due ricerche, e resa
      dopo 8 finestre persi con ripresa dall'odometria — i replay dei run
      64 e 65 non si relocalizzano più a 3 m. Paper twin: il modello del
      sensore ora data i frame alla sosta e aggiunge un frame centrato
      fresco (i frame sempre freschi producevano dislivelli fantasma). Nuova
      metrica: budget 90 minuti, completo = ogni stanza ≥ 80 % del suo
      tetto, tempo di completamento, zero cadute → 20/30 completi,
      copertura al tetto (55 %), corridoio sud 30/30, bagno 24/30, zero
      cadute; ma il tempo di completamento è il budget: le fessure tengono
      il duck a girare (147 m). Prossimi: rimandare le frontiere piccole e
      finire quando restano solo quelle; i 6 semi che non vanno a sud e i 4
      sigillati a nord; `robot.go_to` sulla mappa finita.
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

**Come misuriamo (adottato il 2026-09-08).** Un singolo run MuJoCo non
distingue dieci punti di copertura dal rumore: nei run 70–77 lo stesso
esploratore ha segnato fra il 31 % e il 53 %. Quindi copertura, rifiuti,
metri camminati e permanenza nelle stanze si decidono sul gemello di carta
con **novanta semi per condizione, appaiati seme per seme** (il gemello è
deterministico per seme, quindi lo stesso seme nelle due condizioni è la
stessa casa e la stessa fortuna); una differenza conta quando i semi che
migliorano superano nettamente quelli che peggiorano, non quando si sposta
una mediana. MuJoCo resta per ciò che la carta non sa modellare — cadute,
pose perse, stipiti, gli errori del sensore di profondità — e per la
sicurezza, dove una sola caduta è un risultato. Entrambi i gemelli devono
riportare zero cadute prima di chiamare qualcosa un miglioramento.

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
      rilocalizzato. Poi un giro di due stanze sul gemello — corridoio,
      cucina e ritorno, 26 tappe, un rifiuto, nessuna caduta, "ingresso"
      riconosciuto a 26 cm al ritorno — e il vano scala: la mappa ha
      fermato l'anatra a 36 cm dal bordo solo perché il pavimento sopra
      la buca era *ignoto*; visto un muro al di là, i raggi che
      attraversano la buca la segnano libera. I frame di profondità
      invece la vedono (riga bassa 43–47 cm sul pavimento, 100–119 cm o
      nulla sopra la buca, 44 cm attesi), quindi `quack-places` ha un
      **guardiano del vuoto** (`cliff.rs`): legge lo stream di tofd e la
      posa della testa, riproietta con il crate `kinematics` di Pollen, e
      chiama dislivello un ritorno mancante o lungo 1,5× dove dovrebbe
      esserci il pavimento; `map_status` lo riferisce e `map_step`
      rifiuta di camminarci verso. Inoltre: margine di 25 cm dai muri, e
      un muro a meno di 20 cm su un lato fa sterzare la tappa.
      `robot.move` resta senza margine apposta: per avvicinarsi a qualcosa
      e prenderla bisogna arrivarci accanto).
- [x] Gestire con onestà `seated`/`tracking = false` nelle risposte
      ("non sono sicura di dove sono, devo alzarmi e guardarmi intorno")
      (2026-09-04: `where_am_i` risponde `known: false` con il motivo;
      l'insegnamento viene rifiutato. Dettaglio visto sul gemello: prima
      di `robot.enable` robotd riferisce `seated = false` anche con
      l'anatra seduta — il flag viene dal controller, che esiste solo
      dopo l'enable — quindi la posa di una mappa nuova è "fidata" al
      boot; il flag driving di `robot.state` potrà filtrarla in seguito).

- [x] Mappa tutto (2026-09-04): `robot.map_explore` — esplorazione a
      frontiere in `quack-places/src/frontier.rs` (gruppi di frontiera,
      percorsi BFS con muri gonfiati, un waypoint per tappa) guidata da
      un lavoro in background in `quacksat-core/src/explore.rs` che
      cammina con `map_step` (quindi con tutti i guardiani) su una sua
      lane robotd, blocca le frontiere che non riesce a raggiungere,
      arretra dai dislivelli, e si ferma quando non resta nulla di
      raggiungibile. Raggiunta una zona senza nome lascia una domanda; il
      backend `direct` la pronuncia (`ask_phrase`) e la risposta finisce
      in `remember_place`. Il backend `agent` ha bisogno di un evento di
      protocollo per lo stesso — ancora da fare.
- [x] Esploratore messo a punto sul gemello (2026-09-04, corse 22–26):
      tre strati, come i robot lavapavimenti. La mappa pianifica
      (Dijkstra su una costmap con muri gonfiati di 0,15 m, pavimento
      ignoto più caro del noto); il sensore risponde solo per ciò che la
      mappa non sa (un rilevamento diventa un ostacolo locale di 0,05 m
      più gonfiaggio — con 0,10 sigillava un corridoio da 0,4 m accanto
      a un mobiletto); una sosta rimappa l'ignoto. La tappa è
      dimensionata sul pavimento davanti: margine frontale 0,25 m più
      0,10 m di tolleranza dell'andatura, e un *test di corridoio* — il
      corpo è largo 0,19 m (scafo del gemello), con 0,06 m di aria per
      lato un corridoio deve essere largo 0,31 m — che gira verso il lato
      più largo invece di rifiutare. Le frontiere sono ordinate per costo
      del percorso per cella di frontiera (al massimo 40 celle), non per
      sola distanza: la più vicina prima spendeva il 40 % di una corsa
      sui ritagli intorno alla partenza e toccava quattro stanze su sei
      in dodici minuti; il budget predefinito ora è di trenta (corsa 28,
      30 min: 39 m di percorso reale, 25 % del pavimento contro 20 %,
      sempre quattro stanze su sei — e la posa di maploc è scivolata fino
      a 1,9 m per cinque minuti con `tracking` ancora vero, finché una
      chiusura d'anello l'ha riportata a posto). `map_step`
      stesso ora *accorcia* il
      passo al pavimento che ha davanti (campo `shortened` nel risultato)
      e rifiuta solo quando ci sta meno di un secondo di cammino — le
      tappe fisse da 3 s del giro guidato avevano cominciato a fallire
      sugli oggetti visti dal sensore. Recuperi: becco contro qualcosa per
      la mappa *o* per il sensore → un passo indietro limitato; sei
      rifiuti senza tappe in mezzo, oppure "niente di raggiungibile da
      qui" con frontiere ancora aperte → dimentica gli ostacoli locali
      entro 0,6 m, mette da parte quella frontiera, esce in retromarcia,
      sosta (tre volte, poi si arrende onestamente). Il passo indietro usa
      sempre una rotazione positiva: l'andatura del gemello indietreggia
      solo così (misurato due volte; con rotazione negativa il corpo
      resta dov'è). Corse di dodici minuti da avvio pulito: 84–94 tappe,
      14–16 m di percorso reale, ~80 sottomappe, 24–58 chiusure d'anello,
      nessuna caduta, i bordi del vano scale registrati come dislivelli.
      Ancora aperto: gira per minuti su piccole frontiere dietro oggetti
      bassi vicino alla partenza; la posa di maploc scivola fino a 0,5 m
      nella stanza a nord-est (upstream); la domanda "che stanza è?" ha
      bisogno del suo evento di protocollo per l'agente.
- [x] Regola della mano destra (2026-09-05, idea dell'utente, in prova):
      camminare dritto e tenersi al centro tra i muri mappati (`map_step`
      sterza verso la mezzeria quando entrambi i muri sono entro 1,2 m,
      banda morta 5 cm); quando la via è chiusa, girare sempre dallo
      stesso lato (`[map] explore_turn`, destra di default) salvo che il
      corpo non abbia spazio per ruotare da quella parte. "Il lato più
      largo" cambiava idea a ogni tappa e oscillava nei punti stretti; una
      mano sola aggira l'ostacolo e segue il muro fino alla porta
      successiva. Il pianificatore a frontiere resta sopra a scegliere
      dove andare, a vedere le porte dall'altra parte, le isole e la fine.
      Avvertenza: sul gemello la destra è il lato debole (un arco a destra
      gira un terzo di uno a sinistra), perciò il lato è un parametro.
      Commit separato per un revert facile.
- [x] Correzioni `[gait]` (2026-09-05, idea dell'utente): `yaw_trim` e
      `yaw_gain_left/right`, applicate per ultime a ogni comando di
      camminata che il satellite invia andando avanti; default spente
      (0, 1, 1). Perché: misurato sul gemello, una tappa dritta da 3 s
      vira ogni volta di circa 20° a destra (sei tappe: da -9° a -24°, un
      fuori scala), mentre la risposta in rotazione è rumorosa ma non
      sbilanciata (±0,7 girano uguale; ±0,3 quasi). Quindi quel "sterza
      sempre a destra" è l'andatura stessa che vira quando le si chiede di
      andare dritto; `yaw_trim = 0.2` sul gemello. Zero sull'hardware
      finché non lo si misura lì. Se le manopole si rivelano fragili, il
      passo successivo è l'autotaratura: rotazione ottenuta per rotazione
      comandata, tappa dopo tappa, dalla posa della mappa.
- [x] Indietro e gira, e una lezione sulle manovre cieche (2026-09-05,
      osservazione dell'utente): "indietro e ripianifica" chiudeva un
      cerchio — il passo indietro fa scivolare la coda da un lato,
      l'andatura vira dall'altro tornando verso lo stesso obiettivo — così
      ora al passo indietro segue un quarto di giro nel verso configurato,
      poi una sosta di mappatura piena. La sosta è decisiva: la corsa 31,
      con indietro-e-gira e un solo secondo di fermata, ha perso la mappa
      in due minuti (un muro inchiostrato 40 cm fuori posto, la posa
      nell'ignoto, un falso "nessuna frontiera rimasta"); le corse con
      poche giravolte tenevano la posa entro 0,5 m. maploc mappa alle
      soste e tra una sosta e l'altra si fida dell'odometria, e
      l'odometria di un'andatura bipede è pessima negli archi stretti.
      Inoltre: "finito" ora richiede che la mappa non abbia più celle di
      frontiera — altrimenti è "sigillata" e, dal secondo tentativo, il
      pianificatore si stringe alla mezza larghezza del corpo (0,10 m)
      invece del margine di 0,15, che è come la papera esce dalla tasca
      della camera tra letto, comodino e armadio dove finivano le corse
      29 e 30.
- [x] Calibrazione dell'andatura da un giro umano (2026-09-05, idea
      dell'utente): l'utente ha guidato il gemello con le frecce per 21
      minuti (un telecomando curses che registra comando e posa vera a
      10 Hz, sei luoghi nominati), 85 m, nessuna caduta, tutte e sei le
      zone, 57 % del pavimento mappato in una volta contro il 25 % migliore
      dell'esploratore. L'andatura, misurata su 178 s di marcia dritta:
      0,114 m/s e una virata a destra di 2,9 ± 2,5 °/s — reale ma un terzo
      di quella mostrata dalle tappe da 3 s comandate da fermo; le svolte
      ±0,7 con vx 0,3 danno 25,5 e 26,5 °/s, quindi nessuna asimmetria
      di lato (la calibrazione di fabbrica regge; `yaw_gain` resta 1/1,
      `yaw_trim` 0,08 e non 0,2); la rotazione sul posto (vx 0, vyaw 0,7)
      funziona, a circa 17 °/s, rumorosa; la retromarcia dritta va a
      0,08 m/s quando l'andatura è già in passo, mentre da fermo serve una
      rotazione. Chi guidava teneva una mediana di 0,38 m dall'ostacolo
      più vicino avanzando, 10° percentile 0,19 m — il nostro margine
      frontale di 0,25 e il gonfiaggio di 0,15 sono nel suo intervallo.
      Dati in `private/drives/`.

- [ ] Memoria della mappa e rilocalizzazione dalla nostra parte
      (2026-09-05, richiesta dell'utente): anche prima che Pollen cabli la
      rilocalizzazione all'avvio, la papera non deve perdere mappa e nomi
      dei luoghi a ogni accensione. Da studiare: cosa espone `robot.map`
      che si possa salvare (griglia e posa sono pubblicate; il grafo delle
      sottomappe no), se al maploc di robotd si possa passare una sessione
      salvata (il `wipe_on_boot` della PR 127 suggerisce che un file di
      sessione esista), e altrimenti un ripiego lato quacksat — conservare
      l'ultima griglia, allineare la mappa fresca a quella (scan-to-map o
      griglia-su-griglia in 2D) appena esistono alcune sottomappe, e
      riancorare il registro dei luoghi al nuovo riferimento. Prima
      parlarne con upstream.
- [ ] Iterazione dopo il giro umano (2026-09-05, in prova nella corsa
      39): (1) la frontiera è il bersaglio, il *punto di sosta* sta 0,5 m
      prima lungo il percorso (`Frontier::stand`) — una frontiera sta per
      definizione contro muri e mobili, andarci sopra metteva il becco
      addosso ogni volta; una sosta poco prima la mappa altrettanto bene.
      (2) Indietro e poi dritto: la rotazione che un passo indietro
      richiede è già una correzione di 40° (misurata); il quarto di giro
      che seguiva puntava la parete di fianco e la tappa dopo curvava
      indietro — tolto. (3) Il centraggio in `map_step` è relativo alla
      larghezza del passaggio: correzione piena contro un muro di un
      corridoio da 0,4 m, dove il vecchio guadagno ne dava un decimo. Solo
      sulle tappe dell'esploratore (`centre: true`): sopra lo sterzo di chi
      guida da fuori era una mano estranea sul volante e ha rotto due volte
      il ritorno del giro guidato (5 e 6 su 9).
      Inoltre: le celle di frontiera entro 0,3 m da un ostacolo visto dal
      sensore non sono frontiere (i varchi non inchiostrati del muro est
      creavano frontiere false), i gruppi richiedono 8 celle, l'arrivo è a
      0,3 m dal punto di sosta. (4) Il test sugli ostacoli del sensore è
      una *corsia* larga quanto il corpo (±0,16 m dalla linea di marcia),
      non un cono di ±23°: da mezzo metro il cono conteneva gli stipiti
      di una porta da 0,4 m e la papera non provava mai un passaggio
      stretto (osservazione dell'utente). (5) La panoramica (idea
      dell'utente): a una sosta il sensore vede l'emisfero davanti, quindi
      all'avvio e all'arrivo dove più di metà del pavimento entro 1,5 m è
      ignoto la papera ruota sul posto in quattro passi da 80°, chiusi
      sulla rotta della mappa, con una sosta a ognuno — il giro completo
      visto prima di scegliere; mai due volte entro un metro. Messa a
      punto nelle corse 43–47: da ferma l'andatura non ruota affatto sul
      posto, quindi ogni passo è un secondo di camminata d'avvio e poi
      solo rotazione (~30 °/s, 15 cm di deriva); chiudere il passo sulla
      rotta della mappa a 1 Hz sovraccorreva di 30°, sull'odometria del
      flusso di stato con uno stop anticipato di 20° i passi da 45° escono
      tra 48 e 54° (otto passi, 401°); soste da 8 s, con sei restavano
      settori a metà. Misurato per settori di 30° dopo la panoramica:
      l'anello interno (fino a 0,8 m) noto al 74–100 % in undici settori
      su dodici, il dodicesimo è il vano scale; i buchi esterni stanno
      dietro i muretti. Costo: 1,5 min a panoramica, un terzo di mappa in
      meno in dieci minuti se fatta a ogni punto ignoto (3040 contro 4440
      celle), rifiuti da 32 a 8; perciò si fa solo all'avvio e dove più di
      metà del pavimento davanti entro 1,5 m è ignoto, mai due volte entro
      2,5 m (scelta dell'utente). (6) Dritto se libero (regola
      dell'utente): la tappa punta il punto di sosta stesso quando la
      retta fino a lì, fino a 2 m, non incontra muri mappati nella corsia
      del corpo, e segue il percorso della griglia — a zig-zag per natura,
      con un anticipo di 0,4 m che dava a ogni tappa una piccola sterzata,
      e la somma delle piccole sterzate era una papera che gira sul posto —
      solo quando qualcosa è in mezzo. Le correzioni di rotta partono da
      15° (banda morta 0,25 rad, guadagno 0,6, al massimo 0,2 rad/s).
      (7) Verso lo spazio (regola dell'utente, corsa 49): dopo una
      panoramica e dopo un passo indietro la papera si gira verso la
      direzione con la corsa più lunga di pavimento libero noto sulla
      mappa (24 campioni, almeno 0,6 m) e fa una tappa dritta guardata
      prima che il pianificatore decida — dove i 40° di rotazione del
      passo indietro lasciavano il becco era il caso.
      (8) Accordo mappa-sensore (corse 50–51): a ogni sosta gli ostacoli
      del sensore entro 1,5 m, nelle direzioni in cui la mappa ha un muro,
      o stanno su di esso (entro 0,35 m: accordo) o oltre (disaccordo) —
      vedere attraverso un muro mappato è l'unica cosa che una posa vera
      non può fare. Il pavimento che la mappa mostra oltre un ostacolo non
      è una prova: una mappa in costruzione manca ogni mobile basso, e
      contarlo (corsa 50) ha dato 40 falsi allarmi e 15 panoramiche in
      mezz'ora. Dieci o più oltre, e il triplo di quelli sul muro (tre e
      metà davano nove falsi allarmi nella corsa 51, con la posa giusta),
      fanno una sosta dubbia; due di fila valgono una panoramica perché il mappatore
      chiuda un anello, sei di fila chiudono il lavoro come "posizione
      persa" invece di mappare su una posa falsa (la corsa 49 ha passato
      quindici minuti 3–5 m fuori posto, "tracciata").
      (9) Prima finire la stanza (corsa 52): oltre 2,5 m il punteggio di
      una frontiera cresce con la distanza, così un ritaglio a portata
      batte un'apertura larga due stanze più in là — il criss-cross
      dell'appartamento visto in ogni immagine di mappa finora.
      explore_lite, lo standard ROS, ordina per distanza meno dimensione e
      mette in lista nera una frontiera dopo 30 s senza progresso; il
      nostro è costo per cella con questo fattore di località e la lista
      dei rifiuti. (10) Modalità varco (corsa 52): un passaggio tra la
      larghezza del corpo e 0,6 m è una porta — la tappa sterza sul suo
      asse, fa passi da 1,5 s e chiede a `map_step` i margini da porta
      (`gap`: frontale 0,15 m, tolleranza 0,05, corsia ±0,115 m; anche il
      test di spazio della tappa usa quella corsia e una riserva di 0,20 m
      — con quelli del corridoio non proponeva nemmeno un passo dentro,
      corsa 53). La porta
      da 0,42 m verso lo studio non ha mai lasciato passare la papera con
      i margini del corridoio (una guida da script: 17 retromarce, mai
      oltre la soglia), dove una persona l'ha portata dentro subito. La
      porta è riconosciuta anche dal sensore (qualcosa su entrambi i lati
      entro 0,6 m l'uno dall'altro, davanti e vicino): un mobiletto basso
      che la mappa non ha inchiostrato è uno stipite lo stesso. Corsa 55:
      la papera ha passato da sola la porta da 0,42 m per la prima volta,
      28 % dell'appartamento in trenta minuti (record), 122 tappe, 79
      rifiuti, cucina 49 %, camera 37 %, bagno sfiorato. (11) Anche il
      guardiano del vuoto giudica un dislivello in corsia (±0,30 m dalla
      linea di marcia; un arco lungo la rotta su cui finisce), non nel
      mezzo cerchio davanti: il vano scale di fianco al percorso rendeva
      impraticabile il passaggio da 0,54 m tra esso e il muro, e un bordo
      di dislivello registrato aveva raggio 0,45 m — 0,6 col gonfiaggio —
      che sigillava lo stesso passaggio sulla mappa; ora 0,20 m
      (osservazione dell'utente, corsa 56).
- [x] Il gemello di carta (2026-09-06, idea dell'utente):
      `quacksat-core/examples/paper_twin.rs` fa girare l'esploratore vero
      (`explore.rs`, `frontier.rs`, `tools::plan_step` — i guardiani di
      `map_step`, ora funzione pura) contro un modello cinematico della
      papera nelle scatole dell'appartamento (`apartment.world.json`, dalla
      scena di Pollen): l'andatura misurata (0,114 m/s, 0,65 rad/s per unità
      di rotazione, virata a destra, niente rotazione da ferma, retromarcia
      solo con rotazione positiva), il sensore a raggi con la spazzata della
      testa, la mappa che cresce alle soste, la posa come verità più un
      random walk che le soste riassorbono, un rapimento opzionale.
      L'esploratore ci arriva attraverso il tratto `Body` (anche il `Robot`
      vero lo implementa), con orologio virtuale. Trenta corse da trenta
      minuti simulati in tredici secondi: copertura mediana 29,6 % (18–36),
      114 tappe, 184 rifiuti, 47 m — la fascia del gemello MuJoCo (26–28 %,
      ~110 tappe, 80–170 rifiuti, 30–35 m). Per corsa un log in formato
      watch e un `map.frame`, così `mapshot.py` lo disegna e `mosaic.py`
      affianca le corse; l'utente vuole *vedere* le migliaia di
      simulazioni. Selezionare qui, confermare su MuJoCo, validare
      sull'hardware.
- [x] La scala degli ostacoli (2026-09-07, idea dell'utente): la stessa
      casa con i soli muri, più la tromba delle scale, più i mobili
      grandi, poi completa — trenta semi ciascuna con budget di novanta
      minuti, per vedere quanto costa ogni classe di ostacolo. Soli muri:
      70 % (il tetto), 17 min, un rifiuto — ma dodici su trenta hanno
      camminato fino al budget su una mappa finita, inseguendo schegge.
      La sola tromba: 120 rifiuti, tutti sul suo bordo. Due correzioni
      misurate sulla scala: (1) prima le frontiere grandi (≥ 20 celle,
      ovunque siano) e un criterio di fine — nessun gruppo di quella
      taglia in tutta la mappa, raggiungibile o no, senza contare l'anello
      attorno a un drop, e dodici giri senza trenta celle libere nuove
      chiudono il lavoro; (2) la via d'uscita dall'angolo della tromba: il
      passo indietro si prova intero, a metà e da 0,8 s (il più corto con
      metà del margine sui drop) prima di rinunciare, perché quello intero
      portava il percorso simulato sull'angolo e, non facendone nessuno,
      la papera restava a rifiutare la stessa tappa fino al budget (131
      rifiuti, seme 3). Dopo: muri 30/30 finiti, mediana 16,5 min; muri +
      tromba 30/30, 17 min, 8 rifiuti (da 89), 70 %; + mobili 27/30, 42
      min; casa completa 22/30 complete (da 20), mediana 76 min, 100
      rifiuti (da 134), copertura al tetto, nessuna caduta su 120 corse.
      Restano: il costo dei mobili (copertura minima 28 % su un seme del
      livello 3), gli otto semi incompleti della casa completa (cucina o
      stanza ovest).
- [x] Il run 70 e cosa ha insegnato (pomeriggio del 2026-09-07): il gate
      MuJoCo sulla build della scala, da boot pulito — 73 minuti, 34 %,
      posa entro 5–25 cm per tutto il run, nessuna perdita, nessuna
      caduta, ma un quarto d'ora "chiusa dagli ostacoli locali" nella
      camera e il sud mai tentato. Due cause, trovate ripianificando
      offline sulla mappa del run (`quack-places/examples/replan.rs`: un
      frame salvato, una posa, i libri, una scia): (1) cinque drop
      fantasma sul letto (le righe del ToF che guardano il pavimento
      leggono il mobile basso come un dislivello; il meccanismo nel
      simulatore non è stato inchiodato) più i margini hanno sigillato la
      porta da cui la papera era entrata; (2) i venticinque drop attorno
      alla tromba uccidevano le frontiere entro 0,42 m ciascuno e
      cancellavano l'ingresso del passaggio largo 0,54 m: la frontiera del
      sud (112 celle) sparisce con i libri, c'è senza. Correzioni: **la
      scia** — il percorso del corpo, un punto ogni 5 cm, è una corsia che
      il pianificatore può sempre usare, qualunque cosa dicano gonfiaggio
      e libri (il corpo c'è stato, alla larghezza del corpo); un drop
      uccide le frontiere entro 0,08 m oltre il suo raggio, un ostacolo
      ancora entro 0,30. Il gemello di carta ora mette drop fantasma sui
      mobili bassi (`low` nel file del mondo, un decimo dei fasci da
      pavimento che cadono sul mobile entro mezzo metro dalla sua faccia).
      Casa completa con i fantasmi, trenta semi: 17/30 complete senza
      scia, 21/30 con la scia, 23/30 anche con il raggio dei drop (da 22
      senza fantasmi); copertura minima 26 → 36 %; livelli muri e tromba
      invariati; nessuna caduta su 240 corse. Offline, la mappa del run
      70 mostra ora la frontiera del sud raggiungibile con i libri.
- [x] Le soste nelle stanze, misurate (sera del 2026-09-07, l'occhio
      dell'utente): dalle tracce vere la sosta più lunga in una stanza era
      di 5–11 minuti nei run 59–67 e di 15–32 dal run 69 in poi. Il
      gemello di carta ora la riporta (`stay_max`, `stays5`, `end_s` nel
      riepilogo; `private/drives/dwell.py` per i log MuJoCo, mostrata a
      ogni snapshot). Il sospetto — la lista dei rifiutati riazzerata dopo
      ogni tappa camminata — è stato misurato in tre modi sulla casa
      completa con i fantasmi: dopo ogni tappa 23/30 complete, 76 min, 168
      rifiuti; una volta per lavoro 18/30, 61 min, 114; dopo un metro dal
      punto dell'ultimo azzeramento 21/30, 66 min, 126. La sosta mediana è
      di 12 minuti in tutti e tre: sul gemello di carta non è il
      riazzeramento a trattenere la papera in una stanza, sono i mobili
      (muri + tromba 5 min, + mobili grandi 10, casa completa 12). Nuovo
      default: riazzeramento dopo un metro (`REARM_DIST_M`;
      `QUACKSAT_REFUSED_REARM` 0/1 per misurare). Gli stalli sulle porte di
      MuJoCo ("nessuno spazio davanti" sugli stipiti) restano il costo
      aperto.
- [ ] Uscite dalle porte (notte del 2026-09-07, aperto). La scia ora
      sopravvive al lavoro come i drop (la seconda tranche di un run la
      eredita). La correzione ovvia per i giri sulla porta — girare verso
      la meta invece che verso la mano quando non c'è spazio — ha perso
      sul gemello di carta (casa completa 21/30 → 16/30 complete, rifiuti
      126 → 150; muri + tromba rifiuti 9 → 31, una sosta da 26 minuti) e
      resta dietro `QUACKSAT_TURN_AIM=1`. Nemmeno una sonda economica su
      MuJoCo (`private/drives/exit_test.py`: guida nella camera NE da boot
      pulito, esplora, cronometra l'uscita) riproduce il run 71: su una
      mappa vuota la papera esce in 30–100 s, perché la frontiera è subito
      fuori dalla porta; il caso del run 71 è una metà nord mappata, un
      obiettivo lontano e cinquanta voci nei libri dopo quindici minuti.
      Quindi il costo delle porte si misura solo con la permanenza sui run
      interi. Prossima idea da provare lì: quando i giri sul posto si
      alternano di segno senza una tappa in mezzo, prendere come rotta le
      prime celle del percorso pianificato (libere per la mappa) e
      permettere una tappa più corta (0,6 s) attraverso un varco visto dal
      sensore.
- [x] La scia come prova per le guardie, l'avanzamento vero dell'arco, il
      giro sul posto negli spazi stretti (notte del 2026-09-07, tutto
      misurato sulla casa completa con i fantasmi, trenta semi, novanta
      minuti; base 55 %, 21/30 complete, 126 rifiuti, 43 urti, sosta più
      lunga 41 min): (1) **una tappa i cui primi 30 cm giacciono sulla
      scia è giudicata con la riserva della porta** (0,20 m, non i 0,35
      del corridoio) — il corpo c'è già stato alla sua larghezza; la
      guardia dei dislivelli e l'ostacolo visto dal sensore nella corsia
      stretta hanno ancora voce. 26/30 complete, 104 rifiuti, 34 urti,
      sosta più lunga 31 min; tenuta, attiva di default. (2) L'avanzamento
      dell'arco, misurato sulla guida umana: 0,110 m/s in avanti a vyaw
      0,7 contro 0,121 dritto — non il quarto che esploratore e guardia
      del passo assumevano, ed è per questo che la papera finiva contro i
      muri girando (osservazione dell'utente). Giudicare gli archi con
      l'avanzamento vero è giusto e perde male sul gemello di carta, dove
      un urto non costa nulla: esploratore 55 → 38 %, 10/30; guardia 55 →
      40 %. Entrambi dietro interruttore (`QUACKSAT_ARC_FULL`,
      `QUACKSAT_GUARD_ARC_FULL`), spenti, un debito da saldare su MuJoCo
      dove un urto ha un prezzo. (3) La regola dell'utente — negli spazi
      stretti una correzione oltre i 35° è un giro sul posto (calcio, poi
      rotazione), non un arco: dimezza le soste più lunghe (31 → 18 min,
      solo nei varchi) ma costa complete (26 → 20) e rifiuti (104 → 182);
      criteri più larghi costano di più (14/30). Dietro
      `QUACKSAT_SPIN_TIGHT=1` (solo varchi), spenta. **Misurato su MuJoCo
      nella notte (run 73–76b, 60 min ciascuno da boot pulito):**
      controllo 53 % (bagno 81 % al minuto 49, la prima volta su MuJoCo;
      ogni stanza all'80 % del suo tetto a quel punto; una perdita di posa
      al minuto 58, ripresa non verificata con 0,8 m di errore), 42
      rifiuti, 69 giri, 0 stalli; arco vero nell'esploratore 34 %, 114
      giri, 21 stalli, sud mai tentato; arco vero nella guardia 34 %, 77
      rifiuti, 4 perdite di posa; giro sul posto nei varchi 31 %, 140
      giri, 19 stalli, una sosta di 28 minuti nella camera. Nessuna caduta
      in nessuno. Il verdetto della carta regge su MuJoCo: tutti e tre
      restano spenti. Il primo tentativo del run 76 è finito dopo cinque
      minuti nel passaggio accanto alle scale — tre "chiusa" in trenta
      secondi raggiungono STUCK_MAX e chiudono il lavoro: fine troppo
      frettolosa lì. Corretto il 2026-09-08: un tentativo "chiusa" conta
      solo trenta secondi dopo il precedente o dopo 0,20 m di movimento
      del corpo (`STUCK_GAP_S`, `STUCK_MOVE_M`); gemello di carta neutro
      (muri + tromba identico, casa completa 24/30 contro 26/30, rumore;
      nessuna caduta). **La perdita di posa del run 73 al banco**
      (registrazione 1788809590, `private/drives/runs/73-control/bench/
      replay.txt`): la replica non perde mai la posa — errore vero mediano
      5–36 cm per tutto il run e 15–17 cm negli ultimi dieci minuti, dove
      il vivo era a 51–89 cm e ha dichiarato la perdita. Replica e vivo
      coincidono per quaranta minuti (1–16 cm) e poi divergono: la perdita
      è solo del vivo, la discrepanza vivo/replica già nota, ora con un
      caso pulito. Ingredienti misurati: l'odometria grezza di una papera
      che esplora deriva di 0,4–1,2 m (giri sul posto e retromarce; la
      guida umana derivava di 13 cm), e le chiusure d'anello nel soggiorno
      spostano la posa di 10–24 cm l'una a raffica. Il log vivo di robotd
      del run 73 non era stato archiviato (sovrascritto dai run
      successivi); `mujoco_run.sh` ora lo conserva a ogni run. **Panorama e porta del bagno** (2026-09-08): la sosta di due minuti
      che l'utente ha visto nel run 73 era un panorama (otto soste da 8 s
      con un giro tra l'una e l'altra) innescato dalla guardia
      mappa/sensore nel soggiorno; dopo, l'obiettivo è cambiato perché la
      mappa era cambiata e la vecchia frontiera non c'era più — `pick()`
      tiene un obiettivo solo finché è vivo, ed è giusto così. Le soste da
      sei secondi sono state provate e rimesse a otto: una misura
      precedente per settore le trovava spazzate a metà, e il gemello di
      carta non vede la differenza (la sua scansione alla sosta è
      istantanea). Il bagno non ha porta verso lo studio: il muro wG corre
      intero da x 0,5 a 4,0 a y −1, quindi l'unica uscita del bagno è il
      suo varco a x 0,5 (y −2,5..−1,7) e l'unico ingresso dello studio è
      il varco dell'atrio (y −0,2..0,6) — ogni via tra i due passa dalla
      tromba, lato ovest (0,54 m) o striscia est tra il buco e il muro wC
      (0,44 m). Il "giro lungo" del run 73 era l'unico.
- [x] Il passaggio, in tre modi insieme, e la retromarcia con un verso
      (2026-09-08, le regole dell'utente: "se non centra quei 5 cm non
      passerà mai", "filo muro — un urto è un urto, il buco no", "torni
      indietro anche dall'altra parte"). (1) I lati del passaggio sono
      affinati da ciò che il sensore vede accanto al corpo — il muro da un
      lato, il bordo del dislivello dall'altro — invece del muro della
      mappa e dei drop sui libri, che si muovono con l'errore di posa. (2)
      Con un drop da un lato la linea tenuta è a mezza larghezza del corpo
      e poco più dal muro (`HUG_M` 0,16), non al centro: il bordo del
      buco passa da 5 a ~18 cm fuori dalla corsia della guardia. (3) Una
      tappa del passaggio porta `passage`, e la guardia dei dislivelli del
      passo di mappatura, quando vede il muro a fianco del corpo, giudica
      una corsia di 0,17 m invece di 0,22 — il flag da solo non cambia
      nulla. La larghezza minima del passaggio sale a 0,50 m: a 0,43 la
      striscia di 0,44 m a est della tromba intrappolava due corse su
      trenta una volta che il corpo stava filo muro. (4) La retromarcia
      ha un verso. Misurato sul gemello (`backprobe.py`): da fermo muove
      solo l'imbardata positiva, ma mezzo secondo di quella mette il passo
      in moto e poi −0,7 arretra 0,23 m girando di −87°, e imbardata zero
      arretra dritta (−13°); chi chiama preferisce un verso (coda lontano
      dal drop, lo specchio dell'arco che ha incontrato il muro — che
      ripercorre la via d'ingresso), entrambe le fasi sono giudicate sui
      drop, e tra i versi liberi vince quello la cui traiettoria giace
      sulla scia; il gemello di carta modella la stessa andatura. Gemello
      di carta, trenta semi, novanta minuti, nessuna caduta su novanta
      corse: muri + tromba 17,7 → 16,6 min, rifiuti 10 → 6; mobili grandi
      51 → 37 min, rifiuti 50 → 42, cammino 95 → 68 m; casa completa 64 →
      51 min, sosta più lunga 13 → 10 min, 24/30 complete (26 prima,
      rumore). Su MuJoCo il primo test del passaggio con la papera fatta
      nascere all'imbocco sud (`MICRODUCK_START` aggiunto al body server
      del gemello, `passage_test.py`) è scaduto due volte senza tentare il
      passaggio: su una mappa vuota l'esploratore ha preferito il bagno e
      il soggiorno. Ridisegnato: nascita dentro l'imbocco. **Misurato dall'imbocco della fessura** (nascita a (−0,70, −1,10)
      rivolta a nord, dove il muro ovest comincia e l'unica frontiera
      vicina è l'atrio; sei tentativi per condizione, otto minuti
      ciascuno): base 3 passati, 2 cadute, 1 scaduto, attraversamento
      211 s; centratura sul sensore + filo muro 5 passati, 1 caduta, 0
      scaduti, 202 s. Ogni caduta ha la stessa firma — una tappa rifiutata
      per il dislivello, poi una retromarcia cieca di tre secondi fuori
      dalla scia, e il corpo nel buco pochi secondi dopo: non è la
      primitiva a cadere, è il passo indietro cieco accanto al buco. La
      prima versione della regola ("vicino a un drop, indietro solo sopra
      la scia") ha bloccato la papera all'imbocco: 5 tentativi su 6 hanno
      speso il budget rifiutando, 112 "un dislivello sta dove andrebbe il
      passo indietro" in una sola corsa, perché la papera non aveva ancora
      una scia e ogni direzione aveva un drop. Raffinata e in misura:
      fuori dalla scia vicino a un drop, solo il passo indietro da 0,8 s,
      e solo quando il drop è DAVANTI (un drop di fianco è il caso che
      cadeva). Geometria da ricordare: il muro ovest della tromba comincia
      a y = −1,0 mentre il buco arriva a y = −1,4, quindi il "passaggio" è
      una fessura di 30 cm all'estremità nord del buco; a sud di quella il
      lato ovest si apre sul soggiorno, ed è per questo che una papera
      nata a y = −1,6 sceglieva sempre il soggiorno. **Cosa erano davvero le cadute, e la correzione** (2026-09-08): col
      passo indietro corto la papera ha ripreso a camminare (15 tappe
      contro 0) ma è caduta lo stesso — durante l'*allineamento* della
      primitiva, non durante una tappa. Il meccanismo è dunque ogni
      manovra cieca accanto al buco, giro sul posto compreso: procede a
      chunk di un quarto di secondo senza guardare e deriva di circa 15
      cm, che accanto alla tromba è tutto il margine. Due modifiche: il
      giro sul posto legge il sensore di profondità tra un chunk e l'altro
      e si ferma dov'è se un bordo sta entro 0,30 m dal becco (attivo di
      default: può solo interrompere prima una manovra cieca); e vicino a
      un dislivello, fuori dalla scia, il passo indietro è quello da 0,8 s
      e solo con il dislivello DAVANTI. Cinque tentativi con tutto il
      pacchetto: 5 passati, 0 cadute, 0 scaduti (contro 3/2/1 della base e
      5/1/0 di sensore + filo muro). Da leggere con onestà: in quei cinque
      la guardia del giro non è mai scattata, e quattro attraversamenti su
      cinque hanno camminato ZERO tappe — la papera nasce in mezzo alla
      fessura e 0,55 m di deriva dal panorama e da un passo indietro corto
      bastano a "passare", quindi ciò che si misura è sopravvivere ai
      primi due minuti accanto al buco, non percorrere la fessura. Il
      passo indietro corto è la vittoria di sicurezza (sei passi corti,
      nessuna caduta, contro tre cadute su dodici con quello da tre
      secondi). Percorrere la fessura resta da provare: prossima nascita a
      y = −1,45, a sud del buco, che obbliga a 0,9 m dentro. **Nascita più dura, e perché il pacchetto resta spento**
      (2026-09-08): da y = −1,45, con 0,9 m di fessura da percorrere,
      quattro tentativi per parte: il pacchetto ha attraversato una volta
      in 251 s con sei tappe camminate — l'unico attraversamento vero
      della giornata — la base non ha mai attraversato in quattro; nessuna
      caduta da nessuna parte, perché da sud il lato ovest è aperto e la
      papera di solito va nel soggiorno. Sicurezza su tutto il lavoro
      della fessura: nessuna caduta in nove tentativi col pacchetto,
      contro due su dieci senza. Ma la scala di carta dice che il
      pacchetto costa copertura, e i pezzi si sommano (casa completa,
      trenta semi: base 24–26/30 complete, 119 m camminati; sensore + filo
      muro 18/30, 97 m; passo indietro sulla scia 21/30, 78 m; tutti e tre
      16/30, 55 m — la papera smette di camminare e finisce presto e
      incompleta). Quindi tutti e quattro restano SPENTI di default, e i
      guadagni della giornata sono i due che non costano nulla: il giro
      sul posto sorvegliato dal sensore e il passo indietro corto accanto
      a un dislivello. **La ragione probabile, e la prossima idea**: sul
      gemello di carta ogni mobile basso porta drop fantasma, quindi
      "vicino a un dislivello" è lo stato normale e la regola zittisce il
      recupero ovunque; nella casa vera l'unico dislivello è la tromba. Un
      buco e uno spigolo di mobile si distinguono — lo spigolo mostra un
      dislivello E un ostacolo alla stessa direzione, il buco mostra un
      dislivello con niente dietro. Da misurare: permetterebbe di
      applicare le regole severe solo accanto ai buchi veri. **Costruito e misurato lo stesso giorno**: `record_drops` ora chiede,
      per ogni dislivello visto, se un ostacolo sta alla stessa direzione
      (entro 0,12 rad) e a distanza simile (0,35 m) — in tal caso è lo
      spigolo di quell'ostacolo e finisce sui libri come ostacolo, non
      come buco. I rifiuti della guardia di profondità non cambiano: la
      sicurezza non dipende mai da questo giudizio. Gemello di carta,
      trenta semi: sulla casa completa chiama 1282 dislivelli spigoli e
      392 buchi (tre quarti erano mobili) e copertura, rifiuti e metri
      camminati restano uguali (55,2 %, 100, 120 m; 21/30 complete contro
      24–26 della base, dentro la dispersione già vista); su muri + tromba
      chiama ZERO spigoli — il buco vero non viene mai scambiato — e quel
      livello è identico alla base. Con il pacchetto severo sopra, la casa
      completa recupera parte di ciò che il pacchetto costa (16 → 19/30
      complete) ma non tutto, quindi il pacchetto resta spento e il
      discriminatore va attivo di default. Da confermare su MuJoCo, dove
      dovrebbe anche liberare la porta della camera che cinque drop
      fantasma sul letto avevano chiuso per un quarto d'ora nel run 70:
      serve un run intero da sessanta minuti. **Il run 77 e novanta semi appaiati** (2026-09-08): su MuJoCo il
      meccanismo è risolto — i libri chiudono il run con 6 buchi e 90
      ostacoli (quelli del run 70 erano quasi tutti dislivelli) e attorno
      al letto non c'è più nulla, quindi i drop fantasma non sigillano
      più alcuna porta; nessuna caduta, nessuna posa persa. La copertura è
      stata del 37 %, dentro la banda 31–53 % in cui i singoli run
      oscillano, quindi non decide niente. Decide il gemello di carta:
      novanta semi per condizione, appaiati seme per seme, copertura
      migliore su 30, peggiore su 27, invariata su 33, differenza media
      +0,6 punti contro una dispersione di 9,6 — indistinguibile da zero;
      complete 58/90 contro 59/90; tempo di fine e rifiuti invariati;
      nessuna caduta da nessuna parte. Il discriminatore si guadagna il
      posto per ciò che corregge (uno spigolo di mobile non chiude più una
      porta, e le regole severe si possono riservare ai buchi veri), non
      per la copertura. 
- [x] La ricerca automatica sulle manopole della tappa (2026-09-08, primo
      passo del filone della tappa appresa): dodici costanti del
      pianificatore della tappa — la corsia, le riserve davanti per tappa
      dritta, arco e porta, la corsia e la soglia di porta, le tre soglie
      di rotta, il punto di mira, lo sguardo dritto e la durata della
      tappa in porta — sono ora leggibili dall'ambiente (`QK_*`, ognuna
      col valore misurato in precedenza come default, quindi nulla cambia
      se una ricerca non la imposta), e `private/drives/legsearch.py` le
      campiona a caso, sessanta tentativi da trenta semi, con punteggio
      pari ai semi che finiscono la casa intera e una caduta ovunque che
      squalifica il tentativo. Il tentativo migliore ha battuto i default
      su tre gruppi di semi nuovi appaiati (91–180, 271–360, 361–450): 72
      semi guadagnati contro 42 persi su 270 coppie, p ≈ 0,005, complete
      175/270 → 205/270, nessuna caduta in 540 corse. **E poi non ha retto
      sugli altri livelli.** Arrotondato e misurato su semi nuovi: la casa
      completa non guadagna nulla di significativo (63 → 68, p = 0,53),
      muri + tromba mantiene 90/90 ma impiega il 64 % in più (18,0 → 29,5
      min) con quattro volte e mezzo i rifiuti (8 → 36), e i mobili grandi
      peggiorano (67 → 62). La ricerca era valutata sulla sola casa
      completa e vi si è sovradattata. Non è stato adottato nulla, i
      default restano intatti. La prossima volta il punteggio dev'essere
      i tre livelli insieme — per lo strumento è una riga — e il vincitore
      va confermato su semi nuovi di ogni livello prima di crederci. **Seconda ricerca, valutata sui tre livelli insieme** (2026-09-08):
      il vincitore di sessanta tentativi finisce prima sulla carta (casa
      completa 64 → 52 min, mobili grandi 48 → 41) ma compra quella
      velocità con i rifiuti (113 → 163, 10 → 60, 65 → 92) e, su novanta
      semi nuovi di ogni livello, non guadagna nulla: +31 semi contro −30,
      p = 1,00, e muri + tromba passa da 18 a 23 minuti. Due ricerche, due
      risultati nulli onesti. La lezione riguarda la ricerca, non
      l'esploratore: trenta semi per tentativo non vedono un effetto più
      piccolo dei ±5 case che un gruppo da novanta semi già oscilla, e
      quindi una ricerca casuale su dodici manopole con quel budget
      seleziona rumore; è la conferma su semi nuovi a impedire che venga
      adottato. Se vale un altro giro, servono novanta semi per tentativo
      e le tre o quattro manopole che l'ablazione ha indicato, non dodici;
      altrimenti restano i default, ciascuno misurato uno alla volta
      contro il difetto che correggeva. 
- [x] `go_to`, da punto a punto sulla mappa già costruita (2026-09-09). Il
      pianificatore ha guadagnato `path_to`: lo stesso Dijkstra sulla
      stessa mappa dei costi che usano le frontiere — pavimento noto a
      buon mercato, ignoto caro, muri e libri impassabili, la scia
      camminata sempre percorribile — dalla papera a un punto, con la meta
      agganciata al pavimento percorribile più vicino, così anche un
      bersaglio contro un muro funziona. L'esploratore ha guadagnato una
      modalità meta: quando un lavoro ne porta una, il pianificatore mira
      lì invece che a una frontiera e il lavoro finisce all'arrivo; tutto
      il resto — le tappe, il passaggio accanto a un dislivello, le
      guardie, i libri, i recuperi — è la macchina del lavoro di
      mappatura, estratta in un unico `walk_leg`. Il gemello di carta
      accetta `--goto x,y`: prima mappa, poi ci va, e scrive entrambi i
      percorsi nel frame perché l'immagine mostri quello pianificato sotto
      quello camminato. Trenta semi, cinquanta minuti di mappatura e poi
      una traversata della casa: **arrivata 30/30**, nessuna caduta,
      fermandosi a 0,14 m dal punto, 242 s, nessun rifiuto alla mediana,
      camminando 6,5 m contro un piano di 5,1 (rapporto 1,13: chi segue
      mira a un punto avanti sul percorso e non taglia nulla). Cosa manca
      perché sia uno strumento: `go_to(luogo)` sulla registry dei luoghi
      invece delle coordinate grezze, e l'RPC di goal upstream quando ci
      sarà. **Confermato su MuJoCo** (2026-09-09): quindici minuti di
      mappatura, poi `robot.go_to` verso un punto a 2,04 m nel corridoio
      nord — arrivata in 65 s con nove tappe e un rifiuto, fermandosi a
      0,17 m dal punto sulla propria mappa (0,27 m secondo la verità del
      simulatore, e la differenza è l'errore di posa di maploc in quel
      momento). Lo strumento rifiuta prima di camminare se la mappa non
      mostra alcuna via. Cosa manca perché sia finito: da questa parte
      nulla — `go_to(luogo)` per nome c'è, e l'RPC di goal upstream
      sostituirebbe solo chi segue il percorso, non il piano.
 Inoltre: il giro dal dock su questa build
      (tour72) ha raggiunto 6/9, 0 perdite, 0 cadute — quello del run 69
      era 8/9; i tre mancati sono le tappe di ritorno a sud della cucina,
      guidate dalle linee rette dello script.
- [ ] Una tappa appresa (notte del 2026-09-07, domanda dell'utente: si
      può addestrare la papera a esplorare invece di darle regole?).
      Quello che abbiamo è un esploratore a regole: pianificatore sulle
      frontiere, pianificatore della tappa, guardie. L'apprendimento che
      ci sta è ibrido: tenere pianificatore e guardie (una caduta si
      vieta, non si impara), imparare solo la scelta della tappa — la
      parte regolata a mano con gli interruttori stanotte. Il gemello di
      carta è la palestra: 30 corse da 90 minuti in 15 s, ~2500 volte il
      tempo reale. Passi: (a) una ricerca automatica sui dieci parametri
      della tappa (riserve, soglie di arco e giro, tappa sulla scia) con
      la scala dei trenta semi come punteggio — ancora nessuna rete; (b)
      guide umane registrate con le osservazioni della papera (il ToF
      8×8, non la verità); (c) una piccola policy (stato: finestra locale
      di mappa, ostacoli e drop visti, direzione della frontiera; azione:
      la tappa) addestrata per imitazione e rifinita per rinforzo sul
      gemello di carta, dietro le guardie come `leg()` alternativa, poi
      MuJoCo, poi hardware. Cautele: la carta non prezza gli urti e non ha
      gli stipiti, quindi una policy addestrata lì impara i suoi buchi
      (tre correzioni "giuste" hanno perso lì stanotte); mappa e posa
      restano di maploc; gli errori di una policy non si spiegano.
      microduck-lab (jonathanhawkins, Apache-2.0) allena la camminata (61
      osservazioni → 14 attuatori, PPO su Mac) — lo strato sotto il
      nostro, e la prova che la pipeline è fattibile su Mac.
- [ ] Un'andatura più ferma da provare (notte del 2026-09-07, trovata
      dall'utente): alertform/microduck-walking, fork di microduck_rl con
      una sola modifica di ricompensa (penalità sulla velocità angolare
      del corpo −0,05 → −0,3): 18 % in meno di oscillazione d'imbardata,
      26 % meglio della policy ufficiale su stabilità di rotta e
      inseguimento della velocità, meno cadute, ONNX per lo stack
      ufficiale, Apache-2.0, CUDA per riaddestrare. I nostri tre guai
      d'andatura misurati sono tutti d'imbardata: la deriva a destra (2,9
      °/s), i giri a tempo che variano del triplo con la fase del passo,
      gli archi che avanzano come tappe dritte. Prova economica: caricare
      il loro ONNX nel body server del gemello, ripetere la sonda di
      rotazione e una guida umana breve (deriva, avanzamento in arco), poi
      un run da 60 minuti contro il run 73. Non dà la rotazione sul posto
      da fermo. Una policy non ufficiale sulla papera vera è una scelta a
      parte, da fare con calma. **Provato la notte del 2026-09-08:** il fork non contiene l'ONNX
      (solo la ricetta e un checkpoint rimasto sulla macchina dell'autore),
      quindi l'ablazione è stata riprodotta in microduck-lab sul Mac: due
      camminatori, stesso seme, 3 milioni di passi l'uno (3 min),
      `W_ANG_VEL_XY` 0,05 e 0,3. Entrambi hanno imparato a stare fermi —
      0,000 m/s a comando 0,3 nel valutatore del laboratorio, e nemmeno un
      passo nel gemello — che il README del laboratorio documenta come la
      trappola del budget su CPU: "1,5 milioni di passi da zero comprano
      'non cadere', nient'altro", il warm start distillato cammina ma
      crolla entro un milione di passi di rifinitura, "nessuno ha ancora
      mostrato quale budget lo sfrutti". La via del Mac non può produrre un
      camminatore da confrontare; la via fedele è la pipeline GPU
      (microduck_rl su Hugging Face Jobs, costo e via libera dell'utente).
      Tenuti: `WALK_ONNX` sul lanciatore del gemello e `private/drives/
      gaitprobe.py` (policy ufficiale sul gemello: 0,090 m/s dritto con
      deriva −3,5 °/s, calcio e giro ±28–30 °/s in entrambi i versi,
      retromarcia −0,135 m/s; un client che non scarica i push di robotd
      smette di essere ascoltato dopo un minuto — la sonda ora li scarica).
- [ ] Memoria dei percorsi, tre livelli (2026-09-07, indicazione
      dell'utente): la scia (sopra, per lavoro); un grafo dei percorsi
      persistente in quack-places — luoghi uniti da tratte camminate con
      le loro statistiche (percorrenze, durata, rifiuti, retromarce, drop
      visti, salti di posa), la tratta migliore prima per sicurezza poi
      per tempo, messa alla prova ogni tanto contro la proposta più corta
      della mappa e tenuta solo se ha camminato più pulita, una tratta
      fallita penalizzata e non cancellata; e la mappa metrica sotto
      entrambi. Tratte ancorate ai luoghi, non alle coordinate, e
      verificate mentre si percorrono con il controllo mappa/sensore: la
      posa di maploc è quella che è, e la sessione si azzera al boot.
- [ ] Libreria di mappe e rilocalizzazione al boot (2026-09-07,
      indicazione dell'utente): più mappe salvate; al boot la papera fa
      il panorama (un giro intero se serve), prova ogni mappa con la
      ricerca globale sotto le guardie di unicità e accordo, prende
      l'unica corrispondenza convinta, altrimenti mappa nuova e la domanda
      "Qui dove siamo?". Upstream oggi: un solo file di sessione, ripreso
      fidandosi dell'ultima posa, nessuna ricerca al boot, solo
      `robot.map` e `robot.map_wipe`. Serve upstream (prototipo su
      `maploc-study`): `robot.map_list/load/save`, caricare = partire
      "persa dura" e cercare; misurarlo al banco con una sessione salvata
      e una registrazione che parte altrove (il test di rapimento).
      Cautele: la firma di una stanza col ToF 8×8 è povera (la guardia di
      unicità è la difesa); la sessione è bincode senza schema, le mappe
      salvate muoiono a ogni cambio di formato.
- [x] Le tre chiamate esistono, sui rami locali (2026-09-09).
      `robot.map_save <nome>` copia la mappa viva in una cartella `maps/`
      accanto alla sessione di lavoro; `robot.map_list` dice cosa c'è, con
      dimensioni e date; `robot.map_load <nome>` ne rende viva una e fa
      partire il mapper "perso duro" dentro di essa: torna la mappa, mai
      la posa. Un nome è da 1 a 64 caratteri fra lettere, cifre, `-` e
      `_`, rifiutato e non ripulito: chi intendeva `../../etc/passwd` si
      sente dire di no. Il wipe azzera la mappa viva e lascia in piedi la
      libreria. mediad porta le tre chiamate, btd le rifiuta, l'updater le
      dichiara sconosciute; `robotctl robot map-save|map-list|map-load` le
      guida a mano. quacksat espone gli stessi tre strumenti, scritti per
      nome di metodo con `Control::request_method`, perché il
      `duck-ipc-proto` pubblicato non ne ha nessuno: un robotd più vecchio
      risponde METHOD_NOT_FOUND e l'anatra dice "questo robot non ha
      ancora una libreria di mappe" invece di fallire in modo oscuro.
      Misurato contro un robotd finto, da robotctl e attraverso l'MCP:
      una libreria vuota non elenca nulla, un salvataggio compare
      nell'elenco, `../evil` è rifiutato, un nome sconosciuto lo dice, un
      caricamento è adottato con il mapper che cerca, e un wipe lascia
      stare la libreria. Dove sta: ramo `maploc-study` di
      `microduck-pr202`, commit 4692340; quacksat `maploc-track`, commit
      d875888 — locali, non spinti, e chiesti a upstream in
      docs/study/upstream-asks.it.md §5.
- [x] Il riconoscimento sull'IPC (2026-09-09). L'allineamento è uscito
      dall'esempio al banco ed è diventato `maploc::align`, e sul filo
      `robot.map_match` (i candidati, il migliore per primo: nome, dove
      la mappa viva si colloca in quella salvata, il residuo sui muri, la
      quota di pavimento vivo posato su muri salvati e un punteggio) e
      `robot.map_adopt` (scambia la mappa viva con quella salvata,
      componendo la trasformazione con dove il robot si trova in quel
      momento, così il client non deve congelarlo). La posa adottata è
      impostata prima di costruire il mapper, così la finestra di
      conferma giudica quella e non la posa a cui finì il run salvato; il
      robot riparte sospetto, non tracciato. `robotctl robot
      map-match|map-adopt` le guida a mano.
- [x] Il ritorno a casa, e funziona (run home2, 2026-09-09).
      `quacksat-core/src/homecoming.rs`, spento se non `[homecoming]
      enabled = true`: all'avvio carica la mappa salvata più recente e
      resta ferma un minuto nel caso il mapper confermi una posa da solo;
      se non lo fa, azzera, esplora e fa la domanda mappa contro mappa
      ogni tre minuti; adotta quando due domande nominano la stessa mappa
      nello stesso punto (entro 0,3 m) con la mappa viva più grande la
      seconda volta. Sul gemello, nascendo in cucina a 3,5 m dalla base
      con la mappa del run 71 in libreria: la ricerca all'avvio non ha
      trovato nulla nel suo minuto, come previsto; la prima domanda,
      dopo quattro minuti e con 339 celle di muro, ha nominato la mappa a
      (−3,40, 1,00) contro una nascita vera a (−3,50, 1,30); la seconda,
      tre minuti dopo con 551 celle, ha detto (−3,40, 0,95); ha adottato,
      e la posa che ha preso è (−0,66, 1,99) contro un vero (−0,57,
      1,82) — **19 cm**. Ogni domanda è costata 0,6–0,7 s di mappatura
      in pausa. La regola delle due domande è ciò che rende sicura la
      cosa senza una soglia tarata, e costa poco: la risposta era già
      giusta alla prima domanda. Il run home3, dalla stessa nascita, l'ha
      rifatto in modo indipendente — domande a 417 e 575 celle, entrambe
      nello stesso punto entro 5 cm, adozione a (0,16, 2,05) contro un
      vero (0,05, 2,13), **14 cm** — e poi ha ripreso a esplorare sulla
      mappa adottata. Quest'ultima parte ha richiesto due correzioni che
      home2 ha scoperto: aspettare che il lavoro in corso si fermi
      davvero prima di adottare (un panorama dura un minuto, e
      `robot.map_explore` risponde "sto già girando" invece di partire),
      e restare fermi dopo, finché il mapper non conferma il posto
      adottato, perché l'esploratore non parte senza una posa di cui si
      fida.
- [x] Il controllo negativo esiste, e ha smentito la regola (2026-09-09).
      `sim-maploc/houses/flat_b.xml` è una seconda casa per il gemello —
      cinque stanze attorno a un atrio centrale, su una pianta più alta
      che larga, dove l'appartamento è un corridoio con le stanze ai lati
      su una pianta più larga che alta. La sua prima stesura aveva due
      stanze senza alcuna porta (segmenti di muro che si toccavano
      esattamente dove doveva esserci il varco) e un mobile davanti a una
      terza: per questo ora esiste `private/drives/housecheck.py`, che
      ingrossa ogni cosa solida della larghezza dell'anatra, allaga il
      pavimento dal punto di nascita e nomina ciò che non raggiunge. Casa
      B è raggiungibile al 100 % con 14, 20 e 25 cm di franco; casa A
      scende all'86 % a 25.
      Messa in casa B con in libreria solo casa A, l'anatra **ha adottato
      casa A**. Le domande: (−0,25, 1,75) 0,116; (−1,40, −1,85) 0,119;
      (−1,80, −0,60) 0,133; (−1,80, −0,45) 0,124 — le ultime due a 15 cm
      l'una dall'altra con la mappa cresciuta da 728 a 810 celle, cioè
      esattamente ciò che la regola delle due domande era stata istruita
      ad accettare. Quindi "un candidato sbagliato non sopravvive alla
      propria mappa che cresce" è falso com'è scritto: un candidato
      sbagliato può stare fermo per due domande a quattro minuti.
      Ciò che invece separa le due case, su questa evidenza, è il
      punteggio: 0,066–0,092 in casa propria contro 0,116–0,133
      nell'altra, senza sovrapposizione. E il seguito è stato quieto — il
      mapper non ha mai confermato la posa adottata, così l'anatra si è
      fermata invece di camminare convinta; ma aveva già buttato la mappa
      di casa B che aveva costruito, il che dice che l'adozione dovrebbe
      essere a prova e reversibile, non definitiva.
- [x] Lo strumento era sbagliato prima della regola (2026-09-09, sera).
      Un giro a vuoto nella casa PROPRIA — tredici domande, tutte che
      nominano il posto giusto entro 15 cm — ha mostrato il punteggio che
      *peggiora* mentre la mappa cresce: 0,064 a 321 celle di muro, 0,103
      a 766. Quindi "nella casa giusta migliora" era un artefatto di due
      misure, e peggio: le due gamme quasi si toccavano (propria
      0,064–0,103, altrui 0,110–0,142) e una soglia a 0,10 avrebbe
      rifiutato la casa giusta sei volte su tredici.
      La causa non è la casa ma la domanda. La mappa salvata copre il 46 %
      dell'appartamento; quando quella viva cresce oltre i suoi bordi,
      sempre più celle di muro finiscono dove la salvata non ha opinione,
      e lì "quanto dista il muro salvato più vicino" non risponde a nulla.
      Perciò `maploc::align` calcola il residuo **solo sulla
      sovrapposizione** — le celle di cui la mappa salvata sa qualcosa — e
      riporta la quota di sovrapposizione come numero a sé. Ripunteggiate
      offline, le stesse coppie: casa propria 0,109, 0,116, 0,144 contro
      un'altra casa 0,224, 0,227 — un fattore due dove c'erano sette
      millesimi. La stessa grande mappa viva di casa A vale 0,144 contro
      la mappa di A e 0,227 contro quella di B. Il margine sul secondo
      candidato li ordina allo stesso modo: 0,51–0,79 quando è giusto,
      0,96–0,99 quando è sbagliato — un vincitore falso non si distingue
      dalla propria seconda scelta, che è esattamente l'aspetto che ha il
      non riconoscere un posto.
- [ ] L'accettazione dev'essere ASSOLUTA, mai "la migliore della
      libreria" (2026-09-09, osservazione dell'utente): l'anatra può
      trovarsi in una casa che non è in nessuna mappa che possiede,
      quindi la domanda deve avere "nessuna di queste" fra le risposte.
      Una mappa si adotta perché supera una barra sua; la classifica fra
      mappe può solo ordinare i candidati che l'hanno già superata. Tre
      esiti: una sola la supera → adotta; più d'una → rifiuta, perché un
      robot non può stare in due case e quella vera potrebbe non essere
      né l'una né l'altra; nessuna → resta sulla mappa fresca e continua
      a esplorare, che è il caso ordinario la prima volta che viene
      accesa da qualche parte.
- [x] Misurato con lo strumento della sovrapposizione, in tutte e due le
      case, dal vivo (2026-09-09, notte). Tredici domande in casa A contro
      la mappa salvata di A: 0,107–0,138, tutte col posto giusto.
      Quattordici domande in casa B con ENTRAMBE le mappe in libreria: ha
      nominato `flat_b` ogni volta, 0,043–0,086, con una posa che non si è
      mai spostata di più di 10 cm; e nelle stesse domande la mappa
      sbagliata valeva 0,187 con il 72 % di sovrapposizione — la stessa
      della giusta, quindi non è penalizzata perché copre meno:
      semplicemente non è quella casa. Offline all'inverso: 0,224 e 0,227.
      Quindi tutto ciò che abbiamo si separa a **0,16**: peggiore giusta
      0,138, migliore sbagliata 0,187.
      Il margine sul secondo candidato non sopravvive al passaggio dal
      banco al vivo — 0,74–0,96 in casa A mentre aveva ragione — quindi
      resta informazione e non regola. La ripetizione invece sopravvive:
      tre domande di fila d'accordo entro 15 cm capitano 5 volte su 13 in
      casa A e 9 su 14 in casa B, quindi pretenderne tre si può.
      Nota che riguarda la mappa: la mappa di casa A prende 0,107–0,138 a
      casa sua, quella di casa B prende 0,043 a casa sua, perché `home` è
      vecchia e ha la propria deriva dentro mentre `flat_b` è stata
      costruita la stessa sera. Una mappa migliore si riconosce meglio —
      i due obiettivi sono lo stesso obiettivo.
- [ ] Da misurare ancora con lo strumento della sovrapposizione: le due
      serie a vuoto rifatte dal vivo (il punteggio resta piatto mentre la
      mappa cresce?), casa B con ENTRAMBE le mappe in libreria (la
      risposta giusta è solo `flat_b`), e una terza casa — l'anatra in C
      con A e B in libreria deve dire nessuna. Poi la barra, dalle
      distribuzioni.
- [ ] Prossimo, e misurato prima di decidere: `[homecoming] dry_run =
      true` chiede ogni due minuti e mette a verbale ciò che *avrebbe*
      fatto, così un giro dà tutta la serie invece di fermarsi al primo
      errore. Due serie da raccogliere — casa B contro la mappa di A, e
      casa B contro entrambe una volta mappata e salvata B — e poi una
      regola scelta dalle distribuzioni: un tetto al punteggio, un raggio
      di accordo più stretto di 0,30 m (il falso positivo era a 15 cm),
      tre domande invece di due, o il vincitore che deve battere il
      secondo fra mappe diverse. Restano anche: il caso della base
      misurato dal vivo, e il registro dei luoghi portato oltre lo
      scambio con la stessa trasformazione.

- [ ] Una finestra non può formarsi mentre la posa è sospetta
      (2026-09-10, trovato costruendo il banco di velocità). Dopo
      `robot.map_adopt`, stare fermi e girare sul posto produce finestre
      da 15–48 raggi, che il mapper scarta come troppo magre
      (`min_window_beams` 60) — quindi la conferma di cui la posa adottata
      ha bisogno non può mai arrivare, e l'anatra resta persa su una mappa
      la cui posa era giusta a pochi centimetri. Le stesse soste, quando
      traccia, danno composite da 1000–2400 raggi.
      Il sospetto è il voto dell'accumulatore: tiene un raggio solo se più
      fotogrammi della finestra hanno visto la sua cella terminale, e
      mentre la posa è sospetta la testa spazza di ±0,9 rad per allargare
      il campo, così fotogrammi consecutivi guardano altrove e poche celle
      raccolgono voti. Una difesa dal rumore che svuota la finestra
      proprio quando la finestra conta. È probabilmente anche il motivo
      per cui la rilocalizzazione all'accensione è sempre parsa debole. Da
      misurare: raggi per finestra con la spazzata accesa e spenta, e il
      `min_frames` del voto contro la velocità della spazzata.

## 2b. Una mappa sola, tenuta giusta (2026-09-09, direzione dell'utente)

La libreria va in soffitta. **Una mappa sola**: l'anatra la tiene, la
migliora navigando, e la sostituisce quando le si dice di esplorare
daccapo. Conta invece che quell'unica mappa sia giusta — "una mappa
perfetta, senza derive o spostamenti" — e che l'anatra si muova in fretta
da un punto a un altro. Il riconoscimento sopravvive al cambio,
semplificato: all'accensione la domanda non è più *quale* casa ma *questa,
sì o no*, che è la stessa misura contro una libreria di uno, con la barra
qui sopra. Rifiutare non costa nulla: esplora e chiede.

- [x] Un numero per "perfetta" (2026-09-09). `private/drives/mapquality.py`
      valuta una mappa contro la casa stessa, non contro un'altra mappa: i
      muri stanno nel file del mondo, quindi la verità è disponibile.
      Prima adatta la mappa alla verità in modo rigido — dove sta
      l'origine della mappa è un accidente di dove l'anatra si è accesa —
      e ciò che sopravvive all'adattamento è la qualità: i percentili di
      spostamento, la quota oltre 10 cm, i fantasmi oltre 25 cm (una mappa
      derivata disegna lo stesso corridoio due volte, e la seconda copia
      cade in mezzo al pavimento libero) e la copertura delle superfici di
      muro vere. Il nuovo esempio `dump_frame` di `maploc` trasforma una
      sessione salvata nello stesso JSON che porta un frame di mappa dal
      vivo, così una mappa su disco e una in volo si misurano con un
      unico strumento.
      A che punto siamo, e non è dove la direzione chiede:

      | mappa | mediana | 90° perc. | oltre 10 cm | fantasmi | copertura |
      |---|---|---|---|---|---|
      | run 71 (il riferimento) | 0 cm | 20 cm | 21,0 % | 6,9 % | 46 % |
      | casa A, 30 min stasera | 0 cm | 25 cm | 18,7 % | 8,0 % | 39 % |
      | casa B, 30 min stasera | 0 cm | 20 cm | 21,6 % | 3,4 % | 39 % |

      La mediana è zero — la maggior parte dei muri mappati sta esatta su
      quelli veri — e un quinto è fuori di più di 10 cm, con il 3–8 %
      raddoppiato. Quel quinto è il bersaglio.
- [x] Da dove veniva quel quinto, e metà se n'è andata (2026-09-10).
      La correzione della posa era già accesa, quindi il sospettato era il
      chiuditore d'anelli — ed era lui, ma non per il motivo previsto. Le
      chiusure servono: rigiocando una sessione senza nessuna, i muri
      fuori posto salgono al 27 % contro il 21 % con esse. Sbagliato era
      quanto ci si credeva. L'arco diceva all'ottimizzatore che la posa
      relativa di due sottomappe era nota a 5 cm — una cella e mezza — e
      il grafo si piegava per accontentare ogni chiusura che i rettangoli
      ripetuti dell'appartamento producevano.
      Allargato a 40 cm e 14°, su sette sessioni rigiocate in due case:
      meglio in sei su sette, media dei muri fuori posto 21,1 % → 13,4 %,
      media dei muri raddoppiati 5,4 % → 3,4 %. Sulla registrazione di
      riferimento 20,8 % → 3,6 %. Dal vivo, mezz'ora in casa A: muri
      raddoppiati **8,0 % → 1,1 %**, 90° percentile 25 cm → 15 cm,
      peggiore 55 cm → 40 cm; i muri fuori posto si muovono appena
      (18,7 → 17,1), perché un giro dal vivo è un percorso diverso e non
      un confronto controllato.
      E la mappa che ne è venuta riconosce meglio la sua casa: la serie
      delle domande è passata da 0,107–0,138 a **0,058–0,112**, i margini
      da 0,74–0,96 a 0,34–0,43. Una mappa più vera è una mappa più facile
      da riconoscere.
- [ ] Ciò che NON è risolto: la sensibilità in sé. La stessa
      registrazione, con la fiducia sull'arco spostata di poco, cade
      ancora fra il 3,6 % e il 29 % di muri fuori posto — l'esito è
      caotico in quel parametro, e 40 cm è un posto più tollerante dove
      stare, non una cura. Un nucleo di Huber sugli archi era il rimedio
      ovvio ed è stato misurato: con archi da 40 cm nessun residuo arriva
      a 1,5 sigma e non scatta mai; con archi da 5 cm scatta e il
      risultato è una lotteria (una registrazione +17 punti, un'altra
      −21). Va in produzione spento, `OptimizerConfig::huber_delta`, con
      la misura scritta accanto. Una cura vera deve impedire a una singola
      chiusura sbagliata di dominare: un vincolo commutabile, un consenso
      fra chiusure, o la validazione di una chiusura contro la mappa che
      produrrebbe.
- [x] Strumenti per questo lavoro: `mapquality.py` (una mappa contro i
      muri veri della casa), `dump_frame` di `maploc` (una sessione
      salvata come frame di mappa dal vivo) e `OUT_SESSION` di `evaluate`
      (la mappa costruita da una riprova, salvata come la salva il robot).
      Insieme: rigiocare qualunque registrazione con qualunque
      impostazione e misurare la mappa che ne esce, in una quindicina di
      secondi. E per giunta: una mappa misurata contro la casa sbagliata
      legge 40–50 % di muri fuori posto contro il ~20 % di quella giusta,
      quindi la qualità della mappa fa anche da prova d'identità.
- [ ] Muoversi in fretta. Oggi ogni tappa è cammina-e-fermati, e la sosta
      serve a *mappare*. Su pavimento già mappato e con una posa di cui si
      fida, all'anatra non serve: `go_to` può camminare di continuo col
      sensore di profondità come unico guardiano. Prima la misura — quanto
      ci mette dalla cucina alla camera com'è adesso — poi la modalità
      veloce, e lo stesso numero dopo.

- [x] Quanto è veloce, e le soste non sono il grasso (2026-09-10).
      `private/drives/speed_test.py` e `speed_run.sh`: l'anatra mappa da
      sola per sette minuti, poi le si chiede di attraversare ciò che ha
      mappato, con la verità presa dal server del corpo. Due tragitti da
      tre metri costano **221 s e 192 s**, con arrivo a 9-15 cm — una
      velocità utile di 0,013-0,015 m/s contro un passo di 0,121. Nove
      decimi del viaggio non sono avanzamento.
      La cura ovvia — togliere la sosta fra le tappe su pavimento già
      mappato, fermandosi ogni quinta per tenere onesta la posa — è stata
      costruita e misurata, e non paga: 235 s e 214 s, nessun guadagno, e
      l'anatra ha camminato 8,5 m e 8,7 m per fare gli stessi tre metri,
      con sette tappe che non l'hanno mossa affatto contro una e undici
      retromarce contro quattro. Va in produzione spenta
      (`QK_FAST_GOAL=1` l'accende).
      Cosa ha insegnato: la sosta non è solo il modo in cui la fermata
      raggiunge la mappa, è anche ciò che mette un frame fresco e una posa
      corretta davanti al piano della tappa successiva. Ripartire su un
      piano stantio significa sbattere negli angoli che il piano non
      conosceva.
      Dove va davvero il viaggio: 7-8 m camminati per farne 3, tredici
      giri sul posto per strada, e circa 60 s dei 200 passati a
      camminare. **Il percorso e i giri sono il bersaglio**, non le soste
      — una questione di chi segue il percorso, e dietro di essa la posa:
      a metà giro la convinzione distava 0,70 m dal vero, abbastanza per
      far vedere al pianificatore muri dove c'è pavimento.
      (Una parola per chi legge dopo: `robot.go_to` parla nel sistema
      della mappa dell'anatra, la cui origine è dove si è accesa. Dargli
      coordinate del mondo l'ha mandata a camminare verso un punto fuori
      dall'appartamento ed è costato un giro. Ora `speed_test.py`
      converte.)

- [x] Dimezzato: la corda tirata dritta, e niente panorami per strada
      (2026-09-10). Due modifiche, ciascuna misurata sugli stessi due
      tragitti da tre metri:
      **La mira.** Un percorso su griglia è a scalini, e chi lo seguiva
      puntava al punto otto celle più avanti, così la direzione voluta
      ballava di mezzo quadrante alternandosi — tredici giri sul posto per
      viaggio, da 64° a 101° l'uno. Ora punta al punto più lontano del
      percorso raggiungibile in linea retta, con la corsia libera sia sui
      muri sia sugli appunti locali (`QK_SMOOTH_PATH=0` torna al vecchio
      modo). Giri 13 → 7, e tappe a vuoto, retromarce e rifiuti tutti → 0.
      **I panorami.** Cronometrando ogni giro del ciclo si è visto che la
      tappa normale dura 4,4 s e va benissimo, mentre uno o due giri per
      viaggio duravano 78 s e 113 s — panorami, in mezzo a un tragitto su
      pavimento già mappato, pari al 60–76 % dell'intero viaggio. Il
      panorama è il modo in cui un lavoro di mappatura impara una stanza
      mai vista; un viaggio non ha nulla da impararci, e se incontra
      davvero dell'ignoto i guardiani rifiutano il passo e il pianificatore
      gira intorno. Saltato quando il lavoro ha un obiettivo. (Inoltre: il
      panorama vedeva la richiesta di stop solo alla fine, così un
      `map_explore {stop}` restava inascoltato per cento secondi. Ora
      controlla fra un passo e l'altro.)
      Insieme: **221 s → 106 s e 192 s → 124 s**, rapporto percorso/retta
      2,48 → 1,67 e 2,07 → 1,89, velocità utile 0,013–0,015 →
      **0,025–0,029 m/s**, con la stessa precisione d'arrivo (15–21 cm).
      Il giro più lungo del ciclo è passato da 113 s a 8,6 s.
      Resta la tappa in sé: 1,5 s di cammino e 3 di sosta, quindi ancora un
      quinto della velocità di passo. Togliere la sosta era stato misurato
      e non pagava *prima* di queste due modifiche; vale la pena richiederselo
      ora che il percorso è dritto.

- [x] La sosta resta, deciso per ripetizione (2026-09-10). Cinque
      tragitti da tre metri per ogni impostazione, dopo le correzioni sui
      panorami e sulla mira: sosta piena da 3 s 135 · 136 · 133 · 135 ·
      185 s; breve da 1,5 s 87 · 126 · 86 · 156 · 197; nessuna 63 · 110 ·
      113 · 137 · 81. Le mediane scendono (135 → 126 → 110) ma le
      dispersioni si sovrappongono quasi del tutto: su cinque misure la
      differenza non si distingue dal rumore fra un giro e l'altro — lo
      stesso rumore che ha dato 106 s e 135 s per la stessa identica
      configurazione in due giri diversi.
      Ciò che invece si distingue: con la sosta piena quattro tragitti su
      cinque cadono entro tre secondi l'uno dall'altro, nessuna tappa
      manca di muovere l'anatra e non ci sono retromarce. Senza, una e
      tre; con niente, cinque e undici. Quindi la sosta compra
      prevedibilità e un corpo che non va a sbattere, a un prezzo nascosto
      dentro il rumore. Resta (`QK_FAST_GOAL=1`, `QK_FAST_STAND_S` per
      riprendere il discorso).
- [ ] Cosa fanno i lavapavimenti che potremmo fare anche noi (2026-09-10,
      domanda dell'utente — perché mappano un piano intero senza sbagliare
      di un millimetro?). Gran parte della risposta è che giocano un altro
      gioco: un LIDAR rotante vede 360° in un colpo, migliaia di punti al
      secondo, e il contorno completo di una stanza ha un solo modo di
      combaciare con la mappa, mentre uno spicchio di 45° su un muro
      liscio ci scivola sopra liberamente — che è insieme il nostro
      aliasing, le nostre chiusure d'anello dubbie e le corrispondenze con
      l'immagine speculare. E rotolano su encoder sopra un pavimento
      piano, dove noi camminiamo con un sensore su una testa su un corpo
      che beccheggia.
      Tre dei loro trucchi sono alla nostra portata, in quest'ordine:
      **Raddrizzare a rette.** La mappa sul loro telefono non è la griglia
      grezza: i muri sono ricondotti a segmenti dritti e angoli retti. Una
      casa è fatta di segmenti e la nostra griglia non lo sa. Attacca
      proprio il numero che vogliamo abbassare.
      **Ancorarsi alla base.** Ci tornano e si riancorano a ogni giro, il
      che cancella la deriva accumulata contro un riferimento che non si
      muove mai. Il gemello una base ce l'ha e non la usiamo per nulla.
      **Prima il perimetro.** Il loro primo giro è quasi sempre lungo i
      muri, e tenere una parete in vista è ciò che rende ben condizionata
      la stima. L'esplorazione a frontiere copre benissimo e ancora
      malissimo.

- [x] L'anatra nella casa con le curve (2026-09-10). Casa C mappata per
      25 minuti, poi misurata contro la propria geometria:

      | | casa A | casa B | casa C |
      |---|---|---|---|
      | mediana | 0 cm | 0 cm | **5 cm** |
      | 90° perc. | 25 cm | 20 cm | **42 cm** |
      | oltre 10 cm | 18,7 % | 21,6 % | **32,9 %** |
      | raddoppiati | 8,0 % | 3,4 % | **18,9 %** |

      L'immagine dice ciò che i numeri non dicono. L'isola tonda e il
      tramezzo curvo sono mappati con nettezza — le due cose che temevo di
      più. Manca invece **la grande baia dolce**: il pavimento sotto di
      essa risulta esplorato, quindi l'anatra ci è passata, ma del muro
      restano solo pezzi. La ragione non è la curvatura ma l'incidenza
      radente — una parete concava ampia vista da dentro rimanda echi
      lungo sé stessa, e un raggio radente o non torna o viene scartato
      dal filtro che separa il pavimento dai muri. Lo stesso sensore
      traccia un cilindro a un metro e perde un muro dolce a tre.
      Quindi il 32,9 % non si legge "le curve sono difficili", si legge
      **"ciò che il sensore vede di striscio non arriva in mappa"**, e
      casa C ne ha semplicemente molto di più. Varrà anche per l'anatra
      vera, e peggio, perché un muro vero riflette meno gentilmente di uno
      simulato.
      E risolve la questione del raddrizzamento dall'altro lato: in una
      casa così, chi tira rette avrebbe pochissima evidenza vera proprio
      dove la mappa è più povera, e riempirebbe il vuoto con una propria
      invenzione. La rettitudine va nella correzione della posa, non nel
      disegno dei muri.
      (Due strumenti su tre mostravano di nascosto la casa sbagliata fino
      a questa prova: l'immagine della mappa disegnava le scatole di casa
      A sopra la mappa di casa C, e poi disegnava i muri ruotati di casa C
      senza ruotarli. Corretti entrambi. Le cifre di qualità non ne sono
      state toccate: quello strumento aveva imparato la rotazione prima.)

- [x] **Una correzione: lo strumento di misura sceglieva allineamenti
      sbagliati** (2026-09-10, tardi). `mapquality.py` adatta la mappa alla
      casa in modo rigido prima di valutarla, e l'adattamento minimizzava
      la *mediana* delle distanze — che si minimizza splendidamente con un
      allineamento che incolla qualche muro e rovina tutti gli altri. Ne ha
      scelto uno: la stessa mappa di casa C leggeva 36,3 % oltre 10 cm con
      l'allineamento scelto (−8°) e 22,2 % con uno a quattro gradi di
      distanza. Ora l'adattamento minimizza direttamente la quota oltre 10
      cm, con la mediana solo come spareggio.
      Ogni cifra misurata prima di questo è viziata, e i salti non monotoni
      che facevano sembrare caotico un parametro erano in buona parte
      l'adattamento che saltava fra allineamenti. Rimisurate, le tre mappe
      dal vivo di stasera danno:

      | | casa A | casa B | casa C |
      |---|---|---|---|
      | oltre 10 cm | 4,9 % | 11,2 % | 22,5 % |
      | raddoppiati | 0,9 % | 1,5 % | 15,1 % |
      | copertura | 49 % | 49 % | 51 % |

      Entrambe le decisioni della giornata sopravvivono alla correzione,
      una più grande di come sembrava. L'allargamento delle chiusure
      d'anello (5 cm → 40 cm), appaiato per registrazione a pari portata:
      casa C 29,3 % → 14,4 %, casa A 13,4 % → 1,9 % e 14,2 % → 8,0 %, casa
      B 13,2 % → 5,4 % — quattro su quattro, e i muri raddoppiati di casa C
      dal 20,6 % allo 0,5 %.
- [ ] **L'accumulatore tiene solo i primi due metri**
      (`AccumulatorConfig::max_range_m` = 2.0, "il rumore del sensore oltre
      questo punto costa più di quanto la copertura renda"). Il sensore
      arriva a quattro. È il motivo per cui la metà lontana di una stanza
      aperta non entra mai in mappa, e per cui l'ampia baia di casa C manca
      dalla sua mappa — non la curvatura, e nemmeno l'incidenza radente: il
      ToF simulato è un raycast puro senza alcun modello di incidenza, il
      che corregge quanto questo stesso file diceva un'ora prima.
      Alzandolo a 3 m, appaiato per registrazione: casa C 14,4 % → **1,2 %**
      con la copertura dal 48 % al 62 %; casa A 8,0 % → 1,1 %; casa A 1,9 %
      → 8,8 %; casa B 5,4 % → 5,1 %. Due meglio, una peggio, una pari — e
      la forma della cosa ha senso, perché in un appartamento stretto oltre
      i due metri c'è poco da guadagnare e solo rumore da perdere.
      Promettente, non decisa: vuole ripetizioni, e vuole il rumore del
      sensore vero a tre metri, che è un numero di upstream e non del
      gemello.

- [ ] Una politica di cammino che sa girare sul posto (2026-09-11, su
      segnalazione dell'utente: uduckmoves.com, un registro comunitario di
      politiche per il Microduck, Apache-2.0, 18 mosse, 8 con dichiarazione
      di prova su hardware). Due fatti prima di tutto: la "Alpha Dynamic
      Walk" del registro ha lo stesso SHA256 dell'`alpha_walking.onnx` che
      già usiamo, quindi quella voce è la nostra andatura; e il caricatore
      di robotd passa l'ingresso col nome `obs` e legge la prima uscita
      comunque si chiami, quindi qualunque modello 61 → 14 si innesta
      senza modifiche, quanto ai tensori.
      `backlash.onnx` (Genesis, rifinita con ±1° di gioco simulato su ogni
      servo) è 61 → 14 e **cammina**, quindi la disposizione delle
      osservazioni è compatibile. Confrontata con l'alpha sul gemello, una
      ripetizione:

      | | alpha | backlash |
      |---|---|---|
      | dritto | +0,123 m/s | +0,128 m/s |
      | sbandamento | −4,8 °/s | −8,5 °/s |
      | **giro sul posto da fermo** | **+0,2 °/s** | **+28 / −32 °/s** |
      | arco +0,7 | +38,3 °/s | −1,3 °/s (verso sbagliato) |
      | arco −0,7 | −32,0 °/s, avanzando | −26,8 °/s, senza avanzare |
      | retromarcia | −0,122 m/s | −0,084 m/s |
      | cadute | 0 | 0 |

      Girare sul posto è la capacità che più manca all'esploratore: oggi
      ogni cambio di direzione in uno spazio stretto costa prima un calcio
      in avanti, ed è da lì che vengono i tredici giri per tre metri,
      diverse tappe rifiutate e una parte delle retromarce. Ma gli archi
      sono rotti — chiesto un arco a sinistra gira di un grado dalla parte
      sbagliata — e l'esploratore vive di archi, con `GAIT_M_PER_S` e le
      frazioni di avanzamento in arco tutte misurate sull'alpha.
      Quindi: candidato serio, non un ricambio. Gli serve un giro intero
      sul gemello (qualità della mappa, cadute, tempi di percorrenza) e,
      se regge, chi segue il percorso va ritarato attorno a un'andatura
      che sa perniare — il ramo del giro sul posto smette di essere
      l'ultima risorsa. Con l'avvertenza che accompagna tutto ciò che
      viene da quel registro: nessuna delle politiche Genesis ha mai
      camminato su un'anatra fisica.

- [x] La politica che pernia perde lo stesso (2026-09-11). Un giro intero
      di mappatura da 25 minuti in casa A con ciascuna andatura, stesso
      codice, stessa casa:

      | | alpha | backlash |
      |---|---|---|
      | oltre 10 cm | **1,6 %** | 14,0 % |
      | fantasmi | **0,0 %** | 5,8 % |
      | copertura | 46 % | 43 % |
      | cadute | 0 | 0 |
      | giri sul posto | 33 | **27** |
      | tappe rifiutate | **2** | 11 |
      | retromarce | **7** | 14 |

      Girare sul posto paga un poco — sei giri in meno — ma gli archi
      rotti costano cinque volte i rifiuti e il doppio delle retromarce, e
      lo sbandamento (−8,5 °/s contro −4,8) diventa deriva della posa, e
      la deriva sfuma la mappa. Per questo robot **un cammino pulito conta
      più di uno manovriero**: un'andatura che va dove è puntata fa una
      mappa migliore, e una mappa migliore rende tutto il resto più
      facile. Quindi resta alpha, e il valore del registro per noi è il
      fatto che una politica 61 → 14 si innesti affatto: la porta è
      aperta quando ne comparirà una più educata.
      Quel giro con alpha è anche la migliore mappa dal vivo mai
      misurata: 1,6 % di muri fuori posto e nessun muro raddoppiato. È il
      riferimento da battere, ed è la ripetizione che mancava sulla
      portata a tre metri — dal vivo, in casa A, tre metri è ottimo. La
      contraddizione di stanotte resta confinata a casa C, dove abbiamo
      una misura sola.
- [ ] Mentre l'anatra cammina, il ToF non arriva che ai guardiani
      (2026-09-11, domanda dell'utente). `Mapper::frame` si apre con
      `if !self.was_still { return false }`: in stop-and-scan ogni
      fotogramma che arriva mentre il corpo si muove viene scartato prima
      che qualcuno lo guardi. Gli stessi fotogrammi alimentano però il
      guardiano del vuoto, i guardiani della tappa e gli appunti
      dell'esploratore — la sicurezza legge il sensore di continuo — ma
      nulla di ciò che si vede in movimento arriva alla mappa. A 15 Hz una
      tappa di tre secondi ne butta una quarantina, e su un giro è la
      maggior parte dei dati raccolti.
      La ragione è buona: camminando la posa è stima a naso e il torso
      beccheggia, e l'inchiostro steso a una posa sbagliata è lo sfumato
      che abbiamo passato una notte a togliere. Ma
      `MapperConfig::continuous` esiste già e non l'abbiamo mai misurata,
      e ora c'è lo strumento per giudicarla. La via di mezzo da provare
      per prima: **scavare il pavimento libero in movimento, inchiostrare
      i muri solo da fermi** — lo spazio libero perdona un errore di posa
      dove un muro non lo perdona, e la copertura del pavimento (39–51 %)
      è proprio ciò che manca e ciò che fa scattare quei panorami da
      novanta secondi.

- [x] **La diretta e la riprova differiscono, e NON per l'orologio**
      (2026-09-11, corretto la stessa mattina). Il secondo giro di casa C
      ha mappato al 36,3 % di muri oltre 10 cm e 27,1 % raddoppiati; **la
      sua stessa registrazione, rigiocata con lo stesso codice e gli
      stessi parametri, dà 8,5 % e 1,7 %**. Stessi dati, quattro volte
      l'errore — l'anomalia di settembre, ritrovata e stavolta con un
      numero.
      La prima spiegazione scritta qui era sbagliata ed è ritirata: è vero
      che il worker vivo timbra i fotogrammi con `started.elapsed()` e non
      con l'`at_us` del sensore, ma **anche il registratore timbra ogni
      record col proprio `started.elapsed()`, ed è quello che la riprova
      usa** — quindi i due percorsi corrono sullo stesso orologio, jitter
      compreso. Verificati ed esclusi insieme a esso: la costruzione della
      posa, il filtro degli stati, lo specchio delle colonne, la modalità
      continua e il determinismo del mapper (nessun orologio e nessun
      generatore casuale non seminato nel percorso vivo; l'unica HashMap
      che conta viene solo interrogata, e mcl rompe già i pareggi per
      coordinata).
      Ciò che è accertato è che le due mappe differiscono nel *contenuto*,
      non solo nell'allineamento: dal vivo 815 celle di muro e 121
      sottomappe, in riprova 600 e 114. Un mapper deterministico nutrito
      con la stessa sequenza non può farlo, quindi le sequenze differiscono
      — e scoprire come è la prossima cosa da fare. Il piano: rigiocare una
      registrazione due volte e confrontare le mappe, il che separa "il
      banco non è deterministico" da "la registrazione non è ciò che il
      mapper ha visto".
- [x] Casa C a tre metri, quattro giri (2026-09-11): 22,5 % a due metri,
      poi 36,3 %, **3,7 %** e **4,8 %** a tre, con la copertura 51 → 34,
      60, 55 %. Il 36,3 % era il giro qui sopra, quello rovinato
      dall'orologio. Quindi i tre metri reggono anche nella casa curva, e
      il banco aveva ragione fin dall'inizio.

- [x] **Il banco è affidabile, dopotutto** (2026-09-11, pomeriggio). La
      divergenza fra diretta e riprova non si è riprodotta sotto
      controllo. Un giro di casa C ha conservato il registro del demone, e
      il conteggio del mapper vivo corrisponde quasi esattamente alla
      riprova della registrazione di quel giro — 61 635 campioni di
      odometria contro 61 905, 17 243 fotogrammi contro 17 319, 278
      finestre contro 280, 100 sottomappe contro 100, con differenze pari
      ai cinque secondi fra l'ultima riga di stato e la fine del file. Le
      mappe poi concordano a un decimo di punto: 21,7 % di muri oltre 10
      cm dal vivo, 21,6 % in riprova; 15,5 % raddoppiati contro 15,0 %.
      Quindi il 36,3 % contro 8,5 % di stamattina era quasi certamente una
      registrazione accoppiata al giro sbagliato da parte nostra: quella
      riprova mostrava 114 sottomappe e 600 celle di muro dove la sessione
      viva ne aveva 121 e 815, che non è l'aspetto di uno stesso giro.
      L'anomalia di settembre resta una domanda aperta, ma nulla qui la
      sostiene, e del banco ci si può fidare come sostituto di una diretta.
- [x] **Quanto costa mappare, che non avevamo mai chiesto** (2026-09-11,
      osservazione dell'utente: il processore della papera è un RK3566,
      quattro Cortex-A55 e un gigabyte, e il thread di mappatura li divide
      con la parola di risveglio e un ciclo di controllo a 50 Hz).
      Rigiocando la stessa sessione da 1238 secondi: 1,08 s di CPU con la
      portata dell'accumulatore a due metri, 1,30 s a tre, 1,46 s a
      quattro — quindi **il passaggio a tre metri costa il 20 % di CPU in
      più**, e la mappatura nel suo insieme gira a circa 950 volte il
      tempo reale su un core di questo Mac, cioè un millesimo di esso.
      Portato su un A55 a forse un decimo della velocità, fanno 80–120
      volte il tempo reale: attorno all'uno per cento di un core, e il
      venti per cento in più di quell'uno per cento è nulla. I tre metri
      sono sostenibili.
      Con tre avvertenze: la riprova misura il solo mapper, mentre il
      demone vivo disegna anche una griglia ogni secondo e scrive sette
      megabyte di sessione al minuto sulla flash; il lavoro pesante non è
      la media ma gli eventi — una rilocalizzazione a forza bruta, una
      raffica di chiusure d'anello — e cadono proprio quando la posa serve
      di più; e questi sono i volumi di dati del gemello.
      Ridimensiona anche l'esperimento sul carico di stamattina: se
      mappare costa l'uno per cento di un core, cinque core occupati non
      dovrebbero contare, e 11,0 % contro 18,2 % su una coppia sola, in
      una casa i cui giri vanno dal 3,7 % al 36,3 %, non dice ancora
      nulla. **D'ora in poi ogni parametro ha anche una colonna di CPU.**

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
- **Il maploc dal vivo scivola dove il suo stesso replay non scivola
  (2026-09-05).** Sul gemello, con una mappa da 94 sottomappe ereditata da
  un giro umano, la posa `robot.map` dal vivo è saltata fino a 4,6 m e
  `tracking` è caduto, mentre `maploc/examples/evaluate`, rigiocando la
  stessa registrazione `.mdlg`, ha tracciato l'intera sessione entro 4 cm
  di mediana e 0,29 m di massimo proprio in quella finestra, senza mai
  perdersi. Il carico CPU è escluso (build release, microfono cadenzato,
  Mac scarico). Quindi la pipeline dal vivo di robotd — tempi dei
  fotogrammi, il gate di immobilità, la spazzata di ricerca, o fotogrammi
  persi — differisce dal banco. La registrazione
  `microduck-pr202/recordings/1788604159.mdlg` (79 min) e il log del replay
  (`private/drives/replay-1788604159.txt`) sono la prova da portare
  upstream. Finché non si capisce, esplorare su una grande mappa ereditata
  è inaffidabile sul gemello; le corse da zero (mappe piccole) sono rimaste
  entro 0,5 m. Terzo caso, la stessa sera: corsa 49 da zero, 5000 celle,
  Mac scarico — posa dal vivo sbagliata di 0,6–0,8 m dal minuto 10,
  tracciamento perso al minuto 15, poi rilocalizzata 3–5 m fuori posto e
  rimasta lì "tracciata"; il replay di quella registrazione
  (`1788627740.mdlg`) ha tracciato per tutta la sessione, 0,49 m nel
  peggiore dei casi. Quindi né il carico né la dimensione della mappa: il
  maploc dal vivo di robotd differisce dal suo banco. La nostra difesa è
  un controllo di accordo mappa-sensore alle soste (una posa falsa fa
  vedere al sensore muri dove la mappa mostra pavimento) — vedi
  l'esploratore.
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
