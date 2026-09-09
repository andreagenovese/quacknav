# Cosa quacksat chiederebbe allo stack di Pollen

Scritto il 2026-09-09, dopo due settimane di lavoro sulla mappatura con il
gemello MuJoCo (`pollen-robotics/microduck` PR 127 `maploc` e il simulatore
della PR 202, più `microduck_rl`). Progetto indipendente, nessuna
affiliazione; tutto ciò che segue è un'osservazione con la corsa che l'ha
prodotta, non una lista dei desideri. Nulla di tutto questo è ancora stato
mandato a monte.

Ogni numero viene dal gemello, non dall'hardware: la papera vera arriva a
dicembre. Dove un'osservazione è probabilmente un artefatto del simulatore
e non del robot, è detto.

Le prime quattro sezioni riguardano la correttezza di `maploc` e crediamo
valgano il tempo di upstream a prescindere da quacksat. Il resto è minore.

## 1. Le chiusure d'anello scattano sul rumore della mappa e spostano la posa

**Cosa vediamo.** Sul gemello, la cui odometria è quasi verità (0,13 m e 3°
di deriva su 41 m di un giro guidato a mano), `maploc` chiude anelli decine
di volte per corsa a livello del rumore della mappa: soglia di correzione
0,04 m, tolleranza 0,06 m più 0,08 m per submappa con tetto a 0,6 m, e due
testimoni che possono venire entrambi dalla stessa sosta. Le chiusure
spostano la posa di 0,3–0,5 m dalla verità; poi il cane da guardia dichiara
la posa persa e la rilocalizzazione a forza bruta sceglie un bacino
sbagliato a metri di distanza.

**Prove.** Matrice al banco su cinque registrazioni
(`maploc/examples/evaluate` rigioca un `.mdlg` byte per byte). Stringere la
tolleranza a 0,03 m per submappa con tetto 0,30 m ha eliminato ogni evento
LOST e migliorato la mappa rispetto ai muri veri su 5 registrazioni su 5.

**Modifica proposta.** Portare `max_correction_per_submap_m` a 0,03 e
`max_correction_cap_m` a 0,30 (`maploc/src/pipeline.rs`), o renderli
configurabili con quei valori di default. Pretendere che i due testimoni di
una chiusura vengano da soste diverse.

**Come verificarlo.** `cargo run -p maploc --example evaluate -- <rec.mdlg>
sim-maploc/apartment.toml out/` e confrontare `map walls vs room` e le
righe LOST prima e dopo.

## 2. Fra una chiusura e l'altra nulla corregge la posa sulla mappa

**Cosa vediamo.** Fra le chiusure d'anello la posa tracciata è pura
navigazione stimata: lo scan matcher serve per le chiusure e per
rilocalizzare, mai per tenere la posa sulla mappa che sta costruendo. MCL
c'è nel crate e non è collegato. Così la posa deriva finché una chiusura la
strattona, ed è il meccanismo dietro la sezione 1.

**Prove.** Il nostro fork aggiunge una correzione scan-to-map a ogni
finestra ferma (`Mapper::tracking_correction`, con un test di accordo con
l'ultima ricerca). Sul gemello: la corsa 67 senza è derivata a 0,38 m
mediani e 0,73 m nella parte finale; la corsa 69 con la correzione ha
tenuto 0,15 m mediani per novanta minuti, mai persa, con zero cadute.

**Modifica proposta.** Correggere la posa tracciata contro la mappa a ogni
finestra ferma, con una soglia di miglioramento del residuo e un tetto,
così può solo stringere una posa e mai spostarla di molto. La nostra è
`TrackingConfig` in `maploc/src/mapper.rs` (274 righe del diff, attiva di
default).

## 3. La rilocalizzazione può sbagliare con sicurezza

**Cosa vediamo.** Dopo un LOST la ricerca globale restituisce una posa a
metri dalla verità con residuo 0,000–0,005: una corrispondenza perfetta con
la stanza sbagliata. Una casa ha rettangoli ripetuti e, senza magnetometro,
la ricerca copre anche la rotazione, quindi l'aliasing è atteso; quello che
manca è una qualunque prova che il vincitore sia *unico*.

**Prove.** Le corse 56, 64, 65 e 68 sul gemello si sono rilocalizzate a 3–6
m di distanza. Rigiocando quelle registrazioni con le nostre guardie,
l'errore scende da 3,1 m a 0,10 m e da 2,06 m a 0,07 m.

**Modifica proposta**, quattro pezzi piccoli e indipendenti:
- un rapporto di unicità: accettare il vincitore solo se batte il bacino
  secondo di un margine (`uniqueness_ratio 0.6`, `runner_up` in
  `maploc/src/relocalize.rs`);
- accordo: pretendere che ricerche consecutive concordino entro circa 0,3 m
  prima di agire (`relocalize_agree_windows 2`);
- prima una ricerca locale: se la posa era soltanto persa, cercare attorno a
  dove la papera crede di essere (`hard_lost_search_radius_m 1.0`) prima di
  cercare in tutta la mappa;
- arrendersi con onestà: dopo N finestre senza una risposta convinta,
  riprendere sull'odometria e dirlo (`Note::ResumedUnverified`) invece di
  impegnarsi su una posa sbagliata. Il client può allora fermarsi,
  guardarsi attorno e chiedere.

## 4. La catena viva e il banco non concordano

**Cosa vediamo, e non sappiamo spiegare.** La stessa registrazione che il
banco rigioca pulita è una corsa in cui il demone vivo ha perso la posa. È
l'osservazione che più ci piacerebbe far guardare a monte, perché significa
che il banco non può garantire per il robot.

**Prove.** Corsa 73 (registrazione `1788809590.mdlg`, 60 minuti, boot
pulito): dal vivo l'errore di posa ha superato 0,5 m al minuto 50, le
finestre sono state messe in quarantena dalle 22:24, il tracking è stato
dichiarato perso alle 22:28:23 ed è ripreso non verificato con 0,8 m di
errore. Rigiocando lo stesso file: la posa tracciata resta fra 5 e 36 cm
dalla verità per tutta la corsa e fra 15 e 17 cm in quegli ultimi dieci
minuti, e non si perde mai. Vivo e replica coincidono per i primi quaranta
minuti (fra 1 e 16 cm) e poi divergono. Lo stesso era successo con la corsa
49 e la registrazione `1788627740.mdlg`.

**Dove guarderemmo.** I tempi dei frame e il cancello di immobilità (quali
finestre il worker vivo integra davvero), i frame di profondità persi sotto
carico, e se la ricerca giri su una finestra stantia. Il banco consuma ogni
record; il demone forse no.

## 5. Una libreria di mappe, e rilocalizzarsi al boot

**Cosa c'è.** Un solo file di sessione (`map_path`), salvato allo spegnimento
e con autosalvataggio, ricaricato al boot fidandosi dell'ultima posa
salvata. La superficie IPC è `robot.map` e `robot.map_wipe`.

**Perché non basta.** Un robot che vive in una casa dovrebbe svegliarsi e
sapere in quale casa si trova, e dove. Oggi o viene acceso esattamente dove
era stato spento, oppure la mappa non vale nulla: la posa salvata è
sbagliata e nessuno la controlla.

**Modifica proposta.**
- `robot.map_save {nome}`, `robot.map_list`, `robot.map_load {nome}`: una
  cartella di sessioni con un nome, invece di un file solo.
- Caricare una sessione fa partire il mapper nello stato "persa dura" e lo
  fa cercare, con le guardie della sezione 3, invece di fidarsi di
  `tracked`.
- Dire nel frame della mappa quale sessione è caricata e se la posa è stata
  confermata dal boot, così un client può stare fermo, guardarsi attorno e
  chiedere all'utente invece di partire su un'ipotesi.

**Una cautela che porteremmo insieme.** Un ToF 8×8 a 2 m è una firma povera
di una stanza, e senza direzione assoluta la ricerca è su tre gradi di
libertà. Due segnali economici porterebbero via quasi tutta la domanda "in
quale mappa sono" senza toccare il ToF: il **dock** (un robot che si accende
sul suo caricatore sa esattamente dov'è, e questo da solo risolve il caso
comune) e il vicinato **Wi-Fi**, che `configd` già vede. Nessuno dei due è
raggiungibile da un client di robotd oggi.

**Misurato, 2026-09-09.** Abbiamo costruito la versione minima di questo e
l'abbiamo messa al banco: `Mapper::resumed_lost` fa partire una sessione
caricata nello stato "persa", e `MAP_SESSION=<file>` in `evaluate` rigioca
una registrazione dentro una mappa salvata. Due lezioni.

La prima è di progetto e la passeremmo volentieri: le due impostazioni che
rendono recuperabile un *rapimento* sono sbagliate al *boot*. Un raggio di
ricerca locale attorno a "dove la papera crede di essere" è ancorato
proprio alla posa di cui non ci si deve fidare, e arrendersi significa
tornare a quella. Su una mappa ripresa la ricerca dev'essere globale e non
deve mai ripiegare su quella posa.

La seconda è il risultato onesto. Rigiocando dentro la mappa salvata del
run 71 (536 submappe congelate, la casa al 46 %): una registrazione accesa
**sul dock** si è rilocalizzata dopo 125 s, e la sua posa concordava con i
muri veri a 0,070 m mediani contro i 0,036 m di una mappa nuova — si
ritrova, ma lentamente e peggio. Due registrazioni accese **accanto alla
tromba delle scale** si sono rilocalizzate entro 24 s su pose che
concordano con i muri veri solo a 0,169 e 0,203 m, dove una posa corretta
sta sotto 0,10: veloci, convinte e sbagliate. Quindi con un ToF 8×8 e senza
direzione assoluta la rilocalizzazione al boot non è ancora utilizzabile,
ed è per questo che il dock e il Wi-Fi contano più di quanto sembri. Una
prova più equa resta da fare: le nostre registrazioni lontane dal dock sono
brevi e passate accanto a un muro solo, quindi mostrano poco al sensore.

**La prova equa, e cosa ha deciso (2026-09-09).** L'abbiamo registrata: la
papera accesa in cucina, a 3,5 m dal dock, che esplora per otto minuti —
un panorama, qualche metro di cammino, altri panorami, 4000 celle mappate,
nessun salto di posa. Rigiocata dentro la mappa salvata del run 71, la posa
resta a 4–5 m dalla verità per tutta la replica: non una finestra su 89
arriva a mezzo metro.

La riga di `RELOC_DEBUG` dice perché, e non è quello che ci aspettavamo. La
ricerca il posto giusto lo *trova*: a 38 s il suo vincitore è a 0,6 m dalla
verità, spiega 231 fasci su 231 con un residuo medio di 0,0132 m. Nella
stessa ricerca, un bacino dall'altra parte della casa segna 0,0132 m pure
lui. La guardia di unicità li rifiuta entrambi, che è la risposta giusta a
una domanda ambigua, e la papera resta onestamente persa.

Quindi l'ostacolo non è la soglia di accettazione, né il numero di fasci,
né le guardie: una singola finestra ferma di un ToF 8×8, in una casa di
rettangoli ripetuti, non basta a nominare un luogo. **Ciò che deciderebbe è
la forma proposta dall'utente**: guardarsi attorno, camminare qualche
metro, guardarsi attorno di nuovo, e chiedersi quale ipotesi sopravvive a
entrambi i punti di vista — gli alias vengono contraddetti dal secondo, la
verità no. In codice significa portare avanti con l'odometria le prime
candidate e valutarle alla finestra successiva, invece della sola migliore
(`last_search`) che la guardia di accordo porta oggi. `maploc` contiene già
un modulo MCL non collegato, ed è esattamente a questo che serve.

**Costruito e misurato (2026-09-09).** La ricerca ora restituisce tutti i
bacini che ritiene plausibili, non solo il vincitore, e un mapper ripreso
su una mappa salvata li tiene tutti: ogni ipotesi è trasportata in avanti
con l'**odometria grezza** (la posa tracciata è congelata mentre è persa,
apposta, quindi non si può usare) e valutata su ogni finestra nuova dove
l'odometria dice che quell'ipotesi si troverebbe. Valutare invece di
aspettare che la ricerca la riproponga conta: la ricerca restituisce una
manciata di bacini fra i tanti, e quello vero non è sempre fra loro.
Un'ipotesi si crede solo quando è stata confermata lungo una certa
distanza camminata ed è in testa alle altre.

Non ha salvato il caso. Rigiocando l'accensione in cucina dentro la mappa
del run 71: senza chiedere cammino, si impegna dopo 32 s e sbaglia di 5 m;
chiedendo un metro, si impegna dopo 195 s e sbaglia di 4,5 m; chiedendone
due, non si impegna mai e la papera resta onestamente persa per tutti gli
otto minuti. Quindi un secondo punto di vista a un metro o due non separa
il posto vero dal suo alias in questa casa: gli alias continuano a
valutarsi bene quanto la verità.

Due cose attenuano il risultato. La mappa era il 46 % della casa, quindi
metà di ogni scansione cade dove la mappa non ha opinioni e non può
contraddire una posa sbagliata; una mappa completa sarebbe una prova più
equa e non ce l'abbiamo ancora. E il modo di fallire, quando si chiedono
prove sufficienti, è quello sicuro: la papera dice che non sa, invece di
avviarsi convinta nella stanza sbagliata. Per un robot con una base di
ricarica, "chiedimi di rimetterti sul dock, o dimmi dove sono" è una cosa
ragionevole da fare — ed è ciò che costruiremmo sul lato client finché
questo resta irrisolto.

**Mappa contro mappa: la risposta (2026-09-09).** La conclusione
dell'utente — se non riconosce la casa, che esplori come ha sempre fatto —
si rivela più di un ripiego, perché dopo qualche minuto la papera non ha
più una scansione da confrontare: ha una mappa.
`maploc/examples/align_maps` chiede se una mappa fresca entra dentro una
salvata, trasformando le celle di muro della fresca in una scansione
sintetica e cercandole nella salvata con la stessa macchina grossolana-fine.
Migliaia di celle invece di duecento fasci:

| la mappa fresca | celle di muro | dove è finita | scarto |
|---|---|---|---|
| run 70, accesa sul dock | 1313 | (0,05, 0,00, 0,0°) | **5 cm, 0°** |
| otto minuti dopo l'accensione in cucina | 659 | (−3,50, 0,95, 6,0°) | 0,83 m, 14° |

Contro i 4–5 m di errore della scansione singola, è la differenza fra un
metodo che funziona e uno che no. Entrambe sono ancora *rifiutate* dalla
guardia di unicità, e per come è fatta ha ragione: è tarata per la
scansione contro mappa, dove un buon residuo è 0,01 e il rivale deve essere
0,6 volte peggio. Mappa contro mappa il pavimento del residuo è il rumore
di mappa, 0,07–0,09, e il secondo — l'immagine speculare a 180° della casa,
in entrambi i casi — sta fra 0,76 e 0,81 del vincitore. Quindi la regola di
accettazione ha bisogno della sua taratura per questo uso, e sarebbe
aiutata da una prova che il punteggio oggi non usa: un candidato che
appoggia le celle *libere* della mappa fresca sopra i muri di quella
salvata è sbagliato, e dirlo non costa nulla.

**Cosa costruiremmo su questo.** Al boot: carica la mappa salvata, resta
ferma e cerca; se entro un minuto la posa non è confermata, apre una mappa
nuova ed esplora, che è ciò che la papera sa fare bene. Poi, ogni pochi
minuti, si chiede se la mappa fresca entra in una salvata — e quando entra,
adotta la vecchia con la trasformazione, tenendosi i luoghi e i percorsi
che ci stanno appesi. Il riconoscimento diventa qualcosa a cui il robot
arriva, non qualcosa che deve fare prima di potersi muovere.

**Il pavimento come prova, e cosa manca ancora (2026-09-09).** Un candidato
che appoggia il pavimento della mappa fresca sui muri di quella salvata è
sbagliato, e il residuo sui muri non se ne accorge. Valutando ogni bacino
con i muri più una penalità sul pavimento-su-muro, la verità va prima in
entrambe le coppie e allarga il suo vantaggio: la mappa della cucina passa
da 0,76 a 0,74 del secondo, quella del run 70 da 0,81 a 0,78, e in tutte e
due la verità ha il pavimento-su-muro più basso di ogni bacino (3,7 %
contro 5,3–7,7, e 5,1 contro 7,2–7,8). Quasi gratis, e nella direzione
giusta — ma non abbastanza per una guardia tarata a 0,6, e non possiamo
tararne una onestamente su due esempi che sono entrambi veri. Servirebbe un
controllo negativo: la mappa di una casa in cui la papera non è mai stata,
che non abbiamo e non si può fabbricare specchiando la stessa.

Quindi la regola di accettazione che costruiremmo non si appoggia affatto a
una soglia. Si appoggia alla stessa cosa che fa funzionare tutto il
progetto: **la mappa fresca continua a crescere.** Chiedere ogni pochi
minuti; pretendere che il vincitore sia lo stesso posto, entro un terzo di
metro, in due domande consecutive, con la mappa fresca più grande la
seconda volta. Un bacino sbagliato non sopravvive alla propria mappa che
cresce nelle stanze accanto; quello giusto migliora. È di nuovo l'idea
delle ipotesi multiple, alla scala in cui le prove sono davvero forti.

**A che punto è il lavoro.** Il pezzo del riconoscimento è costruito e
misurato (`maploc/examples/align_maps`), e ora anche la libreria di mappe
che gli serviva.

Abbiamo prototipato le tre chiamate su `maploc-study`, ed è questa la forma
che proporremmo. `robot.map_save {name}` copia la mappa viva in una
cartella `maps/` accanto a `map_path`, così un'installazione che sposta la
sessione si porta dietro la libreria. `robot.map_list` risponde con nome,
dimensione e data. `robot.map_load {name}` ne rende viva una tramite
`Mapper::resumed_lost` — torna la mappa, non la posa — e la mappa caricata
diventa quella di lavoro, così il prossimo autosalvataggio la scrive e un
riavvio la riprende. Un nome è da 1 a 64 caratteri fra lettere, cifre, `-`
e `_`, rifiutato e non ripulito, perché chi intendeva `../../etc/passwd`
deve sentirsi dire di no. `robot.map_wipe` conserva il significato che ha:
azzera la mappa viva e lascia in piedi la libreria. Le due che aspettano il
thread del mapper rispondono dentro `block_in_place`, perché un mapper in
mezzo a una ricerca può metterci secondi e nessun altro client deve
aspettarlo. L'instradamento segue `robot.map_wipe`: `mediad` le porta,
`btd` le rifiuta, l'updater non le conosce. `robotctl robot
map-save|map-list|map-load` le guida a mano.

Resta il client: al boot restare fermi e cercare, arrendersi dopo un
minuto, esplorare, e fare la domanda del riconoscimento strada facendo.

## 6. Fatti sull'andatura che servono a chi segue un percorso, e non sono scritti

Li abbiamo misurati sul gemello perché i nostri primi modelli erano
sbagliati e la papera finiva contro i muri. Se valgono sull'hardware, il
posto giusto è la documentazione; se è il simulatore ad averli sbagliati,
vale la pena saperlo lo stesso.

- **Un arco rallenta pochissimo.** A `vx 0.3, vyaw 0.7` il corpo avanza a
  0,110 m/s contro 0,121 m/s in rettilineo, non un quarto come assumevano i
  nostri modelli. Chi riserva un quarto dello spazio per un arco finisce
  contro il muro.
- **Da fermo non esiste rotazione sul posto.** `vx 0, vyaw ±0.7` muove il
  corpo di 1–2° in sei secondi. Un secondo di cammino prima, poi solo
  imbardata, gira a circa 30°/s con 15 cm di deriva.
- **La retromarcia ha bisogno di un'imbardata positiva per partire.** Da
  fermo, `vx -0.3` con `vyaw -0.7` non muove affatto il corpo; con `+0.7`
  arretra di 0,23 m in tre secondi. Una volta in passo va bene qualunque
  segno, e anche l'indietro dritto: mezzo secondo di `+0.7` basta a
  innescarlo.
- **I giri a tempo non sono ripetibili.** Lo stesso comando varia del triplo
  con la fase del passo, e il corpo prosegue di 5–10° dopo la fine del
  comando. Il controllo di rotta deve chiudersi sull'odometria, non sul
  tempo.

## 7. Piccole cose nel simulatore

- **Una posa di nascita.** `sim-maploc/body_with_map.py` mette sempre la
  papera nell'origine. Un `--start x,y,yaw` (noi usiamo localmente una
  variabile d'ambiente `MICRODUCK_START`) permette di provare un
  comportamento dove accade — accanto alla tromba delle scale, in una porta
  — invece di arrivarci a piedi, che è la maggior parte del tempo di una
  prova. La sovrapposizione della mappa ha poi bisogno della stessa
  trasformazione, altrimenti la mappa è disegnata nell'origine mentre la
  papera è altrove.
- **Il ToF legge i mobili bassi come un buco.** Nella scena
  dell'appartamento le righe che guardano il pavimento e cadono su un letto
  o un tavolino riportano un dislivello che non c'è: in una corsa cinque di
  questi hanno sigillato la porta della camera per un quarto d'ora. Se il
  sensore vero faccia lo stesso su un piumone è esattamente il genere di
  cosa su cui il simulatore dovrebbe avere ragione, perché un client che ci
  crede si rifiuta di camminare. (Il nostro ora distingue un buco da uno
  spigolo chiedendosi se un ostacolo stia alla stessa direzione: sul gemello
  classifica tre dislivelli su quattro come mobilio.)

## Cosa manderemmo insieme

Le quattro modifiche a `maploc` qui sopra stanno su un ramo locale sopra la
PR 202 (`maploc/{pipeline,mapper,relocalize,scan_matcher}.rs`, più le
manopole d'ambiente in `evaluate.rs` per le prove di ipotesi e una riga di
log in `robotd/src/maploc.rs`): circa 435 righe. Le registrazioni dietro
ogni affermazione sono normali file `.mdlg` e possono viaggiare con il
rapporto.
