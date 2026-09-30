# ADR 0009 — Fase due: i metodi dei manuali, sulla papera che abbiamo

Data: 2026-09-25. Stato: accettata.

## Contesto

La release preview (docs/results.it.md, 5 criteri su 7) è stata raggiunta
misurando, sui gemelli, ogni guasto che la papera mostrava e scrivendo una
regola contro di esso: il libro dei drop con i suoi raggi, il filo teso al
massimo 0.6 m, la mira centrata, i contatori di rifiuti, i punti da
evitare, il test della valle. Ogni regola è misurata, e ognuna è una toppa
su un guasto.

Guardati con la letteratura standard, gli strati non sono uguali. `maploc`
è già un progetto da manuale — un piccolo Cartographer: submap, uno scan
matcher Levenberg-Marquardt che restituisce la propria Hessiana, un pose
graph SE(2) rilassato da Gauss-Newton su matrici di informazione, un filtro
a particelle. E l'odometria di robotd fonde già quello che la papera ha: i
punti di contatto delle gambe per la posizione, l'angolo integrato
dell'IMU per la direzione. Le regole fatte a mano stanno sopra, in
quack-nav, dove il manuale ha un metodo per ciascuna:

| Regola di oggi | Il metodo che sostituisce |
|---|---|
| Il test della valle (far scivolare la posa di 0.30 m in 8 direzioni) | La degenerazione dalla Hessiana dello scan matcher (Zhang, Kaess e Singh 2016) |
| Una posa è un punto, confermato o no | Una posa con covarianza, propagata dall'odometria e aggiornata dalla scansione (EKF) |
| Il filo teso, `route_heading_anew`, le rotazioni fisse | Un controllore che segue il percorso: Regulated Pure Pursuit (Nav2) |
| La mira centrata, l'inflazione come muro | Una costmap a strati con un costo di inflazione che decresce |
| Il libro dei drop e i suoi raggi | Uno strato di occupazione per i drop, in log-odds, con un modello del sensore raggio per raggio |
| Le ipotesi e le corde della ricerca all'avvio | Augmented MCL (campionamento KLD, iniezione casuale), confronto globale branch-and-bound |
| Contatori di rifiuti, punti da evitare | Un behaviour tree con i recovery |
| Frontiera scelta per costo | Frontiera scelta per guadagno di informazione contro costo |

L'hardware resta quello che è: un ToF 8×8 di circa 45° e qualche metro,
due IMU, i contatti delle gambe, una telecamera e una NPU a bordo. I metodi
che vogliono una scansione a 360°, profondità densa o una GPU sono fuori.

## Decisione

**La fase due sostituisce regole con metodi, uno alla volta, e solo quando
i numeri lo dicono.** Ogni passo si misura come si è misurata la release —
gemello di carta, poi MuJoCo, poi un A/B contro `main` sui criteri di
docs/results.it.md — ed entra se li migliora, o se li eguaglia togliendo
regole fatte a mano. Un passo che non fa né l'una né l'altra cosa si
annulla, e la misura si scrive lo stesso.

L'ordine, ogni passo appoggiato sui precedenti:

0. **Prima le misure.** Nessun metodo si sostituisce senza un modo per
   mostrare che è migliore:
   - ATE e RPE nella forma standard (traiettorie TUM, compatibili con `evo`);
   - percorsi golden su scene fisse;
   - test su proprietà per le regole di sicurezza;
   - riproduzione deterministica delle registrazioni `.mdlg` come
     regressione;
   - CI a ogni push, gemello di carta compreso.
1. **Una posa con la sua incertezza.**
   - Una covarianza 3×3 sulla posa, fatta crescere dall'odometria tra una
     finestra e l'altra (rumore per metro percorso e per radiante girato) e
     ridotta da ogni scan match (Σ = σ²·H⁻¹).
   - La degenerazione dagli autovalori della Hessiana, misurata contro il
     test della valle prima di sostituirlo.
   - Una sessione ripresa non scrive nulla nella mappa finché covarianza e
     correzioni non si sono assestate: il controllo di coerenza che è
     mancato alla seconda sessione di casa_arredata.
2. Una costmap a strati e il Regulated Pure Pursuit.
3. I drop come strato di occupazione in log-odds.
4. La rilocalizzazione come Augmented MCL, con confronto globale
   branch-and-bound.
5. Un behaviour tree con i recovery.
6. L'esplorazione per guadagno di informazione.
7. La messa in servizio sulla papera: systemd, watchdog, arresto sicuro,
   rotazione dei log.
8. La telecamera, per ultima e con misura: un AprilTag sulla base di
   ricarica, poi il riconoscimento dei luoghi sulla NPU.

**Non nella fase due:** SLAM visivo completo o VIO (ORB-SLAM3, VINS). Su una
papera che cammina la telecamera trema a ogni passo, e la CPU di bordo
pagherebbe molto per ciò che i passi 1–4 danno a meno. Nemmeno ROS 2 viene
adottato: il JSON-RPC di robotd resta l'interfaccia, e un ponte ROS resta
qualcosa che chiunque può costruire sul socket.

## Conseguenze

Il lavoro vive sul branch `phase-2` finché un passo non risulta misurato
migliore. (2026-09-30: il lavoro di `phase-2`, passi 0 e 1, è su `main`. La
mappa ombra del 2026-09-29 — la mappa del cammino di una papera persa,
chiesta mappa-contro-mappa dove sta in quella salvata, senza filtro a
particelle — è un passo verso la rilocalizzazione globale del passo 4, non
l'MCL stessa.) docs/results.it.md acquista le colonne ATE e RPE dal passo 0,
così ogni passo successivo riporta nei numeri che pubblicano gli altri
sistemi. `map_status` acquista l'incertezza della posa con il passo 1, e
l'homecoming, la ripresa e l'esploratore possono chiedere quanto la papera
è sicura, non solo se lo è.

Le regole che un metodo sostituisce si tolgono, non si tengono accanto: due
meccanismi per una sola decisione sono il modo in cui l'esploratore ha
accumulato i suoi contatori. Dove una regola codifica qualcosa che il
metodo non sa — il bordo è a 0.15 m perché il passo da fermo spinge avanti
— diventa un parametro del metodo, con il nome di ciò che è stato misurato.
