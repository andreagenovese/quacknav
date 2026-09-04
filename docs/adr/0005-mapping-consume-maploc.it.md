# ADR 0005: Mappa e localizzazione — consumare il maploc di robotd

- Stato: accettata
- Data: 2026-09-04
- Input: `docs/todo-map.md` (versione del 2026-08-31, ora superata), PR
  upstream 127 "Maploc: mapping & localization as a robotd-hosted
  subcrate", PR 202 "maploc on the MuJoCo twin", PR 126 "ToF
  reprojection & gaze IK", `apirrone/microduck_maploc_rs`, ADR 0001
  (repo separato), ADR 0004 (protocollo agent)

## Contesto

La seconda traccia di quacksat dopo la voce è dare all'anatra una mappa
della casa e il senso di dove si trova, così che l'agente possa
rispondere a "dove sei" e agire su "vai in cucina". Il primo piano
(2026-08-31) assumeva che la scheda non potesse reggerlo: tutto fuori
bordo, telecamera streammata a un server GPU con uno SLAM visivo
monoculare, AprilTag sugli stipiti come prima rilocalizzazione a basso
costo, uno scene graph semantico sopra.

Una ricognizione dei repo Pollen il 2026-09-03 ha cambiato la premessa:

- Un ingegnere di Pollen ha scritto `microduck_maploc_rs` per il runtime
  prototipo: SLAM 2D a submap sul ToF 8×8 più odometria di contatto, con
  chiusura dei loop, ottimizzazione del pose graph, rilocalizzazione
  Monte Carlo da mappa salvata e pianificatore A* con follower. Rust
  puro, dimensionato per i quattro Cortex-A55 della Radxa Zero 3.
- La PR 127 (2026-08-21, aperta, senza review, in conflitto con main al
  momento della scrittura) lo assorbe come sottocrate `maploc/` ospitato
  da robotd su un thread worker a bassa priorità, corregge sei bug che
  rendevano rumorosi i risultati del prototipo e inaffidabile la sua
  rilocalizzazione, ed espone la mappa via IPC: sottoscrizione
  `robot.map`, notifiche `map.frame` a ~1 Hz (posa nel frame mappa, flag
  di tracking, griglia di occupazione ternaria), `robot.map_wipe`. Spento
  di default tramite `[maploc]` in robotd.toml.
- La PR 202 (2026-09-02) l'ha eseguito sul gemello MuJoCo e, dopo aver
  corretto tre problemi lato simulatore, ha misurato la posa tracciata
  entro ~6 cm dalla verità mentre l'odometria grezza derivava fino a
  0,35 m.
- La PR 126 (in main) ha dato a robotd la riproiezione ToF con filtro
  pavimento e un RPC di sguardo `robot.look`.
- Manca ancora upstream: un RPC di goal/navigazione (pianificatore e
  follower portati ma dormienti) e la rilocalizzazione MCL al boot
  cablata in robotd.

La preferenza dichiarata dell'autore è far girare la mappatura sul robot
stesso.

## Decisione

### 1. Mappa e localizzazione sono compito di robotd; quacksat consuma

quacksat non implementa uno SLAM, né a bordo né fuori. Si sottoscrive a
`robot.map` come lo stesso client non privilegiato che già è per
`robot.state` (modello padd, ADR 0001): non apre mai il ToF, la
telecamera o l'odometria, e se tace nulla cambia nella mappa. Un robotd
senza `robot.map` (METHOD_NOT_FOUND) lascia semplicemente la funzione
spenta.

### 2. quacksat possiede lo strato sopra la mappa

Ciò che manca alla mappa è il significato. quacksat aggiunge:

- un **registro dei luoghi**: pose con nome nel frame mappa, insegnate a
  voce o dall'agente, indicizzate per sessione di mappa così che un wipe
  o un ripristino fallito le invalidi invece di puntare in silenzio
  altrove;
- **strumenti per l'agente** sul protocollo esistente (ADR 0004):
  `where_am_i`, `list_places`, `remember_place`, `forget_place`; più
  avanti `go_to` e `look_at`. Esposti tramite la allowlist del bridge, il
  server MCP lato anatra e il backend `direct` come ogni altro strumento
  del robot;
- un **giro di mappatura guidato**: la mappatura stop-and-scan ha bisogno
  di qualcuno che porti l'anatra in giro con delle pause; quacksat
  racconta il progresso da `windows` e `n_submaps`, non finge che la
  mappa si costruisca da sola.

### 3. La navigazione aspetta un RPC di goal upstream

`go_to` ha bisogno del pianificatore e del follower che già vivono nel
crate maploc. Invece di duplicare un follower sopra gli intenti
`robot.move` in quacksat, seguiamo upstream per un RPC tipo `robot.goto`
e, se non ne compare nessuno all'arrivo dell'hardware (dicembre 2026), lo
proponiamo come PR sul repo Pollen, in coerenza con il "patch upstream
via PR quando serve" dell'ADR 0001.

### 4. La pista visiva fuori bordo è retrocessa, non cancellata

La semantica dalla telecamera (cosa c'è nella stanza, `where_is(object)`)
resta una fase successiva opzionale, solo se luoghi più `where_am_i` si
rivelano insufficienti. Quando arriverà resterà locale: il video di casa
non lascia mai il server locale.

## Conseguenze

- Il lavoro a breve è testabile sul Mac senza l'anatra: il client IPC, il
  registro dei luoghi e gli strumenti girano contro sequenze `map.frame`
  registrate, e il percorso del gemello MuJoCo della PR 202 può costruire
  una mappa vera sul Mac.
- Dipendiamo da una PR non ancora in main. Il client fissa la versione
  API contro cui è stato costruito e si aspetta un bump; nulla entra in
  una release finché la forma upstream non si stabilizza.
- Le etichette dei luoghi durano quanto la sessione salvata finché la
  rilocalizzazione al boot non arriva upstream. Il design del registro
  deve renderlo visibile all'utente invece di rispondere con un luogo
  stantio.
- La CPU è condivisa: maploc è la cosa più pesante che robotd possa
  eseguire, e la wake word gira sugli stessi quattro core. Il budget si
  misura a dicembre, e `[maploc]` resta opt-in sul robot.
- Il server GPU, la pipeline di registrazione e la valutazione dei
  modelli SLAM del piano precedente escono dalla roadmap. Se la fase 4
  partirà mai, avrà il suo ADR.
