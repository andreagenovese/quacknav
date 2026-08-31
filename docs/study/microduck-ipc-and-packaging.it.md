# Studio: superficie di duck-ipc-proto, updaterd, ordine di restart

Fonte: `pollen-robotics/microduck` @ clone del 2026-08-31 (workspace
v0.10.0, edition 2024, rust-version 1.89). Note complementari:
`microduck-client-pattern.md`, `microduck-speaker-path.md`,
`microduck-mic-path.md`.

## duck-ipc-proto

- Un solo file (`src/lib.rs`, ~4800 righe). Dipendenze deliberatamente solo
  tre: serde, serde_json, semver ("no http, no tar, no crypto, no tokio" —
  btd fa parte del percorso di recovery). La feature `test-support`
  aggiunge `every_call()` per i test di esaustività, zero dipendenze in
  più.
- **Non è su crates.io**; i consumatori usano dipendenze path. Il repo è
  pubblico per decisione (updater-design.md: "publish
  pollen-robotics/microduck", 2026-08-26). I crate esterni possono
  dipenderne via git.
- **Come quacksat dovrebbe dipenderne**:
  ```toml
  duck-ipc-proto = { git = "https://github.com/pollen-robotics/microduck.git", tag = "daemon-v0.10.0" }
  ```
  Pinnare a un tag di release `daemon-v*` (immutabile), corrispondente alla
  release sulla board; aggiornare deliberatamente. `API_VERSION` (16) è
  informativo — nessun demone rifiuta per differenza di versione; ciò che
  si rompe è una shape di parametri spostata, che si rifiuta da sé per
  nome. Impostare `rust-version = "1.89"`. Usare il re-export `semver` del
  crate. Abilitare `test-support` solo nelle dev-deps. Non vendorizzare i
  tipi.
- Costanti: `socket::ROBOT = /run/robotd.sock`, `socket::PAD`,
  `socket::TOF`, `socket::CONFIG`, `socket::UPDATER`; `JOINT_NAMES` (15);
  `identity_path()` → `/run/<service>/identity.json`.
- Envelope: `Request { id: Option<Id>, method, params }` (id assente =
  notifica), `Response { id, result, error }`. Il percorso pubblico è
  `Request::call/notify/as_call/as_state`, `Response::ok/err/result_as`.
  I tipi dei parametri sono `deny_unknown_fields`; i risultati no.
- Metadati di routing che vale la pena riusare: `Call::method()`,
  `is_mutating()`, `destination() -> Option<(Service, Lane)>`. `Lane` =
  `Prompt | Slow | Operation | Stream`; ogni demone serve una connessione
  una richiesta alla volta → **aprire una connessione separata per lane**
  (una connessione di stream `robot.subscribe` non trasporta mai
  richieste).
- Helper di identità che quacksat dovrebbe usare: `build_info!()`,
  `publish_identity` / `log_startup_identity!("quacksat")` → scrive
  `/run/quacksat/identity.json` (innocuo: la riconciliazione ignora le
  unit che non ha distribuito).
- Codici di errore: i codici della spec JSON-RPC più i codici applicativi
  1–14 (`BUSY=1 … PERMISSION_DENIED=14`).

## Superficie servita da robotd (dispatch, main.rs:2675–3010)

- Intenti accettati come notifica *o* richiesta: `robot.move`, `.head`,
  `.pose`, `.mouth`, `.do` (in hold), `.sound` (hold di wheee).
- Solo richiesta: `robot.look` (IK della testa), `.stop`, `.enable`
  (toggle), `.init`, `.relax`, `.setMode`, `.mode`, `.shutdown`,
  `.theremin`, `.chorale`, `.subscribe`, `.health`, `.safeToRestart`,
  `.modelApi`, `.remoteSessionActive`, `hello`. Tutto il resto →
  `METHOD_NOT_FOUND "<m> is not served by robotd"`.
- Le ragioni di rifiuto sono stringhe in `IntentResult.reason` (per es.
  sound: "this robot has no voice…"; theremin: "no depth frames — is tofd
  running?").
- Le notifiche non ricevono mai risposta; le notifiche non parsabili
  vengono scartate in silenzio. `MAX_LINE = 64 KiB`. Molti client
  concorrenti vanno bene (un task per connessione). I sottoscrittori in
  ritardo ricevono buchi, mai backpressure.
- Non esiste alcun namespace `maintenance.*`; init/relax ecc. stanno in
  `robot.*` e sono tenuti fuori dai transport remoti solo dalla tabella di
  routing di btd.
- Invarianti di design (architecture.md / robotd-design.md): nessun broker,
  un socket per servizio; async con timeout ovunque — "a closed or silent
  socket is a normal, expected answer"; robotd autoritativo sulla
  sicurezza; intenti continui = slot last-writer-wins con scadenza; FSM di
  bring-up `Limp → Homing → Ready` — **robotd non muove mai il robot di
  propria iniziativa**; health calcolata dagli atomics, mai interrogando il
  loop.

## updaterd — come quacksat sopravvive agli aggiornamenti

- Layout: `/opt/robot/daemon/releases/<ver>/`, symlink `current` (rename
  atomico), `golden` mai potato, `keep_previous = 1`. **Niente A/B**,
  nessun rollback fuori da `install_dir`. Artefatti firmati (minisign); gli
  hook viaggiano dentro il tarball firmato.
- `hooks/postinstall` scrive (mai cancella): script di rescue, banco
  suoni, i sysusers e i file di unit della release stessa
  (sovrascrivendoli — personalizzare le unit distribuite solo tramite
  drop-in `/etc/systemd/system/<unit>.d/`). Nulla enumera o rimuove file
  che non ha distribuito → un `quacksat.service` installato a mano non
  viene mai toccato da apply/rollback/select/revert.
- **L'unica trappola — il check degli orfani** (`updater/src/orphan.rs`):
  qualsiasi unit in `/etc/systemd/system` il cui `Exec*=` risolve sotto
  `/opt/robot/daemon/current/` entra nell'insieme gestito; una release
  candidata priva di quel binario rifiuta di applicarsi
  (`WouldOrphanUnit`).
  → **L'ExecStart di quacksat NON deve vivere sotto current/**: usare
  `/usr/local/bin/quacksat`.
- Da non fare: non aggiungere quacksat a `on_apply.units` (un restart
  fallito farebbe rollback del *loro* aggiornamento); non mettere file in
  `releases/`; non cablare la configurazione nella unit (file di config in
  `/etc/robot/`, stato in `/var/lib/quacksat/`, entrambi sopravvivono a
  update e rollback).
- Accoppiamento futuro opzionale: un `[component.quacksat]` separato in
  `/etc/robot/updater.toml` con il proprio `install_dir = /opt/quacksat`,
  source e chiave di firma riusa l'intero motore di update firmati senza
  ingarbugliarsi con il componente daemon. Non serve per la v0.
- Installazione: `install-quacksat.sh` idempotente che scrive binario,
  unit, sysusers (`/etc/sysusers.d/quacksat.conf`, non /usr/lib), config,
  directory di stato; `daemon-reload && enable --now`.

## Ordine di restart / boot

- Insieme di restart di un update = unit distribuite dalla release ∪
  `on_apply.units`, meno updaterd/btd. quacksat non è in nessuno dei due →
  **un update riavvia robotd sotto i piedi di quacksat e non riavvia mai
  quacksat**. Il client deve trattare un `/run/robotd.sock` caduto come
  normale: riconnettere (o uscire; `Restart=always RestartSec=5s`),
  **ri-sottoscriversi**, e assumere stantio lo stato latched — un robotd
  riavviato è `Limp`, `enabled = false`; il robot resta in piedi (i
  Dynamixel mantengono l'ultimo goal).
- Boot: nessun ordinamento tra demoni oltre a padd-dopo-robotd (advisory)
  e btd-dopo-bluetooth. quacksat: `After=robotd.service
  local-fs.target network-online.target`, `Wants=robotd.service
  network-online.target` (After, non Requires — un robotd giù non deve
  impedire a quacksat di partire e ritentare).
- La riconciliazione (`updater/src/reconcile.rs`) itera solo sulle unit
  distribuite — quacksat le è invisibile. Bene così.

## Realtà delle risorse (nessun budget formale)

- Nessun MemoryMax/CPUQuota/Nice da nessuna parte; i vincoli reali:
  journald limitato a 200 MB (il logging per tick sfratta i log utili —
  loggare solo le transizioni); `/var/log` è zram (perso in caso di
  interruzione di corrente); il percorso WebRTC di mediad ≈ 25–40% di un
  core; l'encoding software surriscalda il SoC. Tenere modesta la CPU a
  regime di quacksat; il budget di inferenza della wake word conta.
- Baseline di hardening: copiare i blocchi di padd/tofd; aggiungere
  `AF_INET AF_INET6`; **togliere `MemoryDenyWriteExecute` se si carica un
  runtime ML (ONNX/JIT)**.

## Alternativa degna di nota

Invece di riprodurre l'audio da sé, quacksat potrebbe pilotare
`robot.sound` + `robot.mouth` e lasciare che robotd sia la voce — funziona
solo per i versi dell'anatra (enum chiuso), non per il TTS; vedi
`microduck-speaker-path.md`.
