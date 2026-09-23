# ADR 0007: Il mapper vive in quack-navd, e robotd resta quello di Pollen

- Stato: accettato
- Data: 2026-09-23
- Input: ADR 0005 (consumare maploc), ADR 0006 (la navigazione è un
  demone a sé), `docs/study/upstream-asks.md`, i giri sul gemello del
  2026-09-22/23, la decisione dell'utente del 2026-09-23

## Contesto

L'ADR 0005 ha scelto di consumare `maploc` così come lo ospitava robotd
(PR upstream 127). Tre settimane sul gemello ci hanno aggiunto una
modalità Localize, una libreria di mappe (`robot.map_save`, `map_list`,
`map_load`, `map_match`, `map_adopt`), una ricerca all'avvio e le
correzioni di `upstream-asks.md`: circa 3.400 righe su un branch di
`pollen-robotics/microduck` che esiste su un solo portatile. La
navigazione le usa tutte: senza la libreria non c'è homecoming, senza
Localize non c'è una casa mappata una volta e percorsa molte.

Così quacknav, pubblicato da solo dal 2026-09-22, non poteva farlo
girare nessun altro. La PR 127 è ancora aperta; le aggiunte sopra non
sono nemmeno proposte; se venissero rifiutate, l'unico modo di provare la
navigazione sarebbe un robotd patchato, che sulla papera vuol dire un
binario che updaterd non gestisce, da ribasare a ogni release di Pollen.

Quello che robotd dà davvero a `maploc` è poco: una struttura per tick
del loop di controllo (odometria di contatto, gravità, altezza del
tronco, i giunti della testa misurati, un timestamp monotono, tre
verdetti) e i frame di profondità di tofd. Il robotd ufficiale
(daemon-v0.14.4, API 34) pubblica ogni campo di quella struttura su
`robot.state` dall'API v24, sullo stesso orologio di `tof.stream`.

## Decisione

### 1. quack-navd ospita maploc; robotd è la release, senza modifiche

`maploc` è incluso come crate del workspace (il `maploc-quacknav` del
fork, 16070fd, con il `NOTICE` che riconosce la PR 127 di Pollen).
`quack-nav::mapd` fa girare il worker del fork (`robotd/src/maploc.rs`)
riga per riga, alimentato da fuori:

- `robot.subscribe` senza frequenza dà ogni tick (50 Hz); `tof.stream`
  ogni frame (~14 Hz). I due timestamp abbinano un frame alla testa del
  suo istante, come faceva il fork (9 ms al peggio sul gemello).
- Il `kinematics` ufficiale ha `Reprojector::project` e non `flatten`;
  `maploc::flat` lo ricostruisce con l'API pubblica. Una registrazione
  del gemello, rigiocata con `evaluate`, dà le stesse 200 righe e la
  stessa mappa, byte per byte, qui e nel fork.
- `[maploc]` in `quack-nav.toml` tiene la vecchia sezione di robotd,
  spenta per default. Spenta, la mappa arriva da un robotd che ospita
  maploc (il fork funziona ancora); accesa, da questo demone.

### 2. La mappa tiene il dialetto di robotd, su un socket suo

`quack-navd` serve `robot.map` e la libreria su
`/run/quack-nav/map.sock`, con i metodi e le forme del fork. Il resto
della navigazione, l'homecoming e il viewer del gemello la leggono senza
cambiare; `NavdConfig::map_socket()` dice dove chiedere. Entrambi i
socket del demone stanno nella `RuntimeDirectory` della unit — l'unico
posto sotto `/run` dove può fare bind — con modo 0660 e gruppo `robot`,
come robotd e tofd condividono i loro (ADR 0006 §2, come emendato).

### 3. Due verdetti sono ricostruiti, con la regola del fork

`robot.state` non porta il `moving` né il `sitting` del loop. Entrambi
vengono dall'etichetta dello step: moving è ogni etichetta tranne
`stand`, `sit` e `held`, sitting è `sit`. È la regola del fork (`busy ||
label == "walk"`), non quella della release (`twist_magnitude() > 0.0`):
il twist smussato decade verso zero per un minuto senza arrivarci, e con
la regola della release nessuna sosta arrivava alla mappa (39 finestre in
tre minuti).

### 4. La scansione della testa chiede prima di prendersi la testa

Il loop di robotd muoveva la testa a ogni sosta. Da fuori è `robot.head`
a 20 Hz — e lo slot è condiviso, vince l'ultimo che scrive. La
scansione gira solo mentre la navigazione guida (un'esplorazione, un
viaggio, l'homecoming che esplora) o il mapper cerca la posa, e si fa da
parte mentre la testa comandata porta un pitch, un neck pitch o un roll
— valori che non scrive mai — e per 5 s dopo. Una papera che chiacchiera
in salotto tiene la testa ferma; la posa pensante di quacksat e un
`robot.look` restano in pace.

### 5. Il worker tiene la priorità del fork

robotd gira a nice 0. La unit mette `quack-navd` a 5; il worker di
maploc abbassa il proprio thread a 10, dove girava dentro robotd, così
una ricerca di rilocalizzazione pesa un decimo del loop di controllo
quando i core sono contesi.

## Conseguenze

- Chiunque può far girare la navigazione con il robot che Pollen
  spedisce: il robotd ufficiale, `quack-navd` e (per il gemello)
  `scripts/twin/`. Niente in robotd è patchato, niente aspetta un merge
  upstream.
- Misurato sul gemello contro il fork, 2026-09-23: esplora, riconosce la
  casa salvata all'avvio (78 s contro 95), va in cucina (37 s contro
  32); muri contro l'appartamento vero 0,032 m contro 0,031 sullo stesso
  percorso scriptato, due volte — dentro la variabilità del fork stesso.
- La libreria di mappe, Localize e la ricerca all'avvio ora sono di
  questo repo da mantenere. Se Pollen unisce maploc con una libreria sua,
  si spegne `[maploc]`, subentra il `robot.map` della release e `mapd`
  si toglie.
- Le modifiche del fork a maploc hanno ancora una casa upstream:
  `upstream-asks.md`, e una riga che la release potrebbe prendere dal
  fork — il verdetto `moving` — vale la pena chiederla comunque vada qui.
- Resta aperto: una posa estranea fatta solo di yaw non si distingue da
  quella della scansione; `setpriority` sotto il `SystemCallFilter` della
  unit e il gruppo dei socket sono da verificare sulla papera fisica.
- Misurato la stessa sera contro il fork, sul gemello (`scripts/twin/`):
  - un'esplorazione autonoma di 30 minuti: 82 tratti, 157 submap, nessuna
    caduta, il loop di controllo a 50 Hz senza tick persi; i suoi muri a
    0,043 m da quelli veri (la registrazione di house2, rigiocata con il
    mapper di oggi: 0,061), e `map_match` la trova dentro house2 con una
    trasformazione quasi identica e uno scarto dei muri di 0,030 m;
  - la posa sulla stessa mappa (house2, localize), negli stessi otto punti
    della casa: 0,050 m di media e 0,077 di massimo qui, 0,086 e 0,223 sul
    fork — nessuna perdita di precisione;
  - la camminata con gli stessi comandi è quella del fork (da un punto
    libero, lo yaw entro 0,06 rad/s a ogni comando): le tarature di
    `[gait]` restano;
  - viaggi ciechi su sei mete: il fork 16/18 e 12/12 in cinque round, nessuna
    caduta; qui 12/18 sulla mappa esplorata e una caduta, 4/6 su house2 e una
    caduta nel round dopo — **entrambe le cadute nel passaggio accanto alla tromba delle
    scale**, una con la posa spostata di 17 cm verso il buco. Non ricondotte al
    porting (altrove la posa qui è migliore, e anche il fork ha mancato quella
    meta) e non escluse con così pochi giri: il passaggio è il rischio aperto.
- Trovato la stessa sera: la camminata gira sul posto da ferma sopra una zona
  morta che l'esploratore non ha mai superato (`yaw_max` 0,9): 30°/s a
  +1,2 rad/s, 50–60°/s a ±1,5, il corpo entro 4 cm
  (`scripts/twin/turnprobe.py`). Il colpetto seguito dallo yaw
  dell'esploratore, e le manovre all'indietro accanto a un dislivello dove sono
  avvenute entrambe le cadute, potrebbero non servire.
- Trovato per strada, non causato da questo: la rotazione del panorama
  si bloccava quando il suo colpetto veniva rifiutato (corretto,
  495e8b7), e il socket di tofd dell'esempio e i percorsi di default dei
  socket erano sbagliati per il board.
