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
peggiore perché la posa dal vivo si legge dal frame della mappa, pubblicato
circa una volta al secondo, mentre la verità si legge all'istante: dentro
c'è fino a un secondo di cammino a 0.12 m/s. Questi file non hanno l'angolo,
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

Sul branch `phase-2` (ADR 0009), sul banco di replay, le stesse sessioni di
sopra.

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
giusto, per lo più entro 10 cm — ed è per questo che tornare a casa è lento,
e in casa_arredata è fallito due volte. Ma la Hessiana non può sostituirlo:
le 7 pose sbagliate (tutte in casa_arredata, 6 in un solo avvio, un alias a
circa 4 m) sono ben condizionate, rapporti degli autovalori fino a 0.71;
sono un'altra valle che combacia, non una direzione che scivola. È una
questione globale, per la localizzazione a ipotesi multiple del passo 4. Il
test della valle resta.

**L'assestamento dopo una ripresa** — una sessione ripresa corregge la posa
ma non scrive nulla finché due finestre di fila non sono d'accordo e la
spostano di meno di 2 cm e 1° — è stato misurato sulle sei sessioni riprese
ed è **spento** per default (`MAPLOC_SETTLE=1` sul banco):

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
  diciassette si sono fermati dopo 16 minuti.
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
