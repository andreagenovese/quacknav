# La prima sessione su una papera vera — checklist

quack-nav v0.2.0-rc2, quack-control v0.1.0-rc1 e quacksat v0.1.0-rc1 su un
Microduck di Pollen, per la prima volta. Copia inglese:
[first-duck-session.md](first-duck-session.md). Scritta il 2026-10-03,
prima che la papera arrivasse: ogni comando, chiave, percorso e riga di log
qui sotto è stato verificato sui tre repository a quei tag e sui
`docs/robot/` di Pollen alla daemon-v0.15.0, ma niente è ancora girato su
una scheda. Dove una cosa è una stima, e cosa ha misurato il gemello, è
detto dove conta.

L'obiettivo della giornata non è la casa: è **una stanza, in sicurezza**,
con i numeri scritti accanto a quelli del gemello ([results.it.md](results.it.md),
le [note della rc2](release-notes-v0.2.0-rc2.it.md#risultati-principali)) e
le registrazioni riportate a casa per il banco.

Segnaposto: `<duck>` è l'indirizzo della papera (`duckctl --name <robot> ip`,
o `duckctl scan`), `<robot>` il suo nome Bluetooth. Niente qui richiede di
scrivere un indirizzo, una chiave o un nome veri in un file di questo
repository.

## 0. Prima di tutto, la sicurezza

Vale l'avvertenza del README: un robot che cammina vicino a un dislivello
può cadere e rompersi.

- [ ] **Un assistente** accanto alla papera a ogni movimento, mani libere,
      pronto a prenderla. Una persona scrive, l'altra guarda la papera — mai
      la stessa.
- [ ] **Un pavimento senza dislivelli.** Niente scale, pianerottoli, porte
      verso un piano più basso a portata di cinque minuti di cammino; anche
      il bordo di un divano o di un tavolo da cui potrebbe scendere è un
      dislivello. Chiudere le porte verso tutto il resto.
- [ ] **Una barriera morbida** su ogni apertura della stanza (cuscini, un
      tappeto piegato, cartone di taglio): una papera in fuga incontra
      qualcosa di morbido.
- [ ] **Sapere come fermare tutto**, dal più rapido, e provare ognuno una
      volta prima del primo passo:
  1. lo **STOP** di quack-control (la pagina, §2) — `robot.go_to {"stop": true}`:
     ferma qualunque lavoro di quack-nav e quello che fa di sua iniziativa,
     e trattiene (niente riparte da solo finché non si chiede un lavoro).
     Sul gemello: 0,15 s, 3 cm di inerzia.
  2. Lo stesso da una shell sulla papera:
     `printf '{"jsonrpc":"2.0","id":1,"method":"nav.call","params":{"name":"robot.go_to","args":{"stop":true}}}\n' | nc -U -q1 /run/quack-nav/nav.sock`
     (o `nav robot.go_to '{"stop":true}'` con l'aiutante del §2).
  3. Il **gamepad**: **Select**, pressione breve — coppia tolta al
     rilascio, l'arresto d'emergenza di Pollen: **la papera si affloscia,
     tenerla**; Start la rimette in piedi. Select tenuto 2 s: si siede,
     coppia tolta, spenta.
  4. Sulla papera: `sudo systemctl stop quack-navd` (niente più comandi; il
     deadman di robotd azzera la velocità), `sudo robotctl robot enable --off`
     (la policy smette, il robot tiene la posa), `sudo robotctl robot relax --yes`
     (coppia tolta: **crolla**).
  5. Senza rete, via Bluetooth dal portatile:
     `duckctl --name <robot> call robot.enable '{"on":false}'` (btd rifiuta
     `robot.stop` e `robot.relax` via Bluetooth; `robot.enable` passa).
  6. L'alimentazione: l'interruttore della papera, o la batteria.

  La console di mediad (`:8080`) ha uno "stop" che azzera solo i suoi
  comandi: la documentazione di Pollen dice che **non** è un arresto
  d'emergenza.
- [ ] **La teleguida di Pollen scavalca la guardia del dirupo di
      quack-nav.** Il gamepad e la console guidano robotd direttamente, e
      robotd non ha protezione dai dislivelli
      ([study/upstream-asks.it.md](study/upstream-asks.it.md) §8): guidata
      così la papera scende da una scala come cammina sul pavimento. Guidarla
      a mano solo sul pavimento piano della stanza.
- [ ] **Batteria**: partire carichi. `robotctl monitor` mostra il pacco in
      volt e percento (giallo al 30 %, rosso al 15 %); lo 0 % è dove robotd
      siede la papera e toglie l'alimentazione. Fermare le prove al 30 %.
- [ ] **Se cade**: tenerla, Select (coppia tolta) se si dimena ancora;
      annotare l'ora e cosa girava (`note caduta …`, §2); **salvare il
      journal prima di spegnerla** (il `/var/log` della scheda sta in RAM,
      un taglio di corrente lo perde — il `deploy/README.md` di Pollen);
      `robotctl health` nomina un servo in errore, `sudo robotctl robot reboot-motors`
      lo recupera (prima toglie la coppia: tenere la papera). Rimetterla in
      piedi solo capita la causa; quack-navd scrive
      `maploc: robot fell — pose suspect until a window confirms it`.

## 1. Prima della giornata

### Versioni

| componente | versione | pacchetto |
|---|---|---|
| quack-nav | v0.2.0-rc2 | `quack-nav-0.2.0-rc2-aarch64-linux.tar.gz` (+ `.sha256`) |
| quack-control | v0.1.0-rc1 | `quack-control-0.1.0-rc1-aarch64-linux.tar.gz` — vuole quack-nav ≥ v0.2.0-rc2 |
| quacksat | v0.1.0-rc1 | `quacksat-0.1.0-rc1-aarch64-linux.tar.gz` — compilato contro robotd daemon-v0.14.4, più recenti non provati |
| microduck di Pollen | daemon-v0.15.0 (API 37) | robotd, tofd, mediad — quack-nav gira uguale sulla daemon-v0.14.4 |

La tabella di compatibilità di quack-control: 0.1.0-rc1 ↔ quack-nav ≥
v0.2.0-rc2, mediad ≥ daemon-v0.14.4 per la camera, quacksat ancora senza
socket di controllo.

### Cosa deve dare l'immagine di Pollen

- [ ] Radxa Zero 3 (RK3566, aarch64), Armbian con userland Debian 13:
      `cat /etc/os-release; uname -m; ldd --version | head -1` (glibc ≥ 2.31).
- [ ] I demoni, di una stessa release: `robotctl version` (ogni demone in
      esecuzione contro quello installato, e l'API) e `robotctl health`
      (esce con errore se qualcosa non va).
- [ ] L'account `microduck` con `sudo`, nel gruppo `robot`: `id` (le
      immagini più vecchie avevano `radxa`).
- [ ] I socket, modo 0660 gruppo `robot`:
      `ls -l /run/robotd.sock /run/tofd/tof.sock /run/mediad/media.sock`.
- [ ] Per quacksat: il gruppo `audio`, `arecord`/`aplay` (alsa-utils), e
      `grep -n pet_detect /etc/robot/robotd.toml` — `audio.pet_detect` deve
      essere false (il default di fabbrica), o il microfono è occupato.
- [ ] `command -v nc` — le verifiche con `nc -U` vogliono netcat-openbsd
      (`sudo apt install netcat-openbsd` se manca); `python3` serve per
      `scripts/twin/probe.py` (§3), non è obbligatorio.
- [ ] L'orologio: `timedatectl` (sincronizzato, così journal, note e video
      del telefono combaciano).
- [ ] Un gamepad abbinato (`sudo robotctl pad pair`, il `pair-a-gamepad.md`
      di Pollen): è lo stop del §0 e la guida a mano del §4c.

### Rete e portatile

- [ ] La papera sul Wi-Fi di casa (`duckctl --name <robot> wifi connect …`,
      il `duckctl.md` di Pollen), il suo indirizzo da `duckctl --name <robot> ip`.
- [ ] Un portatile con `ssh`, `scp`, `tar`, `shasum`, `gh` (o `curl`), e una
      chiave ssh su `microduck@<duck>`. Sul portatile, per tutta la
      giornata: `DUCK=microduck@<duck>`.
- [ ] Un telefono sulla stessa rete per la pagina (porta 8090) e per filmare.
- [ ] Un metro, nastro di carta per i segni, un pennarello, un cronometro.

### Scaricare e verificare (sul portatile, il giorno prima)

```sh
mkdir -p ~/duck-day1/pkgs && cd ~/duck-day1/pkgs
for p in quacknav:quack-nav:0.2.0-rc2 quack-control:quack-control:0.1.0-rc1 quacksat:quacksat:0.1.0-rc1; do
  IFS=: read -r repo name v <<< "$p"
  gh release download "v$v" --repo "andreagenovese/$repo" --pattern "$name-$v-aarch64-linux.tar.gz*"
  shasum -a 256 -c "$name-$v-aarch64-linux.tar.gz.sha256"     # deve stampare OK
  tar xzf "$name-$v-aarch64-linux.tar.gz"
done
# la strada indietro (§6): quack-nav rc1 non aveva pacchetto, solo il binario
mkdir -p ../rollback && cd ../rollback
gh release download v0.2.0-rc1 --repo andreagenovese/quacknav --pattern 'quack-navd-aarch64-linux*'
shasum -a 256 -c quack-navd-aarch64-linux.sha256
```

Senza `gh`, le righe `curl -LO …/releases/download/v$V/…` del
`README-install.md` di ogni pacchetto.

- [ ] **I modelli della parola di risveglio di quacksat** non sono nel suo
      pacchetto: l'installatore li scarica **sulla papera** (i due modelli
      di feature di openWakeWord, CC BY-NC-SA 4.0, non commerciale; "hey
      Daffy" dal repository di quacksat), ognuno controllato col suo
      sha256. Durante quell'installazione la papera vuole internet.
- [ ] **La voce, se si usa il primo giorno** — scegliere il backend e
      avere gli endpoint a portata (segnaposto, mai in questo repository):
      `direct` vuole `[direct.llm]`, `[direct.stt]`, `[direct.tts]`
      (`base_url`, `api_key`, `model`/`language`/`voice`) e
      `tool_calling = true` per gli strumenti del robot; `agent` vuole un
      bridge (`[agent] url = "ws://<bridge-host>:8765"`). Con Arkimede, il
      profilo del README del bridge è `http://<server>:3000/api/openai/v1`,
      una chiave `ak_` e `tool_calling = false`, con gli strumenti del robot
      che gli arrivano da un server MCP — quindi "vai in cucina" via
      Arkimede vuole quel collegamento MCP.
- [ ] Un checkout di quack-nav a `v0.2.0-rc2` sul portatile, compilato una
      volta (`cargo build --release -p maploc --features kinematics --examples`),
      per il banco del §5.

## 2. Installare, in ordine

Ogni pacchetto: prima `--dry-run` (stampa ogni comando, non si collega a
niente), poi davvero. Da `~/duck-day1/pkgs`:

- [ ] **quack-nav**: `cd quack-nav-0.2.0-rc2 && ./install-on-duck.sh --dry-run $DUCK && ./install-on-duck.sh $DUCK`
- [ ] **quack-control**: `cd ../quack-control-0.1.0-rc1 && ./install-on-duck.sh --dry-run $DUCK && ./install-on-duck.sh $DUCK`
- [ ] **quacksat**: `cd ../quacksat-0.1.0-rc1 && ./install-on-duck.sh --dry-run $DUCK && ./install-on-duck.sh $DUCK`
      (la config d'esempio lo avvia in modalità di messa in servizio,
      `backend = "none"`: si sveglia su "hey Daffy" e cinguetta, e non parla
      con nessuno).

### Le modifiche alla config per il primo giorno

`sudo nano /etc/robot/quack-nav.toml`, poi `sudo systemctl restart quack-navd`:

```toml
[map]
explore_max_s = 300      # "mappa tutto" senza budget: 5 minuti, non 30

[homecoming]
enabled = false          # fasi a–c: niente cammina da solo all'avvio (il §4d lo accende)

[maploc]
record_dir = "/var/lib/quack-nav/recordings"   # un .mdlg di tutto ciò che il mapper ha letto, per il §5
```

Il resto dell'esempio resta (`[maploc] enabled = true`, `mode =
"stop_and_scan"`). I percorsi devono stare sotto `/var/lib/quack-nav/` —
l'unit non lascia scrivere il demone altrove. Una registrazione è circa
6 KB/s (sui 22 MB l'ora), un nuovo `<ora unix>.mdlg` a ogni avvio del
demone.

quack-control — il token (la pagina lo chiede una volta):

```sh
sudo sh -c 'echo "QC_TOKEN=$(tr -dc a-z0-9 </dev/urandom | head -c 32)" > /etc/robot/quack-control.env'
sudo cat /etc/robot/quack-control.env
sudo systemctl restart quack-control
```

quacksat (`sudo nano /etc/robot/quacksat.toml`; conterrà chiavi API —
tenerlo `root:quacksat` 0640, come lo lascia l'installatore): restare su
`backend = "none"` fino al §4g; poi il backend scelto al §1, e
`[announce] language = "it"`. Se "hey Daffy" non sente una voce italiana,
`[wake] threshold = 0.4` (le sue note di rilascio hanno misurato un
parlante italiano a 0,20–0,37 contro 0,5).

### Verifiche

Sulla papera (`ssh $DUCK`). Da incollare una volta per shell — un aiutante
per le chiamate, uno per le note e una cartella per tutto ciò che si misura:

```sh
mkdir -p ~/qn
nav() {   # nav <strumento> ['<json args>']   —  NAV_WAIT=130 per {"complete":true}
  local args=${2:-'{}'}
  printf '{"jsonrpc":"2.0","id":1,"method":"nav.call","params":{"name":"%s","args":%s}}\n' "$1" "$args" \
    | nc -U -q "${NAV_WAIT:-2}" /run/quack-nav/nav.sock; echo
}
note() { echo "$(date +%T) $*" | tee -a ~/qn/notes.txt; }
```

- [ ] `systemctl status quack-navd quack-control quacksat --no-pager` — tutti attivi.
- [ ] `stat -c '%n %U:%G %a' /run/quack-nav/nav.sock /run/quack-nav/map.sock` →
      `quacknav:robot 660` entrambi.
- [ ] `printf '{"jsonrpc":"2.0","id":1,"method":"nav.catalog","params":{}}\n' | nc -U -q1 /run/quack-nav/nav.sock | grep -o '"name":"robot\.[a-z_]*"'`
      → dodici strumenti.
- [ ] `nav robot.map_status` → `"mapping":true`, `"mode":"stop_and_scan"`,
      `cliff.guard` `"watching"` quando la papera è in piedi.
- [ ] `journalctl -u quack-navd -b --no-pager | grep -E 'serving the map|subscribed to robot.state|connected to tofd|recording session'`
      — le quattro righe (`maploc: serving the map`, `maploc: subscribed to
      robot.state`, `maploc: connected to tofd's depth stream`, `maploc:
      recording session`).
- [ ] La pagina: `http://<duck>:8090/?token=<QC_TOKEN>` dal telefono — la
      mappa, la riga di stato, Services (quack-nav risponde, quacksat "not
      available", come previsto). `journalctl -u quack-control -b`:
      `serving the page` con `token=true`.
- [ ] quacksat: `journalctl -u quacksat -b` — `quacksat starting`, `wake
      word loaded`, `listening`, `the navigation daemon answered` con
      `tools=12`. Dire "hey Daffy": una riga `wake`, e la papera cinguetta.
- [ ] Sul portatile, una copia viva dei journal che sopravvive ai tagli di
      corrente (riavviarla dopo ogni accensione):
      `ssh $DUCK 'journalctl -f -o short-iso -u quack-navd -u quack-control -u quacksat -u robotd -u tofd -u mediad' >> ~/duck-day1/journal-live.log &`
      (se non stampa niente, l'account non legge il journal di sistema:
      `sudo` sulla papera).

## 3. Misure, coi comandi esatti

Da incollare sulla papera; ognuna scrive in `~/qn`:

```sh
sample() {   # sample <etichetta> <secondi>: CPU e memoria per processo ogni 5 s, e le temperature della scheda
  local n=$(( $2 / 5 ))
  ( for i in $(seq "$n"); do echo "$(date +%T) $(cat /sys/class/thermal/thermal_zone*/temp | tr '\n' ' ')"; sleep 5; done ) > ~/qn/temp-$1.txt &
  top -b -d 5 -n "$n" -o %CPU -w 200 | grep -E '^top|^%Cpu|^MiB|quack|robotd|tofd|mediad' > ~/qn/top-$1.txt
  wait
  systemd-cgtop -b -n 1 --raw > ~/qn/cgtop-$1.txt
  systemctl show quack-navd quack-control quacksat -p Id -p MemoryCurrent -p MemoryPeak -p CPUUsageNSec > ~/qn/units-$1.txt
  free -m >> ~/qn/units-$1.txt
}
cat /sys/class/thermal/thermal_zone*/type > ~/qn/thermal-zones.txt   # quale zona è quale (millesimi di grado)
```

- [ ] **A riposo vigile**, la papera in piedi, niente chiesto: `sample idle 300`.
- [ ] **In riposo** (dopo un minuto senza lavori quack-navd riposa): `sample rest 300` durante il §4f.
- [ ] **Mappando** (stop_and_scan): `sample mapping 300` in una seconda shell durante il §4c.
- [ ] **Un go_to**: `sample goto 120` in una seconda shell durante il §4e.
- [ ] Una fotografia sola: `ps -o pid,comm,%cpu,rss,etimes -C quack-navd,quack-control,quacksat,robotd,tofd,mediad > ~/qn/ps.txt`
      (`%cpu` è la media dall'avvio; `top` sopra è il ritmo). Va bene anche
      `pidstat` (sysstat), se c'è.
- [ ] **Le frequenze**, in piedi:
  - profondità e odometria come le riceve quack-navd —
    `journalctl -u quack-navd --since "2 min ago" | grep 'maploc: status' | tail -3 > ~/qn/status-lines.txt`:
    i contatori `odom` e `frames` sono cumulativi, una riga ogni 5 s,
    quindi la differenza divisa per 5 è la frequenza (tofd va a 15 Hz nel
    monitor di Pollen; il flusso di stato segue il ciclo di robotd a 50 Hz);
  - oppure, con `python3` e `scripts/twin/probe.py` copiato dal checkout:
    `python3 probe.py /run/robotd.sock /run/tofd/tof.sock 10 > ~/qn/probe.txt`
    (robot.state e tof.stream in Hz, e i due orologi);
  - l'accoppiamento della testa — `journalctl -u quack-navd -b | grep -E 'head pairing|paired with the head' > ~/qn/pairing.txt`:
    `paired` contro `fell_back` ogni 600 frame (fell_back dovrebbe restare
    vicino a zero) e `mean_lag_ms`;
  - il flusso della posa (`map.pose`, ogni 50 ms tra i `map.frame` a 1 Hz) —
    ```sh
    (printf '{"jsonrpc":"2.0","id":1,"method":"robot.map","params":{}}\n'; sleep 12) \
      | timeout 10 nc -U /run/quack-nav/map.sock > ~/qn/map-10s.ndjson
    grep -c '"map.pose"' ~/qn/map-10s.ndjson; grep -c '"map.frame"' ~/qn/map-10s.ndjson   # attesi ~190 e ~10
    ```
- [ ] **Le finestre di maploc**: `journalctl -u quack-navd -b | grep -E 'still window integrated|window too thin|quarantined|relocalized|loop closed|resting|rest watch' > ~/qn/windows.txt`
      — il tempo da una sosta al suo `maploc: still window integrated` (è
      una sosta di 6 s che mappa), e quante vengono scartate.

Riferimento sul gemello (un Mac, non l'RK3566): quack-navd 2,72 % di un
core sveglio, 2,0–2,35 % in riposo; ~20 MB residenti dopo quattro minuti di
mappatura; l'unit lo limita a 256 MB (high) e 320 MB (max) (quacksat
192/256 MB). Sulla scheda non è misurato niente — questa sezione è il
primo numero.

## 4. Prove a fasi

Prima i segni. Scegliere un **angolo di riferimento O** della stanza; **x**
lungo una parete, **y** lungo l'altra, in modo che y sia a sinistra
guardando lungo x. Croci di nastro con una freccia per la direzione: **S**
(partenza, rivolta verso +x, ad almeno 0,6 m da ogni parete), **A**, **B**,
**C**, **D** sparsi per la stanza, e **K** (dove atterra il rapimento).
Misurare ognuno in cm da O; misurare anche le pareti della stanza, come
segmenti — è il `truth.toml` del §5 (`walls`, `start` = S, `kidnap` = K,
centimetri e gradi, il formato di
[maploc/examples/room_lab.toml](../maploc/examples/room_lab.toml)).
Misurare **h**, l'altezza dal pavimento del sensore della testa, in piedi.

Siccome la mappa parte da S rivolta verso +x (§4c), le coordinate di
mappa di un segno sono `(x_segno − x_S, y_segno − y_S) / 100` in metri.

Prima di ogni fase: `note "fase X inizio"`; dopo: `note "fase X fine: …"`.

### a. In piedi, i sensori

- [ ] Mettere in piedi la papera (Start, due volte — coppia e posa di casa,
      poi la policy; oppure `sudo robotctl robot init` poi `sudo robotctl robot enable`).
- [ ] `robotctl monitor`, **t**: il riquadro del ToF — `15 Hz · 8×8`, quante
      delle 64 zone misurano; **c**: un fotogramma della camera.
- [ ] `nav robot.map_status`: `cliff.guard` `"watching"`, `cliff.frames` che
      cresce tra due chiamate, `cliff.edge_between_m` null sul pavimento
      piano. Una scatola a 40 cm davanti: `cliff.nearest_obstacle.range_m` ≈ 0,4.
- [ ] La testa spazza nelle soste (`[maploc] search_sweep`); `windows`
      cresce di uno a sosta.
- [ ] **Dislivelli fantasma**: metterla su ogni tipo di pavimento della
      stanza (tappeto scuro, piastrella lucida, legno chiaro), una spazzata
      di testa ognuno: `cliff.edge_between_m` deve restare null. Un sensore
      che non torna su un pavimento scuro lo legge come un dislivello
      (`cliff.kind` `"no floor return"`) — l'avvertenza di cliff.rs, mai
      misurata.
- [ ] La camera sulla pagina di quack-control: il pulsante camera,
      frequenza ed età del fotogramma.
- [ ] `sample idle 300`, e le frequenze del §3.

**Passa**: guardia che osserva, tof alla sua frequenza, nessun dislivello
su nessun pavimento piano, l'ostacolo entro ~10 cm dal metro.

### b. Movimenti a mano, e la guardia del dirupo su un bordo sicuro

- [ ] Piccoli passi, le mani dell'assistente vicine:
      `nav robot.move '{"vx":0.3,"duration_s":2}'` (≤ 3 s; la policy di
      cammino non fa passi sotto ~0,25 m/s comandati; sul gemello 0,3 m/s
      hanno percorso 53 cm in 5 s). Atteso `"done":true`,
      `"cliff_guard":"on"`. Misurare la distanza. Una curva:
      `'{"vx":0.3,"vyaw":0.7,"duration_s":2}'` — la papera non gira sul
      posto. Mai indietro verso un bordo: un movimento all'indietro è
      `not covered: backing up`.
- [ ] Lo stesso da quack-control: **Advanced → All tools → robot.move**
      (chiede conferma).
- [ ] **La deriva**: tre movimenti dritti da `duration_s` 3; lo
      scostamento laterale col metro. Una deriva costante è
      `[gait] yaw_trim` in `/etc/robot/quack-nav.toml` (rad/s, + = sinistra;
      il gemello voleva circa 0,2) — annotare il numero, cambiarlo dopo la
      sessione.
- [ ] Un passo di mappatura: `nav robot.map_step '{"vx":0.3,"walk_s":2}'` →
      `new_windows`, `clearance`, `checks` `"map and sensor"`.
- [ ] **Scegliere un bordo di prova sicuro** — mai una scala vera. Dal
      codice (`quack-nav/src/cliff.rs`) un raggio è un dislivello quando il
      ritorno del pavimento **manca** (entro 1,2 m) o è **almeno 1,5 volte
      più lontano** di dove dovrebbe essere il pavimento, due raggi per
      frame, giudicati da fermi. Quindi un gradino in discesa si legge solo
      se è profondo almeno **h/2**: **una pedana di 3–5 cm molto
      probabilmente non si legge affatto** (un gradino non è mai stato
      provato nemmeno sul gemello — todo-map, 2026-10-02). In ordine:
  1. una superficie piana che il sensore forse non vede — una piastrella a
     specchio, velluto nero, un pannello nero lucido a terra: se si legge,
     la guardia si prova senza alcuna caduta (ed è una scoperta: un
     pavimento così in casa è un dislivello fantasma);
  2. solo se (1) non si legge: una pedana stabile alta almeno h/2, la
     papera sopra, cuscini sotto, la mano dell'assistente sul bordo.

  Prima di usare un bordo: la papera a 0,6–0,8 m, rivolta verso di esso,
  una spazzata di testa, `nav robot.map_status` → `cliff.edge_between_m` non
  null, `cliff.bearing_deg` vicino a 0, annotare `cliff.kind`. Se non si
  legge, non è un bordo di prova.
- [ ] **La guardia**: da ~1,0 m, rivolta al bordo,
      `nav robot.move '{"vx":0.3,"duration_s":3}'` ripetuto. Atteso, alla
      chiamata che ci arriverebbe: `"done":false`, `"stopped":"a drop ahead
      (depth sensor): …"`. Misurare la distanza becco-bordo. Gemello: da
      1,15 m si è fermata col tronco 0,56 m prima del bordo; la guardia
      agisce su un bordo entro 0,40 m in una mezza corsia di 0,17 m. Poi nove
      movimenti su pavimento libero: nessuno stop falso (gemello: nessuno).

**Passa**: si muove come chiesto, la guardia si ferma prima del bordo ogni
volta, nessuno stop falso.

### c. Una prima mappa di una stanza

- [ ] La papera su **S rivolta verso +x**. `sudo systemctl restart quack-navd`
      (qui parte una nuova registrazione; aspettare `maploc: recording session`),
      poi `nav robot.map_wipe` (una mappa nuova da qui — non è nel catalogo,
      ma è uno strumento a cui quack-navd risponde). Verifica:
      `nav robot.map_status` → `pose` ≈ `{"x":0,"y":0,"yaw":0}`. Se no,
      annotare lo scarto.
- [ ] O **(A) da sola**: `nav robot.map_explore '{"max_s":300,"save_as":"stanza"}'`
      (risponde subito; guardare la pagina, STOP pronto); o **(B) guidata**:
      `nav robot.map_explore '{"watch":true,"max_s":600}'` e l'assistente
      guida col gamepad — camminate brevi, soste di almeno 6 s (mappa solo
      una sosta), ripassando da posti già mappati — poi
      `nav robot.map_explore '{"stop":true}'`. In (B) `robot.move` è
      rifiutato finché gira e il gamepad scavalca la guardia del dirupo: la
      stanza non deve avere dislivelli.
- [ ] Nel frattempo, una seconda shell: `sample mapping 300`.
- [ ] L'avanzamento: `nav robot.map_status` → `explore.state`, `windows`,
      `submaps`, `loops`, `house.percent_mapped`.
- [ ] Ritorno su S (`nav robot.go_to '{"x":0,"y":0}'` o a mano), in piedi
      10 s. **La stanza contro il metro**: `clearance.ahead.free_m +
      clearance.behind.free_m` e `left + right` su S (i raggi guardano fino
      a 3 m) contro larghezza e lunghezza misurate passando per S.
- [ ] Facoltativo, per la parte del rapimento di `evaluate`: farla sedere
      (DPad-Giù, o `robotctl robot do sit_toggle`), portarla su **K** con
      la sua freccia, rimetterla in piedi.
- [ ] Salvare e chiudere: `nav robot.map_save '{"name":"stanza"}'`, poi
      `NAV_WAIT=130 nav robot.map_explore '{"complete":true,"save_as":"stanza"}'`
      — salvata, dichiarata finita, congelata. `nav robot.map_list` la mostra.
- [ ] Uno screenshot della mappa della pagina; `note` coi numeri di clearance.

**Passa**: nessuna caduta, le pareti della stanza chiuse sulla pagina,
larghezza e lunghezza entro ~10 cm dal metro (gemello: pareti a 3–5 cm in
media, 98 % sulla verità).

### d. Ritorno a casa: spegnimenti sui segni

- [ ] La config per il resto della giornata (`sudo nano /etc/robot/quack-nav.toml`):
      `[maploc] mode = "localize"` (la mappa resta come salvata, la posa si
      corregge contro di essa), e

      ```toml
      [homecoming]
      enabled = true
      resume_explore = false
      start_delay_s = 60      # il tempo per rimetterla in piedi dopo l'accensione
      ```

      `sudo systemctl restart quack-navd`. Con la mappa congelata un avvio
      che non conferma cerca (60 s, poi altri tre budget) e si ferma — non
      parte mai una mappa nuova.
- [ ] **Non** premere STOP subito prima di uno spegnimento: uno STOP con
      niente in corso trattiene comunque il ritorno a casa (un limite noto
      della rc2).
- [ ] Per ognuno di A, B, C, D: farla sedere e spegnerla (Select tenuto
      2 s), portarla sul segno con la sua freccia, accenderla, metterla in
      piedi — `note "d A in piedi"` (la prima nota dopo un avvio: la shell è
      nuova, reincollare gli aiutanti, riavviare il journal vivo del
      portatile). Poi guardare:
      `journalctl -u quack-navd -f | grep homecoming` —
      `homecoming: loaded the newest map; standing still to see if the duck knows where it is`,
      poi `homecoming: home — the pose is confirmed on the saved map`
      (o `no confirmation on the frozen map; standing down`).
      La ricerca all'avvio cammina e guarda: l'assistente resta.
- [ ] Alla conferma: `nav robot.map_status` → `pose` contro le coordinate
      di mappa del segno; il tempo da in piedi alla conferma.

**Gemello**: 28 risvegli su 28 giusti, nessuno sbagliato, mediana 87 s
(75–141 s), 0,01–0,15 m dalla verità. **Passa**: mai una posa confermata
sbagliata (> 0,3 m), 3 su 4 confermati, i tempi annotati.

### e. go_to tra i segni

- [ ] Insegnare i segni come luoghi, con le coordinate del metro
      (indipendenti dalla posa della papera): `nav robot.remember_place '{"name":"cucina","x":<x_A>,"y":<y_A>}'`,
      e B, C, D coi loro nomi (`"x"`/`"y"` in metri di mappa, su pavimento
      mappato). Oppure stare su un segno e `nav robot.remember_place '{"name":"B"}'`.
- [ ] `nav robot.go_to '{"place":"B"}'`; interrogare `nav robot.map_status` →
      `explore.state` `running` … `done` (o `failed` e `explore.reason`).
      Una seconda shell: `sample goto 120`.
- [ ] All'arrivo: metro dal centro della papera (tra i piedi) alla croce. Il
      tempo dalla chiamata a `done`.
- [ ] Almeno quattro viaggi, uno attraverso la stanza, uno intorno a un
      ostacolo.

**Gemello**: 9/9 arrivati a 0,09–0,30 m dalla meta; i viaggi di una casa in
84–111 s di mediana. **Passa**: arriva, ≤ 0,30 m, nessuna caduta, nessuno
stop per un dislivello fantasma.

### f. Il riposo, e un rapimento

- [ ] Lasciarla in piedi senza lavori per 30 minuti. Nel log:
      `maploc: resting — a long idle stand …` dopo un minuto, un
      `maploc: rest watch` ogni due minuti; `nav robot.map_status` →
      `resting: true`, `rest_watch.verdict`. Intanto `sample rest 300`.
      Segnare col nastro i piedi prima e dopo: il riposo vero scivola? (quello
      del gemello gira di ~0,1°/s e scivola da solo).
- [ ] Poi `nav robot.go_to '{"place":"cucina"}'`: si sveglia subito;
      misurare l'arrivo.
- [ ] **Rapimento**: dopo un minuto di riposo, sollevarla, portarla su
      **K**, girarla di ~90°, posarla in piedi. Atteso
      `maploc: the duck may have been moved while it rested — the pose is untrusted; the next job finds it first`,
      `untrusted: true`. Poi `nav robot.go_to '{"place":"B"}'` → la risposta
      porta `"relocalizing":true`; `explore.state` `relocalizing`, poi
      `running`, poi `done`. Cronometrare la rilocalizzazione, misurare
      l'arrivo.

**Gemello**: riposo di 30 minuti, posa sbagliata di 10,0 cm in media, 17,2
al peggio; portata per 3,3 m e girata di 86°: ritrovata in 70 s, arrivata a
0,02 m. **Passa**: untrusted visto, ritrovata senza pose sbagliate, arriva.

### g. La voce con quacksat (se configurata)

- [ ] quacksat sul suo backend (§1), `sudo systemctl restart quacksat`;
      `journalctl -u quacksat -f`.
- [ ] "Hey Daffy" da 1 m e da 3 m, ferma e mentre cammina (il rumore dei
      motori): contare le righe `wake` contro i tentativi; i risvegli falsi
      in 10 minuti di chiacchiere vicino.
- [ ] "Hey Daffy, dove sei?" → `robot.where_am_i`.
- [ ] "Hey Daffy, vai in cucina" → risponde subito e cammina; all'arrivo
      dice "Sono arrivata in cucina" (`[announce]`; con `wyoming` non dice
      niente da sé). Il tempo dalla fine della frase al primo passo.

**Gemello**: "vai in cucina" è arrivata in cucina e l'arrivo è stato detto.
**Passa**: si sveglia ≥ 8 volte su 10 a 1 m, arriva, lo dice.

### h. STOP a metà viaggio

- [ ] Un go_to attraverso la stanza; a metà, **STOP** sulla pagina.
      Filmare: il tempo dalla pressione all'arresto, misurare l'inerzia.
      `nav robot.map_status` → `explore.state` `stopped`,
      `explore.stopped_by_user: true`.
- [ ] Lo stesso con `nav robot.go_to '{"stop":true}'`.
- [ ] Lo stesso a voce ("Hey Daffy, fermati"), se il §4g gira: uno stop
      mandato dall'agente non viene ridetto da sé (lo dice già la sua
      risposta).
- [ ] Dopo uno STOP niente riparte da solo; il lavoro successivo funziona.

**Gemello**: 0,15 s, 3 cm. **Passa**: si ferma entro un passo, ogni volta.

### i. (solo la build `rl-nav`) Tracce per la taratura del pilota

Solo con un quack-navd compilato dal branch sperimentale `rl-nav`
([rl-pilot.it.md](rl-pilot.it.md)); la rc2 non legge queste manopole.
Qui non vola nessun pilota: guida lo stick, e i suoi passi vengono registrati.

```sh
echo 'QK_RL_TRACE=/var/lib/quack-nav/rl-traces' | sudo tee -a /var/lib/quack-nav/knobs.env
sudo systemctl restart quack-navd
journalctl -u quack-navd -b | grep 'rl trace: recording'
```

- [ ] Venti minuti di `go_to` tra i segni del §4e (rotazioni da entrambi i
      lati, una porta, un passaggio accanto al bordo sicuro del §4b con il
      suo bordo sul libro).
- [ ] `ls -la /var/lib/quack-nav/rl-traces/`: un `trace-*.jsonl` che cresce.

L'archivio dello stato del §5 le porta con sé; sul portatile
`scripts/rl/calibrate.sh calib-out rl-runs/r2 rl-traces/*.jsonl` adatta il
simulatore a esse, riaddestra il pilota e dice se può volare.

## 5. Riportarlo a casa, rigiocarlo qui

Sulla papera, alla fine (e prima di ogni spegnimento che conta — il
journal sta in RAM):

```sh
journalctl -o short-iso -u quack-navd -u quack-control -u quacksat -u robotd -u tofd -u mediad -b > ~/qn/journal-boot.log
robotctl health --json > ~/qn/health.json; robotctl version > ~/qn/version.txt
sudo tar czf ~/qn/var-lib-quack-nav.tgz -C /var/lib quack-nav     # places.json, ground.json, maploc.session, maps/, recordings/, rl-traces/
sudo cp /etc/robot/quack-nav.toml /etc/robot/quack-control.toml ~/qn/   # non quacksat.toml: contiene chiavi
sudo chown -R "$USER" ~/qn
```

Sul portatile:

```sh
rsync -av $DUCK:qn/ ~/duck-day1/duck/          # o: scp -r $DUCK:qn ~/duck-day1/duck
```

- [ ] `~/duck-day1/` contiene: `journal-live.log` (la copia del portatile
      attraverso i tagli di corrente), `duck/` (note, campioni, frequenze,
      journal, health, il tarball dello stato con `recordings/*.mdlg`,
      `maps/stanza.session`, `places.json` — versione 2 —, `ground.json`),
      gli screenshot della pagina, i video, e `truth.toml` scritto dal metro.

Qui, nel checkout di quack-nav a `v0.2.0-rc2` (lo stesso mapper della
papera; il README del gemello, [scripts/twin/README.it.md](../scripts/twin/README.it.md),
documenta questi banchi):

```sh
# la stanza contro il metro: tracciamento, ritorno a S, il rapimento, pareti contro verità, due PGM
cargo run -p maploc --release --features kinematics --example evaluate -- \
    <recordings/FASE_C.mdlg> ~/duck-day1/truth.toml ~/duck-day1/eval-c
# un avvio del §4d rigiocato nella mappa salvata, partendo persa: la domanda del ritorno a casa al banco
MAP_SESSION=<maps/stanza.session> cargo run -p maploc --release --features kinematics --example evaluate -- \
    <recordings/AVVIO_A.mdlg> ~/duck-day1/truth.toml ~/duck-day1/eval-d-A
# nessuna verità: la mappa come PGM, e sonde di rilocalizzazione contro di essa
cargo run -p maploc --release --features kinematics --example replay -- <rec.mdlg> ~/duck-day1/replay
# in moto e ferma, dall'odometria
python3 scripts/twin/segs.py <rec.mdlg>
```

`evaluate` presuppone il suo protocollo (`maploc/examples/evaluate.rs`): la
registrazione parte da `start` (S — per questo il riavvio su S al §4c), un
ritorno alla partenza, una seduta come segno del rapimento. Senza la
seduta lo dice e salta quella parte.

**Cosa non si può giudicare senza la verità di un simulatore**: ATE e RPE
lungo il percorso, `trajectory`, `wake_match`, `traj_metrics.py`,
`map_vs_truth.py` e `room_fit.py` vogliono tutti il campionatore di posa o
la scena del gemello. Sulla papera la verità è il metro: i segni (la posa
su ognuno, gli errori d'arrivo), le pareti in `truth.toml`, la posizione
del bordo. Annotarli; sono la verità di riferimento del primo giorno.

## 6. Risultati da compilare, e cosa fare quando va male

Da copiare in `~/duck-day1/results.md`, una tabella per fase:

| fase | cosa | gemello | papera | passa? | note |
|---|---|---|---|---|---|
| 3 | CPU di quack-navd vigile / riposo / mappatura / go_to (% di un core) | 2,7 / 2,0–2,35 / — / — (Mac) | | | |
| 3 | RSS di quack-navd, MemoryPeak | ~20 MB dopo 4 min di mappatura | | | |
| 3 | CPU, RSS di quacksat e quack-control | — | | | |
| 3 | temperatura della scheda vigile / mappatura | — | | | |
| 3 | odom Hz, tof Hz, map.pose in 10 s, fell_back dell'accoppiamento | 50 / 15 / ~190 / ~0 | | | |
| a | frame tof, guardia che osserva, dislivelli fantasma per pavimento | — / sì / 0 | | | |
| b | 2 s a 0,3 m/s percorsi (cm), deriva in 3 s | ~21 cm (da 53 cm in 5 s) | | | |
| b | bordo di prova usato, h, cliff.kind, distanza d'arresto | 0,56 m prima da 1,15 m | | | |
| b | stop falsi in 9 movimenti su pavimento libero | 0 | | | |
| c | larghezza / lunghezza: metro contro clearance | pareti a 3–5 cm | | | |
| c | windows, submaps, loops, minuti | — | | | |
| d | per segno: tempo di conferma, errore di posa | mediana 87 s, ≤ 0,15 m | | | |
| e | per viaggio: tempo, errore d'arrivo | 0,09–0,30 m | | | |
| f | riposo: deriva della posa, scivolamento dei piedi (cm), arrivo del go_to | 10 cm in media | | | |
| f | rapimento: untrusted visto, rilocalizzata (s), arrivo | 70 s, 0,02 m | | | |
| g | risvegli 1 m / 3 m / in cammino, risvegli falsi, arrivo detto | — | | | |
| h | latenza dello STOP, inerzia: pagina / nc / voce | 0,15 s, 3 cm | | | |

**Quando qualcosa va male**

- [ ] Prendere subito: `journalctl … -b > ~/qn/journal-<ora>.log` (prima di
      ogni spegnimento), `nav robot.map_status > ~/qn/status-<ora>.json`,
      `robotctl health --json`, una `note`, una foto di dov'è la papera.
- [ ] Un demone che non parte: `journalctl -u <unit> -b` — una chiave di
      config che non conosce, o un percorso dove non può scrivere, è
      nominato lì.
- [ ] **Tornare a quack-nav rc1**: prima una copia
      (`sudo cp -a /var/lib/quack-nav ~/qn/state-before-rollback`). La rc1
      rifiuta `places.json` versione 2 — spostarlo da parte (una papera
      nuova non ha un file versione 1 a cui tornare). Poi, dalla cartella
      del pacchetto rc2, il binario della rc1 come secondo argomento:
      `./install-on-duck.sh $DUCK ../../rollback/quack-navd-aarch64-linux`.
      Se la rc1 legga le mappe salvate dalla rc2 non è provato.
- [ ] quack-control e quacksat non hanno una release precedente: per
      toglierli, `sudo systemctl disable --now quack-control` (o
      `quacksat`); le righe di disinstallazione sono nel
      `README-install.md` di ogni pacchetto.
- [ ] Aggiornare più avanti: il `./install-on-duck.sh` del pacchetto più
      nuovo allo stesso modo; config, mappe e luoghi restano.

Dopo la sessione: i numeri in [results.it.md](results.it.md) accanto a
quelli del gemello, le sorprese in [todo-map.it.md](todo-map.it.md), e
cosa serve alla v0.2.0 finale ([note di rilascio](release-notes-v0.2.0-rc2.it.md#cosa-serve-per-la-v020-finale)).
