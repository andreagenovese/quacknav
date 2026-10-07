# Risultati — cosa fa oggi quack-nav, misurato

I numeri della release preview (branch `turn-in-place`, 2026-09-25), tutti
sul **gemello MuJoCo** della Microduck (robotd rilasciato di Pollen,
daemon-v0.14.4, e il body server di `microduck_rl`) salvo dove detto. La
papera fisica non ha ancora fatto girare questo codice. Ogni cifra qui sotto
viene da uno script in `scripts/twin/` o dal gemello di carta, e si può
rifare.

## I criteri

Una release "funziona" quando tutti questi valgono sulle tre case di prova.
Dove un criterio è soddisfatto la tabella lo dice; dove non lo è, di quanto
manca — la quota di criteri soddisfatti è il progresso che riportiamo da una
release all'altra.

| # | Criterio | Oggi | |
|---|---|---|---|
| 1 | Nessuna caduta, esplorando o andando nei posti, sulle tre case | 0 cadute in 11 sessioni di esplorazione (5 h 30) e 51 viaggi | ✅ |
| 2 | Homecoming: mai una posa sbagliata; ≥ 90 % confermati | 0 sbagliate (ogni conferma a 0–20 cm dalla verità); 15 avvii su 17 confermati (88 %): due fermati in casa_arredata | ⚠️ |
| 3 | ≥ 90 % dei go_to arrivano | 46 / 51 (90 %): casa_libera 15/15, casa_arredata 16/18, house2 15/18 | ✅ |
| 4 | ≥ 90 % del pavimento di ogni stanza mappato, esplorando una carica alla volta | house2 93–99 % (quattro sessioni), casa_libera 99–100 % (completata da sola dopo tre); il bagno di casa_arredata al 45 % | ⚠️ |
| 5 | Nessun drop fantasma sul libro | 1 su 64 (house2: il bordo ovest del vano scala registrato 34 cm dentro il passaggio accanto) | ⚠️ |
| 6 | Muri della mappa entro 5 cm dalla verità in media | house2 4.5 cm, casa_libera 3.1 cm; casa_arredata 17 cm dopo una ripresa (3.8 cm prima) | ⚠️ |
| 7 | Documentato: cosa fa, come è stato misurato, cosa non fa ancora | questa pagina, l'ADR 0008, il README | ✅ |

**3 criteri su 7 soddisfatti, 4 in parte: 5 su 7 contando a metà quelli parziali — 71 %.**

## Le case

- **house2** — l'appartamento di `microduck_rl`: sei stanze e un corridoio con
  dentro un vano scala (un buco di 0.40 × 0.70 m); i passaggi accanto al buco
  sono larghi 0.54 m (ovest) e 0.44 m (est).
- **casa_libera** — cinque stanze vuote, sei porte, niente mobili, niente buche.
- **casa_arredata** — cinque stanze arredate e un corridoio: due passaggi
  stretti (0.5 e 0.6 m), un oggetto di 7 cm sul pavimento, una tromba delle
  scale nel corridoio (un passaggio di 0.49 m accanto, sulla via del bagno) e
  un angolo ribassato nel soggiorno.

La verità per il punteggio — muri, buche, stanze — è la scena stessa.

## Come si misura

1. **Esplorare, una carica alla volta**: da zero, sessioni di 30 minuti (la
   batteria, sul gemello), fino a quattro; tra una sessione e l'altra il
   gemello si riavvia e la papera deve ritrovare la mappa salvata e se stessa
   su di essa (homecoming) prima di continuare. Dopo ogni sessione: la quota
   del pavimento di ogni stanza che la mappa conosce (dalla registrazione,
   contro la scena), la quota che dichiara la papera, e le cadute.
2. **Dichiarata completa**: se la papera non ha trovato da sola la casa
   completa, il "esplorazione completata" dell'utente chiude la mappa com'è.
3. **Il libro dei drop** — ogni drop sul libro contro i bordi veri: entro
   10 cm, entro 20 cm, oltre (un fantasma).
4. **Tornare a casa e andare nei posti**: tre riavvii sulla mappa finita
   (l'homecoming deve congelarla e non esplorare niente), ognuno seguito da un
   go_to in ogni stanza e ritorno; la posa contro la verità a ogni conferma.
5. **L'A/B**: gli stessi riavvii e giri con la build di `main` (6bed9b8), la
   stessa mappa, lo stesso libro.

## I numeri

### Esplorare, una sessione di 30 minuti alla volta

| Casa | Sessione | Homecoming | Dichiarato | Vero | Cadute |
|---|---|---|---|---|---|
| house2 | 1 | — | 48 % | 61 % | 0 |
| house2 | 2 | a casa, esplora ancora (175 s) | 56 % | 65 % | 0 |
| house2 | 3 | a casa, esplora ancora (95 s) | 85 % | 94 % | 0 |
| house2 | 4 | a casa, esplora ancora (85 s) | 89 % | 96 % | 0 |
| casa_arredata | 1 | — | 59 % | 69 % | 0 |
| casa_arredata | 2 | a casa, esplora ancora (155 s) | 65 % | 87 % | 0 |
| casa_arredata | 3 | posa non trovata, fermata (972 s) | — | — | 0 |
| casa_arredata | 4 | posa non trovata, fermata (977 s) | — | — | 0 |
| casa_libera | 1 | — | 81 % | 87 % | 0 |
| casa_libera | 2 | a casa, esplora ancora (170 s) | 96 % | 100 % | 0 |
| casa_libera | 3 | a casa, esplora ancora (190 s) | 96 % | 100 % | 0 |

"Dichiarato" è il `house.percent_mapped` della papera; "vero" il pavimento
conosciuto delle stanze pesato sulla loro area. casa_libera si è trovata
completa da sola nella terza sessione; house2 è stata dichiarata completa
dall'utente dopo la quarta (89 % dichiarato, 96 % vero); la mappa di
casa_arredata è stata dichiarata completa dal protocollo stesso dopo la
seconda — i due avvii successivi non sono riusciti a ritrovarsi su di essa
per prendere la parola dell'utente.

### Il libro dei drop dopo l'esplorazione

| Casa | Drop | Sul bordo (≤ 10 cm) | Vicini (≤ 20 cm) | Fantasmi (> 20 cm) |
|---|---|---|---|---|
| house2 | 33 | 29 | 3 | 1 |
| casa_arredata | 31 | 30 | 1 | 0 |
| casa_libera | 0 | 0 | 0 | 0 |

### Tornare a casa e andare nei posti sulla casa mappata (A/B)

| Casa | Build | Homecoming confermati | Posa contro verità | go_to arrivati | Mediana | Cadute |
|---|---|---|---|---|---|---|
| house2 | nuova | 3/3 | 0.08–0.13 m | 15/18 | 106 s | 0 |
| house2 | main | 3/3 | — | 10/18 | 89 s | 0 |
| casa_arredata | nuova | 3/3 | 0.12–0.17 m | 16/18 | 111 s | 0 |
| casa_arredata | main | 3/3 | — | 7/18 | 101 s | 0 |
| casa_libera | nuova | 3/3 | 0.07–0.11 m | 15/15 | 111 s | 0 |
| casa_libera | main | 3/3 | — | 15/15 | 66 s | 0 |

### Errore di traiettoria, nella forma standard (ATE, RPE)

Aggiunto il 2026-09-25 (ADR 0009, passo 0), dalle stesse sessioni di
esplorazione: la posa contro la verità del gemello ogni 5 s, come ATE
(errore di traiettoria assoluto, RMSE, così come registrato e dopo il
miglior allineamento rigido) e RPE (l'errore del movimento su ogni metro
percorso). "Dal vivo" è ciò che la papera ha riportato durante il giro;
"replay" è la stessa registrazione rigiocata nel mapper sul banco
(`maploc/examples/trajectory.rs`), che dà gli stessi numeri ogni volta ed è
su cui si misurano le modifiche della fase due. Il dato dal vivo è un po'
peggiore perché, in queste sessioni (prima del 2026-09-29), la posa dal
vivo si leggeva dal frame della mappa, pubblicato circa una volta al
secondo, mentre la verità si legge all'istante: dentro c'è fino a un
secondo di cammino a 0.12 m/s. Dal 2026-09-29 la posa esce anche tra un
frame e l'altro, ogni 50 ms (`map.pose`, vedi sotto). Questi file non hanno l'angolo,
quindi l'RPE è quello dello spostamento nel riferimento del mondo; da ora il
campionatore registra anche l'angolo.

| Casa | | Percorsi | ATE RMSE | allineato | max | RPE per metro |
|---|---|---|---|---|---|---|
| house2 | dal vivo | 116 m | 0.143 m | 0.117 m | 0.57 m | 0.090 m (9.0 %) |
| house2 | replay | 118 m | 0.127 m | 0.111 m | 0.51 m | 0.082 m (8.2 %) |
| casa_arredata | dal vivo | 53 m | 0.164 m | 0.118 m | 0.37 m | 0.088 m (8.8 %) |
| casa_arredata | replay | 52 m | 0.126 m | 0.098 m | 0.35 m | 0.076 m (7.6 %) |
| casa_libera | dal vivo | 69 m | 0.119 m | 0.094 m | 0.30 m | 0.083 m (8.3 %) |
| casa_libera | replay | 63 m | 0.101 m | 0.080 m | 0.28 m | 0.079 m (7.9 %) |

`scripts/twin/houses/traj_metrics.py` li calcola da un file del
campionatore e scrive le traiettorie TUM per `evo`, che dà la stessa ATE al
millimetro (verificato sulle sessioni di house2).

### Fase due, passo 1: l'incertezza della posa, misurata

Sviluppato sul branch `phase-2` (ADR 0009), ora su `main`; sul banco di
replay, le stesse sessioni di sopra.

**Una covarianza sulla posa.** maploc tiene una covarianza 3×3 (x, y,
angolo) come un EKF: l'odometria la fa crescere (5 cm per √metro, 1.7° per
√radiante girato), ogni finestra giudicata dalla mappa la riduce con la
matrice normale dello scan matcher divisa per il residuo. Non cambia nessuna
decisione — le traiettorie rigiocate sono identiche al byte — e
`robot.map_status` la riporta come `pose_uncertainty` (una deviazione
standard, e la direzione in cui la posa è meno sicura). Verificata contro la
verità con il NEES (2 è onesto, e il 95 % degli errori dentro l'ellisse al
95 %):

| Casa | Sessione | | σ (mediana) | NEES | dentro il 95 % | r(σ, errore) |
|---|---|---|---|---|---|---|
| house2 | 1 | nuova | 0.070 m | 2.35 | 88 % | +0.08 |
| casa_arredata | 1 | nuova | 0.075 m | 1.54 | 98 % | −0.25 |
| casa_libera | 1 | nuova | 0.081 m | 1.42 | 98 % | +0.02 |
| house2 | 2 | ripresa | 0.067 m | 3.21 | 88 % | +0.59 |
| house2 | 3 | ripresa | 0.071 m | 4.75 | 74 % | +0.16 |
| house2 | 4 | ripresa | 0.068 m | 2.35 | 89 % | +0.08 |
| casa_arredata | 2 | ripresa | 0.072 m | 6.08 | 64 % | +0.46 |
| casa_libera | 2 | ripresa | 0.071 m | 4.16 | 79 % | −0.38 |
| casa_libera | 3 | ripresa | 0.081 m | 0.60 | 100 % | +0.62 |

Onesta in media su una mappa nuova; troppo sicura di sé su una ripresa, dove
l'errore è soprattutto lo scostamento della mappa salvata dalla casa, che
nessuna finestra può vedere. E non ancora un allarme: la sua correlazione
con l'errore istante per istante va da −0.38 a +0.62. Due cose imparate
strada facendo. Pesata come la dà la `H` dello scan matcher, una finestra
contava ognuno dei suoi raggi e la σ stava a 2.5 cm contro errori di 8
(NEES 30–60): ora pesa come un raggio a 8 cm. E una finestra giudicata
contro la mappa che ha appena disegnato è d'accordo con la posa che deriva e
che l'ha disegnata — la σ ignorava del tutto la deriva (r ≈ 0) finché le
finestre non sono state giudicate solo contro le submap più vecchie delle
ultime dodici.

**Il test della valle contro gli autovalori della Hessiana.** 118 decisioni
di rilocalizzazione rigiocate (6 sessioni riprese, 19 avvii dei giri
finali), ognuna contro la verità:

| Decisione | | posa giusta | posa sbagliata |
|---|---|---|---|
| confermata | 25 | 25 | 0 |
| rifiutata dal test della valle | 91 | 84 | 7 |

Il test della valle è molto prudente — il 92 % di ciò che ha rifiutato era
giusto, per lo più entro 10 cm — ed è per questo che tornare a casa era lento,
e in casa_arredata è fallito due volte (dal 2026-09-29 la mappa ombra, qui
sotto, dà un seme alle finestre e dimezza l'attesa). Ma la Hessiana non può sostituirlo:
le 7 pose sbagliate (tutte in casa_arredata, 6 in un solo avvio, un alias a
circa 4 m) sono ben condizionate, rapporti degli autovalori fino a 0.71;
sono un'altra valle che combacia, non una direzione che scivola. È una
questione globale, per la localizzazione a ipotesi multiple del passo 4. Il
test della valle resta.

**L'assestamento dopo una ripresa** — una sessione ripresa corregge la posa
ma non scrive nulla finché due finestre di fila non sono d'accordo e la
spostano di meno di 2 cm e 1° — è stato misurato sulle sei sessioni riprese
ed è **spento** per default (`MAPLOC_SETTLE=1` sul banco, tolta il 2026-09-30):

| Sessione | muri media / p90, spento | acceso | ATE, spento | acceso |
|---|---|---|---|---|
| house2 2 | 6.5 / 15.9 cm | 4.6 / 7.9 cm | 0.158 m | 0.112 m |
| house2 3 | 6.3 / 18.7 | 6.3 / 16.1 | 0.144 | 0.157 |
| house2 4 | 4.6 / 9.6 | 4.8 / 10.9 | 0.099 | 0.109 |
| casa_arredata 2 | 6.0 / 13.9 | 6.9 / 16.1 | 0.159 | 0.178 |
| casa_libera 2 | 3.0 / 5.5 | 3.1 / 5.2 | 0.124 | 0.115 |
| casa_libera 3 | 3.1 / 5.2 | 3.1 / 5.2 | 0.057 | 0.064 |

Una sessione molto meglio, le altre pari o un po' peggio — e peggio proprio
su quella per cui era stato scritto, perché la seconda sessione di
casa_arredata non è stata danneggiata dalla ripresa (vedi i limiti noti qui
sotto).

### Dopo la release (2026-09-28..30)

Sul gemello e sul banco di replay; le tabelle della release qui sopra non
cambiano.

- **La posa a 20 Hz** (e92aa14). mapd manda un `map.pose` leggero ogni
  50 ms tra i `map.frame` a 1 Hz; la corsia della mappa lo fonde nel frame
  con lo stesso seq, e dopo un wipe, un load o un adopt nessuna posa esce
  finché non arriva il primo frame della mappa nuova. L'errore d'angolo dal
  vivo, così come campionato, è sceso da 17° RMS (una posa vecchia fino a
  un secondo, nelle curve) a circa 2°.
- **Il replay come dal vivo** (f38b341). La registrazione `.mdlg` porta i
  timestamp degli orologi di robotd e di tofd (record di odometria da 53
  byte; quelli da 45 si leggono ancora) e il banco abbina la testa come dal
  vivo (interpolazione al timestamp del frame, i 5 ms di anticipo, la coda
  di attesa). Una sessione con i timestamp si rigioca entro 2–4 cm
  (mediana) dalla posa dal vivo.
- **La regola di adozione dell'homecoming** (3b5d4df): sovrapposizione
  almeno 0.50 (era 0.70), margine al massimo 0.50 (era 0.80), una domanda
  ogni 60 s (era 180; 120 sul gemello), tre risposte concordi come prima.
  Su 27 risvegli rigiocati (`maploc/examples/wake_match.rs`, 920 domande,
  655 con la risposta giusta): la regola vecchia faceva passare 565
  risposte giuste e 9 dell'altra casa, la nuova 626 e nessuna sbagliata.
- **La mappa ombra** (10f5a22, 2247634, 9c37473). Un mapper ripreso perso
  su una mappa salvata tiene una mappa nuova del suo cammino e ogni 30 s
  chiede alla mappa salvata dove combacia (`align::match_maps`). Due
  risposte concordi con margine ≤ 0.5, o quattro con ≤ 0.8, una volta che
  la papera ha camminato 0.5 m, danno un seme morbido — l'allineamento
  composto con l'odometria dall'inizio dell'ombra — che due finestre
  confermano. `MAPLOC_SHADOW=0` la spegne. 37 risvegli rigiocati:
  confermati da 13 a 35, mediana da 123 a 92 s, nessuno sbagliato. Sul
  banco dei risvegli del gemello, prima e con l'ombra:

  | Banco | | giusti | mediana casa_arredata | mediana apartment | sbagliati | cadute |
  |---|---|---|---|---|---|---|
  | w3, partenze sparse per la casa | prima | — | 174 s | 192 s | — | — |
  | w3 | ombra | 11/12 | 87 s | 105 s | 0 | 0 |
  | w4, le stesse partenze girate di 180° | prima | — | 135 s | 126 s | — | — |
  | w4 | ombra | 12/12 | 105 s | 123 s | 0 | 0 |

- **Il salto di una rilocalizzazione non si porta più via la sua posa**
  (7e2825b). Dopo una rilocalizzazione il salto congelava l'ultima submap
  della sessione; le sue chiusure spostavano la catena vecchia, e la submap
  appena aperta (una foglia) trascinava la posa appena confermata di 0.43 m
  e 7° — in fondo a nord del corridoio dell'apartment (x17, sessione 4) la
  papera tracciava 0.55 m fuori, e sembrava un alias ma non lo era. Ora il
  nodo nuovo è attaccato come rilocalizzazione e l'ottimizzazione di quel
  tick non lo sposta. 20 sessioni rigiocate: ATE media da 0.1045 a
  0.0975 m; il caso di x17 da 0.242 a 0.107 m.
- **Un sensore coperto non è un buco** (953285b). Una zona ToF valida sotto
  i 30 mm è il sensore contro qualcosa; un frame con almeno un quarto di
  zone così non propone nessun drop. La papera dell'apartment, con la testa
  sopra la coperta del letto, leggeva 0–1 cm in tutte le 64 zone e aveva
  registrato 18 buchi fantasma sul letto (x16).
- **Una casa nuova, casa_grande, per tutto lo stack** (d60e067, 9016e8c):
  9 x 7 m, sette ambienti, un corridoio largo 1.2 m che gira di 90°, mobili
  con 0.6 m o più attorno, due oggetti bassi (7 e 25 cm), una tromba delle
  scale e un angolo ribassato, e niente che blocchi (ogni meta si collega a
  ogni altra con 0.35 m di margine; `gen.py ... casa_grande`). Mai usata per
  mettere a punto nulla; sul gemello, con tutto quanto sopra: quattro
  sessioni di esplorazione e due giri di viaggi, 16/16 arrivati (mediana
  84 s), 0 cadute; il banco dei risvegli dalle sue otto mete 8/8 giusti,
  mediana 84 s (72–111), 2–16 cm; i muri della mappa al 99 % sulla verità,
  il 95 % del pavimento vero noto, le stanze al 93–98 %, 0 buchi fantasma.
- **Una sessione che è stata ovunque può trova la casa mappata** (fec3863).
  Su una mappa arredata restano sempre celle di frontiera (la fascia di celle
  incerte lungo ogni muro e ogni mobile), e l'ignoto rimasto somma
  l'impronta dei mobili e le buche (10.3 m² in casa_grande): le sue ultime
  sessioni finivano "stuck" e la casa non risultava mai fatta. Ora decide il
  pezzo di ignoto più grande che tocca pavimento noto — 1.0–3.9 m² sulle
  mappe finite di tre case, 5.5–7.3 nelle prime sessioni ancora in
  esplorazione: nessuna frontiera raggiungibile e nessun pezzo da 4.5 m² o
  più, e la casa è mappata.

- **Provati e lasciati spenti.** La sigma d'angolo delle chiusure di loop a
  0.5 rad: meglio su 8 sessioni rigiocate, peggio su altre 4; tornata a
  0.24 (`MAPLOC_LOOP_SIGMA_YAW`, tolta il 2026-09-30). D'ora in poi i parametri di maploc si
  giudicano su almeno 12 sessioni. Cancellare i buchi registrati dove poi
  si è visto il pavimento (`QK_FLOOR_STRIKE=1`, tolta il 2026-09-30): con un errore di posa non
  visto di 11–15 cm cancellava anche punti veri del bordo.
- **daemon-v0.15.0** (API 37) è validato sul gemello solo sul branch
  `microduck-015`: quattro sessioni per casa, nessuna regressione. Lì si è
  misurato che una testa che gira non costa quasi niente alla mappa
  (3.2–3.3 cm di residuo da 0.05 a 1 rad/s; 3.46 cm sull'1 % di frame
  oltre). `main` è fissato a quella dal 2026-10-01.

### Sporgenze e angoli dei vani scala (2026-10-07)

Sul gemello MuJoCo, giri di viaggi sui libri di un'esplorazione precedente
(`final_house.py`, `ROUNDS_ONLY=1`), con `main` congelato come controllo
sulle stesse mappe.

- **Il naso contro qualcosa che la mappa non ha** (d942590). Sotto il
  tavolino del soggiorno di apartment (il piano a 0.14–0.22 m dal
  pavimento, la testa a 0.25 m) la mappa ha pavimento libero; il corpo
  spingeva sul bordo del tavolo per minuti, scivolando qualche centimetro
  a passo, così lo stallo "sotto il centimetro" non scattava mai, e la
  posa scivolava con lui: 0.32 m di errore dopo tre minuti in un giro, poi
  la papera è finita nel buco. Il log del passo ora porta cosa vede il
  sensore di profondità nella corsia (`ahead`): il bordo a 0.10 m in 7–9
  frame su 8 per una quarantina di passi. In un viaggio, due passi avanti
  di fila con qualcosa nella corsia entro 0.15 m nella maggior parte dei
  frame degli ultimi 0.6 s sono uno stallo: una svolta di almeno 20°, e
  ciò che si è visto va sul libro, a meno che un muro della mappa sia
  entro 0.15 m o un drop sul libro entro 0.5 m (l'esplorazione tiene le
  sue regole). Quattro giri per casa: è scattata una volta per giro in
  apartment, al tavolino (24 viaggi su 24 arrivati), mai in
  casa_arredata, e 15 volte in casa_ingombra (11 su 11, `main` 4 su 5).
- **Una corsia percorsa cede a un drop sul libro entro 0.20 m** (1eb83ba,
  era 0.12). Il vano scala di casa_arredata aveva il bordo nord sul libro
  fino a x 2.45 e niente all'angolo nord-est; le corsie percorse lì
  tenevano il percorso del bagno a 9–12 cm dal buco vero (`route_on_map`
  con `TRUTH_HOLES`), e la papera è caduta su quell'angolo in due giri,
  con 9–11 cm d'errore di posa. A 0.20 il percorso tiene 16 cm (21 cm sul
  libro dell'altra esplorazione); nessuna tappa dei giri salvati di
  apartment, casa_arredata e casa_libera si è allungata o chiusa.
  Riempire i vuoti fra i punti del bordo non è servito: la corda passa
  dentro il buco, non attorno al suo angolo.
- **Provato e non tenuto: il go_to registra ciò che incontra** (branch
  `journey-books`). Registrare ciò che il sensore vede davanti prima
  dell'urto, uno stallo "scuff" (un passo che ha fatto meno del 40 % di un
  passo), guardare di fronte un buco prima di registrarlo e un ultimo piano
  ai margini più stretti: contro i 47 viaggi su 48 di `main` e nessuna
  caduta in quattro giri per casa, 18 su 24 con 3 cadute, poi 23 su 24 con
  1, poi 20 su 24 con 1. Registrare l'immagine spostata di un muro della
  mappa spingeva il percorso verso un vano scala; lo scuff contava come
  urto un passo su pavimento libero.
- **Rumore del gemello.** Due cadute della stessa verifica sono arrivate a
  0.1 s l'una dall'altra in due gemelli separati, entrambi i simulatori
  rallentati da 49 a 27–29 tick d'odometria al secondo nei cinque secondi
  prima: il computer, non la papera. Da ora le verifiche girano due
  gemelli alla volta.

## Limiti noti

- **Un bordo registrato dove lo metteva la posa.** Circa un drop su 50 finisce
  20–35 cm fuori dal bordo vero (l'errore della posa quando è stato visto).
  All'aperto non costa niente; in un passaggio accanto a un buco lo restringe
  per il pianificatore, e la meta di house2 oltre il vano scala (g4) è stata
  mancata in tutti e tre i giri — anche dalla build di `main`, con lo stesso
  libro. Un passaggio che la papera ha percorso resta aperto (le corsie), ma
  non uno che ha solo guardato.
- **Corretto dopo la release (2026-09-25): una sessione ripresa poteva
  rompere la mappa.** La seconda sessione di casa_arredata ha portato i suoi
  muri da 3.8 a 17 cm dalla verità. La causa non era la posa con cui era
  tornata a casa ma un bug di maploc: quando un avvio ritrova la papera
  altrove, l'ultima submap (vuota) della mappa salvata viene ri-ancorata lì,
  e il vincolo di odometria che vi entra restava a dire dov'era — 3.7 m più
  in là in un caso rigiocato — finché la prima chiusura di loop non lasciava
  che l'ottimizzatore lo soddisfacesse, e la posa saltava di 1.7 m. In
  replay, undici sessioni registrate delle tre case: errore dei muri da 6.9 a
  4.6 cm in media, il peggior errore di traiettoria da 1.95 a 0.16 m;
  rimappata sul gemello con la correzione, le quattro sessioni di
  casa_arredata hanno migliorato la mappa una dopo l'altra (3.5 cm di errore
  dei muri, dopo la sovrapposizione rigida) e ogni viaggio partito è
  arrivato, bagno compreso. Le tabelle qui sopra sono quelle della release,
  misurate prima della correzione.
- **Tornare a casa in una casa regolare può essere lungo, o fallire.** Il test
  della valle rifiuta una posa che un muro lungo e liscio non riesce a fissare:
  nessuna posa sbagliata è stata creduta, ma in casa_arredata (una casa
  generata, molto regolare, con il bagno mappato a metà) due avvii su
  diciassette si sono fermati dopo 16 minuti. Dal 2026-09-29 la mappa ombra
  dà un seme alle finestre: 23 risvegli su 24 al banco del gemello
  confermati giusti, nessuno sbagliato, mediane di 87–123 s contro 126–192 s
  di prima. Resta lento: la papera dell'apartment svegliata a est del vano
  scala, dove le finestre rifiutano per minuti un seme giusto.
- **Alcune chiusure di loop misurano male l'angolo** — di qualche grado, e
  la posa dal vivo deriva con loro: i buchi fantasma a nord del vano scala
  di casa_arredata (x13) venivano da una posa 35 cm fuori dopo una chiusura
  sbagliata di 5°, e la covarianza di maploc non lo segnalava (σ 0.08 m a
  0.35 m di errore). Il perché resta ignoto — nulla di ciò che una
  chiusura porta ne tradisce una sbagliata — ma dal 2026-10-01 una
  chiusura gira l'angolo di 4° al massimo (erano 26°): sul gemello
  l'errore mediano dell'angolo è sceso da 0.97–1.26° a 0.63–0.92°, mappa
  e viaggi invariati.
- **Più lenta di `main`.** La mediana di un viaggio è 106–111 s contro i 66–101 s
  di `main`; su pavimento libero (casa_libera) 1.7 volte tanto. È il prezzo del
  seguire la rotta pianificata (il filo teso al massimo 0.6 m, mai accanto a un
  drop) — e `main` è arrivata nel 63 % dei viaggi contro il 90 %, ed è caduta
  due volte in casa_arredata il giorno prima.
- **"A che punto sei" sbaglia per difetto**: 7–22 punti sotto la verità dopo
  una sessione, e per eccesso nei primi minuti di una mappa nuova, quando i
  muri che conosce sono quelli di una stanza.
- **L'esplorazione si attarda nei corridoi** (metà di ogni sessione in house2),
  ripassando pavimento conosciuto per chiudere i loop.
- **Solo gemello.** Niente di tutto questo ha ancora girato su una papera
  fisica; il passo, il sensore di profondità e il pavimento sono quelli di
  MuJoCo. Con un assistente che gestisce anche la domotica di una casa
  (Arkimede), "esplora la casa" finiva sulle luci di casa finché la papera non
  veniva nominata nella frase.
