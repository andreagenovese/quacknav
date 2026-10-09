# Installare quack-nav sulla papera

Questo pacchetto installa `quack-navd`, il demone di navigazione del
Microduck, sulla scheda della papera (Radxa Zero 3, Linux aarch64,
Debian 13) dal tuo computer, via ssh. Non servono né una copia del
repository né una compilazione. Copia inglese:
[README-install.md](README-install.md).

Cosa serve:

- una papera preparata da microduck: robotd e tofd in funzione, e il
  gruppo `robot` a cui appartengono i loro socket;
- accesso ssh alla papera con un account che ha `sudo` (`microduck`
  sull'immagine della scheda; le immagini più vecchie avevano `radxa`);
- sul tuo computer: `bash`, `ssh`, `scp`, `tar` e `shasum` o `sha256sum`
  (macOS e Linux li hanno tutti).

## 1. Scaricare e verificare

Da <https://github.com/andreagenovese/quacknav/releases>, il pacchetto e
il suo checksum. Questo pacchetto è `@VERSION@`; per uno più nuovo si mette
in `V` la sua versione (il tag senza la `v`):

```sh
V=@VERSION@
gh release download "v$V" --repo andreagenovese/quacknav \
    --pattern "quack-nav-$V-aarch64-linux.tar.gz*"
# oppure, senza gh:
curl -LO "https://github.com/andreagenovese/quacknav/releases/download/v$V/quack-nav-$V-aarch64-linux.tar.gz"
curl -LO "https://github.com/andreagenovese/quacknav/releases/download/v$V/quack-nav-$V-aarch64-linux.tar.gz.sha256"

shasum -a 256 -c "quack-nav-$V-aarch64-linux.tar.gz.sha256"   # oppure sha256sum -c
```

Deve stampare `OK`. Poi si scompatta:

```sh
tar xzf "quack-nav-$V-aarch64-linux.tar.gz"
cd "quack-nav-$V"
```

## 2. Installare

```sh
./install-on-duck.sh --dry-run microduck@192.168.1.42   # facoltativo: stampa cosa farebbe
./install-on-duck.sh microduck@192.168.1.42
```

Copia i file sulla papera, poi lì con `sudo`:

| sulla papera | da questo pacchetto |
|---|---|
| `/usr/local/bin/quack-navd` | `bin/quack-navd` |
| `/etc/systemd/system/quack-navd.service` | `systemd/quack-navd.service` |
| `/etc/sysusers.d/quack-nav.conf` (utente `quacknav`) | `systemd/sysusers.d/quack-nav.conf` |
| `/etc/robot/quack-nav.toml` — **solo se non c'è** | `quack-nav.example.toml` |
| `/var/lib/quack-nav/pilots/alpha/pilot.json`, `…/velstand/pilot.json` | `pilots/` (usati solo se attivati, sotto) |

Un vecchio `/var/lib/quacksat/places.json` (quack-nav 0.1.0) viene copiato
in `/var/lib/quack-nav/` se lì non ce n'è uno. Poi abilita e riavvia il
servizio, stampando ogni comando. `SSH_OPTS="-p 2222"` passa opzioni a ssh
e scp.

## 3. La configurazione: /etc/robot/quack-nav.toml

L'esempio va bene così com'è su una papera standard. Si modifica sulla
papera (`sudo nano /etc/robot/quack-nav.toml`) quando qualcosa è diverso:

| chiave | predefinito | che cos'è |
|---|---|---|
| `socket` | `/run/quack-nav/nav.sock` | dove ascolta quack-navd (quacksat, quack-control) |
| `robotd_socket` | `/run/robotd.sock` | il socket di robotd |
| `[map] tof_socket` | `/run/tofd/tof.sock` | il flusso di profondità di tofd (guardia dei dislivelli, mappatore) |
| `[map] places_path` | `/var/lib/quack-nav/places.json` | i luoghi con un nome |
| `[map] cliff_guard` | `true` | rifiuta i passi verso un dislivello che il sensore vede |
| `[maploc] enabled` | `true` nell'esempio | quack-navd costruisce la mappa da sé (spento: la mappa di robotd) |
| `[maploc] mode` | `"stop_and_scan"` | **mappatura**: la mappa cresce a ogni sosta. Mettere `"localize"` quando la casa è mappata: la mappa resta com'è salvata, la posa si corregge su di essa |
| `[maploc] map_path` | `/var/lib/quack-nav/maploc.session` | la sessione di lavoro; le mappe con un nome stanno in `maps/` accanto |
| `[homecoming] enabled` | `true` nell'esempio | all'avvio, riconosce una casa già mappata e ne riprende la mappa |
| `[homecoming] resume_explore` | `true` nell'esempio | dopo ogni ricarica, continua a esplorare finché la casa è finita |
| `[gait] profile` | `"velstand"` | **la policy di camminata che usa robotd**: `"velstand"` (il default di Pollen) o `"alpha"` se avete caricato `alpha_walking` + `alpha_stand` (`robotctl policy load walk alpha_walking.onnx`, `robotctl policy load stand alpha_stand.onnx`). Quella sbagliata fa sbagliare alla papera i propri passi |

**Il pilota (facoltativo).** Un modello neurale di navigazione per
camminata è installato in `/var/lib/quack-nav/pilots/`. Per usarlo,
aggiungete la riga `QK_RL_POLICY=/var/lib/quack-nav/pilots` a
`/var/lib/quack-nav/knobs.env` (o impostatela dalla pagina di
quack-control) e `sudo systemctl restart quack-navd`; quack-navd prende il
pilota di `[gait] profile`. Validato solo sui gemelli: su una papera vera
va prima tarato (docs/rl-pilot.it.md).

I percorsi devono restare sotto `/var/lib/quack-nav/` (l'unica cartella in
cui il servizio può scrivere) o `/run/quack-nav/`. Ogni chiave omessa
prende il suo valore predefinito; una chiave sconosciuta ferma il demone
con un messaggio che la nomina. Dopo una modifica:

```sh
sudo systemctl restart quack-navd
```

## 4. Controllare

Sulla papera:

```sh
systemctl status quack-navd
journalctl -u quack-navd -f
# da un utente del gruppo `robot` (o con sudo)
printf '{"jsonrpc":"2.0","id":1,"method":"nav.call","params":{"name":"robot.where_am_i","args":{}}}\n' \
    | nc -U -q1 /run/quack-nav/nav.sock
```

`/run/quack-nav/nav.sock` e `map.sock` esistono, modo 0660, gruppo
`robot`. Senza robotd o tofd il demone parte lo stesso e li aspetta.

## 5. Aggiornare

Si scarica il pacchetto più nuovo, si verifica, si scompatta e si lancia
il suo `./install-on-duck.sh` allo stesso modo. Il binario, la unit e
l'account vengono sostituiti; **se la papera cammina con alpha e la
configurazione non ha una sezione `[gait]`, aggiungete `profile = "alpha"`**
(se manca, ora vuol dire velstand). Il tuo `/etc/robot/quack-nav.toml` e le
mappe e i luoghi in `/var/lib/quack-nav/` restano (fermare il servizio
salva prima la sessione di mappatura).

## 6. Disinstallare

Sulla papera:

```sh
sudo systemctl disable --now quack-navd
sudo rm /usr/local/bin/quack-navd /etc/systemd/system/quack-navd.service /etc/sysusers.d/quack-nav.conf
sudo systemctl daemon-reload
# tenuti di proposito: /etc/robot/quack-nav.toml e /var/lib/quack-nav/ (le
# mappe e i luoghi). Si tolgono, con `sudo userdel quacknav`, solo per
# dimenticare la casa.
```

Altro: il README del progetto, <https://github.com/andreagenovese/quacknav>.
