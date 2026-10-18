# quack-nav v0.3.0-rc2 — note di rilascio

2026-10-18. Copia inglese: [release-notes-v0.3.0-rc2.md](release-notes-v0.3.0-rc2.md).
L'elenco completo delle modifiche è nel [CHANGELOG.it.md](../CHANGELOG.it.md);
le note della candidata precedente sono in [release-notes-v0.3.0-rc1.it.md](release-notes-v0.3.0-rc1.it.md).

## Che cos'è

Una **quarta release candidate, validata solo sui gemelli**: il gemello
MuJoCo del Microduck (il daemon-v0.16.1 rilasciato da Pollen e il corpo di
`microduck_rl`) e il gemello di carta. **Non ancora su una papera vera**:
quella fisica arriva a dicembre 2026, e fino a una prima sessione con lei
ogni numero qui sotto è del gemello.

La rc1 riguardava la papera come la spedisce Pollen e un modo migliore di
fare i passi. La rc2 riguarda ciò che la mappa 2D non ha: le cose che si
muovono, e quelle sotto cui o contro cui la papera cammina alla sua
altezza. Porta anche una prima casa 3D, sperimentale, con i colori della
camera, costruita fuori dalla papera.

## Prima di installare

Niente di nuovo da impostare. La configurazione, il profilo di camminata
(`[gait] profile`) e il daemon-v0.16.1 sono come nella rc1, e le mappe
salvate dalla rc1 si caricano così come sono: il loro strato parte vuoto e
si riempie mentre la papera sta ferma e guarda.

## Cosa è cambiato dalla rc1

- **Le cose che si muovono: quattro regole, attive di default.** Una cosa
  nella corsia si registra da 0,35 m (prima 0,30), e da 0,6 m se si
  avvicina più in fretta di quanto cammini il corpo; dopo un urto o un
  contatto la papera fa un passo indietro prima di girarsi, solo su
  pavimento che la mappa conosce; ciò che un viaggio ha registrato esce dal
  registro quando il sensore vede il pavimento dove stava (i dislivelli
  mai). Ognuna è una manopola ([knobs.it.md](knobs.it.md)); 0 dà il
  comportamento della rc1.
- **Lo strato: 2.5D all'altezza della papera.** Per ogni cella di 5 cm,
  dieci fette da 0,05 a 0,30 m, riempite dai ritorni del sensore di
  profondità e svuotate dai raggi che le attraversano. La navigazione lo
  legge come muro: il piano di un tavolino, il telaio di un letto, una
  vasca, che la mappa 2D leggeva come pavimento libero, ora sono
  d'ingombro. Si salva accanto alla mappa (`<mappa>.band`) e viaggia in
  `map.frame` come `band`. `QK_BAND=0` lo spegne. I ritorni entro 10 cm da
  un muro che la mappa 2D ha già sono lasciati alla mappa, così lo strato
  non ingrossa i muri e non stringe i passaggi.
- **Una casa 3D con texture, fuori dalla papera (sperimentale).**
  `scripts/map3d` registra la camera della testa accanto a una papera che
  esplora, costruisce una casa 3D con i suoi colori su un Mac o un PC, e
  `send.sh` la mette sulla papera, dove la scheda **3D** di quack-control
  v0.1.0-rc2 la disegna. Sulla papera non gira niente di nuovo. Sul
  gemello: muri e mobili a 2,6 cm dalla verità (mediana).
- Il gemello: persone e animali che si muovono (`TWIN_MOVERS`), due case
  con loro, e una camera della testa che guarda avanti.
- CI: azioni su Node 24, provate su ubuntu-26.04.

## Misurato contro la versione precedente

Ogni modifica contro il `main` che la precedeva, sul gemello MuJoCo con
velstand, quattro giri per casa, due gemelli alla volta:

| | prima | dopo |
|---|---|---|
| ostacoli che si muovono: viaggi, appartamento / casa_arredata / casa_ingombra | 24/24, 24/24, 12/12 | 24/24, 24/24, 12/12 |
| ostacoli che si muovono: casa_viva (tre in movimento) / casa_porta (una persona in una porta) | 24/24, 24/24 | 24/24, 24/24 |
| ostacoli che si muovono: esplorazione da zero, tre case | una caduta | nessuna caduta |
| lo strato: viaggi dopo l'esplorazione, appartamento / casa_arredata | 24/24, 24/24 | 24/24, 24/24 |
| lo strato: esplorazione dichiarata finita (su quattro sessioni) | casa_arredata alla 4ª, l'appartamento mai | entrambe alla 3ª |

Nessuna caduta nelle prove dello strato, da nessuna delle due parti.
Dettagli: [results.it.md](results.it.md), "Ostacoli in movimento" e
"Lo strato".

## Limiti noti

- **Solo i gemelli.** Il sensore di profondità del gemello lancia un
  raggio per zona; quello vero vede una zona larga circa 5,6°, e come una
  zona larga che sfiora un muro finisce nello strato non è misurato fino
  alla papera.
- **La casa 3D è un'anteprima.** Gli assi e gli intrinseci della camera
  vanno verificati sulla papera; sul gemello le texture sono sintetiche.
- **Una persona registrata come pochi punti** non chiude mai una porta: la
  regola che dimentica non è ancora scattata sul gemello (il suo guadagno è
  quello del banco).
- La teleguida di robotd non ha ancora una protezione dai dislivelli
  ([study/upstream-asks.it.md §8](study/upstream-asks.it.md)).
