# maploc — flusso dati dentro robotd

Fonte: PR upstream 127 (sottocrate `maploc/` + `robotd/src/maploc.rs`,
API v17) letta il 2026-09-04, più i crate `odometry`, `tof` e
`kinematics::tof` su main. Diagramma: `maploc-dataflow.mermaid`. Tutto
ciò che segue descrive la PR com'era quel giorno; non è in main e può
cambiare.

La versione in un paragrafo: **solo due sensori alimentano la mappa — il
ToF sulla testa e le gambe.** Ogni 20 ms il loop di controllo passa al
mapper una piccola struttura (odometria di contatto, gravità, giunti
della testa, tre verdetti); quindici volte al secondo un thread separato
gli passa un frame di profondità 8×8 preso da tofd. Il mapper inchiostra
solo quando il robot è fermo, fa votare i frame di un'intera fermata in
un'unica scansione larga, la verifica contro la mappa prima di crederci
e la dipinge in una submap di 4×4 m. Il congelamento di una submap
innesca chiusura dei loop e ottimizzazione del grafo. Ne esce una
griglia ternaria più una posa, una volta al secondo, verso chi si è
sottoscritto. La telecamera non c'entra.

## 1. Cosa entra in robotd

| Sorgente | Percorso | Frequenza | Contenuto usato da maploc |
|---|---|---|---|
| Bus Dynamixel (`/dev/ttyS2`) | un `sync_read` per tick nel loop di controllo | 50 Hz | 15 posizioni servo (i giunti della testa sono le posizioni 5–8: neck_pitch, head_pitch, head_yaw, head_roll); quaternione e gravità proiettata dell'IMU del tronco |
| odometria di contatto (crate `odometry`, una struct dentro il loop) | FK dei piedi sul modello MJCF + yaw dell'IMU | 50 Hz | `(x, y, yaw)` nel frame odometrico ("dove guardava il robot al boot", niente magnetometro) e altezza del tronco `z` |
| verdetti del loop | etichetta della policy / controller / safety | 50 Hz | `moving` (la policy dice "walk" o un movimento scriptato è in corso), `sitting`, `fallen` |
| ToF sulla testa VL53L5CX/L8CX | bus I²C condiviso con il codec audio → **tofd**, demone a sé | 15 Hz | `TofFrame { seq, at_us, rows, cols, distance_mm[64], status[64] }` via `tof.stream` su `/run/tofd/tof.sock` |
| `/etc/robot/robotd.toml` `[maploc]` | letto all'avvio | una volta | `enabled`, `mode` (`stop_and_scan` \| `continuous`), `map_path`, `wipe_on_boot`, `search_sweep`, `record_dir` |
| `/var/lib/robot/maploc.session` | caricato all'avvio del worker | una volta | submap precedenti, pose graph, posa tracciata (bincode) |
| client IPC | `/run/robotd.sock` | a richiesta | `robot.map` (sottoscrizione), `robot.map_wipe` |

Non sono input: la telecamera e mediad, la seconda IMU, WebRTC. Il ToF ha
un campo visivo di 45°×45° e la sua portata utile è tagliata a 2 m
dall'accumulatore.

## 2. Come arriva al worker

Due thread oltre al loop di controllo, entrambi con nice +10 così che il
loop vinca ogni contesa per un core:

- **`maploc-tof`**: si connette a tofd come qualsiasi client (`hello` +
  `tof.stream`), si riconnette con backoff fino a 10 s, e spinge ogni
  frame nel canale del worker. tofd spento o assente significa
  mappatura ferma, nient'altro.
- **worker `maploc`**: possiede la pipeline. Alimentato da un canale
  `mpsc` di 128 eventi (`Odom`, `Frame`, `Wipe`, `Shutdown`). Il loop di
  controllo paga esattamente un `try_send` per tick; un canale pieno
  scarta il campione (i delta odometrici si ripiegano sul prossimo
  accettato, un frame di profondità perso è uno dei quindici al
  secondo). Il ritardo della mappatura non può mai diventare
  contropressione sul loop.

Quando `record_dir` è impostato, il worker scrive anche tutto ciò che
consuma in un file `.mdlg` (odom 45 byte per tick + frame ToF grezzi,
~6 KB/s): il bench offline lo riproduce attraverso lo stesso `Mapper`
byte per byte.

## 3. Il percorso per tick (odometria, 50 Hz)

`Mapper::observe(t, sample)`:

1. **Composizione del delta.** L'odometria grezza vive nel suo frame;
   solo il delta in frame corpo tra due letture consecutive viene
   applicato alla **posa tracciata**, che vive nel frame MAPPA. I due
   frame coincidono finché una chiusura di loop o una rilocalizzazione
   non dice altrimenti.
2. **Fermezza.** Una finestra di 0,5 s di odometria: fermo quando la
   traslazione è < 1 cm, |yaw| < 0,05 rad, e non moving/sitting/fallen.
   La fermezza viene dall'odometria stessa, quindi un robot spinto a
   mano non è fermo.
3. **Chiusura della finestra.** Quando una sosta finisce, o 3 s dopo
   l'apertura della finestra, l'accumulatore chiude e il composito va
   ad `absorb_window` (§5).
4. **Inizio sosta → istantanea.** Il render globale corrente viene
   congelato come `stand_grid`: il watchdog giudica le finestre di
   questa sosta contro la mappa *com'era prima della sosta*, così un
   robot rapito non può garantire per sé con l'inchiostro appena
   dipinto.
5. **Seduta o caduta → sospetto.** La posa diventa sospetta
   (`lost = true`) con "non sono stato spostato" come seme morbido.
   Niente inchiostra finché una finestra non conferma la posa.
6. **Se in tracking: `Slam::tick`.** Il gestore delle submap congela la
   submap corrente quando ha più di 8 s (e il robot si è mosso ≥ 15 cm)
   o la posa ha percorso 0,8 m dalla sua ancora; una nuova submap si
   apre ancorata alla posa tracciata e il suo nodo del grafo viene
   concatenato al precedente subito. Al congelamento parte il loop
   closer (§6).

## 4. Il percorso per frame (ToF, 15 Hz)

1. **Decodifica.** 64 zone; una zona conta solo con status 5 o 9 e
   distanza positiva. I millimetri diventano metri.
2. **Postura.** Gravità proiettata e altezza del tronco dall'odometria
   dell'ultimo tick (fallback all'altezza di riposo del modello quando
   il robot non è in piedi).
3. **Riproiezione** (`kinematics::tof::Reprojector::flatten`, già in
   main): la tabella dei raggi 8×8 (FOV 45°, mezza zona di inset) viene
   ruotata attraverso la cinematica diretta della testa *per ogni
   frame*, così una testa che ruota si riproietta correttamente. I
   ritorni sotto 0,10 m vengono scartati (crosstalk del vetrino); un
   raggio la cui componente verso il basso copre l'85 % dell'altezza del
   sensore dal pavimento è pavimento, non ostacolo. I superstiti vengono
   espressi nel frame corpo livellato dalla gravità come azimut +
   distanza orizzontale **misurata dal sensore**, più la posizione del
   sensore in frame corpo. Le origini per raggio sono ciò che permette a
   frame con yaw della testa diversi di fondersi esattamente.
4. **Scansione.** `Scan::from_polar` → `Mapper::frame`:
   - modalità `continuous` e in tracking: inchiostra direttamente alla
     posa tracciata.
   - `stop_and_scan` (default), o continuous mentre è perso: solo se il
     robot è fermo, spinge `(posa tracciata, scansione)`
     nell'accumulatore. I frame durante la camminata vengono scartati.

## 5. Il percorso per finestra (una per sosta, o ogni 3 s di sosta)

`WindowAccumulator::finish` poi `Mapper::absorb_window`:

1. **Voto.** Gli estremi di ogni frame vengono raggruppati in celle da
   5 cm; un raggio sopravvive solo se la sua cella è stata colpita da
   ≥ 3 frame distinti. I raggi più lunghi di 2 m vengono scartati. I
   superstiti si fondono in un'unica scansione composita alla posa
   mediana della finestra. Le finestre con meno di 6 frame passano senza
   filtro. Un passante è in un posto diverso in ogni frame e perde il
   voto; un muro lo vince.
2. **Finestre sottili** (< 60 raggi) vengono scartate. Le finestre di
   detriti a pavimento di un robot seduto misuravano 2–27 raggi; una
   sosta vera si misura in centinaia.
3. **Se perso:** prima si verifica il seme morbido, che deve concordare
   con due finestre consecutive prima che il tracking riprenda su di
   esso; poi l'ultimo candidato della ricerca, che la finestra corrente
   deve confermare (residuo medio ≤ 0,10 m su ≥ 30 % dei suoi raggi);
   altrimenti una rilocalizzazione a forza bruta
   (`relocalize_against_grid`, il composito decimato a 256 raggi,
   griglia grossolana + raffinamento) propone un nuovo candidato che
   sarà la finestra *successiva* a giudicare. Dopo 10 finestre che la
   mappa non ha potuto giudicare affatto, il sospetto morbido si arrende
   e il tracking riprende alla posa portata dall'odometria, non
   verificata. Una finestra che la mappa *confuta* toglie quella via
   d'uscita.
4. **Se in tracking: watchdog.** Il composito viene valutato contro
   `stand_grid`. Se la mappa può giudicare ≥ 100 raggi e ≥ 5 % di essi,
   e il residuo medio supera 0,25 m, la finestra viene messa in
   quarantena (non inchiostrata); due contraddizioni consecutive
   dichiarano il tracking perso. Un esploratore in una stanza nuova
   finisce in territorio che la mappa non può giudicare e continua a
   mappare; un robot rapito finisce dove la mappa conosce e la
   contraddice ovunque.
5. **Inchiostro.** Due passate di log-odds nella submap corrente (+85
   per passata per cella muro, un muro parte da 150, quindi una finestra
   verificata fa un muro); lo spazio libero lungo ogni raggio viene
   decrementato. `windows` sale; la pipeline è marcata sporca.

## 6. Chiusura dei loop e ottimizzazione (al congelamento di una submap)

- Ogni submap è una griglia log-odds locale di 4×4 m a 5 cm, ancorata
  nel frame mappa, con le sue scansioni grezze conservate.
- Al congelamento, le submap più vecchie entro il raggio del loop closer
  (non l'immediata precedente) vengono confrontate: ricerca correlativa
  grossolana (±0,5 m, ±20°) poi raffinamento Gauss-Newton sul campo di
  distanza in cache. Una chiusura richiede i gate per scansione su
  residuo, raggi e copertura, **due testimoni forti concordi** (≥ 150
  raggi; i ritagli da 12 raggi ponevano il veto ai compositi), una
  correzione plausibile per la deriva odometrica sul tratto, e un
  pavimento di correzione così che gli archi di rumore vengano
  scartati.
- Le chiusure accettate aggiungono archi al pose graph SE(2), pesati per
  il residuo del match; un ottimizzatore Gauss-Newton denso rilassa il
  grafo; ogni ancora di submap **e la posa tracciata** si muovono con i
  loro nodi (il prototipo dimenticava la posa tracciata; il port lo
  fissa con un test). I frame già nell'accumulatore vengono scartati,
  perché le loro pose sono pre-correzione.

## 7. Cosa esce

| Uscita | Percorso | Cadenza | Contenuto |
|---|---|---|---|
| notifiche `map.frame` | sottoscrittori di `robot.map` su `/run/robotd.sock`, buffer broadcast 4 | 1 Hz, solo mentre qualcuno è sottoscritto | `seq`, posa `x, y, yaw` nel frame mappa, `tracking`, origine `x_min, y_min`, `cell_m` (0,05), `rows × cols`, `cells` base64 (0 ignota, 1 libera, 2 muro: log-odds > 150 muro, < −50 libera), `n_submaps`, `n_loops`, `windows`, `still`, `seated` |
| override dello yaw della testa | il loop di controllo legge `Host::searching()` | ogni tick in piedi, se `search_sweep` e (in ricerca **oppure** modalità stop-and-scan) | onda triangolare ±0,9 rad su 6 s solo sullo yaw della testa — un cono di 45° diventa un composito di ~150°. **È l'arco di retroazione che rende ciclico il flusso**: lo stato del mapper muove la testa, la testa muove il sensore, il sensore alimenta il mapper |
| file di sessione | `map_path` | autosalvataggio ogni 60 s se sporca, allo shutdown, allo smontaggio da panic | submap + pose graph + posa tracciata, scrittura atomica |
| registrazione `.mdlg` | `record_dir/<unix time>.mdlg` | continua mentre attiva | tutto ciò che il mapper ha consumato |
| journal | tracing | stato ogni 5 s + una riga per nota | `odom/frames/kept/windows/still/tracking/moving/sitting/fallen/window_frames/submaps`; note: finestra integrata / scartata / in quarantena, sospetto dopo seduta/caduta, candidato di rilocalizzazione / rilocalizzato / rifiutato, tracking perso, loop chiuso, ripreso non verificato |
| `robotctl monitor` | si sottoscrive a `robot.map` | 1 Hz | il pannello del percorso diventa la mappa (muri in braille, spazio libero puntinato, marcatore del robot, `?` magenta in ricerca); `m` a schermo intero |
| risposta a `robot.map_wipe` | RPC | a richiesta | accettato / rifiutato ("mapper sovraccarico" o "mappatura non abilitata") |

Dormienti, senza RPC: il pianificatore A* (8-connesso, inflazione degli
ostacoli, semplificazione a linea di vista) e il follower gira-poi-vai
che emette velocità in frame corpo `(vx, wz)`. `robot.look` (IK dello
sguardo) esiste in main indipendentemente.

## 8. I tempi a colpo d'occhio

| Orologio | Periodo |
|---|---|
| tick del loop di controllo | 20 ms |
| frame ToF | 66 ms |
| finestra di fermezza | 0,5 s |
| chiusura della finestra di sosta | a fine sosta o 3 s |
| sweep della testa | 6 s per triangolo |
| congelamento submap | 8 s (se mosso ≥ 15 cm) o 0,8 m percorsi |
| pubblicazione mappa | 1 s |
| log di stato | 5 s |
| autosalvataggio sessione | 60 s |
| ricerca di rilocalizzazione | "qualche centinaio di ms" una tantum su griglia 4×4 m |

## 9. Cosa significa per quacksat

- **Consumiamo un solo stream.** Sottoscrivere `robot.map`, tenere il
  `map.frame` più recente, decodificare la griglia base64 solo quando
  serve. Un robotd più vecchio della v17 risponde METHOD_NOT_FOUND: la
  funzione resta spenta, nessun errore.
- **La posa ha senso solo con `tracking = true`.** Mentre `seated` il
  mapper si rifiuta di mappare o rilocalizzare; mentre cerca la testa
  ruota da sola e gli intenti `robot.head` da parte nostra la
  contrasterebbero. `where_am_i` deve dire "non sono sicura" in quegli
  stati.
- **Il frame mappa può muoversi.** Una chiusura di loop sposta ogni
  ancora e la posa tracciata; un wipe azzera tutto; una sessione
  ripresa parte sospetta. I luoghi vanno salvati nel frame mappa di una
  data sessione e rivalidati dopo un wipe. Oggi non c'è un id di
  sessione sul filo: `seq` riparte da 1 e `n_submaps` scende a 0 dopo
  un wipe, ed è il segnale che abbiamo.
- **Un giro di mappatura è deliberato.** Niente inchiostra mentre
  cammina. Il robot deve fermarsi, restare in piedi ≥ 0,5 s e lasciar
  correre lo sweep; `windows` sul filo dice se le soste stanno
  raggiungendo la mappa. Il giro guidato racconta esattamente questo.
- **CPU.** Worker e feed hanno il nice; il loop tiene i suoi 50 Hz. La
  ricerca di rilocalizzazione costa centinaia di ms e il render cresce
  con la mappa. La wake word condivide i core; a dicembre si misura.
- **Niente con cui navigare, ancora.** `go_to` aspetta un RPC di goal
  che cabli pianificatore e follower; la griglia grezza è sul filo se
  mai volessimo pianificare da soli, ma l'ADR 0005 dice di no.
