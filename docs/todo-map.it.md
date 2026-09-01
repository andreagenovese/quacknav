# TODO — Mappa della casa e localizzazione

Stato: idea, non iniziata. Prerequisito: quacksat parla (core + backend).
Principio: **tutto fuori bordo**. L'anatra streamma camera, IMU, odometria e
ToF; il server ricostruisce, localizza e ragiona. A bordo solo intenti.

## 0. Studio preliminare
- [ ] Leggere `maploc` in `apirrone/microduck_runtime` (mapping, MCL,
      planning): cosa faceva, con quali sensori, perché è "unowned" nel nuovo
      stack. Decidere se portarlo o superarlo.
- [ ] Verificare in `mediad` cosa espone oggi: stream WebRTC, `get_frame`
      JPEG, risoluzione/fps camera, timestamp condivisi con IMU/odometria.
- [ ] Verificare formato e frequenza di `tof.stream` (8×8, 15 Hz) e di
      `odom` nello stream `robot.state`.
- [ ] Valutare i modelli di SLAM monoculare fondazionale: MASt3R-SLAM,
      VGGT-SLAM, SLAM3R, FoundationSLAM, OpenMonoGS-SLAM (geometria +
      semantica). Criteri: real-time su GPU consumer, licenza, stato del
      codice, scala metrica iniettabile.
- [ ] Valutare i grafi di scena open-vocabulary (ConceptGraphs, HOV-SG,
      Hydra) come rappresentazione leggibile dall'agente.
- [ ] ADR: architettura di mapping/localizzazione (sensori, modello,
      rappresentazione, dove gira cosa).

## 1. Infrastruttura
- [ ] Server con GPU sempre acceso (≥12 GB VRAM) — il MacBook non basta.
- [ ] Pipeline di registrazione: salvare video + IMU + odom + ToF sincronizzati
      di un giro dell'anatra (dataset per lavorare offline).
- [ ] Misurare banda Wi-Fi e latenza dello stream in movimento.

## 2. Fase A — Rilocalizzazione economica (sblocca subito l'agente)
- [ ] AprilTag sugli stipiti delle porte; rilevamento sul server dai frame.
- [ ] Mappa topologica: grafo stanze/passaggi in un file (YAML/JSON) con
      etichette in linguaggio naturale, arricchite dall'agente ("cucina:
      forno, frigo").
- [ ] Tool per l'agente: `where_am_i()`, `go_to(room)` → sequenza di intenti
      `robot.move`/`robot.head` con conferma visiva del tag.
- [ ] Comportamento "cerca il tag": rotazione + scansione ToF/camera.

## 3. Fase B — Geometria metrica offline
- [ ] Far girare MASt3R-SLAM (o alternativa scelta) sui video registrati.
- [ ] Iniettare la scala metrica da ToF/odometria; misurare la deriva.
- [ ] Capire cosa vede davvero una camera a 25 cm da terra (gambe di sedie,
      non tavoli): valutare keyframe da fermo (policy stand) per ridurre il
      motion blur.
- [ ] Persistenza della mappa e ricarica tra sessioni (batteria ~1 h).

## 4. Fase C — Localizzazione online
- [ ] Stream live → SLAM sul server → posa pubblicata (topic/RPC) verso
      l'agente e verso quacksat.
- [ ] Fusione con odometria di bordo per la stima tra un frame e l'altro.
- [ ] Rilocalizzazione a freddo: mappa persistita + AprilTag della Fase A.
- [ ] Evitamento ostacoli locale dal ToF (reattivo, fuori dal ciclo LLM);
      valutare oscillazione del collo per una scansione a ventaglio.

## 5. Fase D — Semantica e agente
- [ ] Grafo di scena 3D open-vocabulary: stanze, oggetti, relazioni,
      coordinate.
- [ ] Tool per [Arkimede](https://github.com/andreagenovese/yesSir): `where_is(object)`, `go_to(place)`, `look_at(x)`,
      `describe_surroundings()`; pianificazione waypoint sul server.
- [ ] Ciclo chiuso: obiettivo dall'agente → waypoint → intenti → posa
      aggiornata → verifica visiva → risposta all'utente.
- [ ] Missioni di prova: "vai in cucina e dimmi se il forno è acceso",
      "dov'è la palla", "torna alla base".

## Rischi e domande aperte
- Motion blur e inquadrature basse dell'andatura bipede.
- Deriva dell'odometria a contatto (nessun magnetometro): frame mondo =
  dove guardava il robot all'avvio.
- Costo GPU e consumo energetico di un server sempre acceso.
- Privacy: il video di casa gira sul server locale, mai fuori (vincolo
  "elaborazione locale").
- Base di ricarica: senza, ogni sessione parte da una posa sconosciuta.
- Più anatre (chorale!): mappa condivisa e localizzazione multi-robot.
