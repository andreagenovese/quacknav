# maploc — audit delle perdite di posa (2026-09-06)

Complemento di `maploc-dataflow.it.md` (cosa entra ed esce dal worker).
Questa nota risponde a due domande sollevate dai run sul twin: *cosa fa
davvero maploc con la posa* e *perché la perde*. Tutto è stato misurato
offline sulle registrazioni `.mdlg` di robotd, rigiocate nel bench
`evaluate` (lo stesso `Mapper` che gira sul robot, quindi il replay
riproduce le decisioni dal vivo byte per byte), nel worktree della PR 202
sul branch `maploc-study`. Script del bench e patch stanno in
`private/drives/maploc-bench/`.

## 1. Cosa fa maploc con la posa

- **Tra una chiusura di loop e l'altra la posa tracciata è dead
  reckoning.** I delta dell'odometria a contatto vengono composti a
  50 Hz e nient'altro la tocca. Lo scan matcher Hector presente nel crate
  è usato solo dal loop closer, submap contro submap; nessuna finestra
  viene mai confrontata con la mappa per correggere la posa. Il modulo
  MCL non è collegato affatto.
- **Ogni finestra da fermo viene giudicata, mai usata.** Il composito di
  una sosta viene valutato contro la mappa com'era all'inizio della
  sosta; una finestra che la mappa sa giudicare (≥ 100 raggi, ≥ 5 %) e
  contraddice (residuo medio > 0.25 m) va in quarantena, due di fila
  mettono il mapper in *lost*.
- **La chiusura di loop gira quando una submap si congela** (8 s e
  ≥ 0.15 m percorsi, oppure 0.8 m). Candidati: submap più vecchie entro
  1.5 m e almeno tre indietro. Due compositi della submap nuova vengono
  allineati alla griglia vecchia (ricerca grossolana ±0.5 m/±20°, poi
  Gauss-Newton); gate: residuo ≤ 0.10 m, copertura ≥ 40 %, testimoni
  concordi entro 0.12 m/5°, correzione ≥ 0.04 m e ≤ 0.06 + 0.08 m per
  submap di distanza (tetto 0.6 m). Gli archi accettati (σ 0.05 m,
  quattro volte più rigidi di un arco odometrico) rilassano tutto il
  grafo e spostano la posa tracciata.
- **Lost significa ricerca a forza bruta** su ogni cella libera e 36
  yaw del render globale; la finestra successiva deve confermare il
  vincitore (≤ 0.10 m su ≥ 30 % dei raggi). Non c'è un test di unicità.

## 2. Cosa dicono le registrazioni

**L'odometria del twin è quasi verità.** Sul giro umano di 21 minuti
(`drive-human-1.jsonl` contro `1788604159.mdlg`) l'odometria grezza è
rimasta entro 0.13 m e 3° dalla verità MuJoCo su 41 m di cammino,
distanza entro il 2–3 % per finestra di 30 s. Lo yaw IMU nel simulatore è
esatto. L'odometria degrada solo dopo una caduta (run `1788640409`:
l'accordo con i muri veri salta da 0.014 a 0.18 m subito dopo `FELL` a
336 s).

**La posa tracciata si allontana dall'odometria alle chiusure di
loop.** Run 49 (`1788627740`, il "terzo caso di deriva"): 114 chiusure in
26 minuti; dopo le prime tre la posa è 0.25 m e 14° fuori
dall'odometria, 0.33–0.56 m dal minuto 5 in poi, mai recuperata. Le
correzioni sono di 2–10 cm (rumore di mappa: la soglia minima è 4 cm,
sotto il rumore stesso della griglia, 5–9 cm) con alias occasionali di
0.3–0.47 m lungo i muri. Entrambi i testimoni vengono dalla stessa sosta,
quindi il gate di consenso non li coglie.

**Il replay non ha mai contraddetto il run dal vivo.** La lettura
precedente "il replay resta entro 0.49 m" era un clamp: il `vs-TRUTH` del
bench è la distanza media dei punti finali del composito dai muri veri,
tagliata a 0.5 m, e non vede uno scivolamento lungo un muro. Il bench ora
stampa anche il composito valutato alla posa odometrica grezza e la
distanza posa tracciata–odometria.

**Lost + relocalizzazione è da dove arrivano i metri.** Run 56
(`1788644274`): lost a 1256 s e 1353 s, relocalizzato 3 m più in là con
residuo 0.007 (un appartamento simmetrico crea alias attraverso una
fessura di 150°), poi tracciato lì. Il watchdog upstream ha un solo
rimedio per l'incoerenza, ed è la ricerca globale.

## 3. Matrice del bench

Cinque registrazioni (quattro run dell'esploratore, il giro umano
tagliato a 2000 s dove finisce la parte pulita). `walls` = distanza media
delle celle muro inchiostrate dai muri veri; `lost` = eventi di tracking
perso; la colonna umana dà anche l'errore medio/massimo della posa
tracciata rispetto alla verità. Le configurazioni migliorate inchiostrano
*più* celle muro dell'upstream (1000–1300 contro 800), quindi non vincono
mappando meno.

| configurazione | run 49 | run 56 | 1788642355 | 1788640409 (caduta) | giro umano |
|---|---|---|---|---|---|
| A default upstream | 0.156 | 0.267 · 3 lost | 0.163 · 1 lost | 0.052 · 1 lost | 0.078 · posa 0.06/0.31 m |
| B solo odometria (nessuna chiusura) | 0.074 | 0.178 | 0.059 · 3 lost | 0.139 | 0.061 · posa 0 |
| C chiusure con allowance stretto (0.03 m/submap, tetto 0.3) | 0.137 | 0.086 | 0.087 | 0.045 | 0.061 · posa 0.06/0.49 |
| D = C + correzione scan-to-map della posa | 0.075 | 0.068 | 0.072 | 0.050 | 0.093 · posa 0.06/0.31 |
| E = C + correzione conservativa (soglia 0.10 m, tetto 0.15, residuo dimezzato) | 0.177 | 0.062 | 0.109 · 3 lost | 0.039 | 0.062 · posa 0.06/0.41 |
| F nessuna chiusura + correzione | 0.045 | 0.059 | 0.062 | 0.060 · 1 lost | 0.123 · posa 0.08/0.58 |
| G nessuna chiusura + correzione conservativa | 0.063 | 0.125 | 0.059 · 3 lost | 0.093 | 0.058 · posa 0.00/0.11 |

Lettura: su un giro tranquillo l'upstream va già bene (6 cm di errore
medio di posa). L'esploratore lo rompe: i panorami girano sul posto, le
submap si congelano ogni 8 s nello stesso punto, decine di chiusure dalla
stessa sosta iniettano rumore di mappa in un'odometria quasi perfetta.
L'allowance stretto da solo elimina tutti gli eventi lost e migliora la
mappa su tutte e cinque le registrazioni; aggiungere la correzione della
posa aiuta i run dell'esploratore e peggiora il giro umano, perché sul
twin il rumore della mappa è maggiore dell'errore odometrico. Nessuna
impostazione vince ovunque; il twin lusinga l'odometria, quindi i
guadagni finali li decide dicembre.

## 4. Cosa è cambiato nel worktree (branch `maploc-study`)

- `scan_matcher.rs`: `ScanMatchResult` porta la matrice normale alla
  posa finale (quanto la scena vincola x, y, yaw).
- `mapper.rs`: `TrackingConfig` e una correzione scan-to-map a ogni
  finestra approvata — allineata alla mappa pre-sosta, regolarizzata
  sulla posa tracciata, proiettata via dalle direzioni non vincolate,
  limitata, con gate su copertura, miglioramento e soglia di rumore di
  mappa. **Spenta di default**; `Note::TrackingCorrected` quando scatta.
- `examples/evaluate.rs`: variabili d'ambiente per il loop closer, la
  correzione e un tetto di tempo; per finestra la posa odometrica e
  l'accordo con la verità; un riepilogo posa tracciata–odometria.
- `robotd/src/maploc.rs`: la riga di log per la nuova nota.

## 5. Cosa farne

1. Segnalare a Pollen con i numeri sopra (le registrazioni sono loro da
   rigiocare): chiusure al livello del rumore di mappa, testimoni della
   stessa sosta, un allowance dieci volte la deriva del twin, nessun test
   di unicità sulla relocalizzazione.
2. Per i nostri run sul twin, ricompilare robotd da `maploc-study` con
   l'allowance stretto come default (`max_correction_per_submap_m 0.03`,
   tetto 0.3) e rifare le due regressioni; attesi zero "tracking lost" e
   molte meno accensioni della guardia mappa-contro-sensore
   dell'esploratore.
3. Tenere la correzione della posa opt-in finché a dicembre non si
   misura l'odometria del duck vero; sull'hardware, dove l'odometria sarà
   peggiore della mappa, è il pezzo che impedisce alla deriva di arrivare
   al watchdog.
