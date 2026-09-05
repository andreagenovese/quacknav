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
  entro 0,5 m.
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
