# Microduck — Architettura di sistema (prima lettura)

Fonte: `docs/design/architecture.md` (draft 2026-07-22) + README + roadmap.
Stack: Rust, un workspace, nessun framework. Supervisore: systemd.
SoC: Rockchip RK3566 · 1 GB RAM · 15 servo Dynamixel + IMU su un solo bus UART.

## 1. Griglia dei daemon

| Daemon | Possiede | Socket / porta | Parla con | Sopravvive a robotd morto |
|---|---|---|---|---|
| **robotd** | motori, cinematica, odometria, policy ONNX, safety, `robot.health` | `/run/robotd.sock` | bus Dynamixel (UART) | — |
| **configd** | wifi, identità/nome robot, PIN, bonding gamepad, reboot | `/run/configd.sock` | BlueZ + NetworkManager via D-Bus | ✅ (recovery path) |
| **updaterd** | update: verifica firma, swap atomico, health gate, rollback | `/run/updaterd.sock` | GitHub releases, systemctl, robotd | ✅ (recovery path) |
| **btd** | nulla — transport BLE (subset API) | servizio GATT BLE | robotd, configd, updaterd | ✅ (recovery path) |
| **padd** | nulla — transport gamepad (`pad.input`) | `/run/padd/pad.sock` | robotd | ❌ (dipende da robotd) |
| **mediad** | pipeline **camera + microfono**, encode, percezione, gateway WebRTC | TCP `:8080` console, `:8443` signalling | robotd, configd, updaterd | ❌ |
| **tofd** | sensore ToF (matrice depth 8×8) — pubblica, non legge nessuno | `/run/tofd/tof.sock` | bus I²C dell'HAT | ✅ |
| robotctl | nulla — la CLI, deve funzionare a robot rotto | — | tutti i socket | ✅ |

## 2. Principi cardine (invarianti)

1. **Solo robotd tocca l'hardware del robot.** I client mandano *intenti*
   ("vai a questa velocità", "guarda lì", "alzati"); il safety layer dentro
   robotd decide cosa è eseguibile. Nessun altro processo può comandare un motore.
2. **configd, updaterd, btd sopravvivono a robotd morto**: sono il percorso di
   recovery (niente dipendenza systemd da robotd, niente ML runtime, niente media stack).
3. **Il loop a 50 Hz non blocca mai su altri servizi**: letture cross-service
   sempre come cache last-value-wins, mai RPC sincrone.
4. **Un solo writer per ogni pezzo di stato**; tutti gli altri leggono/sottoscrivono.

## 3. Comunicazione

- **Control plane**: JSON-RPC 2.0, un oggetto per riga (NDJSON), un unix socket
  per servizio. Niente broker/bus. Autorizzazione via permessi filesystem
  (mode 0660 + gruppo) e `SO_PEERCRED` (uid/gid per audit e per le chiamate mutanti).
- **Data plane** (video/audio, ~27 MB/s): **non attraversa mai un socket**.
  robotd riceve *feature* derivate ("palla a (x,y)", "suono forte"), non frame.
  Percezione accanto al sensore, dentro mediad.
- Una definizione API, più transport: BLE (subset) · unix socket · WebSocket
  (per agenti/LLM server-side) · datachannel WebRTC.

## 4. Stato: chi possiede cosa, cosa sopravvive agli update

| Stato | Owner | Note |
|---|---|---|
| `/etc/robot/robotd.toml` | per-board | feature switch (policy, walk/roller, **audio**, pet detection, camera…); mai sovrascritto dagli update |
| `/var/lib/robot/config/config.json` | configd | nome robot + PIN (file + flock + rename atomico) |
| Credenziali wifi | NetworkManager | mai memorizzate dai daemon |
| `/opt/robot/daemon/releases/<ver>/` | updaterd | binari + policy, swap atomico del symlink `current` |
| Calibrazioni / stato appreso | servizio owner | fuori dalle release: sopravvive a update E rollback |

Update flow: push branch → CI builda e firma → `robotctl update apply` →
updaterd verifica firma → swap `current` → restart unit → chiede `robot.health`
→ se non sano, rollback automatico. Crash-loop coperto da boot counter.

## 5. Remote access e agenti

- **mediad è il gateway remoto**: possiede la PeerConnection WebRTC con
  video track, **audio track bidirezionale (mic + speaker!)**, datachannel
  "control" (affidabile → API robot) e "teleop" (unreliable → input alta frequenza).
- **Per gli agenti/LLM server-side la via consigliata è WebSocket**, non WebRTC:
  `get_frame` → JPEG on demand + state blob + intenti. "Poche decine di righe,
  nessun media stack".
- Safety remoto: deadman/heartbeat (se i comandi si fermano, robotd ferma il
  robot), intenti e mai scritture motore, arbitraggio di autorità
  (fisico > remoto), 1 sessione media alla volta.
- Privacy: consenso esplicito per sessioni remote, indicatore visibile quando
  streamma, DTLS-SRTP end-to-end.

## 6. Note per il progetto "satellite vocale HA" 🦆🎤

- **L'audio vive in mediad** ("camera/mic, encode"), e il codec audio sta sul
  bus **I²C condiviso con il ToF**. La sintesi voce (quack/chorale/theremin) è
  oggi dentro robotd (la chorale è ~55 KB di robotd — segnalato in roadmap come
  pattern da correggere con il futuro "behaviour layer").
- **Via A (zero modifiche a bordo)**: client server-side che apre la sessione
  WebRTC/WebSocket verso mediad e usa l'audio track bidirezionale come
  mic/speaker remoti → bridge verso Wyoming/HA sul server. Il robot resta stock.
- **Via B (daemon a bordo)**: ottavo daemon "voiced/satellited" accanto agli
  altri, stesso pattern (unix socket, JSON-RPC), che parla Wyoming/ESPHome verso
  HA. Più integrato, ma da mantenere allineato alle release firmate di updaterd.
- La roadmap prevede già "ambient sound events" e "voice tags" come input del
  futuro behaviour layer: possibile convergenza upstream.
- Il file `robotd.toml` ha uno switch `audio`: capire cosa abilita/disabilita.

## 7. Prossime letture (in ordine)

1. `docs/design/robotd-design.md` — il loop 50 Hz nel dettaglio: bus Dynamixel,
   osservazioni, policy, safety ("cos'altro è appeso al tick").
2. Sorgenti `mediad/` — la pipeline audio: GStreamer o webrtc-rs? Il mic è
   esclusivo della pipeline o condivisibile (ALSA dmix)?
3. `docs/design/remote-webrtc.md` + `webrtc-console.md` — sessioni, signalling,
   canale di controllo (per la Via A).
4. `docs/design/updater-design.md` + `restart-order.md` — per capire come
   pacchettizzare un daemon extra senza farsi sovrascrivere.
5. `docs/project/roadmap.md` — milestone e problemi aperti (autonomous.rs).
6. Repo `microduck_rl` — MuJoCo + PPO, per la simulazione sul MacBook.
