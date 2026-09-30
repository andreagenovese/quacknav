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
```

I numeri qui sono stati misurati con il set alpha; Pollen pubblica le
policy di serie sull'Hub di Hugging Face.

Il viewer disegna la mappa, la rotta, i raggi del sensore di profondità
e la corsia della guardia (`viewer/`: l'overlay `sim-maploc` della PR 202,
di Peter Schade, con le aggiunte del fork e `QUACK_NAV_SOCKET`);
`VIEWER=off` fa girare il body server semplice e mostra solo l'anatra,
`VIEWER_DIR` punta a un'altra copia.

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
salvata; `RESUME=on` è l'esplorazione progressiva (a casa su una mappa
ancora in esplorazione, salvata a fine sessione), `EXPLORE_S` il budget
dell'esplorazione che l'homecoming avvia (720) e `BOOT_SEARCH_S` quanto
resta fermo a confermare prima di cercare (240). `PORT` è la porta del
simulatore (7872), `SCENE` la scena MuJoCo (default l'appartamento).
`ASK_PHRASE` è ciò che l'esploratore chiede in una zona senza
nome (default "Qui dove siamo?"). Per parlarci, puntare `[nav] socket` di un satellite vocale su
`$STATE/nav.sock` e il suo `robotd_socket` su `$STATE/robotd.sock`.

## Misurare

| script | a cosa risponde |
|---|---|
| `probe.py <robotd.sock> <tof.sock>` | cosa riceve un mapper fuori da robotd: frequenze, campi, i due orologi |
| `spin.py <robotd.sock>` | quanto gira la rotazione del panorama (22–24°/s con entrambi i robotd) |
| `headwatch.py <robotd.sock> <s> [sway]` | chi ha la testa; con `sway`, una posa pensante a cui la scansione deve cedere |
| `turnprobe.py <robotd.sock> <porta> <etichetta> [ripetizioni]` | rotazione da fermo: nulla sotto ~1,2 rad/s, 30–60°/s sopra |
| `segs.py <file.mdlg> [da] [a]` | una registrazione come tratti in movimento e fermi — come si è trovato l'avvio lento |
| `ab_round.sh <n>` | lo stesso percorso sul fork e qui, entrambi valutati sui muri veri (`FORK_TWIN`) |
| `scan_walk.py <s> <robotd.sock> <porta>` | quel percorso (di Peter Schade, PR 202) |

Il banco di `maploc` rigioca qualsiasi registrazione: `cargo run -p maploc
--release --features kinematics --example evaluate -- <rec.mdlg>
<verità.toml> <out>`. Accanto, con le stesse feature:

- `trajectory -- <sessione.mdlg> <pose.tsv> <out.tsv>`: la posa rigiocata
  contro la verità, per `traj_metrics.py`; `MAP_SESSION` /
  `MAP_LOAD_AT_S` rigiocano nella mappa salvata su cui la sessione è
  ripartita, `SAVE_SESSION` salva la mappa costruita; `CORR_LOG`,
  `ODOM_LOG`, `LOOP_LOG` (ogni chiusura con il suo errore d'angolo contro
  la verità) e `TRACK_LOG` scrivono log per evento; `ODOM_SIGMA_XY/YAW` e
  `LOOP_SIGMA_XY/YAW` sostituiscono le sigma del grafo.
- `wake_match -- <sessione.mdlg> <pose.tsv> <start_s> <mappa.session>...`:
  la domanda mappa-contro-mappa dell'homecoming posta offline, ogni
  risposta giudicata contro la verità (`ASK_EVERY_S`, `ASK_FOR_S`).
- `drop_replay` di quack-nav, `-- <sessione.mdlg> <pose.tsv> <truth.json>
  "x,y;..."` (`cargo run -p quack-nav --release --example drop_replay`):
  dove cadono i drop di una registrazione secondo la posa rigiocata e
  secondo la verità; `BOOK=` per la modalità libro.

`MAPLOC_SHADOW=0` (di maploc, dal vivo e sul banco) spegne la mappa ombra;
`MAPLOC_SHADOW_EVERY_S` (30) e `MAPLOC_SHADOW_ASKS` (2) la regolano.

Copia inglese canonica: `README.md`.

## Le case di prova e il protocollo di release

`houses/` contiene ciò con cui è stato misurato `docs/results.it.md`:

| file | cos'è |
|---|---|
| `gen.py <robot dir> <out> [casa ...]` | scrive le case indicate (casa_libera e casa_arredata di default; anche casa_grande, 9 x 7 m, un corridoio che gira di 90°, niente che blocchi): le scene MuJoCo (nella cartella robot di microduck_rl), la verità di maploc (`.toml`), il mondo del gemello di carta (`.world.json`) e buche, stanze e mete (`.truth.json`) |
| `final_house.py <nome> <scena> <state> <porta> <truth> <out> [session_s] [sessioni] [giri]` | il protocollo di release su una casa: esplorazione progressiva da zero, "esplorazione completata" se la papera non ha finito, tre riavvii con un giro di go_to sulla mappa congelata, e lo stesso con la build di `main` (`AB_REPO`, un worktree di main con la sua release compilata). `ROUNDS_ONLY=1` parte dai giri, dalla mappa e dal libro lasciati dall'esplorazione in `<out>` |
| `aggregate.py` | le tabelle di `docs/results.it.md` dagli output del protocollo |
| `modes_test.py` | ripresa, "a che punto sei", completata, la mappa congelata dopo un riavvio, una mappa nuova che sostituisce la vecchia solo quando salva |
| `run_house.py`, `prog_house.py` | le versioni con un'esplorazione sola e con le sole sessioni |
| `poseerr.py <nav.sock> <porta> <out.tsv>` | la posa della mappa contro la verità del simulatore ogni 5 s (`POSEERR_DT`; 0.5 per le chiusure di loop), `<out>.untracked` mentre il mapper non ne garantisce nessuna |
| `wake_bench.py <nome> <scena> <state> <porta> <truth> <out> <maps_dir> <book.json> [limit_s]` | risvegli da punti sparsi per la casa su una mappa salvata (la papera messa lì con `MICRODUCK_START`), ognuno giudicato giusto, sbagliato o mai contro la verità; `WAKE_TURN` gira ogni partenza, `WAKE_SPAWNS="x,y,yaw;..."` le sostituisce |
| `traj_metrics.py <pose.tsv>` | ATE e RPE da un file del campionatore (dal vivo o di `trajectory`), `--tum` per `evo` |
| `map_vs_truth.py <frame.json> <truth.toml> <truth.json> [book.json nome]` | una mappa esplorata contro la casa, stanza per stanza: muri giusti, ispessiti o fantasma, libero dentro i muri, facce, pavimento, allineamento, libro |
| `room_fit.py <frame.json> <truth.toml> <truth.json>` | lo scostamento rigido della mappa e di ogni stanza contro i muri veri |
| `oracle_book.py <truth.json> <nome> <libro in> <libro out>` | il libro dei drop dell'oracolo: i bordi dei buchi veri al posto di quelli registrati |
| `cut_mdlg.py <in.mdlg> <secondi> <out.mdlg> [pose.tsv out.truth.tsv]` | i primi secondi di una registrazione come fixture, con le sue righe di verità |

Servono `MICRODUCK`, `MICRODUCK_RL` e `POLICY_DIR` come per `twin.sh`,
`TWIN_WORK` per gli output (predefinito `/tmp/quack-twin-work`), `QN_REPO` per
il checkout di quack-nav di cui girano `quack-navd`, e `AB_REPO` per il
checkout con cui un giro si confronta (i giri `main` di `final_house.py`; saltati, e lo dice, se non è un checkout completo e compilato). Una casa
richiede circa quattro ore sul gemello; tre girano in parallelo su un Mac a 12
core con `VIEWER=off` su due di esse.
