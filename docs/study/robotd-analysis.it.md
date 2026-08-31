# robotd — analisi approfondita (robotd-design.md, draft 2026-08-20)

Hardware v1: solo variante alpha, solo board Radxa (RK3566), solo scheda
`imu_to_dxl` v2. Il modo "roller" (ruote) non è una variante hardware ma un
preset: `policy.mode = "roller"` seleziona policy e tuning.

## 1. Il bus — un solo UART, un solo proprietario

- `/dev/ttyS2` a 1 Mbps, protocollo Dynamixel v2, **16 dispositivi**: 15 servo
  (id 20–24 gamba sx, 30–34 collo/testa/bocca, 10–14 gamba dx) + la scheda IMU
  come **id 200 sullo stesso bus** — la quaternione SFLP esce dagli stessi
  registri dei servo, un solo code path, zero astrazione IMU.
- Esclusione del port *organizzata, non imposta*: `TIOCEXCL` blocca solo gli
  open non privilegiati, ma robotd gira da root. Trappola nota: Armbian mette
  un `serial-getty` su UART2 — `setup-board.sh` maschera l'unit;
  `fuser -v /dev/ttyS2` risponde a "chi ha il bus".
- Due transazioni per tick (un `sync_read` registri 124–136 per tutti, un
  `sync_write` dei goal) + una al secondo per tensione/temperatura (144–146).
  `return_delay_time=0` è load-bearing: al default 250 sarebbero ~8 ms/tick di
  turnaround, il 40% del budget. Il `sync_read` è all-or-nothing: un servo muto
  fa fallire l'intera transazione e si tiene il campione precedente.
- Tensione mediata sul pack (scartando chi risponde 0); temperatura MAI mediata
  — riporta il giunto più caldo per nome. Terza sorgente: temperatura SoC da
  sysfs, fuori da RobotIo, così risponde anche a bus morto.
- Staleness IMU tracciata per sempre: blocchi identici consecutivi contati e
  loggati (soglia 0,5 s), rate-limited per non evacuare il journal.

## 2. Il tick (50 Hz, tokio task su runtime dedicato)

```
sync_read (IMU+15 servo) → safety.observe (fallen? debounce 0,2 s)
  → Observation::build [f32;61] ← Command ← gate(deadman) ← intent snapshot
  → Policy::infer (ONNX) [f32;14]  (bocca esclusa, slot 9 = 0)
  → home pose + action_scale × azione → low-pass su testa e gambe
  → safety.apply (UNICO RobotIo: rifiuta non-finiti, clampa al range attuatore)
  → sync_write goal positions
  → publish: atomics sempre; frame di stato solo se qualcuno è iscritto
```

- Osservazione 61-D: `gyro(3) | gravità proiettata(3) | pos giunti(14) |
  vel(14) | ultima azione(14) | comando(13)` con comando = twist(3) + testa(4)
  + body(6). Tre insidie documentate: body tutto-zero è la codifica nominale;
  i target testa viaggiano NEL comando (non si sommano all'output → doppia
  piega); ordine body `z, roll, pitch` (invertirli inclina di lato).
- Catena di priorità skill: roulade > kick > ground-pick > sit/rise > stand
  (per |twist| o forzato) > walk. Quirk preservati apposta: kick e rise girano
  al tuning "standing" perché così sono stati addestrati. "Non regredire ciò
  che già funziona" è la regola.
- `MissedTickBehavior::Skip`: Burst accumula comandi motore, Delay fa derivare
  il rate; Skip mantiene lo schedule e scarta i tick persi.
- `driving = enabled ∧ policy caricata ∧ sensori questo tick ∧ ¬limp-fall`.
  Edge: inizio guida → controller.reset() (altrimenti lurch da stato stantio);
  fine guida → hold = posa corrente catturata una volta (rileggerla ogni tick
  affloscerebbe il robot sotto gravità).
- Validazione policy al LOAD, non all'inferenza: shape obs[1,61]→act[1,14],
  warm-up prima del loop. Workaround per `ort` che PANICA (non erra) se ONNX
  Runtime manca: `ensure_runtime` sonda la dylib prima di toccare ort.
  `policy.enabled=false` ≠ policy rotta: il primo è sano (bench), il secondo
  fa rollback. ONNX Runtime è prerequisito di board, non nella release.

## 3. Safety — struttura, non convenzione

- `safety` possiede l'unico handle di scrittura `RobotIo`: policy e client
  *propongono* target, il borrow checker impedisce che li scrivano. "Rendere
  irrappresentabile lo stato rotto" invece di ricordarsi una regola.
- Regole incondizionate: rifiuto NaN (non clampato: rifiutato) + clamp al range
  dell'ATTUATORE (non limiti anatomici per giunto — punto aperto §9.3).
- **Deadman**: intenti fermi → velocità a zero. Stop ≠ limp: perdere le comm
  fa STARE FERMO il bipede (stato sicuro), non lo affloscia.
- **Il verdetto di caduta riporta e non gate-a niente**: un robot a terra resta
  enable-abile, init-abile, guidabile — è proprio quando serve che funzionino.
  Le versioni con gate `fall_limp`/`fall_recover` nel safety sono state RIMOSSE:
  "una regola di safety che il recovery deve bypassare non è una regola".
- **limp_fall è un terzo evento** (attivo di default): rilevatore predittivo su
  `ġ = −ω × g` dal gyro (esatto, senza il lag del filtro SFLP), scatta a ~26°
  in ribaltamento previsto oltre soglia, debounce 3 tick. Sequenza: limp a
  gain_limp seguendo i giunti giù → attesa gyro quieto → rampa ~1 s alla posa
  standing → handover (twist a zero seleziona da sé la rete standing). Tuning
  volutamente "tardivo": un falso positivo È una caduta causata.
- Battery shutdown: EMA ~10 s, a 6,6 V si siede e spegne la board (un sag da
  carico non può farla scattare).

## 4. API e concorrenza

- Due vocabolari: **intenti in ingresso** (notifiche JSON-RPC senza id,
  last-writer-wins, 50 Hz: `robot.move`, `robot.head`) e **richieste discrete**
  con risposta (`robot.stop`, `robot.enable`). Su WebRTC le notifiche andranno
  sul canale unreliable "teleop", le richieste sul reliable "control" — la
  distinzione cade dalla famiglia di messaggio, non da una regola da ricordare.
- Twist e testa in slot separati apposta: single-writer di fatto, un gamepad
  sul corpo e un altro client sulla testa non si pestano. Ogni slot è
  timestampato: al loop interessa "quanto è vecchio", non "quanto vale".
- **Stato in uscita**: uno stream `robot.subscribe`, decimato per-subscriber,
  broadcast bounded drop-on-lag (mai backpressure sul loop). Riporta il
  RIFIUTATO oltre all'applicato: `requested` vs `applied` + `limited_by` —
  senza, una UI col joystick avanti e il robot fermo è inutilizzabile. Batteria
  in volt E percento (mappa 6,6–8,2 V NP-F550 calcolata lato robot, così due
  schermate non mostrano due numeri diversi). Nessun frame assemblato se
  nessuno è iscritto.
- **Bring-up a stati**: Limp → (enable: policy caricata + campione fresco) →
  Homing (torque on, rampa 2 s) → Ready. robotd NON muove mai il robot perché
  un processo è partito: al riavvio adotta la posa corrente e non tocca il
  torque — un robot in piedi resta in piedi attraverso un update (asserito da
  test sull'ASSENZA di scritture). `robot.init`/`robot.relax` serviti dal
  daemon dal loop stesso (niente secondo writer sul bus); non raggiungibili
  via BLE (un bottone sul telefono che affloscia il robot non si offre).
- **Health pubblicata, mai chiesta**: calcolata lato IPC da atomics (timestamp
  ultimo tick + contatori) — un loop incastrato si dichiara unhealthy invece
  di appendere il chiamante. Solo ciò che è imputabile a una RELEASE può
  toccare il verdetto: batteria e temperature sono descrizione (gate sulla
  batteria = robot non aggiornabile finché il pack è scarico).
- **Namespace maintenance separato** (§3.5): init, torque-off, calibrazione,
  scritture raw fuori dagli intenti, così l'allow-list per-transport del relay
  li tiene fuori dai transport remoti.

## 5. §4.5 — Le chicche audio (decisive per il satellite vocale 🎤)

| Modulo | Cosa fa | Implicazione per noi |
|---|---|---|
| `sound.rs` | voce a runtime: UN figlio `aplay`, un suono nuovo uccide il vecchio, **"il PCM del codec è esclusivo"** | lo speaker NON è condivisibile (no dmix): il TTS del satellite deve passare da sound.rs (via intent/RPC) o coordinarsi, non aprire ALSA in parallelo |
| `pet-detect/` | CNN ~20 KB su finestra log-mel a 40 bande **dal mic onboard**, in un worker dedicato | il microfono è già letto in cattura continua da un processo: da verificare se via ALSA condivisibile (dsnoop) o esclusivo — punto n.1 da chiarire sul codice |
| `theremin.rs` | profondità da tofd 15 Hz → nota + apertura bocca | conferma: audio out pilotabile da eventi esterni |
| `chorale.rs` | più anatre cantano insieme, il minor id dirige, btd porta i beacon | esiste già un pattern di coordinamento audio multi-robot |

Il codec audio sta su I²C condiviso col ToF (da architecture.md). La pipeline
"camera/mic → mediad" è roadmap M5: oggi il mic lo consuma pet-detect dentro
robotd. Quindi il quadro audio è ANCORA IN MOVIMENTO verso mediad — conviene
seguire i commit di mediad/ prima di scegliere Via A o Via B.

## 6. Il modello per il nostro daemon: padd

`padd` è la sagoma esatta del satellite vocale: crate proprio, **client non
privilegiato** (solo gruppi `input` e `robot`), parla al socket di robotd,
"safe with no pad" perché se tace il deadman tiene il robot. Un daemon
`voiced` identico: gruppi minimi, intents/RPC verso robotd (es. far
starnazzare una conferma, girare la testa verso chi parla), stream audio verso
HA. Bonus enorme per lo sviluppo remoto:
`ssh -L /tmp/robotd.sock:/run/robotd.sock` — il satellite può girare sul
MacBook contro il robot vero senza toccare la board.

## 7. Punti aperti loro / rischi nostri

- Rate 50 Hz ereditato dal Pi Zero 2W, non ancora misurato sul Radxa → c'è
  probabilmente headroom CPU, buono per noi.
- Niente limiti anatomici per giunto (solo range attuatore).
- `gilrs` porta `libudev-sys`: preferire crate pure-Rust per ciò che va a board
  — vale anche per il nostro daemon (scegliere bene il crate audio).
- Cross-build da Mac: manca il sysroot aarch64 → buildare il set shipped con
  `-p updater -p robotd -p robotctl`, o usare Linux/Docker (`--docker`).
- robotd non ricarica (no SIGHUP, no swap ort a caldo) — restart per cambiare
  policy, per ora.
