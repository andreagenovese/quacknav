# quack-navd come servizio

Stato: design, prima stesura · Data: 2026-11-02 · Responsabile: andreagenovese · Roadmap: N4, N5

Questa pagina possiede il contratto tra `quack-navd` e qualunque cosa gli stia sopra — il
satellite vocale, la pagina di controllo, un agente, uno script. I tool stessi (cosa fa
`robot.go_to`, cosa riporta `robot.map_status`) restano in
[`control-contract.it.md`](../control-contract.it.md); qui c'è la forma intorno a loro: come un
client sa con chi parla, sente cosa è successo senza chiedere, tiene la papera per sé mentre la
usa, le passa una lista di cose da fare, e le chiede di *fare* qualcosa invece di camminare.

Segue apposta la forma di `robotd` (`docs/design/robotd-design.md` di Pollen, `duck-ipc-proto`):
un crate di protocollo tipizzato e versionato, un solo hello, una differenza di versione che si
registra e si serve invece di essere rifiutata, NDJSON JSON-RPC 2.0 su un socket locale.

## 1. Cosa c'è oggi

Un socket Unix, NDJSON, JSON-RPC 2.0, quattro metodi: `nav.catalog` (i tool con i loro schemi
JSON), `nav.call` (`{name, args}`, un tool), `nav.knobs`, `nav.restart`. Nessuna versione sul filo;
niente di spinto — il satellite e la pagina di controllo interrogano `robot.map_status` per sapere
che un viaggio è arrivato; chiunque può guidare in qualunque momento, vince l'ultima chiamata; una
chiamata è un lavoro. Ogni client porta la sua copia del filo (`nav_client.rs` di quacksat, il
bridge di quack-control).

Tutto quanto segue si aggiunge. `nav.catalog` e `nav.call` restano, con le risposte di oggi, finché
il daemon esiste: un agente che conosce solo loro continua a funzionare.

## 2. La versione

`API_VERSION`, un intero nel crate del protocollo, alzato da ogni cambiamento che un client
potrebbe notare: un metodo nuovo, un campo nuovo, un evento nuovo. Mai per una correzione che
lascia il filo com'è.

`nav.hello` → `{api_version, daemon_version, revision, methods, events}`. Un client lo manda per
primo e registra cosa ha ricevuto. **Una differenza si registra e si serve, mai si rifiuta** (la
regola di robotd): un client più vecchio semplicemente non chiama ciò che non conosce; un client
più nuovo che chiama un metodo che questo daemon non ha riceve `METHOD_NOT_FOUND` (−32601) e un
campo sconosciuto riceve `INVALID_PARAMS` (−32602) che lo nomina — rifiuta solo una rotta o un
parametro che davvero manca. La pagina del protocollo tiene una breve storia: `vN — cosa è
arrivato`.

## 3. Eventi, spinti

`nav.subscribe {topics?}` fa della connessione un flusso: la risposta sono gli argomenti concessi,
e da lì in poi il daemon scrive notifiche `{"method": "nav.event", "params": {topic, seq, t,
data}}` su quella connessione, intercalate con le risposte a qualunque altra cosa il client chieda
ancora su di essa. `seq` è per connessione e senza buchi, così un client sa se ha perso qualcosa;
`t` sono i secondi monotoni del daemon.

I primi argomenti, ciascuno da un fatto che il daemon ha già:

| argomento | quando | dati |
|---|---|---|
| `job` | lo stato di un lavoro cambia — partito, in corso, fatto, fermato, fallito, rifiutato | `{state, kind, goal, reason, progress}` |
| `job.progress` | al più una volta al secondo mentre un lavoro gira | `{progress, route_m, legs}` |
| `pose` | la posa della mappa diventa fidata, o smette di esserlo | `{tracking, untrusted, pose}` |
| `fall` | il corpo è a terra o seduto durante un lavoro | `{pose}` |
| `question` | l'esploratore chiede alla persona cos'è un posto | `{phrase, pose}` |

La consegna è al meglio e in ordine. Un iscritto che non legge non viene aspettato: oltre una coda
limitata la sua connessione si chiude e il fatto si registra — il daemon non ferma mai un lavoro
per un ascoltatore lento. Interrogare `robot.map_status` resta corretto; gli eventi lo rendono solo
inutile.

## 4. Un proprietario alla volta

Oggi vince l'ultimo chiamante. Con una voce, una pagina di controllo e un agente sulla stessa
papera questo è un viaggio tagliato a metà dal `go_to` di qualcun altro. La risposta è un
**prestito** esplicito e breve:

- `nav.claim {holder, ttl_s}` → `{token, holder, until}`. `holder` è un nome per le persone
  ("quacksat", "pagina di controllo", "l'agente"); `ttl_s` al più 300. Rinnovato richiedendolo
  col token.
- Mentre un prestito è tenuto, i tool che **muovono la papera** (`robot.go_to`,
  `robot.map_explore`, `robot.map_step`, `robot.move`, `nav.act`, `nav.jobs`) da chiunque altro
  sono rifiutati con `LEASED` (−32010) e il nome di chi lo tiene. Le letture non si rifiutano mai.
- **Uno stop non si rifiuta mai.** `robot.go_to {"stop": true}` da chiunque chiude il lavoro del
  prestito e il prestito con lui; la sicurezza non aspetta la proprietà.
- `nav.release {token}` lo chiude; lo chiudono anche il ttl e la chiusura della connessione di
  chi lo tiene.
- Nessun prestito, nessun cambiamento: chiunque può guidare, come oggi. Un client che non chiede
  mai si comporta esattamente come adesso.

Il prestito è della navigazione, non dei motori. `robotd` resta l'unico proprietario delle
giunture e il suo deadman resta la rete di sicurezza; il prestito dice solo di chi sono i *lavori*
che quack-navd esegue.

## 5. Una coda di lavori

`nav.jobs {steps: [{tool, args}…], on_failure: "stop"|"continue"}` → `{queue}`. Il daemon esegue i
passi in ordine, ciascuno fino alla sua fine (un viaggio fino all'arrivo, una skill fino al
termine), e riporta ciascuno sull'argomento `job` col suo indice. `on_failure: "stop"` (il
predefinito) chiude la coda al primo passo che fallisce o è rifiutato, e dice quale e perché.
`nav.jobs {}` risponde lo stato della coda; uno stop la chiude. Una coda alla volta; tiene il
prestito implicitamente mentre gira se il chiamante ne tiene uno.

È la lista "vai in bagno, prendi i calzini, portali in lavanderia, torna da me": l'agente la
pianifica, la papera la esegue sulla scheda, passo per passo, e l'agente sente la fine di ogni
passo. La pianificazione resta dell'agente; l'esecuzione resta della papera.

## 6. Fare, non solo camminare — `nav.act` (N5)

`nav.act {skill, at?}`: una delle skill che il daemon ha — lette da `robot.skills` di `robotd` a
run time (voci della tabella e built-in), mai una lista scritta qui — eseguita come un lavoro solo:

1. `at` assente o `"here"`: come la papera sta. `at` un posto con nome o `{x, y}`: prima ci
   cammina (un viaggio, con tutto ciò che un viaggio ha).
2. Squadrarsi: l'ultima sosta gira il corpo verso la direzione che il posto dà, se ne dà una.
3. `robot.do {skill}` di `robotd`; attendere che il corpo finisca (l'etichetta della policy di
   nuovo `walk` o `stand`, o la fine propria della skill).
4. Rispondere `{done, skill, at, ok, reason}`: `ok` falso con il motivo quando il daemon ha
   rifiutato la skill, il corpo è caduto, o la fine della skill non si è vista nel suo tempo.

Le skill sono le policy di Pollen, eseguite da `robotd`; quack-navd non ne addestra, distribuisce o
sostituisce mai una. Se una presa ha *tenuto* la cosa lo dice il corpo (il divario del becco,
`ledger::BodyWatch`) e si riporta, non si indovina. Mirare a una cosa vista dalla camera è di N7,
non di qui.

## 7. Il crate del protocollo e il crate client

- `quack-nav-proto`: `API_VERSION`, i nomi dei metodi, parametri e risultati di `nav.hello`,
  `nav.subscribe`, `nav.event`, `nav.claim`, `nav.release`, `nav.jobs`, `nav.act`, i codici
  d'errore. Nessun I/O, nessuna dipendenza oltre `serde`. Il daemon e ogni client compilano
  contro di lui, così un campo rinominato è un errore di compilazione da entrambe le parti invece
  di un silenzio sul filo.
- `quack-nav-client`: un client bloccante sul socket — `connect`, `hello`, `call(tool, args)`,
  `catalog`, `subscribe(topics) -> events`, `claim`/`release`, `jobs`, `act`. `nav_client.rs` di
  quacksat e il bridge di quack-control passano su di lui; è la prova di N4.

## 8. Ordine del lavoro

1. Il crate del protocollo, `nav.hello`, `docs/protocol.it.md` con la sua storia delle versioni. (v1)
2. Eventi: `nav.subscribe` e i cinque argomenti. (v2)
3. Il crate client; quacksat e quack-control su di lui.
4. Il prestito. (v3)
5. La coda. (v4)
6. `nav.act` su `robot.do`, `robot.skills` letto a run time. (v5)

Ogni passo il suo ramo, i suoi test, l'A/B del gemello dove tocca ciò che la papera fa (4–6), docs
EN+IT, fuso al via dell'utente.

## 9. Non qui

Il trasporto oltre il socket locale (una corsia TCP autenticata per un client nella LAN) aspetta un
client che ne abbia bisogno. La camera, il registro degli oggetti e il lavoro di ricerca sono N7.
Più papere non sono un caso.
