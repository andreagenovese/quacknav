# Studio: percorso speaker del Microduck (robotd sound.rs)

Fonte: `pollen-robotics/microduck` @ clone del 2026-08-31. Alimenta l'ADR 0003.

## Come robotd riproduce i suoni

- `robotd/src/sound.rs` (~950 righe) è l'intero layer di uscita audio. Non
  c'è **nessun binding ALSA in-process**: ogni suono è un figlio `aplay`
  lanciato come processo.
  - Wav one-shot: `aplay -q -D <device> <file.wav>` (sound.rs:305–311).
  - Synth in streaming (theremin/chorale/wheee): `aplay -t raw -f S16_LE -c 1
    -r 48000 --buffer-time=40000 --period-time=10000` con stdin in pipe.
- **Un solo figlio, il suono nuovo uccide il vecchio**: `Sound::stop_child()`
  (236–252) uccide e raccoglie l'unico `child: Option<Child>`; `play()` lo
  chiama incondizionatamente. Un `enum Ride { Off, Riding, Landing, Theremin,
  Singing }` a 5 stati traccia chi possiede il PCM.
- **Nessuna coda**: le richieste di suono dei client sono una bitmask
  (`intents.rs:299 request_sound` fa `fetch_or`), svuotata una volta per
  tick a 50 Hz, massimo una per tag. I one-shot che arrivano durante un
  ride/theremin vengono scartati, tranne il peck di commiato bloccante
  (max 1500 ms).
- robotd **non** trattiene il PCM quando è in idle — `aplay` termina dopo
  ogni suono. Tra un suono e l'altro il device è libero.
- Il fallimento è soft in entrambe le direzioni: se `aplay` non riesce ad
  aprire il device, robotd logga una riga a livello debug e va avanti
  (nessun impatto sulla health). Un secondo processo che apre il PCM mentre
  robotd riproduce riceve `-EBUSY`.
- Caso pericoloso: `robot.theremin` / `robot.chorale` trattengono il PCM
  **indefinitamente** finché attivi.

## Superficie IPC per i suoni

- Un unico metodo: `robot.sound` (`duck-ipc-proto/src/lib.rs:371`, parametri
  a 1587–1598). `SoundParams { tag: SoundTag, hold: Option<bool> }` con
  `deny_unknown_fields`.
- `SoundTag` è un **enum chiuso**: `alarm | greet | inquire | peck | chirp |
  coo | wheee`. Il tag seleziona una directory nel banco suoni
  (`/var/lib/robot/sounds/<tag>/`, di proprietà di root) e un wav casuale al
  suo interno. **Non esiste alcun parametro file/path/buffer/stream** → il
  TTS non può passare da robotd.
- Funziona come notifica o come richiesta; viene rifiutato solo se il robot
  non ha voce (`audio.enabled && bank non-empty`). Nessuna autorizzazione
  per metodo sul socket — chiunque nel gruppo `robot` può chiamarlo. Le
  route BLE lo rifiutano; WebRTC lo consente.
- `robot.theremin` / `robot.chorale` attivano/disattivano i synth; un
  client non può fornire campioni.

## Configurazione ALSA

- Device di riproduzione: `plughw:aic3104` (config `deploy/robotd.toml:259`,
  default in `robotd-params/src/lib.rs:435`). Scheda = codec TLV320AIC3104,
  overlay devicetree `deploy/audio/aic3104-i2c3.dts`, MCLK 12.288 MHz =
  256 × 48 kHz → 48 kHz nativi; `plughw:` ricampiona le altre frequenze.
- **Nessun asound.conf, nessun dmix, nessun softvol in tutto il repo.** Il
  PCM è genuinamente single-open; quell'esclusività è la premessa di
  sound.rs.
- La cattura è sulla stessa scheda: `capture_device()` restituisce
  `"<device>,0"` → `plughw:aic3104,0`; pet-detect tiene aperto
  `arecord -f S16_LE -r 16000 -c 1` **in continuo** quando
  `audio.pet_detect = true`. Nota: `capture_device()` aggiunge ciecamente
  `,0` a meno che il nome non contenga una virgola — un nome di device di
  riproduzione non-hw rompe il device di cattura derivato.
- Gruppi: il repo crea solo il gruppo `robot`; robotd gira con `User=root,
  Group=robot`, quindi raggiunge `/dev/snd` come root. Nulla gestisce un
  gruppo `audio` — un quacksat non-root ha bisogno dell'appartenenza a
  qualunque gruppo possieda `/dev/snd` su Armbian (per convenzione `audio`).
- Il mixer è impostato una volta al boot da `aic3104-init.sh` (righe amixer
  cset); nessuna API per il volume in proto/robotd/robotctl. Lo speaker non
  ha uscita utilizzabile sotto ~300 Hz (`SPEAKER_ROLLOFF_HZ = 300.0`,
  sound.rs:70) — le voci TTS suoneranno sottili senza un high-pass/boost
  armonico.

## Opzioni per l'uscita TTS di quacksat

- **A (baseline): aprire `plughw:aic3104` direttamente**, accettando la
  mutua esclusione. Il device è libero quando robotd è in idle; le
  collisioni falliscono soft in entrambe le direzioni. Mitigare con
  retry/backoff su `-EBUSY`; opzionalmente `audio.greet = false` per
  evitare che il quack di boot faccia race con la prima frase. Impostare
  `audio.enabled = false` silenzia robotd del tutto ma uccide anche il mic
  di pet-detect. Fare backoff (non spin) se un theremin trattiene il PCM.
- **B: `/etc/asound.conf` additivo con dmix**, puntando `audio.device` di
  robotd su di esso. Mixing vero, puramente additivo rispetto al deploy, ma
  aggiunge latenza al percorso synth finemente calibrato di robotd
  (`SYNTH_LEAD_S = 0.03`) e interagisce con la derivazione `,0` di
  `capture_device()`.
- **C: RPC `robot.sound` solo per i versi dell'anatra** — zero contesa,
  serializzato da robotd, ma l'enum chiuso non può trasportare il TTS.
  Usarlo insieme ad A per i segnali espressivi (chirp di ack, inquire alla
  wake word). Nota: i suoni di robotd e l'aplay di quacksat possono
  comunque collidere sul device.

Forma della raccomandazione: **A + C**, passare a B solo se le collisioni
nel mondo reale si rivelano fastidiose.
