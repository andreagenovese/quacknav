# quack-nav v0.3.0-rc3 — note di rilascio

2026-10-27. Copia inglese: [release-notes-v0.3.0-rc3.md](release-notes-v0.3.0-rc3.md).
L'elenco completo delle modifiche è nel [CHANGELOG.it.md](../CHANGELOG.it.md);
le note della candidata precedente sono in [release-notes-v0.3.0-rc2.it.md](release-notes-v0.3.0-rc2.it.md).

## Che cos'è

Una **quinta release candidate, validata solo sui gemelli**: il gemello
MuJoCo del Microduck (i daemon v0.16.1 e v0.17.6 di Pollen e il corpo di
`microduck_rl`) e il gemello di carta. **Non ancora su una papera vera**:
quella fisica arriva a dicembre 2026, e fino a una prima sessione con lei
ogni numero qui sotto è del gemello.

La rc2 riguardava ciò che la mappa 2D non ha. La rc3 riguarda la nuova
linea di daemon di Pollen e il non cadere: la linea 0.17 con i suoi numeri
di camminata e il suo pilota, uno sguardo al pavimento prima di un passo
di cui la mappa non può garantire, e una svolta sul posto che si ferma
dove le è stato detto. Porta anche persone che camminano per la casa, sul
gemello e sul banco allo stesso modo, e un errore in quanto lontane
venivano registrate le cose.

## Prima di installare

- **Niente di nuovo da impostare.** La configurazione della rc2 va così
  com'è. Una nuova chiave facoltativa `daemon` (`"auto"`, il default:
  robotd dice quale daemon è all'avvio; `"0.16"` o `"0.17"` per
  nominarlo) sceglie i numeri della camminata per la linea di Pollen
  ([README, Il daemon](../README.it.md#il-daemon)).
- **Compilata contro daemon-v0.17.6**; la stessa build gira su ogni
  daemon dalla v0.14.4.
- Le mappe e i luoghi salvati dalla rc2 si caricano così come sono.
- Il pacchetto d'installazione porta un pilota in più,
  `pilots/0.17/velstand/`, che quack-navd prende sulla linea 0.17 quando
  `QK_RL_POLICY` nomina la cartella dei piloti. Senza, guida lo stick,
  come prima.

## Cosa è cambiato dalla rc2

- **La linea di daemon 0.17.** Dal daemon-v0.16.2 la papera usa il set di
  policy v8: velstand cammina più lenta in avanti (0,095 m/s a vx 0,3) e
  gira circa due volte più in fretta (una svolta sul posto a 1,0 fa circa
  65°/s). Un file per linea tiene i numeri di velstand; `daemon = "auto"`
  sceglie la linea. Su quella linea anche le svolte sul posto
  dell'esplorazione si fermano in anticipo della coda imparata.
- **Uno sguardo al pavimento prima di un passo su ciò che la mappa non
  conosce.** Il sensore di profondità non vede il pavimento più vicino di
  0,4–0,6 m, e una tromba di scale contro un muro si nasconde in un
  secondo modo. Ogni caduta spericolata sul banco era quella: una svolta
  andata troppo oltre, poi passi dritti in una tromba che nessun
  fotogramma aveva visto. Prima di un passo la cui falcata finisce su
  celle che la mappa tiene né pavimento né muro, o del primo dopo una
  svolta sul posto di 0,5 rad o più, quack-navd ora inclina la testa
  verso il pavimento 0,30 m avanti per 0,45 s e la restituisce; quel che
  vede passa dalle guardie e va nel registro. `QK_LOOK_DOWN=0` lo spegne.
- **Una svolta sul posto si ferma dove le è stato detto.** Un movimento a
  tempo finiva senza uno stop e robotd teneva l'ultimo comando fino al
  suo deadman, 500 ms: un terzo d'angolo retto alla velocità del set v8,
  a ogni svolta. L'esploratore ora manda uno stop alla fine di ogni
  svolta sul posto (`QK_TURN_STOP=0` per la via vecchia). Le svolte
  finiscono 12–16° oltre il chiesto invece di 43–44°, e la coda di cui le
  svolte successive si fermano in anticipo impara quella propria
  dell'andatura, non la tenuta.
- **Il pilota v8l-s3 per velstand sulla linea 0.17**, addestrato sulle
  tracce e sulle case del gemello con il sensore del simulatore reso
  quello del gemello: il primo pilota su quella linea non più lento dello
  stick. Il passaggio di mano fra pilota e stick è più fermo: lo stick
  prende le gambe a un pilota che non si avvicina e le tiene finché il
  corpo è a 0,3 m da dove le ha prese; entro 0,5 m da un bordo registrato
  le gambe le cammina lo stick.
- **Le cose viste venivano registrate 7,8 cm più vicine al corpo di
  quanto fossero.** kinematics misura la distanza di un ritorno dal
  sensore, non dall'asse del tronco che la sua documentazione nomina
  (pollen-robotics/microduck#430); il registro ora usa la distanza dal
  tronco, e una cosa vista è registrata dalla posa del fotogramma che
  l'ha vista.
- **Persone che camminano fra le stanze**, sul gemello e sul banco allo
  stesso modo: `casa_gente` (quattro persone su percorsi fra le stanze,
  un gatto), figure che aspettano la papera entro 0,35 m e si fanno da
  parte, la stessa regola con gli stessi numeri nel gemello e nel
  simulatore.
- Le case del simulatore sono quelle del gemello (`--families
  twin:<casa>`), una papera ribaltata si rialza dopo 4 s sulla linea
  0.17, le righe del sensore seguono l'inclinazione della testa, e il
  simulatore tiene l'ultimo comando di una piroetta fino al deadman se
  non fermata, come fa robotd.
- Il gemello: `twin.sh down` pulisce i socket lasciati da un file pid
  perso; una mappa caricata all'avvio è piazzata una volta confermata la
  posa; `explore_ab.sh` parte in ogni casa da uno stato vuoto.

## Misurato contro la versione precedente

Ogni modifica contro il `main` che la precedeva, sul gemello MuJoCo con
velstand su daemon-v0.17.6, quattro giri per casa, due gemelli alla volta
(le case: casa_ingombra, casa_arredata; 24 viaggi per braccio e casa):

| | prima | dopo |
|---|---|---|
| la chiave `daemon` e i numeri della linea, su daemon-v0.16.1: arrivi, cadute, risvegli | `main` | nessuna regressione |
| pilota v8l-s3 contro lo stick del `main` congelato: viaggi | 48/48 | 48/48, 4,2 e 3,9 s a viaggio più veloce, nessuna caduta |
| lo sguardo al pavimento: viaggi | 48/48 | 48/48, nessuna caduta, 6,2 e 2,7 s a viaggio in più (4,4 e 3,4 sguardi a viaggio) |
| lo sguardo al pavimento: case esplorate da zero, pavimento mappato dopo due sessioni | 63 / 69 % | 76 / 78 %, nessuna caduta |
| lo sguardo al pavimento: cervelli spericolati sul banco, 25 200 viaggi | 13 cadute in un buco | 0 |
| lo stop della svolta: viaggi | 48/48 | 48/48, nessuna caduta, 2,0 s più lenta nella casa ingombra, 5,9 s più veloce in quella arredata |
| lo stop della svolta: dove finisce una svolta oltre il chiesto (mediana) | 43–44° | 12–16° |
| persone in casa (banco, casa_gente, 192 viaggi): arrivi, contatti con persone a viaggio | stick 84 %, 8,0 | pilota 89 %, 2,2 |

Dettagli: [results.it.md](results.it.md), "Secondo giro sulla linea
0.17", "La zona cieca del sensore", "L'eccesso della svolta sul posto".

## Limiti noti

- **Solo i gemelli.** Il deadman, le righe del sensore e i numeri della
  camminata sono misurati sul gemello; quelli della papera sono il
  compito della prima sessione ([first-duck-session.it.md](first-duck-session.it.md)).
- **La tenuta di una gamba camminata fino al deadman non è modellata**:
  circa 5 cm al passo del set v8 dopo l'ultimo pezzo di una gamba, dentro
  la velocità stimata dalle tracce. E una svolta chiesta con `robot.move`
  (il movimento manuale di quack-control) prosegue ancora fino al
  deadman, circa 33° a 1,0: solo le svolte dell'esploratore mandano lo
  stop.
- **Lo sguardo copre la falcata davanti, non la casa.** La zona cieca del
  sensore sotto 0,4–0,6 m resta; un buco di fianco al corpo, o raggiunto
  di lato o all'indietro, è nel registro o da nessuna parte.
- **La coda imparata vive nel processo**: a ogni riavvio la coda di una
  svolta riparte dal valore iniziale della linea (17°) e converge in
  poche svolte. Tenerla, con gli altri numeri della camminata, accanto
  alla mappa è il prossimo lavoro (il registro d'andatura del todo).
- **Un bordo è registrato dove la posa lo metteva**: circa 1 dislivello
  su 50 finisce a 20–35 cm dal bordo vero ([results.it.md](results.it.md),
  "Limiti noti").
- La teleguida di robotd non ha ancora una protezione dai dislivelli
  ([study/upstream-asks.it.md §8](study/upstream-asks.it.md)).
