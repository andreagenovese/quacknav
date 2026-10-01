# quack-nav v0.2.0-rc1 — note di rilascio

2026-10-01. Copia inglese: [release-notes-v0.2.0-rc1.md](release-notes-v0.2.0-rc1.md).
L'elenco completo delle modifiche è in [CHANGELOG.it.md](../CHANGELOG.it.md).

## Cos'è

Una **release candidate validata sui gemelli** — il gemello MuJoCo del
Microduck (il robotd rilasciato di Pollen daemon-v0.14.4 e il corpo di
`microduck_rl`) e il gemello di carta — **non ancora su una papera vera**.
La papera fisica arriva a dicembre 2026; allora si misurano i numeri che
solo l'hardware può dare (passo delle celle, estensione della mappa, costo
di CPU sull'RK3566) ([todo-map](todo-map.it.md)). Fino a una prima sessione
sulla papera vera, ogni numero qui sotto è del gemello, non una promessa.

Dalla 0.1.0 il mapper gira dentro `quack-navd` sopra il robotd rilasciato,
la posa esce a 20 Hz, la papera torna a casa su una mappa salvata più in
fretta e mai sbagliando, esplora una casa una carica alla volta, e ogni
viaggio gira su un solo anello. Gli interruttori sono passati da 169 a 122.

## Risultati principali

Sul gemello MuJoCo salvo dove detto; fonti in [results.it.md](results.it.md)
e [todo-map.it.md](todo-map.it.md) §2d.

- **Nessuna caduta.** 0 cadute in 11 sessioni di esplorazione (5 h 30) e 51
  viaggi su tre case (il protocollo della release); nessuna da allora, su
  casa_grande (quattro sessioni, 16 viaggi, 8 risvegli) e nei cinque giri
  final-house che hanno validato il tetto sulle chiusure d'anello.
- **I viaggi arrivano.** 46/51 (90 %) all'A/B della release, dove la build
  precedente arrivava 32/51 (63 %); da allora ogni viaggio è arrivato: 16/16
  su casa_grande, e 6/6 e 8/8 per giro nei giri final-house.
- **La mappa.** Muri sulla verità 95–100 % nei giri final-house, pavimento
  noto 94–96 %; casa_grande 99 % dei muri sulla verità, 0 dislivelli
  fantasma. Errore medio dei muri 3,1–4,5 cm su house2 e casa_libera alla
  release.
- **Il ritorno a casa.** Sul banco dei risvegli (12 punti di partenza in due
  case, poi gli stessi girati di 180°) 23 risvegli su 24 confermati giusti,
  nessuno sbagliato, mediane 87–123 s (126–192 s prima della mappa ombra);
  casa_grande 8/8, mediana 84 s (72–111 s), 2–16 cm dalla verità. Alla
  release ogni posa confermata era a 0,07–0,17 m dalla verità.
- **La direzione.** Con le chiusure d'anello limitate a 4° l'errore mediano
  di direzione è stato 0,63–0,92° in tutti e cinque i giri, contro
  0,97–1,26° in tutti e quattro senza; 20 sessioni rigiocate, ATE medio da
  0,0946 a 0,0891 m.
- **La soglia del gemello di carta** (CI, semi fissi): esplorazione 40 giri,
  0 cadute, copertura media 53,2 %; `go_to` 30/30. 153 test passano (1
  ignorato).

## Compilare, installare, far girare

Segui il [README](../README.it.md#farlo-girare): compilazione,
configurazione, unità systemd e gemello di carta. Il gemello MuJoCo è in
[scripts/twin/README.it.md](../scripts/twin/README.it.md).

## Aggiornare dalla 0.1.0

- **Interruttori**: ogni variabile d'ambiente `QUACKSAT_*` si legge solo come
  `QK_*` — non c'è alias. Gli interruttori degli esperimenti conclusi non ci
  sono più (nomi tolti in cima a [todo-map.it.md](todo-map.it.md)); uno tolto
  ma ancora impostato viene ignorato in silenzio. Quelli che esistono sono
  in [knobs.it.md](knobs.it.md).
- **Socket**: `quack-navd` ascolta su `/run/quack-nav/nav.sock` (era
  `/run/quack-nav.sock`); con `[maploc] enabled` la mappa è su
  `/run/quack-nav/map.sock`. Chi chiama (quacksat compreso) deve seguire.
- **Posti**: `places_path` ha come predefinito
  `/var/lib/quack-nav/places.json`, e il vecchio
  `/var/lib/quacksat/places.json` non si legge più: va spostato.
- **robotd**: fissato a daemon-v0.14.4 (API 34). daemon-v0.15.0 è validato
  sul gemello solo nel branch `microduck-015`.
- **Configurazione**: `[map] explore_turn` si carica ancora e non fa nulla.
- **API Rust**: `maploc::Mapper::boot_search`, `ExploreHandle::forget_ground`,
  `Grid::unknown_around`, `Control::request_method`, i limiti della testa in
  `quack_duck::body` e `frontier::waypoint` non ci sono più;
  `explore::Job::new` è `Job::new(known, max_s, ask, now)`, e anche
  `to_goal`, `start` e `start_goto` hanno perso l'argomento della mano di
  virata.
- **Log**: la riga di stato di maploc non ha più il campo `boot`.
- **Visore del gemello**: `QUACK_NAV_MCP` / `QUACK_NAV_MCP_TOKEN` non ci sono
  più; chiede a `QUACK_NAV_SOCKET`.

## Limiti noti

- **La posa si ferma nelle virate veloci**: durante una virata veloce sul
  posto la posa della mappa può restare ferma fino a ~1,5 s e poi
  recuperare (visto su casa_arredata).
- **Dislivelli fantasma vicino ai buchi**: un bordo messo a libro con una
  posa sbagliata finisce 20–35 cm fuori da quello vero (circa 1 dislivello
  su 50); accanto a un buco restringe un passaggio per il planner.
- **Risvegli lenti a est della tromba delle scale dell'appartamento**: le
  finestre rifiutano un seme giusto per minuti.
- **La covarianza non è ancora un allarme**: onesta in media su una mappa
  nuova, troppo sicura su una ripresa, e non ha segnalato una deriva di
  0,35 m (σ 0,08 m).
- **La mappatura a soste chiede un giro guidato**: qualcuno deve portare in
  giro la papera con delle pause; il giro è ancora da progettare.
- **La CPU sull'RK3566 non è misurata**, come il passo delle celle e
  l'estensione della mappa sull'hardware; maploc è la cosa più affamata di
  CPU che la papera possa fare.
- I viaggi sono più lenti di quelli della build precedente (mediana
  106–111 s contro 66–101 s); "a che punto è" sbaglia per difetto; tutto è
  solo sul gemello. Vedi [results.it.md](results.it.md).

## Cosa serve per la v0.2.0 finale

Una prima sessione sulla papera vera: lo stack che gira sull'RK3566 sopra il
robotd rilasciato, con le guardie accese e qualcuno accanto, e i suoi numeri
scritti accanto a quelli del gemello.
