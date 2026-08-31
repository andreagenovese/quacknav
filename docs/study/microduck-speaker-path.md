# Study: Microduck speaker path (robotd sound.rs)

Source: `pollen-robotics/microduck` @ clone of 2026-08-31. Feeds ADR 0003.

## How robotd plays sound

- `robotd/src/sound.rs` (~950 lines) is the whole audio output layer. There
  is **no in-process ALSA binding**: every sound is a spawned `aplay` child.
  - One-shot wavs: `aplay -q -D <device> <file.wav>` (sound.rs:305–311).
  - Streamed synth (theremin/chorale/wheee): `aplay -t raw -f S16_LE -c 1
    -r 48000 --buffer-time=40000 --period-time=10000` with stdin piped.
- **One child, new sound kills old**: `Sound::stop_child()` (236–252) kills
  and reaps the single `child: Option<Child>`; `play()` calls it
  unconditionally. A 5-state `enum Ride { Off, Riding, Landing, Theremin,
  Singing }` tracks who owns the PCM.
- **No queue**: client sound requests are a bitmask
  (`intents.rs:299 request_sound` does `fetch_or`), drained once per 50 Hz
  tick, max one per tag. One-shots arriving during a ride/theremin are
  dropped, except the blocking goodbye peck (max 1500 ms).
- robotd does **not** hold the PCM when idle — `aplay` exits after each
  sound. Between sounds the device is free.
- Failure is soft in both directions: if `aplay` can't open the device,
  robotd logs a debug line and moves on (no health impact). A second
  process opening the PCM while robotd plays gets `-EBUSY`.
- Danger case: `robot.theremin` / `robot.chorale` hold the PCM
  **indefinitely** while active.

## IPC surface for sound

- Single method: `robot.sound` (`duck-ipc-proto/src/lib.rs:371`, params at
  1587–1598). `SoundParams { tag: SoundTag, hold: Option<bool> }` with
  `deny_unknown_fields`.
- `SoundTag` is a **closed enum**: `alarm | greet | inquire | peck | chirp |
  coo | wheee`. The tag selects a directory in the sound bank
  (`/var/lib/robot/sounds/<tag>/`, root-owned) and a random wav inside it.
  **No file/path/buffer/stream parameter exists** → TTS cannot go through
  robotd.
- Works as notification or request; refused only if the robot has no voice
  (`audio.enabled && bank non-empty`). No per-method authorization on the
  socket — anyone in the `robot` group can call it. BLE routes refuse it;
  WebRTC allows it.
- `robot.theremin` / `robot.chorale` toggle synths on/off; a client cannot
  feed samples.

## ALSA configuration

- Playback device: `plughw:aic3104` (config `deploy/robotd.toml:259`,
  default in `robotd-params/src/lib.rs:435`). Card = TLV320AIC3104 codec,
  devicetree overlay `deploy/audio/aic3104-i2c3.dts`, MCLK 12.288 MHz =
  256 × 48 kHz → 48 kHz native; `plughw:` resamples other rates.
- **No asound.conf, no dmix, no softvol anywhere in the repo.** The PCM is
  genuinely single-open; that exclusivity is the premise of sound.rs.
- Capture is the same card: `capture_device()` returns `"<device>,0"` →
  `plughw:aic3104,0`; pet-detect holds `arecord -f S16_LE -r 16000 -c 1`
  open **continuously** when `audio.pet_detect = true`. Note:
  `capture_device()` blindly appends `,0` unless the name has a comma — a
  non-hw playback device name breaks the derived capture device.
- Groups: repo creates `robot` group only; robotd runs `User=root,
  Group=robot` so it reaches `/dev/snd` as root. Nothing manages an `audio`
  group — a non-root quacksat needs membership in whatever owns `/dev/snd`
  on Armbian (conventionally `audio`).
- Mixer set once at boot by `aic3104-init.sh` (amixer cset lines); no
  volume API anywhere in proto/robotd/robotctl. Speaker has no usable
  output below ~300 Hz (`SPEAKER_ROLLOFF_HZ = 300.0`, sound.rs:70) — TTS
  voices will sound thin without a high-pass/harmonic boost.

## Options for quacksat TTS output

- **A (baseline): open `plughw:aic3104` directly**, accept mutual
  exclusion. Device is free when robotd is idle; collisions fail soft both
  ways. Mitigate with retry/backoff on `-EBUSY`; optionally
  `audio.greet = false` to avoid the boot quack racing the first utterance.
  Setting `audio.enabled = false` silences robotd entirely but also kills
  the pet-detect mic. Back off (don't spin) if a theremin holds the PCM.
- **B: additive `/etc/asound.conf` with dmix**, point robotd's
  `audio.device` at it. True mixing, purely additive to the deploy, but
  adds latency to robotd's tightly tuned synth path (`SYNTH_LEAD_S = 0.03`)
  and interacts with the `capture_device()` `,0` derivation.
- **C: `robot.sound` RPC for duck noises only** — zero contention,
  serialized by robotd, but the closed enum can't carry TTS. Use alongside
  A for expressive cues (chirp ack, inquire on wake word). Note robotd's
  sounds and quacksat's aplay can still collide on the device.

Recommendation shape: **A + C**, move to B only if real-world collisions
prove annoying.
