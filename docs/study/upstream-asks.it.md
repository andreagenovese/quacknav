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

Anche il client è costruito, e funziona. Il ritorno a casa di `quacksat`
carica all'avvio la mappa salvata più recente, resta fermo un minuto nel
caso il mapper confermi una posa da solo, e altrimenti azzera, esplora e
fa la domanda mappa contro mappa ogni tre minuti, adottando quando due
domande nominano la stessa mappa nello stesso punto con la mappa viva più
grande la seconda volta. Sul gemello, acceso in cucina a 3,5 m dalla base
con in libreria una mappa da 536 sottomappe: la ricerca all'avvio non ha
trovato nulla, come previsto; la prima domanda, dopo quattro minuti e con
339 celle di muro, era già giusta, e la seconda l'ha confermata tre minuti
dopo. Ha adottato, e il posto che ha preso era a 19 cm dalla verità. Ogni
domanda è costata 0,7 s di mappatura in pausa.

Quindi la forma che proponiamo non è uno schizzo: `robot.map_save`,
`robot.map_list`, `robot.map_load`, `robot.map_match` e
`robot.map_adopt` bastano perché un robot si svegli, capisca da solo in
quale casa si trova e si riprenda la sua vecchia mappa con i nomi e i
percorsi appesi — senza una soglia che qualcuno abbia dovuto tarare.

## 5a. Tre costanti, ciascuna misurata contro i muri veri di una casa

Sono uscite dal valutare le mappe contro la verità invece che l'una
contro l'altra — lo strumento è `private/drives/mapquality.py` di
quacksat, che adatta una mappa alla casa in modo rigido e poi chiede
quanto ogni muro mappato disti da uno vero. Tutte e tre sono modifiche di
una riga.

**Una chiusura d'anello sa meno di quanto dichiara.** `edge_sigma_xy` è
0,05 m, quindi l'arco dice all'ottimizzatore che la posa relativa di due
sottomappe è nota a una cella e mezza. Il grafo si piega allora per
accontentare ogni chiusura che un appartamento di rettangoli ripetuti
produce, e la mappa esce sfumata. Allargato a 0,40 m (e lo yaw a 0,24),
su otto sessioni registrate in due appartamenti simulati: meglio su sei,
media dei muri fuori posto 21,4 % → 13,4 %, media dei muri raddoppiati
5,3 % → 2,5 %. Ricontrollato più tardi con uno strumento di adattamento
corretto su quattro registrazioni: meglio su quattro su quattro, e i muri
raddoppiati di una casa dal 20,6 % allo 0,5 %.

**L'accumulatore tiene solo i primi due metri.**
`AccumulatorConfig::max_range_m` è 2,0 con il commento che oltre quel
punto il rumore costa più di quanto la copertura renda; il sensore arriva
a quattro. La metà lontana di una stanza aperta non raggiunge quindi mai
la mappa. A 3 m, su otto registrazioni: mediana dei muri fuori posto
6,3 % → 2,2 %, raddoppiati 0,6 % → 0,3 %, copertura delle superfici di
muro 44 % → 48 %; a 4 m comincia a restituire (3,9 / 0,8 / 47). In un
appartamento con un'ampia baia aperta la differenza è tutta la scoperta —
14,4 % → 1,2 %, copertura 48 % → 62 %. L'avvertenza è che questo è il
rumore di un sensore simulato (3 mm che crescono a 20 mm a quattro metri)
e un VL53L8 vero in piena luce è uno strumento peggiore, quindi due metri
potrebbero essere giusti per l'hardware; serve il rumore vero a tre metri
per deciderlo.

**Una finestra ferma non può formarsi mentre la posa è sospetta.** Dopo
una ripresa o un'adozione, stare fermi e girare produce finestre da 15–48
raggi, che `min_window_beams` (60) scarta — quindi la conferma di cui la
posa sospetta ha bisogno non può mai arrivare, e l'anatra resta persa su
una mappa la cui posa era giusta a pochi centimetri. Le stesse soste
mentre traccia danno composite da 1000–2400 raggi. Il sospetto è il voto
dell'accumulatore che incontra la spazzata della testa: un raggio è
tenuto solo se più fotogrammi della finestra hanno visto la sua cella
terminale, e mentre la posa è sospetta la testa spazza di ±0,9 rad, così
fotogrammi consecutivi guardano altrove e poche celle raccolgono voti.
Non abbiamo confermato la causa, solo l'effetto, ma l'effetto è
riproducibile e rende la rilocalizzazione all'accensione molto più debole
di quanto sembri.

## 5b. Una correzione della posa senza barra assoluta

**Cosa esiste.** La correzione di tracciamento del `Mapper` confronta ogni
finestra ferma con la mappa e sposta la posa tracciata su di essa,
accettando lo spostamento quando migliora il residuo della finestra di un
fattore (`min_improvement`, 0,8) e quando passano alcune prove di
condizionamento. È accesa di default, e giustamente: senza, fra una
chiusura d'anello e l'altra la posa è stima a naso.

**Cosa va storto.** Migliorare rispetto a dov'eri non basta, se dov'eri
eri già perso. Tirare la posa sulla mappa ripara la deriva finché la
mappa è giusta, e la rinforza appena la mappa è storta — e l'inchiostro
steso dopo rende la mappa ancora più storta.

**Misurato.** Abbiamo registrato la posa vera del gemello accanto alla
convinzione del mapper, una volta al secondo per giri di venti minuti,
come spostamenti dal proprio inizio, e valutato le mappe risultanti
contro i muri della casa (entrambi gli strumenti stanno in
`private/drives/` di quacksat; le case in `sim-maploc/houses/`). Su cinque
giri la deriva mediana predice la mappa in modo monotono: 6,8 cm di
deriva hanno dato una mappa con il 2,4 % dei muri oltre 10 cm dal vero, e
14,0 cm hanno dato il 12,2 %.
Nel giro peggiore la posa ha superato **il metro**, e ogni salto verso
l'alto cadeva su una correzione: 19 → 37 cm, 15 → 29, 34 → 47. Ognuna di
quelle correzioni migliorava il residuo della propria finestra. Ognuna
finiva attorno a 0,055 m. Quelle che hanno aiutato finivano a
0,009–0,026.

**Modifica proposta.** Una correzione deve anche finire sotto una barra
assoluta, non solo migliorare: dove mappa e sensore continuano a
discordare dopo lo spostamento, lo spostamento è andato verso una
menzogna. Sette sessioni registrate della stessa casa, rigiocate con lo
stesso codice e valutate contro i suoi muri:

| | mediana | media | peggiore |
|---|---|---|---|
| com'è | 4,7 % | 7,8 % | 21,6 % |
| correzione spenta | 3,4 % | 4,9 % | 14,6 % |
| **barra a 0,02 m** | **1,2 %** | **2,5 %** | **10,9 %** |

Meglio su sei registrazioni su sette e su tutte e tre le statistiche
insieme. A 0,03 m dà 2,1 / 4,8 / 16,0, quindi il valore conta e andrebbe
verificato contro il rumore di un sensore vero.

**Confermata dal vivo.** Tre giri nuovi della stessa casa con la barra
attiva, condotti dall'esploratore invece che riprodotti, danno 1,7 / 8,6
/ 2,0 % — mediana 2,0, media 4,1, peggiore 8,6, contro 4,7 / 7,8 / 21,6
dei sette giri precedenti. Ciò che è sparito è la coda cattiva. Con essa
la deriva della posa: mediana 5,9, 6,1 e 6,5 cm, mai oltre 26, dove il
giro peggiore di prima aveva superato i 127.

Uno dei tre portava la posa migliore del gruppo e la mappa peggiore dei
tre, quindi una posa buona non garantisce più una mappa buona. È un
secondo guasto, locale e raro dove questo era una deriva, ed è quello che
guarderemmo dopo.

**Perché conta oltre il numero.** Altri cinque interventi che abbiamo
provato — ricerca delle chiusure più larga, nucleo di Huber, rinnegare
l'arco peggiore, obbligare l'anatra a ripassare, tagliare la sottomappa
alla correzione — miglioravano ciascuno quattro o cinque registrazioni su
sette e rovinavano le altre, con oscillazioni di dieci-venticinque punti
e la mediana ferma. Questa è l'unica modifica che ha spostato tutte e tre
le statistiche, ed è l'unica venuta dal guardare il guasto accadere
invece che dall'indovinarlo.

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

## 6a. velstand e maploc non si sono ancora incontrati: `moving` resta vero per sempre

Trovato il giorno dopo che main ha reso `velstand.onnx` l'andatura di
default (set v5, `stand = "none"`, 2026-09-14). Il `moving` di robotd è
`busy || label == "walk"`; senza una rete di stazione il controller non
esce mai da `Net::Walk`, quindi una papera velstand ferma è etichettata
`walk` e `moving` non cala mai. Due cose leggono quel flag: il cancello
di immobilità di maploc, che non si è mai aperto — un intero giro
stop-and-scan sul gemello ha chiuso zero finestre e non ha inchiostrato
una cella — e `safeToRestart`, che rispondeva "il robot sta camminando" a
un robot fermo, per cui l'updater non avrebbe mai avuto la sua finestra.
La nostra correzione (`Step::walking`): decide l'etichetta quando esiste
una rete di stazione, la soglia di stazione quando non esiste; test sul
fixture feedforward. Su main nessuno lo vede finché maploc non entra — che
è esattamente quando lo vedranno.

Le leggi dell'andatura qui sopra valgono anche per velstand, misurate in
un'arena vuota: nessun giro da ferma (0,5° in 5 s), calcio e poi
rotazione (~100°/150° in 5 s), una deriva a destra da compensare — più
grande: bias −0,13 rad/s a richiesta zero contro il −0,05 di alpha, e
0,129 m/s in dritto contro 0,150.

## 6b. L'ottimizzatore del grafo di pose non scala oltre qualche centinaio di sottomappe

Lo dice `optimizer.rs` stesso: "per le nostre scale (≤ 50 nodi), una H
densa 3N × 3N va bene". Una mappa cresciuta in più sessioni sul gemello è
arrivata a 602 sottomappe e 915 chiusure d'anello (2026-09-15); ogni
chiusura ha allora eseguito un'eliminazione gaussiana densa 1806 × 1806
per iterazione di Gauss-Newton, robotd è rimasto al 100 % di CPU per
~100 s, i frame della mappa si sono fermati, e ogni client ha letto il
demone come sparito. Nulla limita il numero di sottomappe (il gestore ne
apre una per regola di percorso/età e non le fonde né le ritira), quindi
una lunga giornata di mappatura ci finisce dritta. Due cose basterebbero:
un solutore sparso (il grafo è una catena più qualche chiusura — Cholesky
sulla H sparsa è banale) e un tetto o una fusione delle sottomappe. Fino
ad allora una sessione dovrebbe restare sotto le ~150 sottomappe.

## 6c. Un frame di profondità viene proiettato con la testa del tick successivo

`robotd/src/maploc.rs`, `Event::Frame`: ogni frame viene appiattito con
`latest`, il campione di odometria dell'ultimo tick di controllo — la
testa com'era letta circa 12 ms (misurati) dopo la cattura del frame.
Durante lo sweep di ricerca è una frazione di grado per frame, sempre
nel verso dello sweep, e il composito della finestra di stillness lo
eredita. Misurato sul gemello (2026-09-15), heading tracciato contro la
verità del simulatore, diviso in secondi da fermo e secondi in cammino:
tre sessioni di mappatura (due explorer, una guidata a mano) scivolavano
di **+0,56, +0,80 e +0,88°/min da fermo**, un piccolo negativo in
cammino; con `search_sweep = false` la deriva da fermo era +0,03 e
−0,23°/min; fermo senza alcun movimento per cinque minuti, zero. La
causa è lo sweep. Una mappa cresciuta per venti minuti finisce ruotata
di 5°, e nessuna chiusura di loop la raddrizza — l'heading non ha un
vincolo proprio nel grafo.

La correzione che usiamo: `OdomSample` porta il tempo di lettura dei
sensori (`CLOCK_MONOTONIC`, lo stesso orologio di `TofFrame::t_ns`), il
worker tiene gli ultimi 128 campioni, un frame più nuovo di ogni
campione aspetta il tick successivo, e testa, gravità e altezza del
tronco sono interpolate all'istante del frame. Deriva da fermo con lo
sweep acceso: **+0,17 e +0,22°/min** — quattro volte meno; il resto è
probabilmente il ritardo tra posizione riportata e vera del servo, una
latenza fissa da misurare e sottrarre. Ripiega su `latest` per un frame
senza `t_ns` (un `tofd` precedente alla v24).

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
