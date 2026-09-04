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
- [ ] Riprodurre il bench offline sul Mac: esempi `evaluate`/`replay` di
      `maploc` sulle registrazioni `.mdlg` committate.
- [ ] Provare il percorso del gemello MuJoCo (`scripts/duck-sim` +
      `sim-maploc/`) sul Mac: l'unico modo, prima di dicembre, di vedere
      una mappa costruirsi dal vivo.

## 1. quacksat consuma la mappa
- [ ] Sottoscrizione `robot.map` in quacksat-core (client robotd):
      decodificare `map.frame`, tenere l'ultimo frame, esporre posa +
      tracking + griglia. Controllare la versione API; un robotd senza
      `robot.map` risponde METHOD_NOT_FOUND e la funzione resta spenta in
      silenzio.
- [ ] Supporto `robotd --fake`: verificare se il finto serve `robot.map`;
      altrimenti una fixture che riproduce una sequenza di frame
      registrata.
- [ ] Rilevare "il frame mappa è cambiato" (wipe, ripristino fallito,
      rilocalizzazione dopo un reset di sessione) e invalidare tutto ciò
      che vi è ancorato.

## 2. Luoghi e `where_am_i` (senza telecamera, senza server)
- [ ] Registro dei luoghi: pose con nome nel frame mappa, indicizzate
      per identità della sessione di mappa, salvate nella directory di
      stato di quacksat. Insegnate a voce ("questa è la cucina") o
      dall'agente.
- [ ] Strumenti per l'agente: `where_am_i()` → luogo più vicino +
      distanza + confidenza del tracking; `list_places()`,
      `remember_place(name)`, `forget_place(name)`. Esporli nella
      allowlist del bridge, nel server MCP lato anatra e nel backend
      `direct`.
- [ ] Passeggiata di mappatura come comportamento guidato: l'agente (o
      l'utente) conduce un giro stop-and-scan; quacksat riferisce a
      parole il progresso di `windows`/`n_submaps`. Richiede
      `[maploc] enabled = true` sul robot.
- [ ] Gestire con onestà `seated`/`tracking = false` nelle risposte
      ("non sono sicura di dove sono, devo alzarmi e guardarmi intorno").

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
