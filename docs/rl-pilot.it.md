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
case generate, con quello che la mappa non sa lungo la strada — cose
posate dopo, animali e piedi che attraversano, porte socchiuse, passaggi
accanto a una buca, mobili bassi, una mappa un po' spostata — e tarato,
come lo tarerà la papera, su tracce registrate (qui quelle del gemello MuJoCo).

Nient'altro cambia: la rotta è del planner, il libro è il libro, le soste
per il mapper sono quelle dello stick, e gli **scudi** stanno sopra il
pilota (sotto). Senza pilota, quack-navd guida con lo stick esattamente come
prima (la soglia del gemello di carta dà gli stessi numeri al decimale).

## Sulla papera

| manopola | cosa |
|---|---|
| `QK_RL_POLICY=/var/lib/quack-nav/pilot.json` | il file del pilota; non impostata, lo stick. Un file che non si carica (un'altra versione dell'osservazione, un file rotto) lo dice nel log e guida lo stick |
| `QK_RL_TRACE=/var/lib/quack-nav/rl-traces` | ogni passo, sosta e caduta registrati per la taratura (sotto), un file JSONL a ogni avvio di quack-navd |

Entrambe vanno in `/var/lib/quack-nav/knobs.env` (la pagina delle manopole
di quack-control lo scrive) e valgono dal prossimo `systemctl restart
quack-navd`. Il pilota da cui partire è `quack-rl/pilots/v3-r6/pilot.json`
(addestrato sui numeri di partenza del simulatore);
`quack-rl/pilots/v3-r6-mujoco/` è lo stesso pilota tarato sulle tracce del
gemello MuJoCo, la prova generale di ciò che faranno le tracce della
papera. La rete è un MLP 351 → 256 → 256 → 9 calcolato in Rust puro
(`quack_nav::rlnav::Pilot`), circa 160 mila moltiplicazioni-somme a passo:
ben sotto il millisecondo sul Cortex-A55 della scheda. La stessa rete è
esportata in ONNX (`pilot.onnx`); quack-navd legge il JSON.
`rl_pilot_check` dimostra che l'aritmetica in Rust dà i logit
dell'addestramento (differenza massima 2·10⁻⁶).

## Cosa legge, cosa fa

L'osservazione (`quack_nav::rlnav::observe`, una funzione sola per il
simulatore e la papera, versione 3, 351 valori):

- la rotta a 0,2, 0,4, 0,7 e 1,0 m davanti, e la meta, nel riferimento del corpo;
- la memoria del sensore in 12 settori d'angolo su ±1,2 rad (i frame in
  cammino e la spazzata della sosta), per settore il ritorno più vicino, il
  secondo più vicino (un ritorno isolato spesso non è niente; una cosa la
  vedono più zone e più frame) e il dislivello più vicino, negli ultimi
  0,7 s e nei 0,7 s prima (ciò che si muove si vede come un cambiamento);
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
   va sul libro. Corretta su questo branch per tutti, stick compreso:
   prendeva un dislivello con un ostacolo subito dietro per il bordo di una
   scatola bassa, quindi lasciava passare una tromba delle scale addossata
   a un muro. Ora un ostacolo spiega un dislivello solo se non è più
   lontano di esso (+5 cm); la soglia del gemello di carta non cambia.
2. **Niente retromarcia alla cieca**: indietro solo su pavimento libero
   che la mappa conosce, lontano dai drop del libro (il primo pilota è
   finito all'indietro in una tromba delle scale non registrata; una
   retromarcia contro un muro può ribaltare la papera).
3. **Nessun passo attraverso un drop**: il passo giocato nel modello della
   camminata non deve attraversare un drop del libro, né l'ignoto entro
   0,35 m da uno. L'ignoto lontano dai drop è pavimento che nessuno ha
   guardato — rifiutato anche lì, una macchia in un corridoio bloccava il
   pilota per sempre.
4. **Non insistere**: nessun passo dentro qualcosa che sta al becco
   (0,15 m), e, dopo due passi che non hanno mosso il corpo o l'hanno mosso
   appena (strisciate), nessuno se c'è qualcosa davanti entro 0,25 m.
   "Qualcosa" vuol dire visto in almeno metà dei frame dell'ultimo 0,6 s
   (almeno tre): bastava la parola di un frame per fermare ogni passo, con
   un sensore realistico. Ciò che ferma il pilota va sul libro, come l'urto
   dello stick, senza l'urto.
5. **Lo stick prende il passo**: dopo due mosse rifiutate di fila, o
   quattro rotazioni sul posto di fila, quel passo lo fa lo stick. Una
   policy deterministica può richiedere ciò che le è stato rifiutato, o
   fermarsi a girare avanti e indietro; con lo stick sotto non può bloccarsi
   peggio dello stick.

**Cervelli spericolati** li verificano: un "pilota" che va solo indietro,
uno che va solo dritto, uno che sceglie a caso, viaggio dopo viaggio nelle
case generate, con i numeri di partenza e con quelli tarati su MuJoCo.
Nessuno deve cadere in una buca (`rl_eval --reckless random|back|straight`,
la colonna `hole`; `cargo test` ne fa girare una versione piccola;
`finalize.py` e `gate.py` rifiutano altrimenti il pilota). Ognuno degli
scudi 2-4 e la correzione della guardia vengono da una caduta trovata da
questi cervelli. Da allora: nessuna in 3.760 viaggi.

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
  corretta alle soste e pubblicata ogni 50 ms (`map.pose`); un urto
  scivola lungo la faccia e può **ribaltare la papera** (per urto contro
  una scatola, un palo sottile, qualcosa che si muove).
- **Il sensore** (`world.rs`) risponde zona per zona, 8 × 8, come quello
  vero: ogni riga guarda il pavimento a 0,25-2,0 m da 0,25 m d'altezza, e
  incontra una cosa alta H a distanza D quando la sua distanza dal
  pavimento d soddisfa d ≥ D ≥ d (1 − H / 0,25) — un muro risponde in
  molte zone, una scatola bassa in una o due, una buca dove dovrebbe
  esserci il pavimento è un dislivello. Rumore e bias della distanza,
  buchi di lettura (vicini e oltre 1,4 m), ritorni spuri isolati, drop fantasma.
- **Il ciclo è quello di quack-navd.** Ogni viaggio d'addestramento esegue
  `Job::to_goal` — la rotta, il libro, le soste, gli scudi — sul corpo
  simulato; il cervello sui passi dello stick risponde dal learner
  attraverso una pipe (`rl_env`). Ciò su cui la rete impara è ciò in cui vola.
- **L'esperto** (`expert.rs`) vede la verità: il campo delle distanze del
  mondo, più caro vicino ai bordi e ai mobili, e ciò che si muove. Guida per primo.

**L'addestramento** (`scripts/rl/train.py`): imitazione con DAgger (dieci
iterazioni: l'esperto guida, poi il pilota guida sempre di più mentre
l'esperto etichetta ciò che ha incontrato), poi PPO sulla ricompensa —
avanzamento lungo la via vera, tempo, urti, rifiuti degli scudi, un poco a
ogni rotazione, vicinanza al bordo; +3 arrivata, −10 caduta o ribaltata —
con le etichette dell'esperto come perdita ausiliaria che scende fino a una
soglia. 256 viaggi insieme, circa 10.000 passi al secondo su un Mac a 12
core: 600 aggiornamenti in circa 35 minuti.

**I banchi** (`rl_eval`): scenari generati mai usati in addestramento, lo
stick, l'esperto e il pilota attraverso il ciclo di quack-navd; il
checkpoint si sceglie sui semi da 100000 e si riporta sui semi da 200000
(`finalize.py`, `report.md`).

## La taratura sulla papera

Ciò che il simulatore assume viene da MuJoCo; la papera sarà diversa. Lo
strumento adatta il simulatore alle tracce della papera, riaddestra il
pilota su di esso, e lo fa volare solo se lì batte lo stick:

1. **Registrare.** Sulla papera, `QK_RL_TRACE=/var/lib/quack-nav/rl-traces`
   e viaggi come al solito (`go_to` tra i segni: bastano i passi dello
   stick; con un pilota caricato si registrano anche retromarce e attese).
   Venti minuti di viaggi danno centinaia di passi.
2. **Prendere le tracce**: `scp 'microduck@<papera>:/var/lib/quack-nav/rl-traces/*.jsonl' traces/`.
3. **Tarare**: `scripts/rl/calibrate.sh calib-out quack-rl/pilots/v3-r6 traces/*.jsonl`.
   - `rl_calib` misura, numero per numero, contro il valore di partenza:
     velocità, deriva del passo dritto, guadagno e dispersione
     dell'impulso, rotazioni sul posto per lato, retromarcia; frequenza del
     sensore, bias e rumore della distanza contro la mappa alle soste,
     buchi di lettura lontani, drop fantasma su pavimento noto; i ritorni
     spuri isolati (un ritorno molto più corto della mappa che torna nello
     stesso punto del mondo un secondo dopo è una cosa che la mappa non ha,
     non rumore); cadute per urto (la traccia registra le cadute); la
     deriva dell'odometria dove esce dal rumore della posa della mappa. Ciò
     che le tracce non possono dire tiene il valore di partenza, e
     `calib.md` lo dice. Poi rigioca ogni passo registrato nel modello
     della camminata con il valore di partenza e con quello stimato.
   - il pilota continua ad addestrarsi (PPO, 150 aggiornamenti) nel
     simulatore con i numeri stimati, variati di poco attorno ad essi;
   - `finalize.py` mette sul banco il pilota nuovo, il vecchio e lo stick
     sul simulatore tarato, e i cervelli spericolati;
   - `gate.py`: il pilota nuovo vola solo senza cadute in buca, con non più
     ribaltamenti dello stick, almeno i suoi arrivi e non più di 2 punti
     sotto il vecchio pilota. Stampa le righe `scp`/`install`; altrimenti
     dice cosa vola nel frattempo.

`scripts/rl/test_calib.sh` verifica lo strumento da capo a fondo: una
papera simulata con numeri volutamente sbagliati viaggia con le tracce
accese, e la stima deve ritrovarli (velocità 0,097 contro 0,095, deriva
0,040 contro 0,040, guadagno dell'impulso 1,11 contro 1,1, rotazioni 0,70 /
1,15 contro 0,70 / 1,15 rad/s, retromarcia, frequenza del sensore, bias
della distanza 0,019 contro 0,020; l'errore d'angolo del replay scende da
0,121 a 0,043 rad).

### La prova generale: il gemello MuJoCo al posto della papera

`scripts/rl/twin_ab.py` fa girare lo stick e un pilota sul gemello in
`casa_ingombra` — casa_arredata più una borsa e un cesto nel corridoio, una
scatola che restringe la porta del soggiorno, le gambe di una sedia in
cucina, un giocattolo sulla via della camera, nella scena e non nella
mappa: l'oracolo disegna la verità come la disegnerebbe un mapper
(`QK_ORACLE_AS_MAPPED`), i bordi veri sul libro (`QK_ORACLE_BOOK`) e la
posa vera, così ciò che si misura è la navigazione. Le sue tracce sono
passate per `calibrate.sh` come passeranno quelle della papera. Cosa ha
insegnato il gemello, trovato nelle sue tracce e ora nel simulatore:

- **Gli urti ribaltano la papera.** Lo stick è caduto contro la borsa e
  contro il giocattolo, il primo pilota contro le gambe della sedia: in
  MuJoCo un urto non è solo un fermo.
- **Il sensore è 8 × 8 e onesto.** Restituisce una zona per riga: 29-38
  ritorni a frame dove il primo simulatore ne dava ~3. I ritorni molto più
  corti della mappa arrivavano a raffiche — il 93 % entro 0,5 m da una cosa
  non mappata; il rumore isolato del sensore è lo 0,04 % delle zone. Oltre
  1,4 m perde il 38 % dei ritorni. Un pilota addestrato sul sensore pulito
  prendeva ogni ritorno vicino per un muro e girava sul posto.
- **La camminata curva di più**: un impulso breve in curva gira 1,34 volte
  ciò che dava il modello (dispersione 0,60), le rotazioni sul posto 0,85 /
  0,96 rad/s, i passi 0,122 m/s a vx 0,3, le distanze del sensore 7 cm più
  corte (sta davanti al tronco).

La taratura (`quack-rl/pilots/v3-r6-mujoco/calib.md`) ha stimato quei
numeri; il pilota riaddestrato su di essi ha passato il gate.

## Risultati

**Case generate, i numeri di partenza** (pilota v3-r6, semi di prova da
200000, 60 per famiglia, il ciclo di quack-navd):

| | arrivi | in buca | ribaltamenti | s medi | urti a viaggio |
|---|---|---|---|---|---|
| esperto (vede la verità) | 94,3 % | 0 | 1 | 73 | 0,5 |
| **pilota** | **91,4 %** | **0** | **7** | 75 | **1,2** |
| stick | 80,0 % | 0 | 72 | 78 | 12,6 |

Per famiglia il pilota supera lo stick nel disordine (85 contro 62 %),
nelle porte (83 / 65), nelle case miste (75 / 60), con i mobili bassi (100
/ 90), negli angoli (97 / 92) e con ciò che si muove (100 / 97); nelle
trombe delle scale entrambi sono vicini al massimo (100 / 95). Le sue
perdite sono tempi scaduti, non cadute.

**Case generate, i numeri tarati su MuJoCo** (il banco del gate): il
pilota tarato 96,2 % (1 ribaltamento), quello non tarato 90,0 %, lo stick
82,4 % (59 ribaltamenti), l'esperto 98,3 %.

### L'A/B sul gemello

Sul gemello MuJoCo, `casa_ingombra`, due giri delle sue sei mete per
braccio (`twin_ab.py`, mappa e posa dell'oracolo, viewer acceso), il
pilota tarato sulle tracce del gemello (`v3-r6-mujoco`) contro lo stick:

| | arrivi | cadute | s medi (arrivi) |
|---|---|---|---|
| **pilota** | **11 / 12** | 0 | **116** |
| stick | 9 / 12 | 0 | 138 |

L'unico mancato del pilota è il bagno nel secondo giro (tempo scaduto); lo
stick ha mancato la cucina nel primo giro, il bagno e il ritorno a casa nel
secondo. Dodici viaggi per braccio sono un campione piccolo: in un giro
precedente della stessa casa lo stick si era ribaltato una volta su dodici
(contro il giocattolo), e il primo pilota non tarato — addestrato su un
sensore pulito — era arrivato a 3 mete su 5, girando sul posto accanto alle
cose non mappate. Le tracce di quel pilota sono ciò da cui la taratura ha imparato.

## Limiti

- Misurato solo su simulatori: le case generate e il gemello MuJoCo. La
  papera non l'ha ancora fatto girare.
- La policy di camminata è quella di Pollen, invariata: scavalcare le cose
  non è compito del pilota (servirebbe riaddestrare la camminata con il
  terreno nella sua osservazione).
- Gli ostacoli in movimento esistono nelle case generate, non sul gemello MuJoCo.
- Le cadute per urto sono eventi rari: il gemello ne ha data una in circa
  130 urti; le tracce della papera diranno la sua, lentamente.
- Il pilota è una policy reattiva con 1,4 s di memoria del sensore: non
  ricorda una cosa vista ed evitata un minuto fa; il libro sì (urti e
  drop), e il planner ci gira attorno.
