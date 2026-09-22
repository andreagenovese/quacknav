# ADR 0006: La navigazione è un demone a sé

- Stato: accettata
- Data: 2026-09-22
- Ingressi: ADR 0001 (repo separato), ADR 0005 (consumare maploc),
  `docs/todo-map.it.md` (tre settimane di misure sul gemello MuJoCo),
  la decisione dell'utente del 2026-09-22

## Contesto

L'ADR 0005 metteva il client della mappa, il registro dei posti e la
guardia del dirupo in `quack-places`, "nessuna dipendenza dalla voce, un
domani un repo suo". Quel domani è arrivato dall'altra parte:
l'esploratore, la legge del passaggio, i libri di terra, l'homecoming e
il gemello di carta sono cresciuti dentro `quacksat-core` finché
novemila delle sue undicimila righe erano navigazione e duemila erano il
satellite vocale che dà il nome al repo.

Due cose hanno reso la separazione urgente e non solo ordinata. La
navigazione vale anche senza microfono — un bridge ROS, un agente, uno
script vogliono "dove sono" e "vai in cucina", e nessuno di loro vuole
una parola di richiamo. E il satellite vale anche senza navigazione: una
papera che ascolta e risponde è un prodotto per conto suo, e non deve
portarsi dietro un planner su mappa dei costi per farlo.

È stata la domanda dell'utente a decidere la forma: se sono due repo
separati, come fa un quacksat installato a sapere che c'è anche
quack-nav? Una feature di Cargo risponde a tempo di compilazione, che
non è una risposta per chi ha installato un binario.

## Decisione

### 1. Tre crate, due repo

- `quack-duck` — la lane di robotd (NDJSON JSON-RPC sul socket unix), i
  limiti della gait, i comandi del corpo e gli helper che ogni tool usa.
  Li manda entrambe le parti.
- `quack-nav` — client della mappa, guardia del dirupo, planner,
  registro dei posti, esploratore, homecoming, i tool di navigazione e
  il gemello di carta. Pubblica `quack-navd`.
- `quacksat-core` e i suoi backend — audio, parola di richiamo, VAD,
  riproduzione, il segnale "sto pensando" e i tool del corpo.

`quack-duck` e `quack-nav` stanno nel repo della navigazione: una
libreria di navigazione che si porta il suo client del robot è cosa
normale, e tiene il satellite libero da una dipendenza che altrimenti
erediterebbe.

### 2. La navigazione risponde su un socket

`quack-navd` ascolta su `/run/quack-nav.sock` e parla il filo di
robotd: NDJSON, JSON-RPC 2.0, una connessione per chiamante. Due metodi:
`nav.catalog` restituisce il catalogo dei tool (JSON Schema, la forma
dell'ADR 0004), `nav.call` ne esegue uno. Il demone possiede la lane
della mappa, la guardia, il registro, il lavoro di esplorazione e
l'homecoming, e legge il suo `/etc/robot/quack-nav.toml` — `[map]`,
`[gait]` e `[homecoming]` si sono spostate lì col codice.

### 3. Il satellite sonda, e funziona anche senza

All'avvio quacksat chiede il catalogo al socket. Se un demone risponde,
i suoi tool vengono annunciati accanto ai propri e ogni chiamata per
quei nomi passa sul lane. Se non risponde nessuno, il satellite lo dice
una volta e resta un assistente vocale la cui papera non può essere
mandata da nessuna parte. `[nav] socket` è tutto ciò che resta della
navigazione nella sua config, e la sua unit systemd `Wants` il demone,
non lo richiede.

### 4. Un guidatore alla volta, come prima

Il lavoro di esplorazione rifiuta un passo manuale mentre guida, come
prima; il rifiuto ora torna indietro sul lane, invariato, a chi l'ha
chiesto.

## Conseguenze

- I due si installano, si aggiornano e si versionano separatamente. Ciò
  che li lega è un filo, e un filo lo può parlare qualunque cosa.
- La navigazione è pubblicabile da sola — che è il punto: sono tre
  settimane di misure che su questo robot non ha nessun altro.
- Un processo in più su una scheda da 1 GB. La sua unit è recintata come
  quella del satellite (`Nice=5`, `CPUWeight=70`, 256/320 MB), e il
  ciclo a 50 Hz di robotd vince comunque ogni contesa per un core.
- Un salto in più per una chiamata di navigazione: un giro sul socket
  unix invece di una chiamata in processo. Un viaggio è un lavoro in
  background che risponde subito, quindi il salto si paga sulla
  risposta, non sul cammino.
- I test del satellite non coprono più la navigazione, e quelli della
  navigazione non hanno più bisogno di un microfono. 105 test, verdi da
  entrambe le parti; il banco di carta è invariato dalla separazione
  (guardato 22/30, cieco 25/30).
