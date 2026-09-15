# maploc: la matematica, letta per correttezza

2026-09-15. Una revisione delle formule e degli algoritmi di `maploc`
(PR 202 di Pollen, il nostro ramo `maploc-study` nel worktree), modulo per
modulo, chiesta dall'utente dopo il lavoro sui risvegli: *la matematica e
gli algoritmi dello SLAM sono giusti?* Copia inglese (canonica):
`maploc-math-review.md`.

La risposta breve: **la geometria è giusta ovunque sia stata controllata**
— composizione e inversa in SE(2), gli jacobiani e le equazioni normali del
matcher, la trasformata di distanza, la linearizzazione del grafo di pose,
la contabilità dei sistemi di riferimento nelle chiusure d'anello, il
modello ad ancora dell'odometria. Ciò che *non* è giusto non è una formula
ma un **modello**: il giudice che dice "questa posa concorda con la mappa"
è cieco in tre modi che una formula non può vedere, e ogni alias
inseguito per una settimana vive in quella cecità. Le scoperte 1–3 sono
quelle; 4–6 sono minori.

## Verificato corretto

| modulo | cosa è stato controllato | verdetto |
|---|---|---|
| `pose_graph.rs` | `compose`, `inverse` (−Rᵀt, −θ), `between` (Rₐᵀ(t_b−t_a), θ_b−θ_a), `wrap_pi` | corretto; andata-ritorno testato |
| `pipeline.rs::observe_odom` | delta odometrico preso nel sistema del corpo *precedente* e riapplicato nel sistema tracciato | dead reckoning SE(2) corretto |
| `scan_matcher.rs` | Gauss-Newton alla Hector sul campo di distanza: d e ∇d bilineari ai centri cella (l'offset −0,5 combacia col floor di `world_to_cell`), ∂e/∂θ = R′(θ)b, H = JᵀJ, g = Jᵀr, passo −H⁻¹g, prior come (1/σ²) sulla diagonale e (x−p)/σ² sul gradiente, residuo riportato alla posa *finale* | corretto |
| `grid.rs` | log-odds ±0,85/−0,40 con clamp ±4; Bresenham libero/colpito; inviluppo di parabole 1-D di Felzenszwalb con il salto dei +∞, righe poi colonne, √ poi ×cella | corretto |
| `submap.rs` | scansione nel sistema del corpo con origini per raggio; integrazione via `world_to_local(anchor, body)`; scansione grezza tenuta solo se ha toccato la griglia | corretto |
| `accumulator.rs` | voto per cella mondo su frame *distinti*, vicinato 3×3 per perdonare le linee del reticolo, superstiti riespressi nel sistema del frame mediano | corretto |
| `optimizer.rs` | jacobiani di `between`: J_a = [[−c, −s, pred.y], [s, −c, −pred.x], [0, 0, −1]], J_b = [[c, s, 0], [−s, c, 0], [0, 0, 1]] — verificati a mano; Huber come peso IRLS δ/e su Ω; nodi fissi bloccati azzerando righe/colonne con diagonale unitaria; H Δ = −b | corretto |
| `loop_closer.rs` | scansione nel sistema locale della sottomappa vecchia via `between(older_anchor, new_anchor ∘ pose_in_submap)`, match lì, ancora nuova implicata = `older ∘ result ∘ pose_in_submap⁻¹`, consenso per media xy e media circolare dello yaw, arco = `between(older, corrected_new)` | corretto |
| `mapper.rs::tracking_correction` (nostra) | autodecomposizione 2×2 dell'hessiana traslazionale, autovettore debole (b, λ−a), proiezione della correzione fuori da esso, soglia di rigidità in yaw, delta nel corpo = Rᵀ(posa)·d | corretto |
| `odometry/src/lib.rs` | modello a piede-ancora: lo spigolo di suola più basso tiene la sua xy mondo, tronco = ancora − R·(tronco→contatto); cambio dopo N tick di conferma; yaw dritto dal quaternione IMU | corretto (lo yaw IMU deriva sull'hardware; è del sensore, non della formula) |

## Scoperta 1 — il giudice non guarda lungo il raggio

`relocalize::score_pose` — la funzione dietro il cane da guardia, ogni
conferma di candidato (`check_candidate`), il punteggio delle ipotesi e
`unique_at` — valuta una posa con **la distanza media dall'estremo di
ogni raggio al muro più vicino**, sulle celle che la mappa ha osservato.
Non chiede mai se il raggio sia *passato attraverso* un muro per
arrivarci. Una posa che mette il sensore nella stanza accanto, con ogni
raggio che attraversa il muro comune per finire sul muro lontano della
stanza in cui è davvero, fa lo stesso punteggio della verità. Questa è
l'immagine speculare, la camera al posto dell'ufficio, il gemello a 180°
del corridoio: nessuno di loro contraddice un test sugli estremi.

La ricerca (`score_offsets`) e il filtro a particelle un test di
trasparenza ce l'hanno — ma solo **a metà raggio**, un campione, e solo
contro un muro sicuro (`see_through_fp` 300, tre colpi netti). Quindi un
candidato è *nominato* da un giudice con un occhio e *confermato* da uno
senza. Un muro attraversato a un terzo del raggio è invisibile a
entrambi.

**Correzione, motivata matematicamente:** valutare un raggio per la
distanza dell'estremo *e* per la coerenza lungo il raggio — percorrerlo a
qualche frazione (¼, ½, ¾) e, se un campione cade in una cella di muro
sicuro che non è il muro dell'estremo stesso (l'esenzione di sfioramento
che la ricerca già ha), contare il raggio come contraddizione al clamp.
Stessa regola in `score_pose`, `score_offsets` e nell'MCL, così nomina e
conferma concordano su cosa sia un match.

## Scoperta 2 — i raggi fuori mappa sono perdonati da un giudice e addebitati dall'altro

`score_offsets` e l'MCL valutano un estremo che esce dalla griglia al
clamp pieno ("saltarlo lasciava competere su un sottoinsieme scelto le
pose che buttano raggi fuori mappa" — loro commento). `score_pose` lo
*salta*. Per il cane da guardia è giusto: esplorare oltre la mappa
renderizzata non deve sembrare un rapimento. Per un candidato su una
mappa salvata è sbagliato: una posa al bordo della casa che butta metà
dei raggi nel nulla è giudicata sulla metà che combacia. Il giudice deve
sapere a quale domanda risponde; oggi risponde a entrambe nello stesso
modo indulgente.

## Scoperta 3 — lo spazio libero viene buttato via

`kinematics::tof::flatten` tiene solo i raggi `Zone::Hit`. Un ritorno di
pavimento — un raggio che ha raggiunto il pavimento a 0,8 m — dimostra
0,8 m di pavimento libero e viene **scartato**. Le celle libere della
mappa vengono solo dal tratto libero dei raggi che colpiscono muri,
quindi il centro di una stanza resta *ignoto* finché un raggio di muro
non capita a spazzarlo. Da qui i rifiuti dell'esploratore "solo 0,08 m di
pavimento noto e poi spazio non mappato", il costo delle frontiere
com'è, e parte del vagare dei viaggi: la papera non conosce il pavimento
che ha già visto. La correzione è piccola nella forma — un raggio con
`hit: false` integrato solo come celle libere (`integrate_ray` accetta
già `hit_is_occupied`) — e tocca `Scan`, `flatten`, l'accumulatore (i
raggi liberi non votano) e il matcher (i raggi liberi non combaciano).
Da fare prima sul banco: più spazio libero cambia anche il quadro delle
frontiere su cui l'esploratore pianifica.

## Scoperta 4 — gli archi odometrici hanno una sola confidenza qualunque sia la lunghezza

`pipeline.rs` incatena i nodi delle sottomappe con `information_from_sigmas(odom_sigma_xy, odom_sigma_yaw)`, uguale per una sottomappa aperta a 0,5 m dalla precedente e per una a 3 m. L'errore odometrico cresce con il percorso; una σ per arco proporzionale alla distanza percorsa (σ₀·√d o σ₀·d) è il modello standard e lascerebbe alle chiusure d'anello piegare le catene lunghe più delle corte. Non è un baco — la regola di percorso delle sottomappe limita l'intervallo — ma è parte del perché "le chiusure d'anello sul rumore della mappa portano via la posa" (`upstream-asks.md` §1): a ogni arco viene detta la stessa cosa.

## Scoperta 5 — la composita della finestra ferma sovrappesa il muro vicino

L'accumulatore fonde *ogni raggio superstite di ogni frame* nella composita (una fermata da 100 frame porta lo stesso muro 100 volte), poi `integrate_scan_weighted` la inchiostra `passes` volte in più. I log-odds saturano a ±4 dopo cinque colpi, quindi la mappa in sé non ne soffre; ma ogni residuo calcolato sulla composita (`score_pose`, il matcher, il chiusore d'anelli) è una media sui raggi, e 100 copie del muro verso cui la testa puntava dominano 5 copie dell'angolo che ha sfiorato. Una de-duplicazione per cella prima del punteggio (un raggio per cella d'estremo, o peso 1/conteggio) farebbe dire al residuo ciò che dice.

## Scoperta 6 — cose piccole

- `score_pose` restituisce `mean_residual_m: 0.0` quando `n_observed == 0`. Ogni chiamante controlla prima `n_observed`, quindi oggi è innocuo; un `NaN` o `INFINITY` sarebbe onesto.
- `tracking_correction` decima a 512 raggi e `loop_closer` a `max_probe_beams`, entrambi a passo; su una composita ordinata frame per frame un passo sceglie frame interi, non una distribuzione di direzioni. Il passo uniforme su una lista ordinata per frame va bene solo perché i frame si sovrappongono; una decimazione ordinata per direzione sarebbe strettamente meglio.
- `align.rs` eredita le scoperte 1–2 tramite `relocalize_against_grid`: la risposta mappa-su-mappa che ha incastrato l'ufficio nella camera non aveva nemmeno lei il test sul raggio. La sua penalità pavimento-su-muro (`wall_penalty`) è la stessa idea dall'altro lato ed è ciò che ha messo la verità al primo posto sul banco; il test sul raggio è il suo complemento.

## Cosa fare, in ordine

1. Scoperta 1, il test sul raggio nell'unico giudice (`score_pose`), misurato sulle registrazioni del banco (i rapimenti e le rilocalizzazioni al boot) e sulla tornata dei risvegli: un'immagine speculare che attraversa un muro deve ora fallire.
2. Scoperta 2 insieme, come flag su `score_pose` (`off_map_counts`), vera per i candidati su una mappa salvata, falsa per il cane da guardia.
3. Scoperta 3, i raggi di spazio libero, prima sul banco (qualità della mappa, numero di frontiere), poi i viaggi.
4. Scoperte 4–5 come manopole, misurate sul banco di replay come ogni altra modifica.
