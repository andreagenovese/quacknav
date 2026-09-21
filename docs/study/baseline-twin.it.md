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

Rifatta la notte del 2026-09-20 (tag baseline-twin-2026-09-20): camera
111 s, soggiorno 112 s, bagno 159 s (una scansione per una prima
occhiata di 0,98 m), studio 117 s — dopo un boot INCASTRATO sullo
stipite della porta dello studio per otto minuti, ora liberato da un
calcio camminato cieco — cucina 344/332 s; posa a cinque minuti 4–8 cm;
nessuna caduta.

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

## Dove restano i negativi (2026-09-19)

Regge: il boot dal corridoio (65–79 s, sei nascite, nessuna caduta); i
viaggi ciechi su mappa congelata coi libri (undici giri a sei goal, 66
gambe su 66, nessuna caduta, 472–604 s); le modalità separate. Ancora
negativo, per gravità:

1. **I libri e la posa.** Un libro vale la posa con cui è scritto: a
   17 cm di errore una fermata ha iscritto trenta fantasmi e sigillato
   il corridoio (rim1); NON c'è un segnale di qualità della posa (il
   residuo della finestra ferma non correla con l'errore vero,
   `agreecheck.py`). Il rischio aperto più grave per la papera vera.
2. **La bocca della tromba, guardie accese.** Una volta su tre la papera
   non riesce a girarsi accanto al buco; il sigillo arriva dopo 3,5 min
   (trust1/3, retreat8, study1). Mitigato (3 andata-e-ritorno su 5, da
   1 su 5), non risolto.
3. **Il giro intorno.** Funziona da capo a fondo, non sta nel budget
   (una bocca fallita più 10 m; rim14, retreat8, study1). Facile: un
   budget proporzionato alla rotta, o un sigillo più rapido.
4. **Il boot nelle stanze arredate.** Cucina 375 s (nessun metro di
   pavimento tra isola e sgabelli), camera 154 s; "0,19 m tutt'intorno"
   per un'occhiata dopo un urto.
5. **Persa in localize.** Persa a metà viaggio, rilocalizzata a 77°
   (lane1, 2026-09-16); non più vista, nessuna regola.
6. **Ostacoli bassi.** Cubetto e palla trascinati: né il boot né i
   viaggi li vedono.
7. **Esplorazione fresca** non rimisurata su MuJoCo dopo le modifiche
   della settimana (explmap1: 46 min, corridoio sud al 22°); la
   calibrazione del bordo (iscritto 10–16 cm corto) mai fatta.
8. **L'allineamento a sinistra** si ferma corto di 8–13°, dentro la
   tolleranza di 11° (`alignprobe.py`).
9. **La varianza dei viaggi ciechi**: studio e bagno camminano 2,3–2,6×
   la linea; cucina 100–192 s. Tempo, non sicurezza.
10. **Il banco**: il gemello di carta non mappa mai il soggiorno (32 %
    in 900 s), quindi non misura sigillo e giro intorno; manca un mondo
    `--known`.
11. **Upstream**: PR 202 chiusa; le correzioni del worktree (pairing
    frame/testa, localize congelato) non hanno più una PR.
12. **La modalità guida** è uno script privato e un registratore, non
    una funzione.

La sicurezza sta in 1, 5 e 6; il resto è tempo o strumenti.

### Notte del 2026-09-20, dopo il passaggio sulla lista

Chiusi o spostati: 3 (un bordo sigillato aggiunge 300 s al budget, una
volta); 4 (la cucina 375 → 332 s: il verdetto della scansione sopra
un'occhiata chiusa, gli impulsi da 5° passano al calcio e allo yaw; il
boot incastrato su uno stipite si libera con un calcio cieco); 5, 6 e 1
(il 19); 7 (il cane da guardia di maploc conta solo i raggi lunghi —
explmap5 zero "tracking lost"; quacksat continua a mappare con una posa
non fidata ma stabile); 8 (un knob, non un default: la carta non ha il
bias a sinistra e la tolleranza più stretta le costava); 10 (`--known`:
la carta misura il sigillo, e mostra la cucina di carta chiusa da nord
dalla sua stessa mobilia, quindi la bench del giro largo vuole il mondo
ritoccato). Ancora aperti: 2 (la bocca, una su tre), 9 (varianza), 11
(upstream), 12 (modalità guida — lunedì); l'arrivo ora è giudicato sulla
posa dopo la sosta. Giro a sei goal dopo tutto: house19tour 6/6, 549 s,
nessuna caduta. Negativo nuovo: il giro largo per la cucina raggiunge la
porta cucina/soggiorno dal lato CUCINA e resta lì (goround2 ritorno,
14 min; gli sgabelli davanti alla porta) — il giro largo è una via solo
dal lato soggiorno. E il sigillo ora arriva anche dai giri rifiutati
accanto alla buca (dodici di fila), non solo dalle gambe (goround2
andata: dieci minuti, 172 giri rifiutati, nessun sigillo). Andata e
ritorno con questo: goround3 3/3 (28, 246, 266 s), nessuna caduta.

### 2026-09-20/21, punto 2 chiuso (tag baseline-twin-2026-09-21)

L'asse della legge del passaggio è la linea del muro stimata sulle sue
celle, la direzione tenuta piegata verso la linea (`QK_WALL_FIT`, il
default guardato); le gambe si accorciano prima di un bordo visto dal
sensore; un bordo si sigilla dopo dodici giri rifiutati o tre rifiuti
con moto in mezzo; la partenza del planner esce da un'inflazione fino a
1,5 m; "no room" incolpa il limite più vicino; i viaggi su mappa
congelata non iscrivono drop; il libro di terra di house2 è quello a 39
dell'utente. Guardato, corridoio → soggiorno → corridoio, libro a 39:
rimH 3/3 andata-ritorno, sei passaggi su sei, nessun sigillo (andata
304, 223, 196 s; ritorno 169, 155, 248 s), nessuna caduta. Il passaggio
passa se la posa è entro 10 cm (carta `--bias`: 30/30 a 10 cm, 25 e
13/30 a 15, 13/30 a 20); oltre è di maploc. Carta: guardato 23/30 (15
prima), cieco 29/30 invariato, mondo noto 30/30 col sigillo acceso
(0/30 prima).
