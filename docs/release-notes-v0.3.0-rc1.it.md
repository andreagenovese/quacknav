# quack-nav v0.3.0-rc1 — note di rilascio

2026-10-09. Copia inglese: [release-notes-v0.3.0-rc1.md](release-notes-v0.3.0-rc1.md).
L'elenco completo delle modifiche è nel [CHANGELOG.it.md](../CHANGELOG.it.md);
le note della candidata precedente sono in [release-notes-v0.2.0-rc2.it.md](release-notes-v0.2.0-rc2.it.md).

## Che cos'è

Una **terza release candidate, validata solo sui gemelli**: il gemello
MuJoCo del Microduck (il daemon-v0.16.1 rilasciato da Pollen e il corpo di
`microduck_rl`, con entrambe le policy di camminata di Pollen) e il gemello
di carta. **Non ancora su una papera vera**: quella fisica arriva a
dicembre 2026, e fino a una prima sessione con lei ogni numero qui sotto è
del gemello.

La rc2 ha fatto riposare la papera, l'ha fatta portare in braccio e
guidare a mano. La rc3 riguarda la papera come la spedisce Pollen, con la
sua camminata di default e il daemon più recente, e un modo migliore di
fare i passi: uno stick che sa cosa fare con ciò che la mappa non ha, e un
modello neurale di navigazione che può prenderne il posto.

## Prima di installare

1. **Dite quale camminata usa la papera**, in `/etc/robot/quack-nav.toml`:

   ```toml
   [gait]
   profile = "velstand"   # il default di Pollen; "alpha" per alpha_walking + alpha_stand
   ```

   Se manca, vuol dire velstand. Una papera su alpha la cui
   configurazione non ha una sezione `[gait]` deve aggiungere
   `profile = "alpha"` (l'installer lascia com'è una configurazione
   esistente).
2. Il daemon validato è il **daemon-v0.16.1** di Pollen.

## Cosa cambia rispetto alla rc2

- **Entrambe le camminate.** velstand (il default di Pollen dal set di
  policy v5: una sola rete che cammina e sta in piedi) prima non
  funzionava: robotd etichetta "walk" una papera velstand ferma, quindi
  quack-nav non la vedeva mai ferma, e dalla 0.16.1 gli sguardi automatici
  di robotd si prendevano la testa. Ora decide la velocità applicata, la
  papera conta come ferma 0,6 s dopo (il corpo prosegue tanto) e le soste
  sono più lunghe di altrettanto. Sul gemello: gli stessi viaggi di alpha,
  la posa più vicina (6–7 cm di mediana), circa il 10 % più lenta.
- **Il pilota, un modello neurale di navigazione, uno per camminata.** Un
  MLP (351 ingressi, 9 mosse) sceglie ogni passo di un viaggio al posto
  delle regole dello stick, mai la rotta, con scudi rigidi attorno. Il
  pacchetto installa `pilots/alpha/` e `pilots/velstand/` (`pilot.json`
  per quack-navd, `pilot.onnx` per tutto il resto) in
  `/var/lib/quack-nav/pilots/`; spento finché non si imposta
  `QK_RL_POLICY=/var/lib/quack-nav/pilots` (knobs.env), poi il pilota
  della camminata. Banco: 97,1 % contro il 93,3 % dello stick (alpha),
  95,2 % contro 94,8 % (velstand); sul gemello con velstand, 12/12 senza
  cadute.
- **Lo stick, in un viaggio**: il naso contro qualcosa per due passi è uno
  stallo (il tavolino di apartment, dove il corpo spingeva per minuti e la
  posa scivolava di 0,3 m); ciò che il sensore continua a vedere entro
  0,30 m va sul libro prima dell'urto; una svolta sul posto si ferma prima
  dello slancio che impara (oscillazioni da 232 a 5, viaggi più veloci del
  9–22 %). Una corsia percorsa cede a un drop sul libro entro 0,20 m, e la
  rotta resta lontana dall'angolo di un vano scala.
- **La guardia dei buchi contro un muro.** Un buco contro un muro veniva
  letto come il bordo di una scatola; un cervello che sceglie a caso c'è
  entrato una volta sul banco. Corretto nella sola guardia; 0 buchi in
  7.560 viaggi casuali da allora.
- **daemon-v0.16.1** (API 41): il riferimento è spostato; lo sweep della
  testa ora si riprende la testa dagli sguardi automatici di robotd.

## Misurato contro main

Quattro giri per casa sul gemello MuJoCo, gli stessi libri, due gemelli
alla volta:

| | main | v0.3.0-rc1 |
|---|---|---|
| viaggi, velstand | 48/48 + casa_ingombra 11/12 | 48/48 + 12/12 |
| viaggi, alpha | 48/48 + 11/12 | 48/48 + 12/12 |
| esplorazione, velstand (tre case) | copertura, nessun fantasma, 34/34 dopo | lo stesso |
| cadute in un buco | 0 | 0 |

Dettagli: [results.it.md](results.it.md), "v0.3.0-rc1: le verifiche contro
main".

## Limiti noti

- **Solo i gemelli.** Tarate il pilota sulle tracce della papera prima di
  fidarvi (docs/rl-pilot.it.md, "La taratura sulla papera").
- **velstand esplora più lento** nella prima sessione di una casa (le sue
  soste sono più lunghe): circa una sessione in più per finire una casa.
- **Ciò che si muove** (persone, animali) va sul libro solo entro 0,30 m e
  ci resta finché un "nessuna strada" non lo cancella: il prossimo lavoro,
  dopo questa release.
- Il teleop di robotd non ha ancora protezione dai dislivelli
  (docs/study/upstream-asks.it.md §8).
