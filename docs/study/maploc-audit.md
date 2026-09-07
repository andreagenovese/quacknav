# maploc — audit of the pose losses (2026-09-06)

Companion to `maploc-dataflow.md` (what enters and leaves the worker).
This note answers two questions the twin runs raised: *what does
maploc actually do with the pose*, and *why does it lose it*. Everything
was measured offline on robotd's own `.mdlg` recordings, replayed
through the `evaluate` bench (the same `Mapper` the robot runs, so a
replay reproduces the live decisions byte for byte), in the PR 202
worktree on branch `maploc-study`. Bench scripts and the patch live in
`private/drives/maploc-bench/`.

## 1. What maploc does with the pose

- **Between loop closures the tracked pose is dead reckoning.** Contact
  odometry deltas are composed onto it at 50 Hz and nothing else touches
  it. The Hector scan matcher in the crate is used only by the loop
  closer, submap against submap; no window is ever matched against the
  map to correct the pose. The MCL module is not wired in at all.
- **Every still window is judged, never used.** The composite of a stop
  is scored against the map as it stood when the stop began; a window
  the map can judge (≥ 100 beams, ≥ 5 %) and contradicts (mean residual
  > 0.25 m) is quarantined, two in a row flip the mapper to *lost*.
- **Loop closure runs when a submap freezes** (8 s and ≥ 0.15 m of
  travel, or 0.8 m). Candidates: older submaps within 1.5 m and at least
  three back. Two stored composites from the new submap are matched to
  the old grid (coarse ±0.5 m/±20°, then Gauss-Newton); gates: residual
  ≤ 0.10 m, coverage ≥ 40 %, witnesses agreeing within 0.12 m/5°,
  correction ≥ 0.04 m and ≤ 0.06 + 0.08 m per submap of gap (cap
  0.6 m). Accepted edges (σ 0.05 m, four times stiffer than an odometry
  edge) relax the whole graph and move the tracked pose.
- **Lost means a brute-force search** over every free cell and 36
  yaws of the global render; the next window must confirm the winner
  (≤ 0.10 m over ≥ 30 % of its beams). There is no uniqueness test.

## 2. What the recordings say

**The twin's odometry is almost truth.** On the 21-minute human drive
(`drive-human-1.jsonl` against `1788604159.mdlg`), raw contact
odometry stayed within 0.13 m and 3° of MuJoCo's ground truth over 41 m
of walking, distance within 2–3 % per 30 s window. The IMU yaw is exact
in the simulator. Odometry only degrades after a fall (run
`1788640409`: agreement with the true walls jumps from 0.014 to 0.18 m
right after `FELL` at 336 s).

**The tracked pose walks away from the odometry at loop closures.** Run
49 (`1788627740`, the "live maploc drift, third case"): 114 closures in
26 minutes; after the first three the pose is 0.25 m and 14° off the
odometry, 0.33–0.56 m off from minute 5 on, never recovered. The
corrections are 2–10 cm (map noise: the floor is 4 cm, below the
grid's own 5–9 cm noise) with occasional 0.3–0.47 m aliases along
walls. Both witnesses come from the same stand, so the consensus gate
does not catch them.

**The replay never contradicted the live run.** The earlier "replay
tracked within 0.49 m" reading was a clamp: the bench's `vs-TRUTH`
number is the mean distance of the composite's endpoints to the true
walls, clamped at 0.5 m, and it cannot see a slide along a wall. The
bench now also prints the composite scored at the raw-odometry pose and
the tracked-versus-odometry distance.

**Lost + relocalize is where the metres come from.** Run 56
(`1788644274`): lost at 1256 s and 1353 s, relocalized 3 m away with
residual 0.007 (a symmetric flat aliases through a 150° keyhole), then
tracked there. The upstream watchdog has one remedy for inconsistency,
and it is the global search.

## 3. Bench matrix

Five recordings (four explorer runs, the human drive capped at 2000 s
where the clean drive ends). `walls` = mean distance of inked wall cells
to the true walls; `lost` = tracking-lost events; the human column also
gives the tracked pose's mean/max error against truth. Improved
configurations ink *more* wall cells than upstream (1000–1300 vs 800),
so they do not win by mapping less.

| configuration | run 49 | run 56 | 1788642355 | 1788640409 (fall) | human drive |
|---|---|---|---|---|---|
| A upstream defaults | 0.156 | 0.267 · 3 lost | 0.163 · 1 lost | 0.052 · 1 lost | 0.078 · pose 0.06/0.31 m |
| B odometry only (no closures) | 0.074 | 0.178 | 0.059 · 3 lost | 0.139 | 0.061 · pose 0 |
| C closures with a tight allowance (0.03 m/submap, cap 0.3) | 0.137 | 0.086 | 0.087 | 0.045 | 0.061 · pose 0.06/0.49 |
| D = C + scan-to-map tracking correction | 0.075 | 0.068 | 0.072 | 0.050 | 0.093 · pose 0.06/0.31 |
| E = C + conservative correction (floor 0.10 m, cap 0.15, halve residual) | 0.177 | 0.062 | 0.109 · 3 lost | 0.039 | 0.062 · pose 0.06/0.41 |
| F no closures + tracking correction | 0.045 | 0.059 | 0.062 | 0.060 · 1 lost | 0.123 · pose 0.08/0.58 |
| G no closures + conservative correction | 0.063 | 0.125 | 0.059 · 3 lost | 0.093 | 0.058 · pose 0.00/0.11 |

Reading: on a gentle drive upstream is already fine (6 cm mean pose
error). The explorer breaks it: panoramas spin on the spot, submaps
freeze every 8 s at the same place, dozens of same-stand closures
inject map noise into a near-perfect odometry. A tight allowance alone
removes every lost event and improves the map on all five recordings;
adding the tracking correction helps the explorer runs and hurts the
human drive, because on the twin the map's noise is larger than the
odometry's error. No single setting wins everywhere; the twin
over-flatters odometry, so December decides the final gains.

## 4. What was changed in the worktree (branch `maploc-study`)

- `scan_matcher.rs`: `ScanMatchResult` carries the normal matrix at the
  final pose (how well the scene constrains x, y, yaw).
- `mapper.rs`: `TrackingConfig` and a scan-to-map correction at every
  vetted window — matched against the pre-stand map, regularized at the
  tracked pose, projected off unconstrained eigen-directions, bounded,
  gated on coverage, improvement and a map-noise floor. **Off by
  default**; `Note::TrackingCorrected` when it fires.
- `examples/evaluate.rs`: env knobs for the loop closer, the correction
  and a time cap; per-window odometry pose and truth agreement; a
  tracked-versus-odometry summary.
- `robotd/src/maploc.rs`: the log line for the new note.

## 5. What to do with it

1. Report to Pollen with the numbers above (the recordings are theirs
   to replay): closures at map-noise level, same-stand witnesses, an
   allowance ten times the twin's drift, and no uniqueness test on
   relocalize.
2. For our twin runs, rebuild robotd from `maploc-study` with the tight
   allowance as the default (`max_correction_per_submap_m 0.03`, cap
   0.3) and re-run the two regressions; expect zero "tracking lost" and
   the explorer's map-versus-sensor guard to fire far less.
3. Keep the tracking correction opt-in until the real duck's odometry
   is measured in December; on hardware, where odometry will be worse
   than the map, it is the piece that keeps drift from ever reaching
   the watchdog.
