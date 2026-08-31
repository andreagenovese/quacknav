# Studio: padd come modello per il client robotd di quacksat

Fonte: `pollen-robotics/microduck` @ clone del 2026-08-31. Blueprint per il
client di `quacksat-core`. Note complementari: `microduck-speaker-path.md`,
`microduck-mic-path.md`.

## padd in un paragrafo

Demone gamepad non privilegiato: un solo thread, `UnixStream` `std`
bloccante, niente tokio. Tutti i tipi di wire vengono da `duck-ipc-proto`
(`proto::Call`, `Request::{call,notify}`, `Response`); JSON-RPC 2.0 NDJSON,
un oggetto per riga, framing `\n`, sempre `flush()`. La doc del crate
enuncia la tesi: "no privileged access… the intent API is the path the app,
the SDK and any remote client will use".

## Contratti di wire da interiorizzare

- **Continuo → notifica** (niente id, niente risposta): `robot.move`,
  `robot.head`, `robot.pose`, `robot.mouth`, `robot.do`/`robot.sound` in
  hold. **Discreto → richiesta** (`Id::Number` incrementale):
  `robot.enable`, `robot.do` one-shot, `robot.setMode`, `robot.shutdown`,
  `robot.stop`, `robot.init`, `robot.relax`, `robot.theremin`.
- `IntentResult { accepted: false, reason }` è una **risposta normale**,
  non un errore; aggiornare lo stato locale solo quando `accepted` (il
  pattern setMode di padd). Il caso `error` JSON-RPC è separato. Le struct
  dei parametri sono `deny_unknown_fields`; quelle dei risultati no —
  tollerare campi sconosciuti e righe sconosciute.
- robotd limita le righe a `MAX_LINE = 64 KiB`.
- `hello` esiste (`HelloResult { api_version: 16, … }`) ma è un check di
  liveness, **non un gate** — nessun demone rifiuta per differenza di
  versione; avvisare una volta sola.

## Il deadman (contratto di sicurezza)

- `deadman_ms = 500` (`proto::pad_link::DEADMAN_MS`). Solo `robot.move`
  aggiorna il clock del twist; `robot.head` deliberatamente no. Superata
  l'età, robotd azzera il twist ("stop, not limp"); esposto come
  `move.limited_by = "deadman"` nello stream di stato.
- Due posture valide: guidare a ≥20 Hz (padd: 50 Hz) inviando `robot.move`
  a ogni tick, **oppure non inviare nulla** quando non c'è alcun comando —
  il deadman ferma il robot da solo. **Mai inviare un `robot.move` a zero
  fabbricato come keepalive**: maschera "nessun pilota" come "il pilota ha
  detto stop". Quando è attivamente in una modalità di non-guida (posando
  la testa), padd *invia* uno zero esplicito per tick — quella è una
  decisione, non un keepalive.
- Non esiste alcun metodo di ping/keepalive. Head/pose/mouth/suoni/skill
  non sono soggetti a deadman; l'hold di `wheee` decade (freschezza
  300 ms) se non viene rinviato.

## Stream (robot.subscribe / robot.state)

- `robot.subscribe { hz: Option<u32> }` → `SubscribeResult` (nomi file
  delle policy + `unavailable`), poi notifiche `robot.state` per sempre
  sulla stessa connessione. La decimazione è lato server per sottoscrittore;
  chiedere l'hz più basso di cui si ha bisogno. Ri-sottoscriversi
  sostituisce il rate.
- La backpressure non raggiunge mai il loop di controllo: broadcast
  limitato (256), il client lento riceve buchi (`Lagged` saltati). Usare
  `RobotState::t`, mai assumere continuità.
- Una connessione che sia chiama che si sottoscrive ha bisogno di **un solo
  BufReader a lunga vita**, correlando le risposte per `id` e saltando le
  notifiche (regola: una notifica porta `method`, una risposta no). Il
  BufReader-nuovo-per-richiesta di padd funziona solo perché padd non si
  sottoscrive mai. Alternativa: connessioni separate per lane
  (`Lane::Stream` vs `Prompt`), come fa `robotctl monitor` (quattro
  connessioni, tre demoni).
- Altri stream: `pad.input`→`pad.report` (il socket di padd
  `/run/padd/pad.sock`), `tof.stream`→`tof.frame` (tofd).

## Ciclo di vita della connessione

- padd: connette una volta; su qualsiasi errore logga + esce con codice non
  zero; systemd ritenta (`Restart=always`, `RestartSec=5s`,
  `After=robotd.service` + `Wants=robotd.service`, non `Requires`).
- Alternativa in-process (meglio per quacksat, che mantiene stato di
  sessione): il client theremin di robotd — una funzione per durata di
  connessione, il loop esterno dorme 2 s fissi, riconnette **e si
  ri-sottoscrive**. Limitare connect e write (btd: timeout 3 s / 5 s),
  chiudere la connessione su errore di scrittura.
- Path del socket da `proto::socket::ROBOT` con un override `--socket`
  perché il lavoro da banco con
  `ssh -L /tmp/robotd.sock:/run/robotd.sock` continui a funzionare.

## Privilegi e autorizzazione

- Socket di robotd: `0o660`, owner `root:robot` (il `Group=robot` sulla
  unit è portante). **L'appartenenza al gruppo è l'intera autorizzazione** —
  nessun check SO_PEERCRED, nessun handshake, nessuna ACL per metodo in
  robotd. quacksat nel gruppo `robot` ottiene l'intera superficie
  `robot.*`.
- La restrizione per namespace vive a livello di *transport*: il match
  esaustivo `permits()` di btd (senza wildcard, quindi le nuove varianti di
  Call non compilano finché non vengono classificate) rifiuta controllo
  motori, suoni, subscribe, shutdown ecc. via BLE con `PERMISSION_DENIED`
  (14). updaterd aggiunge un secondo livello: gruppo = può parlare,
  allowlist di uid = può mutare.
- **Implicazione**: qualsiasi policy più restrittiva per le chiamate
  originate dall'LLM spetta a quacksat scriverla — imitare il match
  esaustivo di btd se il backend agent inoltra chiamate.

## Pattern systemd/utente da copiare (padd.service)

- `sysusers.d/quacksat.conf`: `u quacksat - "…" - -`.
- Unit: `User=quacksat`, `Group=quacksat`,
  `SupplementaryGroups=robot audio`, `Restart=always`, `RestartSec=5s`,
  `Environment=RUST_LOG=info`, output sul journal, tracing su stderr con
  EnvFilter.
- Blocco di hardening preso alla lettera da padd: `NoNewPrivileges`,
  `ProtectSystem=strict`, `ProtectHome`, `PrivateTmp`,
  `ProtectKernelTunables/Modules/ControlGroups`, `RestrictSUIDSGID`,
  `RestrictNamespaces`, `LockPersonality`, `MemoryDenyWriteExecute`,
  `SystemCallFilter=@system-service`, `CapabilityBoundingSet` vuoto.
  Adattare `RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6` (backend di
  rete); l'`AF_NETLINK` di padd è un'esigenza di gilrs, non nostra.
  **Non** `PrivateDevices` (ALSA ha bisogno di /dev/snd).
- `RuntimeDirectory=quacksat` → `/run/quacksat/` è l'unico posto
  scrivibile sotto `ProtectSystem=strict`; identity.json e qualsiasi
  socket servito vivono lì; systemd lo pulisce allo stop.
- Prima cosa nel main:
  `duck_ipc_proto::log_startup_identity!("quacksat")` (scrive
  `/run/quacksat/identity.json`, logga l'identità di build).

## Se quacksat serve un proprio socket (verso il bridge)

Copiare `padd/src/tap.rs`: bind sotto RuntimeDirectory, rimuovere il
socket stantio, chmod 0660, poi `getgrnam("robot")` + `chown(path, -1,
gid)` nel codice (i gruppi supplementari non vengono ereditati dai file
creati). Rispondere alla subscribe prima della prima notifica; coda per
sottoscrittore limitata (256) con contatore dei drop, mai bloccare il
producer; `METHOD_NOT_FOUND` che nomina ciò che si serve per i metodi
estranei.

## Disciplina di logging

Nulla per tick. `warn` = eventi greppabili (connessione persa, backend
su/giù), `info` = cambi di modalità, `debug` = dettaglio per messaggio.
Una riga per transizione.
