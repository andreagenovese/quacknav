# Study: Microduck microphone path (pet-detect, ALSA, mediad)

Source: `pollen-robotics/microduck` @ clone of 2026-08-31. Feeds ADR 0003.

## How pet-detect captures audio

- **Not a separate service**: pet-detect runs as a thread (`pet-worker`)
  *inside robotd* (`robotd/src/main.rs:1262–1287`); the `pet-detect` binary
  in releases is a stdin-fed debug tool.
- Capture is an `arecord` subprocess, no in-process ALSA:
  `arecord -D plughw:aic3104,0 -f S16_LE -r 16000 -c 1 -t raw`
  (`pet-detect/src/worker.rs:322–330`). Reader consumes 4096-byte chunks
  (128 ms); sentry frame 512 samples (32 ms).
- **Exclusive open**: `plughw:` = plug over direct `hw` — no dsnoop, no
  sharing. Crate header states it: "The capture device is single-client, so
  everything that analyses the mic shares this one stream" (worker.rs:5–6).
  That's why the ambient SoundSentry lives inside the same crate.
- On contention pet-detect degrades quietly: restart backoff 250 ms → 30 s,
  goes silent (debug-level) after 5 fast failures (worker.rs:246–255).
- **Default state: OFF.** `audio.pet_detect` defaults to false
  (`robotd-params/src/lib.rs:448–454`; `deploy/robotd.toml:273` ships it
  commented out). On a stock robot **nothing holds the capture PCM** —
  quacksat can open it exclusively today with zero changes.
- Capture device is *derived* from the playback param: `capture_device()`
  returns `audio.device + ",0"` (`robotd-params/src/lib.rs:461–467`). There
  is no independent capture-device setting.

## Hardware and ALSA config

- Codec TLV320AIC3104 (I²C 0x18, bus i2c3) on the RPI Robot HAT; card name
  `aic3104` via DT overlay `deploy/audio/aic3104-i2c3.dts`; CPU DAI
  `i2s3_2ch` (stereo), MCLK 12.288 MHz = 256 × 48 kHz. Driver is a DKMS
  out-of-tree module (card probe deferred until it autoloads —
  `aic3104-init.sh` polls up to 15 s).
- **No asound.conf/dsnoop/dmix anywhere in the repo**, and setup-board.sh
  installs none.
- **Mic routing**: single onboard mic on **Mic3R → Right PGA only**; left
  channel dead (`deploy/audio/aic3104-init.sh`). PGA capture gain fixed at
  60/119, set once at boot by oneshot `aic3104-init.service`
  (`Before=robotd.service`).
- Consequence: pet-detect's `-c 1` makes plug average L+R → mic at ~half
  amplitude. **quacksat should capture 2ch @ 48 kHz and take the right
  channel itself** for full-scale STT audio.
- Nothing else reads the mic: mediad, sounds, duck-detect, btd, padd, tof —
  zero capture code.

## mediad and the audio migration

- mediad today is **video only**: camera → mpph264enc (VPU) → webrtcsink +
  control datachannel + NPU duck detection. Its "camera, mic, WebRTC"
  tagline is aspirational; no audio elements, no TODOs, no design section.
- Roadmap M5 open items are transport, SDK, privacy/LED — **audio is not
  among them**. robotd-design.md:815 still lists pet-detect as robotd's own
  worker. There is no designed mic-to-mediad migration to build against.
- Governing principle if it ever happens: architecture.md:288 "put
  perception next to the sensor" (publish derived features, not samples).

## Privileges

- robotd runs `User=root, Group=robot` (provisional per unit comment), so
  its arecord bypasses group checks. **No unit in the repo uses the `audio`
  group**; mediad has `SupplementaryGroups=video render robot`.
- `/dev/snd/*` is root:audio 0660 on the Debian/Armbian base → quacksat's
  unit needs `SupplementaryGroups=audio` (+ `robot` for the robotd socket).
  Model unit + sysusers on mediad's (`mediad/systemd/`).
- Order after the card exists: `After=aic3104-init.service` (or poll like
  the init script does).

## Echo / preprocessing

- **No AEC, AGC, or noise suppression anywhere.** Speaker and mic share the
  codec; robotd's quacks/theremin go straight into the mic. A wake-word
  engine will hear every robot sound — quacksat must plan barge-in
  suppression coordinated with sound playback, or software AEC. Nothing in
  microduck solves this.

## Coexistence options (mic side)

- **A (recommended): additive `/etc/asound.conf` with `dsnoop`** on
  `hw:aic3104,0` fixed at native params (48 kHz, 2ch), plus `plug` wrappers
  per client. Requires repointing pet-detect too — cleanest via a small
  upstream PR adding a distinct `audio.capture_device` param (~10 lines:
  `robotd-params` field + registry entry + use at robotd main.rs:1268).
  All dsnoop clients must agree on slave params; bare `plughw` opens still
  bypass it.
- **B: robotd publishes PCM/features on a socket** — house style
  (architecture.md:288) but real upstream work, and pet-detect's 16 kHz
  mono downmix is poor STT input anyway.
- **C: mutual exclusion by policy** — document that quacksat requires
  `audio.pet_detect = false` (the default). Zero work; loses petting
  detection while the satellite runs.
