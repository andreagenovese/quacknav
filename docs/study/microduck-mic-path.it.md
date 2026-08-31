# Studio: percorso microfono del Microduck (pet-detect, ALSA, mediad)

Fonte: `pollen-robotics/microduck` @ clone del 2026-08-31. Alimenta l'ADR 0003.

## Come pet-detect cattura l'audio

- **Non è un servizio separato**: pet-detect gira come thread (`pet-worker`)
  *dentro robotd* (`robotd/src/main.rs:1262–1287`); il binario `pet-detect`
  nelle release è uno strumento di debug alimentato da stdin.
- La cattura è un sottoprocesso `arecord`, nessun ALSA in-process:
  `arecord -D plughw:aic3104,0 -f S16_LE -r 16000 -c 1 -t raw`
  (`pet-detect/src/worker.rs:322–330`). Il reader consuma chunk da 4096
  byte (128 ms); frame del sentry di 512 campioni (32 ms).
- **Apertura esclusiva**: `plughw:` = plug sopra `hw` diretto — niente
  dsnoop, nessuna condivisione. Lo dice l'header del crate: "The capture
  device is single-client, so everything that analyses the mic shares this
  one stream" (worker.rs:5–6). È per questo che il SoundSentry ambientale
  vive nello stesso crate.
- In caso di contesa pet-detect degrada silenziosamente: backoff di
  restart 250 ms → 30 s, diventa muto (livello debug) dopo 5 fallimenti
  rapidi (worker.rs:246–255).
- **Stato di default: OFF.** `audio.pet_detect` è false di default
  (`robotd-params/src/lib.rs:448–454`; `deploy/robotd.toml:273` lo
  distribuisce commentato). Su un robot stock **nulla trattiene il PCM di
  cattura** — quacksat può aprirlo in esclusiva già oggi senza alcuna
  modifica.
- Il device di cattura è *derivato* dal parametro di riproduzione:
  `capture_device()` restituisce `audio.device + ",0"`
  (`robotd-params/src/lib.rs:461–467`). Non esiste un'impostazione
  indipendente per il device di cattura.

## Hardware e configurazione ALSA

- Codec TLV320AIC3104 (I²C 0x18, bus i2c3) sull'RPI Robot HAT; nome scheda
  `aic3104` via overlay DT `deploy/audio/aic3104-i2c3.dts`; DAI della CPU
  `i2s3_2ch` (stereo), MCLK 12.288 MHz = 256 × 48 kHz. Il driver è un
  modulo out-of-tree DKMS (il probe della scheda è differito finché non si
  autocarica — `aic3104-init.sh` fa polling fino a 15 s).
- **Nessun asound.conf/dsnoop/dmix in tutto il repo**, e setup-board.sh non
  ne installa alcuno.
- **Routing del mic**: unico mic onboard su **Mic3R → solo Right PGA**;
  canale sinistro morto (`deploy/audio/aic3104-init.sh`). Guadagno di
  cattura del PGA fisso a 60/119, impostato una volta al boot dal oneshot
  `aic3104-init.service` (`Before=robotd.service`).
- Conseguenza: il `-c 1` di pet-detect fa sì che plug medi L+R → mic a
  circa metà ampiezza. **quacksat dovrebbe catturare 2ch @ 48 kHz e
  prendersi da sé il canale destro** per avere audio STT a piena scala.
- Nient'altro legge il mic: mediad, sounds, duck-detect, btd, padd, tof —
  zero codice di cattura.

## mediad e la migrazione audio

- mediad oggi è **solo video**: camera → mpph264enc (VPU) → webrtcsink +
  datachannel di controllo + rilevamento anatra su NPU. La sua tagline
  "camera, mic, WebRTC" è aspirazionale; nessun elemento audio, nessun
  TODO, nessuna sezione di design.
- Le voci aperte della roadmap M5 sono transport, SDK, privacy/LED —
  **l'audio non è tra queste**. robotd-design.md:815 elenca ancora
  pet-detect come worker interno di robotd. Non esiste una migrazione
  mic-verso-mediad progettata su cui basare il design.
- Principio guida se mai avverrà: architecture.md:288 "put perception next
  to the sensor" (pubblicare feature derivate, non campioni).

## Privilegi

- robotd gira con `User=root, Group=robot` (provvisorio secondo il commento
  della unit), quindi il suo arecord bypassa i controlli di gruppo.
  **Nessuna unit nel repo usa il gruppo `audio`**; mediad ha
  `SupplementaryGroups=video render robot`.
- `/dev/snd/*` è root:audio 0660 sulla base Debian/Armbian → la unit di
  quacksat ha bisogno di `SupplementaryGroups=audio` (+ `robot` per il
  socket di robotd). Modello: unit + sysusers di mediad
  (`mediad/systemd/`).
- Ordinamento dopo che la scheda esiste: `After=aic3104-init.service`
  (oppure polling come fa lo script di init).

## Eco / preprocessing

- **Nessun AEC, AGC o soppressione del rumore da nessuna parte.** Speaker e
  mic condividono il codec; i quack e il theremin di robotd finiscono
  dritti nel mic. Un motore di wake word sentirà ogni suono del robot —
  quacksat deve pianificare una soppressione del barge-in coordinata con la
  riproduzione dei suoni, oppure un AEC software. Nulla in microduck
  risolve questo problema.

## Opzioni di coesistenza (lato mic)

- **A (raccomandata): `/etc/asound.conf` additivo con `dsnoop`** su
  `hw:aic3104,0` fissato ai parametri nativi (48 kHz, 2ch), più wrapper
  `plug` per client. Richiede di ripuntare anche pet-detect — la via più
  pulita è una piccola PR upstream che aggiunga un parametro distinto
  `audio.capture_device` (~10 righe: campo in `robotd-params` + voce nel
  registry + uso in robotd main.rs:1268). Tutti i client dsnoop devono
  concordare sui parametri dello slave; le aperture `plughw` nude lo
  bypassano comunque.
- **B: robotd pubblica PCM/feature su un socket** — è lo stile della casa
  (architecture.md:288) ma è vero lavoro upstream, e il downmix mono a
  16 kHz di pet-detect è comunque un pessimo input per l'STT.
- **C: mutua esclusione per policy** — documentare che quacksat richiede
  `audio.pet_detect = false` (il default). Zero lavoro; si perde il
  rilevamento delle carezze mentre il satellite è in funzione.
