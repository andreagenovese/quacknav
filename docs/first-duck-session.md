# The first session on a physical duck — checklist

quack-nav v0.2.0-rc2, quack-control v0.1.0-rc1 and quacksat v0.1.0-rc1 on a
Pollen Microduck, for the first time. Italian copy:
[first-duck-session.it.md](first-duck-session.it.md). Written 2026-10-03,
before the duck arrived: every command, key, path and log line below was
checked against the three repositories at those tags and Pollen's
`docs/robot/` at daemon-v0.15.0, but none of it has run on a board yet.
What is a guess, and what the twin measured, is said where it matters.

The goal of the day is not the house: it is **one room, safely**, with the
numbers written down beside the twin's ([results.md](results.md), the
[rc2 release notes](release-notes-v0.2.0-rc2.md#headline-results)) and the
recordings brought home for the bench.

Placeholders: `<duck>` is the duck's address (`duckctl --name <robot> ip`,
or `duckctl scan`), `<robot>` its Bluetooth name. Nothing here needs a real
address, key or name written into a file of this repository.

## 0. Safety first

The README's warning holds: a walking robot near a drop can fall and break.

- [ ] **A spotter** beside the duck for every move, hands free, ready to
      catch it. One person types, the other watches the duck — never the
      same person.
- [ ] **A floor with no drops.** No stairs, no landing, no doorway to a
      lower floor within reach of a 5-minute walk; a sofa or a table edge
      it could walk off is a drop too. Close the doors to anything else.
- [ ] **A soft barrier** across every opening of the room (cushions, a
      folded rug, cardboard on edge), so a runaway duck meets something soft.
- [ ] **Know how to stop everything**, fastest first, and try each once
      before the first walk:
  1. quack-control's **STOP** (the page, see §2) — `robot.go_to {"stop": true}`:
     stops any job of quack-nav's and whatever it does on its own, and
     holds (nothing self-starts again until a job is asked for). On the
     twin: 0.15 s, 3 cm of coasting.
  2. The same from a shell on the duck:
     `printf '{"jsonrpc":"2.0","id":1,"method":"nav.call","params":{"name":"robot.go_to","args":{"stop":true}}}\n' | nc -U -q1 /run/quack-nav/nav.sock`
     (or `nav robot.go_to '{"stop":true}'` with the helper of §2).
  3. The **gamepad**: **Select**, short press — torque off on release,
     Pollen's emergency stop: **the duck drops, hold it**; Start stands it
     again. Select held 2 s: sit, torque off, power off.
  4. On the duck: `sudo systemctl stop quack-navd` (no more commands;
     robotd's deadman zeroes the velocity), `sudo robotctl robot enable --off`
     (the policy stops, the robot holds its pose), `sudo robotctl robot relax --yes`
     (torque off: **it collapses**).
  5. No network, over Bluetooth from the laptop:
     `duckctl --name <robot> call robot.enable '{"on":false}'` (btd refuses
     `robot.stop` and `robot.relax` over Bluetooth; `robot.enable` passes).
  6. Power: the duck's switch, or the battery.

  mediad's console (`:8080`) has a "stop" that only zeroes its own intents:
  Pollen's docs say it is **not** an emergency stop.
- [ ] **Pollen's teleop bypasses quack-nav's cliff guard.** The gamepad and
      the console drive robotd directly, and robotd has no drop protection
      ([study/upstream-asks.md](study/upstream-asks.md) §8). Driven that
      way the duck walks off a stair as onto the floor. Drive by hand only
      on the flat floor of the room.
- [ ] **Battery**: start charged. `robotctl monitor` shows the pack in
      volts and percent (yellow at 30 %, red at 15 %); 0 % is where robotd
      sits the duck down and cuts power. Stop the tests at 30 %.
- [ ] **If it falls**: hold it, Select (torque off) if it is still
      thrashing; note the time and what was running (`note fell …`, §2);
      **save the journal before switching it off** (the board's `/var/log`
      is in RAM, a power cut loses it — Pollen's `deploy/README.md`);
      `robotctl health` names a servo in error, `sudo robotctl robot reboot-motors`
      brings it back (torque off first: hold the duck). Stand it again only
      once the cause is understood; quack-navd logs
      `maploc: robot fell — pose suspect until a window confirms it`.

## 1. Before the day

### Versions

| component | version | package |
|---|---|---|
| quack-nav | v0.2.0-rc2 | `quack-nav-0.2.0-rc2-aarch64-linux.tar.gz` (+ `.sha256`) |
| quack-control | v0.1.0-rc1 | `quack-control-0.1.0-rc1-aarch64-linux.tar.gz` — needs quack-nav ≥ v0.2.0-rc2 |
| quacksat | v0.1.0-rc1 | `quacksat-0.1.0-rc1-aarch64-linux.tar.gz` — built against robotd daemon-v0.14.4, newer untested |
| Pollen microduck | daemon-v0.15.0 (API 37) | robotd, tofd, mediad — quack-nav runs the same on daemon-v0.14.4 |

quack-control's compatibility table: 0.1.0-rc1 ↔ quack-nav ≥ v0.2.0-rc2,
mediad ≥ daemon-v0.14.4 for the camera, quacksat with no control socket yet.

### What Pollen's image must provide

- [ ] Radxa Zero 3 (RK3566, aarch64), Armbian with the Debian 13 userland:
      `cat /etc/os-release; uname -m; ldd --version | head -1` (glibc ≥ 2.31).
- [ ] The daemons, one release: `robotctl version` (every daemon running
      against installed, and the API) and `robotctl health` (exits non-zero
      when unhealthy).
- [ ] The account `microduck` with `sudo`, in the group `robot`: `id`
      (older images had `radxa`).
- [ ] The sockets, mode 0660 group `robot`:
      `ls -l /run/robotd.sock /run/tofd/tof.sock /run/mediad/media.sock`.
- [ ] For quacksat: the `audio` group, `arecord`/`aplay` (alsa-utils), and
      `grep -n pet_detect /etc/robot/robotd.toml` — `audio.pet_detect` must
      be false (the factory default), or the microphone is taken.
- [ ] `command -v nc` — the `nc -U` checks need netcat-openbsd
      (`sudo apt install netcat-openbsd` if missing); `python3` is handy
      for `scripts/twin/probe.py` (§3), not required.
- [ ] The clock: `timedatectl` (synchronized, so the journal, your notes
      and a phone video line up).
- [ ] A gamepad paired (`sudo robotctl pad pair`, Pollen's
      `pair-a-gamepad.md`): it is the stop of §0 and the hand drive of §4c.

### Network and laptop

- [ ] The duck on the home Wi-Fi (`duckctl --name <robot> wifi connect …`,
      Pollen's `duckctl.md`), its address from `duckctl --name <robot> ip`.
- [ ] A laptop with `ssh`, `scp`, `tar`, `shasum`, `gh` (or `curl`), and an
      ssh key on `microduck@<duck>`. On the laptop, for the whole day:
      `DUCK=microduck@<duck>`.
- [ ] A phone on the same network for the page (port 8090) and to film.
- [ ] A tape measure, masking tape for the marks, a marker, a stopwatch.

### Download and verify (on the laptop, the day before)

```sh
mkdir -p ~/duck-day1/pkgs && cd ~/duck-day1/pkgs
for p in quacknav:quack-nav:0.2.0-rc2 quack-control:quack-control:0.1.0-rc1 quacksat:quacksat:0.1.0-rc1; do
  IFS=: read -r repo name v <<< "$p"
  gh release download "v$v" --repo "andreagenovese/$repo" --pattern "$name-$v-aarch64-linux.tar.gz*"
  shasum -a 256 -c "$name-$v-aarch64-linux.tar.gz.sha256"     # must print OK
  tar xzf "$name-$v-aarch64-linux.tar.gz"
done
# the way back (§6): quack-nav rc1 had no package, only the bare binary
mkdir -p ../rollback && cd ../rollback
gh release download v0.2.0-rc1 --repo andreagenovese/quacknav --pattern 'quack-navd-aarch64-linux*'
shasum -a 256 -c quack-navd-aarch64-linux.sha256
```

Without `gh`, the `curl -LO …/releases/download/v$V/…` lines of each
package's `README-install.md`.

- [ ] **quacksat's wake models** are not in its package: the installer
      downloads them **on the duck** (openWakeWord's two feature models,
      CC BY-NC-SA 4.0, non-commercial; "hey Daffy" from quacksat's
      repository), each checked against its sha256. The duck needs the
      internet during that install.
- [ ] **Voice, if used on day one** — decide the backend and have the
      endpoints at hand (placeholders, never in this repository):
      `direct` needs `[direct.llm]`, `[direct.stt]`, `[direct.tts]`
      (`base_url`, `api_key`, `model`/`language`/`voice`) and
      `tool_calling = true` for the robot tools; `agent` needs a bridge
      (`[agent] url = "ws://<bridge-host>:8765"`). With Arkimede, the
      bridge's README profile is `http://<server>:3000/api/openai/v1`, an
      `ak_` key and `tool_calling = false`, the robot tools reaching it
      through an MCP server instead — so going "vai in cucina" through
      Arkimede needs that MCP wiring.
- [ ] A checkout of quack-nav at `v0.2.0-rc2` on the laptop, built once
      (`cargo build --release -p maploc --features kinematics --examples`),
      for the bench of §5.

## 2. Install, in order

Each package: `--dry-run` first (prints every command, connects to
nothing), then for real. From `~/duck-day1/pkgs`:

- [ ] **quack-nav**: `cd quack-nav-0.2.0-rc2 && ./install-on-duck.sh --dry-run $DUCK && ./install-on-duck.sh $DUCK`
- [ ] **quack-control**: `cd ../quack-control-0.1.0-rc1 && ./install-on-duck.sh --dry-run $DUCK && ./install-on-duck.sh $DUCK`
- [ ] **quacksat**: `cd ../quacksat-0.1.0-rc1 && ./install-on-duck.sh --dry-run $DUCK && ./install-on-duck.sh $DUCK`
      (the example config starts it in bring-up mode, `backend = "none"`:
      it wakes on "hey Daffy" and chirps, and talks to nobody).

### The config edits for day one

`sudo nano /etc/robot/quack-nav.toml`, then `sudo systemctl restart quack-navd`:

```toml
[map]
explore_max_s = 300      # "map everything" without a budget: 5 minutes, not 30

[homecoming]
enabled = false          # stages a–c: nothing walks on its own at boot (§4d turns it on)

[maploc]
record_dir = "/var/lib/quack-nav/recordings"   # a .mdlg of everything the mapper read, for §5
```

The rest of the example stays (`[maploc] enabled = true`, `mode =
"stop_and_scan"`). Paths must stay under `/var/lib/quack-nav/` — the unit
lets the daemon write nowhere else. A recording is about 6 KB/s (some
22 MB an hour), a new `<unix time>.mdlg` at every start of the daemon.

quack-control — the token (the page asks for it once):

```sh
sudo sh -c 'echo "QC_TOKEN=$(tr -dc a-z0-9 </dev/urandom | head -c 32)" > /etc/robot/quack-control.env'
sudo cat /etc/robot/quack-control.env
sudo systemctl restart quack-control
```

quacksat (`sudo nano /etc/robot/quacksat.toml`; it will hold API keys —
keep it `root:quacksat` 0640, as the installer leaves it): stay on
`backend = "none"` until §4g; then the backend chosen in §1, and
`[announce] language = "it"`. If "hey Daffy" misses an Italian voice,
`[wake] threshold = 0.4` (its release notes measured an Italian speaker at
0.20–0.37 against 0.5).

### Checks

On the duck (`ssh $DUCK`). Paste once per shell — a call helper, a note
taker and a folder for everything measured:

```sh
mkdir -p ~/qn
nav() {   # nav <tool> ['<json args>']   —  NAV_WAIT=130 for {"complete":true}
  local args=${2:-'{}'}
  printf '{"jsonrpc":"2.0","id":1,"method":"nav.call","params":{"name":"%s","args":%s}}\n' "$1" "$args" \
    | nc -U -q "${NAV_WAIT:-2}" /run/quack-nav/nav.sock; echo
}
note() { echo "$(date +%T) $*" | tee -a ~/qn/notes.txt; }
```

- [ ] `systemctl status quack-navd quack-control quacksat --no-pager` — all active.
- [ ] `stat -c '%n %U:%G %a' /run/quack-nav/nav.sock /run/quack-nav/map.sock` →
      `quacknav:robot 660` both.
- [ ] `printf '{"jsonrpc":"2.0","id":1,"method":"nav.catalog","params":{}}\n' | nc -U -q1 /run/quack-nav/nav.sock | grep -o '"name":"robot\.[a-z_]*"'`
      → twelve tools.
- [ ] `nav robot.map_status` → `"mapping":true`, `"mode":"stop_and_scan"`,
      `cliff.guard` `"watching"` once the duck stands.
- [ ] `journalctl -u quack-navd -b --no-pager | grep -E 'serving the map|subscribed to robot.state|connected to tofd|recording session'`
      — the four lines (`maploc: serving the map`, `maploc: subscribed to
      robot.state`, `maploc: connected to tofd's depth stream`, `maploc:
      recording session`).
- [ ] The page: `http://<duck>:8090/?token=<QC_TOKEN>` from the phone —
      the map, the status line, Services (quack-nav answers, quacksat "not
      available", which is expected). `journalctl -u quack-control -b`:
      `serving the page` with `token=true`.
- [ ] quacksat: `journalctl -u quacksat -b` — `quacksat starting`, `wake
      word loaded`, `listening`, `the navigation daemon answered` with
      `tools=12`. Say "hey Daffy": a `wake` line, and the duck chirps.
- [ ] On the laptop, a live copy of the journals that survives power cuts
      (restart it after each boot):
      `ssh $DUCK 'journalctl -f -o short-iso -u quack-navd -u quack-control -u quacksat -u robotd -u tofd -u mediad' >> ~/duck-day1/journal-live.log &`
      (if it prints nothing, the account cannot read the system journal:
      `sudo` on the duck instead).

## 3. Measurements, with exact commands

Paste on the duck; each writes into `~/qn`:

```sh
sample() {   # sample <label> <seconds>: CPU and memory per process every 5 s, and the board's temperatures
  local n=$(( $2 / 5 ))
  ( for i in $(seq "$n"); do echo "$(date +%T) $(cat /sys/class/thermal/thermal_zone*/temp | tr '\n' ' ')"; sleep 5; done ) > ~/qn/temp-$1.txt &
  top -b -d 5 -n "$n" -o %CPU -w 200 | grep -E '^top|^%Cpu|^MiB|quack|robotd|tofd|mediad' > ~/qn/top-$1.txt
  wait
  systemd-cgtop -b -n 1 --raw > ~/qn/cgtop-$1.txt
  systemctl show quack-navd quack-control quacksat -p Id -p MemoryCurrent -p MemoryPeak -p CPUUsageNSec > ~/qn/units-$1.txt
  free -m >> ~/qn/units-$1.txt
}
cat /sys/class/thermal/thermal_zone*/type > ~/qn/thermal-zones.txt   # which zone is which (millidegrees)
```

- [ ] **Idle**, the duck standing, nothing asked: `sample idle 300`.
- [ ] **Resting** (after a minute with no job quack-navd rests): `sample rest 300` during §4f.
- [ ] **Mapping** (stop_and_scan): `sample mapping 300` in a second shell during §4c.
- [ ] **A go_to**: `sample goto 120` in a second shell during §4e.
- [ ] A one-off snapshot: `ps -o pid,comm,%cpu,rss,etimes -C quack-navd,quack-control,quacksat,robotd,tofd,mediad > ~/qn/ps.txt`
      (`%cpu` is the average since start; `top` above is the rate).
      `pidstat` (sysstat) works too if installed.
- [ ] **The rates**, standing:
  - depth and odometry as quack-navd receives them —
    `journalctl -u quack-navd --since "2 min ago" | grep 'maploc: status' | tail -3 > ~/qn/status-lines.txt`:
    the counters `odom` and `frames` are cumulative, one line every 5 s,
    so their difference divided by 5 is the rate (tofd runs at 15 Hz in
    Pollen's monitor; the state stream follows robotd's 50 Hz loop);
  - or, with `python3` and `scripts/twin/probe.py` copied from the
    checkout: `python3 probe.py /run/robotd.sock /run/tofd/tof.sock 10 > ~/qn/probe.txt`
    (robot.state and tof.stream in Hz, and the two clocks);
  - the head pairing — `journalctl -u quack-navd -b | grep -E 'head pairing|paired with the head' > ~/qn/pairing.txt`:
    `paired` against `fell_back` every 600 frames (fell_back should stay
    near zero) and `mean_lag_ms`;
  - the pose stream (`map.pose`, every 50 ms between the 1 Hz `map.frame`) —
    ```sh
    (printf '{"jsonrpc":"2.0","id":1,"method":"robot.map","params":{}}\n'; sleep 12) \
      | timeout 10 nc -U /run/quack-nav/map.sock > ~/qn/map-10s.ndjson
    grep -c '"map.pose"' ~/qn/map-10s.ndjson; grep -c '"map.frame"' ~/qn/map-10s.ndjson   # ~190 and ~10 expected
    ```
- [ ] **maploc's windows**: `journalctl -u quack-navd -b | grep -E 'still window integrated|window too thin|quarantined|relocalized|loop closed|resting|rest watch' > ~/qn/windows.txt`
      — the time from a stop to its `maploc: still window integrated`
      (a stop of 6 s is what maps), and how many are discarded.

Reference on the twin (a Mac, not the RK3566): quack-navd 2.72 % of a core
awake, 2.0–2.35 % resting; ~20 MB resident after four minutes of mapping;
the unit caps it at 256 MB high, 320 MB max (quacksat 192/256 MB). Nothing
on the board is measured yet — this section is the first number.

## 4. Staged tests

Marks first. Pick a **reference corner O** of the room; **x** along one
wall, **y** along the other, so that y is to the left looking along x.
Tape crosses with a heading arrow: **S** (start, facing +x, at least
0.6 m from every wall), **A**, **B**, **C**, **D** spread across the room,
and **K** (the kidnap's landing spot). Measure each in cm from O; measure
the room's walls too, as segments — that is the `truth.toml` of §5
(`walls`, `start` = S, `kidnap` = K, centimetres and degrees, the format of
[maploc/examples/room_lab.toml](../maploc/examples/room_lab.toml)). Measure
**h**, the head sensor's height above the floor, standing.

Since the map starts at S facing +x (§4c), a mark's map coordinates are
`(x_mark − x_S, y_mark − y_S) / 100` in metres.

Before each stage: `note "stage X start"`; after: `note "stage X end: …"`.

### a. Standing, the sensors

- [ ] Stand the duck (Start, twice — torque and home pose, then the
      policy; or `sudo robotctl robot init` then `sudo robotctl robot enable`).
- [ ] `robotctl monitor`, **t**: the ToF block — `15 Hz · 8×8`, how many of
      64 zones ranged; **c**: a camera frame.
- [ ] `nav robot.map_status`: `cliff.guard` `"watching"`, `cliff.frames`
      growing between two calls, `cliff.edge_between_m` null on the flat
      floor. A box 40 cm in front: `cliff.nearest_obstacle.range_m` ≈ 0.4.
- [ ] The head sweeps at a stand (`[maploc] search_sweep`); `windows`
      grows by one per stop.
- [ ] **Phantom drops**: stand it on every kind of floor in the room
      (dark rug, glossy tile, light wood), a head sweep each:
      `cliff.edge_between_m` must stay null. A sensor that returns nothing
      on a dark floor reads it as a drop (`cliff.kind` `"no floor return"`)
      — cliff.rs's caveat, never measured.
- [ ] The camera on quack-control's page: the camera button, frame rate
      and age.
- [ ] `sample idle 300`, and the rates of §3.

**Pass**: guard watching, tof at its rate, no drop on any flat floor, the
obstacle within ~10 cm of the tape.

### b. Manual moves, and the cliff guard at a safe edge

- [ ] Small steps, the spotter's hands near:
      `nav robot.move '{"vx":0.3,"duration_s":2}'` (≤ 3 s; the walking policy
      does not step below ~0.25 m/s commanded; on the twin 0.3 m/s walked
      53 cm in 5 s). Expect `"done":true`, `"cliff_guard":"on"`. Tape the
      distance. A turn: `'{"vx":0.3,"vyaw":0.7,"duration_s":2}'` — the duck
      does not turn in place. Never back toward an edge: a backward move is
      `not covered: backing up`.
- [ ] The same from quack-control: **Advanced → All tools → robot.move**
      (it asks for confirmation).
- [ ] **The veer**: three straight `duration_s` 3 moves; the sideways drift
      by tape. A steady veer is `[gait] yaw_trim` in
      `/etc/robot/quack-nav.toml` (rad/s, + = left; the twin needed about
      0.2) — write the number down, change it after the session.
- [ ] A mapping step: `nav robot.map_step '{"vx":0.3,"walk_s":2}'` →
      `new_windows`, `clearance`, `checks` `"map and sensor"`.
- [ ] **Choosing a safe test edge** — never a real stair. By the code
      (`quack-nav/src/cliff.rs`) a beam reads as a drop when the floor's
      return is **missing** (within 1.2 m) or **at least 1.5× farther**
      than the floor should be, two beams per frame, judged while standing.
      So a step down reads only if it is at least **h/2** deep: a
      **3–5 cm platform will most likely not read at all** (a step was
      never tried even on the twin — todo-map, 2026-10-02). In order:
  1. a flat patch the sensor may not see — a mirror tile, black velvet,
     glossy black board on the floor: if it reads, the guard can be tested
     with no fall at all (and it is a finding: such a floor at home is a
     phantom drop);
  2. only if (1) does not read: a stable platform at least h/2 high, the
     duck on it, cushions below, the spotter's hand at the edge.

  Before using an edge: the duck 0.6–0.8 m from it, facing it, one head
  sweep, `nav robot.map_status` → `cliff.edge_between_m` not null,
  `cliff.bearing_deg` near 0, `cliff.kind` noted. If it does not read,
  it is not a test edge.
- [ ] **The guard**: from ~1.0 m, facing the edge,
      `nav robot.move '{"vx":0.3,"duration_s":3}'` repeatedly. Expect, at
      the call that would reach it: `"done":false`, `"stopped":"a drop ahead
      (depth sensor): …"`. Tape the beak-to-edge distance. Twin: from 1.15 m
      it stopped with the trunk 0.56 m short of the rim; the guard acts on
      an edge within 0.40 m in a 0.17 m half-lane. Then nine moves over open
      floor: no false stop (twin: none).

**Pass**: moves as asked, the guard stops before the edge every time, no
false stop.

### c. A first map of one room

- [ ] The duck on **S facing +x**. `sudo systemctl restart quack-navd`
      (a new recording starts here; wait for `maploc: recording session`),
      then `nav robot.map_wipe` (a fresh map from here — not in the
      catalog, but a tool quack-navd answers). Check: `nav robot.map_status`
      → `pose` ≈ `{"x":0,"y":0,"yaw":0}`. If not, note the offset.
- [ ] Either **(A) on its own**: `nav robot.map_explore '{"max_s":300,"save_as":"stanza"}'`
      (answers at once; watch the page, STOP ready); or **(B) guided**:
      `nav robot.map_explore '{"watch":true,"max_s":600}'` and the spotter
      drives with the gamepad — short walks, stops of 6 s or more (only a
      stop maps), back through places already mapped — then
      `nav robot.map_explore '{"stop":true}'`. In (B) `robot.move` is
      refused while it runs and the gamepad bypasses the cliff guard: the
      room must have no drop.
- [ ] Meanwhile, a second shell: `sample mapping 300`.
- [ ] Progress: `nav robot.map_status` → `explore.state`, `windows`,
      `submaps`, `loops`, `house.percent_mapped`.
- [ ] Back to S (`nav robot.go_to '{"x":0,"y":0}'` or by hand), stand
      10 s. **The room against the tape**: `clearance.ahead.free_m +
      clearance.behind.free_m` and `left + right` at S (rays look up to
      3 m) against the room's measured width and length through S.
- [ ] Optional, for `evaluate`'s kidnap section: sit it (DPad-Down, or
      `robotctl robot do sit_toggle`), carry it to **K** facing its
      arrow, stand it.
- [ ] Save and close: `nav robot.map_save '{"name":"stanza"}'`, then
      `NAV_WAIT=130 nav robot.map_explore '{"complete":true,"save_as":"stanza"}'`
      — saved, declared done, frozen. `nav robot.map_list` shows it.
- [ ] A screenshot of the page's map; `note` the clearance numbers.

**Pass**: no fall, the room's walls closed on the page, width and length
within ~10 cm of the tape (twin: walls 3–5 cm off on average, 98 % on the
truth).

### d. Homecoming: power-cycles at the marks

- [ ] Config for the rest of the day (`sudo nano /etc/robot/quack-nav.toml`):
      `[maploc] mode = "localize"` (the map stays as saved, the pose is
      corrected against it), and

      ```toml
      [homecoming]
      enabled = true
      resume_explore = false
      start_delay_s = 60      # time to stand it up after power-on
      ```

      `sudo systemctl restart quack-navd`. With a frozen map a boot that
      cannot confirm searches (60 s, then three more budgets) and stands
      down — it never starts a fresh map.
- [ ] Do **not** press STOP just before a power-cycle: a STOP with nothing
      running still holds the homecoming (a known limit of rc2).
- [ ] For each of A, B, C, D: sit it and power off (Select held 2 s), carry
      it to the mark facing its arrow, power on, stand it — `note "d A
      standing"` (the first note after a boot: the shell is new, paste the
      helpers again, restart the laptop's live journal). Then watch:
      `journalctl -u quack-navd -f | grep homecoming` —
      `homecoming: loaded the newest map; standing still to see if the duck knows where it is`,
      then `homecoming: home — the pose is confirmed on the saved map`
      (or `no confirmation on the frozen map; standing down`).
      The boot search walks and looks: the spotter stays.
- [ ] At confirmation: `nav robot.map_status` → `pose` against the mark's
      map coordinates; time from standing to confirmation.

**Twin**: 28 of 28 wakes right, none wrong, median 87 s (75–141 s), 0.01–0.15 m
from the truth. **Pass**: never a wrong confirmed pose (> 0.3 m off),
3 of 4 confirmed, the times written down.

### e. go_to between the marks

- [ ] Teach the marks as places, by their tape coordinates (independent of
      the duck's own pose): `nav robot.remember_place '{"name":"cucina","x":<x_A>,"y":<y_A>}'`,
      and B, C, D under their names (`"x"`/`"y"` in map metres, on mapped
      floor). Or stand on a mark and `nav robot.remember_place '{"name":"B"}'`.
- [ ] `nav robot.go_to '{"place":"B"}'`; poll `nav robot.map_status` →
      `explore.state` `running` … `done` (or `failed` and `explore.reason`).
      A second shell: `sample goto 120`.
- [ ] At arrival: tape from the duck's centre (between the feet) to the
      cross. Time from call to `done`.
- [ ] Four journeys at least, one across the room, one round an obstacle.

**Twin**: 9/9 arrived 0.09–0.30 m from the goal; journeys of a house in
84–111 s median. **Pass**: arrives, ≤ 0.30 m, no fall, no stop for a
phantom drop.

### f. Rest, and a kidnap

- [ ] Leave it standing with no job for 30 minutes. In the log:
      `maploc: resting — a long idle stand …` after a minute, a
      `maploc: rest watch` every two minutes; `nav robot.map_status` →
      `resting: true`, `rest_watch.verdict`. `sample rest 300` meanwhile.
      Tape the feet against the floor before and after: does the real
      stand creep? (the twin's turns ~0.1°/s and slides by itself).
- [ ] Then `nav robot.go_to '{"place":"cucina"}'`: it wakes at once; tape
      the arrival.
- [ ] **Kidnap**: after a minute of rest, lift it, carry it to **K**, turn
      it ~90°, put it down standing. Expect
      `maploc: the duck may have been moved while it rested — the pose is untrusted; the next job finds it first`,
      `untrusted: true`. Then `nav robot.go_to '{"place":"B"}'` → the
      answer carries `"relocalizing":true`; `explore.state` `relocalizing`,
      then `running`, then `done`. Time the relocalization, tape the
      arrival.

**Twin**: 30-minute rest, pose off by 10.0 cm mean, 17.2 worst; carried
3.3 m and turned 86°: found in 70 s, arrived 0.02 m off. **Pass**: untrusted
seen, found again with no wrong pose, arrives.

### g. Voice through quacksat (if configured)

- [ ] quacksat on its backend (§1), `sudo systemctl restart quacksat`;
      `journalctl -u quacksat -f`.
- [ ] "Hey Daffy" from 1 m and 3 m, standing and while it walks (the
      motors' noise): count the `wake` lines against the tries; false wakes
      in 10 minutes of talk nearby.
- [ ] "Hey Daffy, dove sei?" → `robot.where_am_i`.
- [ ] "Hey Daffy, vai in cucina" → it answers at once and walks; on
      arrival it says "Sono arrivata in cucina" (`[announce]`; with
      `wyoming` it says nothing on its own). Time from the end of the
      sentence to the first step.

**Twin**: "vai in cucina" walked to the kitchen and the arrival was said.
**Pass**: wakes ≥ 8 of 10 at 1 m, arrives, says so.

### h. STOP mid-journey

- [ ] A go_to across the room; halfway, **STOP** on the page. Film it:
      time from the press to the stop, tape the coasting. `nav robot.map_status`
      → `explore.state` `stopped`, `explore.stopped_by_user: true`.
- [ ] The same with `nav robot.go_to '{"stop":true}'`.
- [ ] The same by voice ("Hey Daffy, fermati"), if §4g runs: a stop the
      agent sent is not said again on its own (its reply says it).
- [ ] After a STOP nothing starts by itself; the next job works.

**Twin**: 0.15 s, 3 cm. **Pass**: stops within a step, every time.

### i. (the `rl-nav` build only) Traces for the pilot's calibration

Only with a quack-navd built from the experimental branch `rl-nav`
([rl-pilot.md](rl-pilot.md)); rc2 does not read these knobs. No pilot
flies here: the stick drives, and its legs are recorded.

```sh
echo 'QK_RL_TRACE=/var/lib/quack-nav/rl-traces' | sudo tee -a /var/lib/quack-nav/knobs.env
sudo systemctl restart quack-navd
journalctl -u quack-navd -b | grep 'rl trace: recording'
```

- [ ] Twenty minutes of `go_to` between the marks of §4e (both turning
      directions, a doorway, a passage beside the safe edge of §4b with
      its rim on the books).
- [ ] `ls -la /var/lib/quack-nav/rl-traces/`: a `trace-*.jsonl` growing.

The state tarball of §5 carries them; on the laptop
`scripts/rl/calibrate.sh calib-out quack-rl/pilots/v2-r3 rl-traces/*.jsonl` fits the
simulator to them, retrains the pilot and says whether it may fly.

## 5. Bring it back, replay it here

On the duck, at the end (and before any power-off you care about — the
journal is in RAM):

```sh
journalctl -o short-iso -u quack-navd -u quack-control -u quacksat -u robotd -u tofd -u mediad -b > ~/qn/journal-boot.log
robotctl health --json > ~/qn/health.json; robotctl version > ~/qn/version.txt
sudo tar czf ~/qn/var-lib-quack-nav.tgz -C /var/lib quack-nav     # places.json, ground.json, maploc.session, maps/, recordings/, rl-traces/
sudo cp /etc/robot/quack-nav.toml /etc/robot/quack-control.toml ~/qn/   # not quacksat.toml: it holds keys
sudo chown -R "$USER" ~/qn
```

On the laptop:

```sh
rsync -av $DUCK:qn/ ~/duck-day1/duck/          # or: scp -r $DUCK:qn ~/duck-day1/duck
```

- [ ] `~/duck-day1/` holds: `journal-live.log` (the laptop's copy across
      power cuts), `duck/` (notes, samples, rates, journals, health, the
      state tarball with `recordings/*.mdlg`, `maps/stanza.session`,
      `places.json` — version 2 —, `ground.json`), the page's screenshots,
      the videos, and `truth.toml` written from the tape.

Here, in the quack-nav checkout at `v0.2.0-rc2` (the same mapper as on the
duck; the twin's README, [scripts/twin/README.md](../scripts/twin/README.md),
documents these benches):

```sh
# the room against the tape: tracking, return to S, the kidnap, walls vs truth, two PGMs
cargo run -p maploc --release --features kinematics --example evaluate -- \
    <recordings/STAGE_C.mdlg> ~/duck-day1/truth.toml ~/duck-day1/eval-c
# a boot of §4d replayed into the saved map, starting lost: the homecoming's question on the bench
MAP_SESSION=<maps/stanza.session> cargo run -p maploc --release --features kinematics --example evaluate -- \
    <recordings/BOOT_A.mdlg> ~/duck-day1/truth.toml ~/duck-day1/eval-d-A
# no truth at all: the map as a PGM, and relocalize probes against it
cargo run -p maploc --release --features kinematics --example replay -- <rec.mdlg> ~/duck-day1/replay
# moving and still, from the odometry
python3 scripts/twin/segs.py <rec.mdlg>
```

`evaluate` assumes its protocol (`maploc/examples/evaluate.rs`): the
recording starts at `start` (S — hence the restart at S in §4c), a return
to start, a sit as the kidnap marker. Without the sit it says so and skips
that part.

**What cannot be judged without a simulator's truth**: ATE and RPE along
the path, `trajectory`, `wake_match`, `traj_metrics.py`, `map_vs_truth.py`
and `room_fit.py` all need the twin's pose sampler or scene. On the duck
the truth is the tape: the marks (pose at each, arrival errors), the walls
in `truth.toml`, the edge's position. Write them down; that is the
ground truth of day one.

## 6. Results to fill in, and what to do when it goes wrong

Copy into `~/duck-day1/results.md`, one table per stage:

| stage | what | twin | duck | pass? | notes |
|---|---|---|---|---|---|
| 3 | quack-navd CPU idle / rest / mapping / go_to (% of a core) | 2.7 / 2.0–2.35 / — / — (Mac) | | | |
| 3 | quack-navd RSS, MemoryPeak | ~20 MB after 4 min mapping | | | |
| 3 | quacksat, quack-control CPU, RSS | — | | | |
| 3 | board temperature idle / mapping | — | | | |
| 3 | odom Hz, tof Hz, map.pose per 10 s, pairing fell_back | 50 / 15 / ~190 / ~0 | | | |
| a | tof frames, guard watching, phantom drops per floor | — / yes / 0 | | | |
| b | 2 s at 0.3 m/s walked (cm), veer per 3 s | ~21 cm (from 53 cm in 5 s) | | | |
| b | test edge used, h, cliff.kind, stop distance | 0.56 m short from 1.15 m | | | |
| b | false stops in 9 open-floor moves | 0 | | | |
| c | width / length: tape vs clearance | walls 3–5 cm off | | | |
| c | windows, submaps, loops, minutes | — | | | |
| d | per mark: time to confirm, pose error | 87 s median, ≤ 0.15 m | | | |
| e | per journey: time, arrival error | 0.09–0.30 m | | | |
| f | rest: pose drift, feet creep (cm), go_to arrival | 10 cm mean | | | |
| f | kidnap: untrusted seen, relocalized (s), arrival | 70 s, 0.02 m | | | |
| g | wakes 1 m / 3 m / walking, false wakes, said arrival | — | | | |
| h | STOP latency, coasting: page / nc / voice | 0.15 s, 3 cm | | | |

**When something goes wrong**

- [ ] Grab, at once: `journalctl … -b > ~/qn/journal-<time>.log` (before
      any power-off), `nav robot.map_status > ~/qn/status-<time>.json`,
      `robotctl health --json`, a `note`, a photo of where the duck is.
- [ ] A daemon that will not start: `journalctl -u <unit> -b` — a config
      key it does not know, or a path it cannot write, is named there.
- [ ] **Rolling back quack-nav to rc1**: keep a copy first
      (`sudo cp -a /var/lib/quack-nav ~/qn/state-before-rollback`). rc1
      refuses `places.json` version 2 — move it aside (a new duck has no
      version-1 file to go back to). Then, from the rc2 package's folder,
      the rc1 binary as the second argument:
      `./install-on-duck.sh $DUCK ../../rollback/quack-navd-aarch64-linux`.
      Whether rc1 reads maps saved by rc2 is untested.
- [ ] quack-control and quacksat have no earlier release: to take them
      out, `sudo systemctl disable --now quack-control` (or `quacksat`); the
      uninstall lines are in each package's `README-install.md`.
- [ ] Upgrading later: the newer package's `./install-on-duck.sh` the same
      way; configs, maps and places stay.

After the session: the numbers into [results.md](results.md) beside the
twin's, the surprises into [todo-map.md](todo-map.md), and what v0.2.0
final needs ([release notes](release-notes-v0.2.0-rc2.md#what-v020-final-needs)).
