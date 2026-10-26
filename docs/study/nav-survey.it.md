# quack-nav e lo stato dell'arte della navigazione — riassunto

Riassunto in italiano di [nav-survey.md](nav-survey.md) (2026-10-26, in
inglese, con le fonti): dove le nostre tecniche sono in linea con la
pratica consolidata, dove le abbiamo reinventate, dove siamo fuori dagli
schemi e cosa conviene adottare.

## I verdetti

- **In linea:** maploc (grafo di pose a sottomappe, scan matcher
  Gauss-Newton, chiusura dei cicli, MCL, covarianza della posa) ha la forma
  di Cartographer e SLAM Toolbox; le finestre ferme "fermati e scandisci"
  sono imposte dal sensore e hanno un parallelo pubblicato (NanoSLAM, che
  accumula fotogrammi VL53L5CX durante una rotazione di 45°). Dijkstra su
  costmap gonfiata e l'esplorazione a frontiere (Yamauchi 1997) sono da
  manuale.
- **In linea, reinventato:** lo stick è pure pursuit (Coulter 1992) più il
  Rotation Shim di Nav2, senza la regolazione di velocità di RPP (la
  camminata ha una sola velocità); portarlo da Nav2 darebbe solo i nomi.
  Lo strato 2.5D ("band") è la proiezione in 2D della fascia d'altezza
  dello strato voxel/STVL di Nav2. "Dimentica su pavimento visto" è il
  ray-trace clearing della costmap; il passo indietro dopo l'urto è il
  BackUp di Nav2.
- **In linea con lo schema raccomandato, ma non provato:** il pilota
  (MLP su rotta + 1,4 s di memoria del sensore + mappa locale, 9 azioni
  discrete, clonazione poi PPO con regolarizzazione, scudature, passaggio
  allo stick) segue la via che la letteratura consiglia — locale ibrido
  classico+appreso, con scudi (Rana 2020, Goldsztejn 2024, Dawood 2025,
  la rassegna di Xiao) — ma è misurato solo in simulazione; sulla linea
  0.17 due piloti che passavano il banco hanno perso sul gemello. BARN
  2022 (il vincitore in simulazione ultimo sull'hardware, 2/9 contro 8/9
  del classico) è il precedente da ricordare.
- **Fuori dagli schemi, giustificato, e contributi veri:** la guardia del
  dirupo da un sensore 8×8 in testa, inclinato, giudicata da fermo, con i
  limiti del bordo per colonna — nessun precedente di produzione (gli
  aspirapolvere usano coppie IR verso il basso); il test delle scudature
  con i piloti sconsiderati; il simulatore calibrato dalle tracce che
  gira il ciclo vero di quack-navd con un cancello di rilascio.
- **Fuori rotta strutturalmente:** i recuperi come contatori sparsi
  nell'esploratore (Nav2 usa un albero di comportamento); già nel debito
  tecnico (ADR 0009, passo 5).
- **Mover:** VO/ORCA, Social Force, CADRL, CrowdNav, DRL-VO assumono
  agenti tracciati con velocità da lidar a 360° o camere; non esiste un
  metodo pubblicato per una sola riga di profondità 8×8 stretta. La regola
  "in avvicinamento da 0,6 m" è una stima a velocità costante su un solo
  bersaglio, degenerata e non provata (A/B sul gemello dentro il rumore).
- **Valutazione:** l'A/B sul gemello contro main congelato è in linea
  nella forma ma poco potente: 24/24 vuol dire un intervallo di Wilson al
  95 % di [86 %, 100 %], 48/48 [93 %, 100 %], 12/12 [76 %, 100 %]. Il
  gemello mostra cadute e regressioni, non guadagni di 5 punti; il banco
  da centinaia di viaggi sì.

## Le raccomandazioni, in ordine

1. Intervalli di confidenza su ogni tasso, semi appaiati sul banco, la
   differenza minima rilevabile dichiarata per ogni protocollo; l'A/B sul
   gemello come cancello di regressione e sicurezza, il banco per i
   miglioramenti.
2. Un valutatore di rollout stile DWA sulle 9 azioni del pilota, con le
   stesse scudature, come baseline spiegabile: se pareggia, il pilota è un
   DWA reinventato con una storia sim-to-real peggiore; se no, il pilota
   si guadagna il posto.
3. I libri come strato di costmap con log-odds, cancellazione su pavimento
   visto e decadimento nel tempo (STVL); i dirupi asimmetrici (ADR 0009,
   passo 3).
4. Randomizzazione del simulatore più larga attorno alla calibrazione, su
   entrambe le linee di daemon, con gli effetti noti del sensore
   (bersagli scuri, bias di distanza): un pilota per camminata, non per
   linea.
5. I recuperi in una macchina a stati esplicita, con il round-robin di
   Nav2 come forma.
6. Per i mover: stima a velocità costante e attesa sul tempo-alla-collisione
   invece di prenotare a 0,6 m; non provato su questo sensore, da tenere
   spento salvo differenza fuori dall'intervallo.
7. Prima di credere a tutto questo: caratterizzare il sensore sull'hardware
   (pavimenti scuri, radenza ai muri, deriva termica).
8. Niente ricorrenza nel pilota (Hart et al. 2021): la memoria a due fette
   di 1,4 s è la scelta onesta.

## Domande aperte

Se le soste di 2 s bastano al sensore vero; se una riga a 13 Hz e ±22° sa
tracciare un corpo in movimento abbastanza da decidere un'attesa; se il
margine del pilota sul banco regge sull'hardware; cosa fa lo strato con
una zona di 5,6° radente a un muro; se un dirupo che decade riapre un bordo
vero dopo un errore di posa; quanti viaggi per casa servono al gemello per
vedere un cambiamento di 5 punti (centinaia, non 24).
