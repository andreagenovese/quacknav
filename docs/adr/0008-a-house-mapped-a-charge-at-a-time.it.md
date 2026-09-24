# ADR 0008 — Una casa mappata una carica alla volta

Data: 2026-09-24. Stato: accettata.

## Contesto

La batteria di una Microduck non dura i novanta minuti che servono per
esplorare una casa, e sul gemello MuJoCo un'esplorazione lunga è andata
comunque peggio di più esplorazioni corte: più a lungo girava un lavoro,
più se ne perdeva nello stesso angolo (30 minuti fermi in un punto accanto
alla tromba delle scale di casa_arredata), e meno casa raggiungeva (il
bagno di house2 al 15 % dopo 90 minuti). Un utente, intanto, vuole tre cose
che l'esploratore non offriva: sapere a che punto è la mappa, dire "basta
così", e ripartire da zero quando la casa è cambiata.

## Decisione

**Ogni esplorazione è una sessione di un'esplorazione progressiva.**
`robot.map_explore` salva la mappa con un nome quando la sessione finisce —
per il suo tempo, per una batteria sotto `battery_min_pct` (letta da
`robot.health`), o perché non resta niente da esplorare — e scrive il
progresso accanto al libro dei drop (`<nome>.progress`: sessioni, minuti,
quota mappata, completata). Il mapper rifiuta di salvare mentre la papera
non sa dov'è, e una sessione così rifiutata non viene contata.

**La carica successiva riprende da dove si era fermata la precedente.** Con
`[homecoming] resume_explore`, un avvio su una mappa ancora in esplorazione
torna a casa su di essa e continua a esplorare: le frontiere rimaste sono
dove l'ultima sessione si è fermata. Un avvio che non riesce a confermare la
posa non comincia mai una mappa nuova — continua a cercare, poi si ferma —
così la mappa salvata non viene mai sostituita da una su cui la papera non
ha saputo ritrovarsi.

**Completa è un verdetto, della papera o dell'utente.** Una sessione che
finisce senza frontiere, o con solo frontiere irraggiungibili e meno di
2 m² di pavimento sconosciuto alla loro portata, trova la casa completa.
L'utente può dirlo prima: `complete: true` ferma la sessione, salva,
dichiara la mappa completa con la quota che ha, e la congela. Una mappa
completa non viene più esplorata; `robot.map_explore` lo dice. Solo
`fresh: true, confirmed: true` comincia una mappa nuova — la prima chiamata
senza `confirmed` risponde cosa si perderebbe — e la mappa salvata resta
nella libreria finché la prima sessione della nuova non la sovrascrive.

**Una mappa completa si naviga.** L'homecoming la congela all'avvio
(`quack.map_freeze`, in mapd, a runtime — il `localize` di maploc senza un
riavvio), e i viaggi su di essa sono ibridi: una gamba cammina alla cieca
dove la mappa conosce il pavimento, e passa dalla guardia dove attraversa
una cella che la mappa non ha visto.

**A che punto è** lo dice `robot.map_status` `house.percent_mapped`, dal
vivo sulla mappa in mano: pavimento conosciuto su pavimento conosciuto più
lo sconosciuto che una frontiera raggiunge dentro i muri della mappa (le
sacche chiuse da muri — l'interno di un divano — non sono da esplorare).
Sul gemello sta 7–12 punti sotto la verità a sessione finita, e sopra nei
primi minuti di una mappa nuova, quando i muri che conosce sono quelli di
una stanza sola.

## E cosa hanno insegnato le sessioni

Misurato sul gemello mentre si costruiva tutto questo, ora ognuna è una regola:

- *Prima via dal bordo.* Entrambe le cadute del 2026-09-23/24 erano una
  papera ferma a 3–12 cm da un bordo, ogni mossa rifiutata così vicino,
  mentre il passo da fermo la spingeva dentro. Un drop più vicino di quanto
  consenta una rotazione sul posto (0.15 m) si lascia prima di ogni altra cosa.
- *Un punto su cui ci si blocca più volte è da evitare* per il resto del
  lavoro — e i punti da evitare si dimenticano per primi quando la papera è
  chiusa dentro.
- *Un rifiuto accanto al corpo non è della frontiera*: contato contro una
  frontiera a due stanze di distanza, è costato a house2 il bagno per due
  sessioni.
- *Una posa su una mappa di un'altra volta si crede solo dove la scansione
  la fissa* (il test della valle di maploc): un muro lungo e liscio si è
  ritrovato 1.7 m più in là, e un avvio lo ha confermato.
- *Dopo una caduta, prima la posa*: il lavoro aspetta e la papera si guarda
  intorno finché una posa non è confermata; nessun avvio riparte da una posa
  che niente ha potuto giudicare.

## Conseguenze

Esplorare una casa sono più sessioni corte, ognuna interrompibile senza
danni. La stima della papera su a che punto è sbaglia per difetto dopo una
sessione e per eccesso nei primi minuti di una mappa nuova. Una mappa
chiusa prima dall'utente si naviga com'è: le parti sconosciute si
percorrono con la guardia, non si esplorano.
