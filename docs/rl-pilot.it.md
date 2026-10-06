# Il pilota: una policy appresa per i passi dello stick

Branch `rl-nav`, sperimentale. Copia inglese canonica: [rl-pilot.md](rl-pilot.md).

Il viaggio (`go_to`) e gli spostamenti dell'esplorazione camminano la
rotta del planner con lo **stick** (`quack-nav/src/explore/stick.rs`):
gira sul posto quando la rotta è fuori dal becco, fa un passo curvando su
di essa, mette sul libro ciò contro cui il becco ha spinto dopo tre passi
che non hanno mosso il corpo. Il **pilota** è una piccola rete che sceglie
lei la mossa del passo, da ciò che la papera vede adesso: la rotta davanti,
l'ultimo 1,4 s del sensore di profondità, la mappa e il libro attorno al
corpo. È stato addestrato su centinaia di migliaia di viaggi simulati in
case generate, con quello che la mappa non sa lungo la strada: cose posate
dopo, animali e piedi che attraversano, porte socchiuse, passaggi accanto a
una buca, mobili bassi, una mappa un po' spostata.

Nient'altro cambia: la rotta è del planner, il libro è il libro, le soste
per il mapper sono quelle dello stick, e gli **scudi** stanno sopra il
pilota (sotto). Senza pilota, quack-navd guida con lo stick esattamente come
prima (la soglia del gemello di carta dà gli stessi numeri al decimale).

## Sulla papera

| manopola | cosa |
|---|---|
| `QK_RL_POLICY=/var/lib/quack-nav/pilot.json` | il file del pilota; non impostata, lo stick. Un file che non si carica (un'altra versione dell'osservazione, un file rotto) lo dice nel log e guida lo stick |
| `QK_RL_TRACE=/var/lib/quack-nav/rl-traces` | ogni passo e ogni sosta registrati per la taratura (sotto), un file JSONL a ogni avvio di quack-navd |

Entrambe vanno in `/var/lib/quack-nav/knobs.env` (la pagina delle manopole
di quack-control lo scrive) e valgono dal prossimo `systemctl restart
quack-navd`. La rete è un MLP 327 → 256 → 256 → 9 calcolato in Rust
puro (`quack_nav::rlnav::Pilot`), circa 150 mila moltiplicazioni-somme a
passo: ben sotto il millisecondo sul Cortex-A55 della scheda. La stessa rete
è esportata in ONNX (`pilot.onnx`) per chi vuole guardarla con altri
strumenti; quack-navd legge il JSON.

## Cosa legge, cosa fa

L'osservazione (`quack_nav::rlnav::observe`, una funzione sola per il
simulatore e la papera, versione 2, 327 valori):

- la rotta a 0,2, 0,4, 0,7 e 1,0 m davanti, e la meta, nel riferimento del corpo;
- la memoria del sensore in 12 settori d'angolo su ±1,2 rad (i frame in
  cammino e la spazzata della sosta): l'ostacolo più vicino e il dislivello
  più vicino per settore, negli ultimi 0,7 s e nei 0,7 s prima (ciò che si
  muove si vede come un cambiamento);
- la mappa attorno al corpo, 16 × 16 celle da 10 cm da 0,4 m dietro a 1,2 m
  davanti, con il libro sopra (libero 0, ignoto 0,5, muro o drop 1);
- la sua ultima mossa, quante di fila gli scudi ne hanno rifiutate, quanti
  passi non hanno mosso il corpo, di quanto l'ha mosso l'ultimo.

Le mosse (9): cinque passi (vx 0,3 per 0,6 s, yaw da −0,7 a +0,7: quelli
dello stick), una rotazione sul posto per lato (0,4 rad, chiusa
sull'odometria come quella dello stick), una retromarcia (vx −0,3 con yaw
+0,7: l'unica che la camminata fa da ferma), e un'attesa (una sosta di
0,6 s, perché passi ciò che si muove).

## Gli scudi

Il pilota propone, gli scudi dispongono — sulla papera e in addestramento,
quindi la rete ha imparato con loro:

1. **La guardia dei buchi** (quella dello stick): un passo avanti con un
   buco vero nella sua corsia non si cammina; il corpo si gira e il bordo
   va sul libro.
2. **Niente retromarcia alla cieca**: indietro solo su pavimento che la
   mappa conosce, o contro un muro (un urto, non una caduta), lontano dai
   drop del libro. Il primo pilota è finito all'indietro in una tromba
   delle scale non registrata, girando e indietreggiando a turno lì accanto.
3. **Nessun passo attraverso un drop**: il passo giocato nel modello della
   camminata non deve attraversare un drop del libro, né l'ignoto entro
   0,35 m da uno (una buca non è mai mappata come pavimento). L'ignoto
   lontano dai drop è pavimento che nessuno ha guardato: rifiutato anche
   lì, una macchia in un corridoio bloccava il pilota per sempre.
4. **Non insistere**: dopo due passi che non hanno mosso il corpo, nessun
   passo se c'è qualcosa davanti (nella corsia del sensore entro 0,25 m, o
   il muro della mappa al becco) — nel banco, un corpo che strisciava lungo
   un muro è scivolato di lato in una buca non registrata che il muro
   nascondeva al sensore.
5. **Lo stick prende il passo**: dopo due mosse rifiutate di fila, quel
   passo lo fa lo stick. Un pilota deterministico che richiede ciò che gli
   è stato rifiutato resterebbe fermo per sempre.

**Cervelli spericolati** li verificano: un "pilota" che va solo indietro,
uno che va solo dritto, uno che sceglie a caso, viaggio dopo viaggio nelle
case generate. Nessuno deve cadere (`rl_eval --reckless random|back|straight`;
`finalize.py` e `gate.py` rifiutano altrimenti il pilota). Prima degli
scudi 2-4, quello dritto cadeva 8 volte su 40 accanto alle trombe delle
scale; dopo, nessuna in 2.100 viaggi.

## Il campo d'addestramento (`quack-rl`)

- **Gli scenari** (`scenarios.rs`): sette famiglie — `clutter`, `doorway`,
  `stairwell`, `corners`, `movers`, `low`, `mixed` — su quattro livelli di
  curriculum; ognuno un pezzo di casa, una partenza, una meta a 2-6 m, e
  ciò che la mappa non sa. La mappa è disegnata come la disegna un mapper
  (i mobili come una fascia, l'interno ignoto; macchie di pavimento mai
  viste), fino a 0,2 m spostata dal mondo.
- **Il corpo** (`body.rs`) è il modello del gemello di carta, i suoi numeri
  in una [`Calib`](../quack-rl/src/calib.rs): velocità per vx, lo yaw per
  unità, la deriva del passo dritto, il guadagno casuale dell'impulso
  breve, la zona morta della rotazione sul posto e la sua velocità per
  lato, la retromarcia; odometria e posa della mappa che derivano, la posa
  corretta alle soste e pubblicata ogni 50 ms (`map.pose`); il sensore a
  15 Hz, 8 × 8, con rumore e bias della distanza, buchi di lettura, cose
  basse perse in cammino, drop fantasma; un urto scivola lungo la faccia.
- **Il ciclo è quello di quack-navd.** Ogni viaggio d'addestramento esegue
  `Job::to_goal` — la rotta, il libro, le soste, gli scudi — sul corpo
  simulato; il cervello sui passi dello stick risponde dal learner
  attraverso una pipe (`rl_env`). Ciò su cui la rete impara è ciò in cui vola.
- **L'esperto** (`expert.rs`) vede la verità: il campo delle distanze del
  mondo, più caro vicino ai bordi e ai mobili, e ciò che si muove. Guida per primo.

**L'addestramento** (`scripts/rl/train.py`): imitazione con DAgger
(l'esperto guida, poi il pilota guida sempre di più mentre l'esperto
etichetta ciò che ha incontrato), poi PPO sulla ricompensa — avanzamento
lungo la via vera, tempo, urti, rifiuti degli scudi, vicinanza al bordo;
+3 arrivata, −10 caduta — con le etichette dell'esperto come perdita
ausiliaria che si spegne. 256 viaggi insieme, circa 30.000 passi al secondo
su un Mac a 12 core.

**I banchi** (`rl_eval`): scenari generati mai usati in addestramento, lo
stick, l'esperto e il pilota attraverso il ciclo di quack-navd; il
checkpoint si sceglie sui semi da 100000 e si riporta sui semi da 200000
(`finalize.py`, `report.md`).

## La taratura sulla papera

Ciò che il simulatore assume viene da MuJoCo; la papera sarà diversa. Lo
strumento adatta il simulatore alle tracce della papera, riaddestra il
pilota su di esso, e lo fa volare solo se batte lo stick su quel simulatore:

1. **Registrare.** Sulla papera, `QK_RL_TRACE=/var/lib/quack-nav/rl-traces`
   e viaggi come al solito (`go_to` tra i segni: bastano i passi dello
   stick; con un pilota caricato si registrano anche retromarce e attese).
   Venti minuti di viaggi danno centinaia di passi.
2. **Prendere le tracce**: `scp 'microduck@<papera>:/var/lib/quack-nav/rl-traces/*.jsonl' traces/`.
3. **Tarare**: `scripts/rl/calibrate.sh calib-out quack-rl/pilots/v3-r6 traces/*.jsonl`.
   - `rl_calib` misura, numero per numero, contro il valore di partenza:
     velocità, deriva del passo dritto, guadagno e dispersione
     dell'impulso, rotazioni sul posto per lato, retromarcia; la frequenza
     del sensore, il bias e il rumore della distanza contro la mappa alle
     soste, i buchi di lettura, i drop fantasma su pavimento noto; la
     deriva dell'odometria dove esce dal rumore della posa della mappa. Ciò
     che le tracce non possono dire tiene il valore di partenza, e
     `calib.md` lo dice. Poi rigioca ogni passo registrato nel modello
     della camminata con il valore di partenza e con quello stimato, e
     riporta entrambi gli errori.
   - il pilota continua ad addestrarsi (PPO, 150 aggiornamenti) nel
     simulatore con i numeri stimati, variati di poco attorno ad essi;
   - `finalize.py` mette sul banco il pilota nuovo, il vecchio e lo stick
     sul simulatore tarato, e i cervelli spericolati;
   - `gate.py`: il pilota nuovo vola solo senza cadute, con almeno gli
     arrivi dello stick e non più di 2 punti sotto il vecchio pilota. Stampa
     le righe `scp`/`install`; altrimenti dice cosa vola nel frattempo (il
     vecchio pilota se passa, altrimenti lo stick).

Verificato su tracce sintetiche (un simulatore con numeri volutamente
sbagliati al posto della papera): la stima ha ritrovato la velocità (0,098
contro 0,095), la deriva (0,040 contro 0,040), guadagno e dispersione
dell'impulso (1,12 / 0,30 contro 1,1 / 0,3), le rotazioni per lato (0,70 /
1,15 contro 0,70 / 1,15 rad/s), la retromarcia (0,063 contro 0,06), la
frequenza del sensore (12,0 Hz) e i suoi fantasmi (0,008 contro 0,01); il
rumore della distanza e i buchi di lettura escono come limiti superiori
(0,045 contro 0,035, 0,076 contro 0,06: le celle della mappa e la posa
della sosta aggiungono i loro); la deriva dell'odometria era sotto il
rumore della posa della mappa, quindi è rimasto il valore di partenza e il
rapporto lo dice. L'errore d'angolo del replay si è dimezzato (0,102 →
0,055 rad).

## Risultati

Vedi [Risultati](#risultati-1) più sotto, dai `rl-runs/*/report.md`.

## Limiti

- Misurato solo su simulatori: le case generate, il modello del gemello di
  carta, il gemello MuJoCo. La papera non l'ha ancora fatto girare.
- La policy di camminata è quella di Pollen, invariata: scavalcare le cose
  non è compito del pilota (servirebbe riaddestrare la camminata con il
  terreno nella sua osservazione).
- Gli ostacoli in movimento esistono nelle case generate, non sul gemello MuJoCo.
- Il pilota è una policy reattiva con 1,4 s di memoria del sensore: non
  ricorda una cosa vista ed evitata un minuto fa; il libro sì (urti e
  drop), e il planner ci gira attorno.
