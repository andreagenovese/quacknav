# Roadmap

Stato: prima stesura · Data: 2026-10-28 · Responsabile: andreagenovese

Accompagna il [README](../README.it.md) (cos'è quack-nav), [`todo-map.it.md`](todo-map.it.md)
(il lavoro in corso, con i suoi numeri) e [`results.it.md`](results.it.md) (cosa è stato
misurato). Questo è *ordine e direzione*: cambierà; le misure no.

La forma è quella di Pollen: `robotd` è un daemon con un contratto tipizzato e versionato che
tutto ciò che gli sta sopra — un gamepad, una console, una voce, un agente — usa senza che il
daemon sappia chi sono. quack-nav vuole essere la stessa cosa un piano più su: la navigazione
della papera e le sue azioni, autonome sulla scheda, esposte come servizio a qualunque cosa
stia sopra.

## Dove siamo

| | |
|---|---|
| mappa | una mappa della casa dalla matrice ToF della testa, costruita mentre la papera cammina, tenuta vera tra le sessioni, salvata col nome della casa — **rilasciata** |
| rilocalizzazione | la papera portata e accesa altrove si ritrova sulla mappa salvata prima di muoversi — **rilasciata** |
| guardie | gradini e dislivelli visti prima del passo; uno sguardo al pavimento davanti dove il sensore non vede; uno stop dopo ogni svolta — **rilasciate** |
| esplorazione | la casa mappata una carica alla volta, stanza per stanza, da zero, ripresa da dove si era fermata — **rilasciata** |
| viaggi | `go_to` a un luogo con nome lungo una rotta pianificata, persone e animali come ostacoli in movimento — **rilasciato** |
| il pilota imparato | una camminata allenata sul gemello simulato che batte quella scritta a mano sul gemello — **rilasciato** |
| i tool | tutto quanto sopra come tool JSON-RPC su un socket locale, con un catalogo, per un satellite vocale, una pagina di controllo e qualsiasi agente — **rilasciati** |
| il gemello | una copia MuJoCo della papera in più case, dove ogni cambiamento è messo in A/B contro la release congelata prima di uscire — il metodo, non una funzione |
| in volo | il registro della camminata: la papera misura la propria camminata e tiene ciò che ha imparato |
| manca | un contratto tipizzato con eventi e un client, azioni oltre la camminata, limiti decisi dal padrone, un secondo sensore, un secondo corpo |

## Traguardi

Ognuno ha una prova che dice quando è fatto ("Fatto quando"); lo stato è nel titolo. I numeri sono identificativi, non una sequenza;
[l'ordine del lavoro](#lordine-del-lavoro) è una sezione a sé.

### N1 — Una papera che passa · **fatto**

Mappare, rilocalizzarsi, guardarsi, esplorare, andare. La prima roadmap aveva una
destinazione sola: una papera che mappa una casa con i propri sensori, si ritrova dentro, e
cammina dove le si dice senza cadere. È arrivata con la v0.3.0.

**Fatto quando:** le case del gemello, quattro giri ciascuna contro la release congelata, ogni
viaggio arrivato, nessuna caduta; la mappa salvata rilocalizzata da ogni punto di accensione.

### N2 — Una camminata che si misura da sola · **in corso**

I numeri della camminata — quanto un comando muove davvero il corpo, quanto deriva, come
finisce una svolta — non sono più costanti per release: la papera li impara sulle proprie
gambe, li tiene accanto alla mappa, e parte da lì a ogni sessione. Un aggiornamento del daemon
o delle policy di Pollen cambia la camminata; la papera si adatta e continua a navigare. Un
gruppo di numeri per stato del corpo (un carico nel becco cambia la camminata), lo stato letto
dal daemon. Restano: l'unico numero che la camminata non dà come un numero, e la prova di una
camminata cambiata davvero.

**Fatto quando:** il gemello su un daemon più nuovo di quello su cui i numeri erano stati misurati,
col registro vuoto, mappa e naviga come sul proprio; nessun file da modificare, nessuna
release da tagliare.

### N3 — Un corpo che si ferma dove dice

Un movimento risponde quando il corpo è fermo, con quello che ha fatto, non quando il comando
è partito. L'ultimo tratto della gamba e la fine del tool sono tenuti come già lo è la svolta.

**Fatto quando:** un `robot.move` di lunghezza dichiarata finisce entro una tolleranza dichiarata
sul gemello, e l'A/B del gemello non mostra regressioni.

### N4 — Un servizio nello stile di robotd

Un contratto tipizzato e versionato per tutto ciò che quack-nav fa, con un `hello` che dice
quale versione parla; eventi spinti a chi si è iscritto — il progresso di un viaggio, un
arrivo, un rifiuto, una caduta, la posa persa e ritrovata — invece del polling; una libreria
client perché il satellite vocale, la pagina di controllo e un ponte smettano di scrivere JSON
a mano; un proprietario alla volta quando più client vogliono la papera; una coda di lavori
che la papera esegue da sola, fermandosi al primo che fallisce e riportando ciascuno. Un
documento del protocollo con la propria versione.

**Fatto quando:** il satellite vocale e la pagina di controllo girano sulla libreria client con il
controllo di versione del contratto; un evento raggiunge un iscritto prima di quando sarebbe
arrivato il polling successivo.

### N5 — Fare, non solo camminare

La papera esegue le skill che il suo daemon ha già — una presa, un calcio, sedersi e alzarsi —
quando le si chiede, come un lavoro solo: si avvicina, si squadra alla distanza allenata,
esegue la skill, aspetta che il corpo finisca, dice se è riuscita. Le skill sono del daemon e
si leggono da lui a run time, così quello che Pollen aggiunge viene offerto com'è.

**Fatto quando:** presa e calcio riescono da un lavoro di navigazione tanto spesso quanto dal tasto
del pad a mano, misurato sul gemello e sulla papera.

### N6 — Limiti decisi dal padrone

Zone proibite disegnate sulla mappa; un'esplorazione limitata a un rettangolo; sessioni
dimensionate su batteria e calore invece che sull'orologio; un punto di ricarica come luogo
con nome a cui la papera torna da sola prima che debba.

**Fatto quando:** la papera non entra mai in una zona proibita sul gemello nei giri consueti, e
chiude ogni sessione al punto di ricarica con carica d'avanzo.

### N7 — Un secondo sensore

La camera, sempre opzionale: nulla nelle guardie ne dipende, e senza non cambia nulla. In
quest'ordine — persone e animali come mover tracciati; riconoscere il luogo da un'immagine,
così un'accensione altrove costa secondi; un registro delle cose viste e di dove, e un lavoro
di ricerca che ne cerca una; poi il significato — quale stanza, cosa c'è a terra. Reti piccole
sulla scheda dove ci stanno; le grandi su un server in casa, con il video che non esce mai di
casa. I casi difficili che la papera incontra alimentano l'allenamento del suo pilota, in un
ciclo che gira il server di casa.

**Fatto quando:** l'accensione altrove si rilocalizza più in fretta con la camera che senza; una cosa
vista si ritrova dal registro; gli scudi del gemello passano con la camera spenta.

### N8 — Un altro corpo

La navigazione su un bipede diverso. Tutto ciò che quack-nav chiede a un corpo è un'interfaccia
piccola — un comando di velocità e uno stop, odometria, in piedi o caduto, una sorgente di
profondità che diventa raggi — documentata come il contratto da implementare, con la papera
come prima implementazione.

**Fatto quando:** una seconda implementazione mappa e naviga una casa sul gemello.

## L'ordine del lavoro

Prima N2: è in volo ed è ciò su cui viaggia ogni cambiamento successivo. Poi N3, che costa
poco e rende onesta la risposta di ogni lavoro. Quindi N4 e N5 insieme — il servizio è ciò
attraverso cui le azioni vengono esposte — con i pezzi piccoli di N6 infilati dove costano
poco. N7 dopo che i limiti noti del sensore sono chiusi, nell'ordine che dichiara. N8 quando
un secondo corpo è in vista.

## Decisioni che orientano il lavoro invece di seguirlo

- **Misurare, non scrivere.** Un numero che la papera può misurare su di sé si misura; una
  costante resta solo come prior con limiti.
- **Il daemon possiede i motori; le skill sono sue.** quack-nav chiede, non guida mai un
  giunto, non distribuisce mai una policy propria per una skill.
- **Niente sopra la papera è necessario.** Una voce, una pagina di controllo, un agente, un
  modello: ciascuno è un client dello stesso contratto; la papera naviga senza nessuno di
  loro.
- **Le cadute si contano, mai si nascondono.** Ogni cambiamento esce con il suo A/B contro la
  release congelata e la sua prova scudo, mancati compresi.
- **Il video di casa resta in casa.**
- **Priorità: passare, poi la mappa, poi la rilocalizzazione, poi la camminata fluida.**

## Cosa non facciamo, apposta

- Uno SLAM visivo al posto della mappa ToF: il sensore che la papera ha funziona, la camera si
  aggiunge.
- Un modello che guida: un modello dà obiettivi, la scheda reagisce.
- Calciare via una cosa sconosciuta di propria iniziativa.
