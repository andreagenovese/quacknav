# Il gemello, con il robotd ufficiale

Tutto ciò che serve per far girare quack-nav sul gemello MuJoCo con il
robotd **ufficiale** di Pollen (daemon-v0.14.4) e il mapper ospitato in
`quack-navd` (`[maploc]`, vedi ADR 0007) — e per ripetere le misure del
2026-09-23 che dicono che si comporta come il fork di robotd.

## Preparare

```sh
# il demone di Pollen alla release, robotd e tofd compilati per il simulatore
git clone https://github.com/pollen-robotics/microduck && cd microduck
git checkout daemon-v0.14.4 && cargo build -p robotd -p tof

# il simulatore (il suo README prepara la .venv)
git clone https://github.com/pollen-robotics/microduck_rl

# questo repository
cargo build --release
```

```sh
export MICRODUCK=~/src/microduck          # alla daemon-v0.14.4, compilato
export MICRODUCK_RL=~/src/microduck_rl    # con la sua .venv
export POLICY_DIR=~/policies              # il set alpha: alpha_walking.onnx, alpha_stand.onnx,
                                          # alpha_sitstand.onnx, alpha_ground_pick.onnx,
                                          # ball_kick_left.onnx, ball_kick_right.onnx, roulade.onnx
export VIEWER_DIR=...                     # facoltativo: body_with_map.py + maploc_overlay.py
```

I numeri qui sono stati misurati con il set alpha; Pollen pubblica le
policy di serie sull'Hub di Hugging Face.

`VIEWER_DIR` disegna nel viewer la mappa, la rotta, i raggi del sensore
di profondità e la corsia della guardia (la `sim-maploc/` del fork di
robotd, con il supporto a `QUACK_NAV_SOCKET`). Senza, gira il body server
semplice e il viewer mostra solo l'anatra.

## Farlo girare

```sh
scripts/twin/twin.sh up        # simulatore, tofd, robotd, quack-navd
scripts/twin/twin.sh enable    # l'anatra si avvia seduta
python3 scripts/twin/call.py /tmp/quack-twin/nav.sock robot.map_explore '{}'
python3 scripts/twin/call.py /tmp/quack-twin/nav.sock robot.where_am_i
scripts/twin/twin.sh down
```

`STATE` (default `/tmp/quack-twin`) contiene i socket, i log, la
sessione, le mappe salvate e una registrazione `.mdlg` di ogni giro;
tenerlo corto, su macOS il percorso di un socket unix è al massimo di 104
byte. `MAPLOC_MODE` (`stop_and_scan` o `localize`), `HOMECOMING`
(`on`/`off`) e `WIPE` (`on`/`off`) preparano un avvio su una casa
salvata. Per parlarci, puntare `[nav] socket` di un satellite vocale su
`$STATE/nav.sock` e il suo `robotd_socket` su `$STATE/robotd.sock`.

## Misurare

| script | a cosa risponde |
|---|---|
| `probe.py <robotd.sock> <tof.sock>` | cosa riceve un mapper fuori da robotd: frequenze, campi, i due orologi |
| `spin.py <robotd.sock>` | quanto gira la rotazione del panorama (22–24°/s con entrambi i robotd) |
| `headwatch.py <robotd.sock> <s> [sway]` | chi ha la testa; con `sway`, una posa pensante a cui la scansione deve cedere |
| `segs.py <file.mdlg> [da] [a]` | una registrazione come tratti in movimento e fermi — come si è trovato l'avvio lento |
| `ab_round.sh <n>` | lo stesso percorso sul fork e qui, entrambi valutati sui muri veri (`FORK_TWIN`) |
| `scan_walk.py <s> <robotd.sock> <porta>` | quel percorso (di Peter Schade, PR 202) |

Il banco di `maploc` rigioca qualsiasi registrazione: `cargo run -p maploc
--release --features kinematics --example evaluate -- <rec.mdlg>
<verità.toml> <out>`.

Copia inglese canonica: `README.md`.
