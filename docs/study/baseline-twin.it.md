# Punto fermo sul gemello — 2026-09-17

Cosa fa la papera sul gemello MuJoCo a questo commit, perché un cambio
futuro si possa misurare contro di esso: stessi comandi, confronto dei
numeri. Una regressione è una caduta, un goal mancato, o un tempo ben
fuori dalla fascia qui sotto (sono corse singole; due corse uguali
variano del ±20 %).

Condizioni: casa `house2` (il giro umano del 2026-09-16, robotd
corretto), il suo libro a 51 drop (`private/drives/runs/house2/ground-51.json`),
robotd dal worktree `maploc-study` in modo `localize` con
`MAPLOC_RAY_JUDGE=1`; quacksat coi suoi default — su mappa congelata un
viaggio cammina cieco (`QK_NO_GUARDS`, `QK_FOLLOW_ROUTE`, `QK_FAST` non
impostati), rotta tirata e tenuta; la ricerca al boot guarda prima di
camminare e scansiona l'orizzonte quando davanti non c'è nulla.

## Boot: la posa confermata sulla mappa salvata

`MICRODUCK_START="x,y,yaw" private/drives/boottest.sh <etichetta>` — o
tutte e cinque con `private/drives/queue-spawn.sh`. Tempo da "loaded the
newest map" a "the pose is confirmed"; l'errore di posa è quello di
posetrack a cinque minuti.

| nascita (mondo x, y, yaw) | boot | passi | rifiuti | posa a 5 min |
|---|---|---|---|---|
| corridoio (0,05, 0, 0) — default | 78–79 s | 8 | 0 | 3–5 cm / 0,2–0,8° |
| soggiorno (−2,5, −1,5, 0) | 83 s | 8 | 0 | 3 cm / 1,3° |
| bagno (2,5, −2,2, 1,57) | 125 s | 12 | 1 | 11 cm / 1,5° |
| studio (2,5, 0,0, 3,14) | 128 s | 10 | 0 | 11 cm / 2,3° |
| camera (2,0, 2,5, −1,57), schiena al muro | 154 s | 11 | 0 | 7 cm / 0,1° |
| cucina (−2,5, 2,0, 3,14), isola e sgabelli | 375 s | 11 | 0 | 8 cm / 0,7° |

Nessuna caduta. Il pavimento sono i cancelli di maploc (un metro di
corda, tre finestre di vantaggio, mezzo metro per confermare; fermate da
sei secondi): circa 65 s. La cucina è quella da tenere d'occhio: lo scan
non trova un metro di pavimento e la ricerca resta "chiusa" cinque volte
prima che lo scan del secondo budget trovi la porta.

## Viaggio: sei goal per la casa, cieca, su mappa congelata

`GOALS="1.50,2.50 2.50,0.17 0.90,-2.40 -2.64,-2.12 -2.30,2.10 -0.30,1.50"
MAPLOC_MODE=localize private/drives/abgoto.sh <etichetta>` (boot alla
nascita default, poi `speed_test.py`; "veri" è la distanza del gemello
dal goal quando il job dice di essere arrivato).

| tappa | house1tour (notte, 39 drop) | house2tour (mattino, 51 drop) |
|---|---|---|
| boot | 116 s | 79 s |
| camera (1,5, 2,5) | 23 s, 0,15 m | 47 s, 0,10 m |
| studio (2,5, 0,17) | 104 s, 0,13 m | 93 s, 0,14 m |
| bagno (0,9, −2,4) | 99 s, 0,20 m | 130 s, 0,05 m |
| soggiorno (−2,64, −2,12), oltre la tromba | 66 s, 0,09 m | 70 s, 0,12 m |
| cucina (−2,3, 2,1) | 100 s, 0,08 m | 103 s, 0,17 m |
| corridoio (−0,3, 1,5) | 35 s, 0,07 m | 29 s, 0,12 m |
| viaggi | 427 s, 6/6 | 472 s, 6/6 |
| posa a 5 e 10 min | 9 / 8 cm, 0,6 / 1,0° | 8 / 8 cm, 0,6 / 0,6° |
| la verità più vicina al bordo ovest | sopra (libro senza bordo) | 20 cm |

Nessun rifiuto, nessuno stallo, nessun drop cancellato, nessuna caduta;
il libro invariato dopo un giro cieco (un percorso cieco non cancella
nulla e non deposita corsie).

## 2026-09-18, le modalità separate (commit b288348)

`explore.rs` è diventato `explore/` — mod.rs (il ciclo), journey.rs,
mapping.rs, guarded.rs, recover.rs, gait.rs, books.rs, mode.rs — e una
politica per modalità: mappatura e viaggio cieco tengono il
comportamento di questa baseline, il viaggio guardato da solo porta gli
esperimenti della legge del passaggio del 17/18. Giro cieco a sei goal
dopo: house7tour 604 s (40, 108, 139, 87, 192, 38), house8tour 512 s
(43, 94, 115, 78, 152, 30), entrambi 6/6, nessuna caduta, posa 8–14 cm
— la gamba della cucina è quella che varia (100–192 s). Prima della
separazione la falla era costata 693 s (house6tour). Il viaggio
guardato dal fianco della tromba: la configurazione esatta di rim7
ripetuta cinque volte, 0/5 — una su cinque in ogni configurazione
provata (`queue-rim7.sh`, `paper30.sh`).

## Fallimenti noti, oggi invariati

Un viaggio CON le guardie dal fianco della tromba (rim2, rim3: gambe da
421 e 447 s, goal a 2,4 m) — il nodo guardie-vs-tromba del 2026-09-16. È
ciò che incontrerebbe una casa nuova senza libri.

## Dove sono le corse

`private/drives/runs/<etichetta>/` conserva la traccia di posa e i log di
ogni corsa (spawn-*, look*, scan*, house1tour, house2tour, rim2, rim3);
`private/drives/daytable.py` ne tabula un insieme.
