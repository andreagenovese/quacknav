# maploc: the mathematics, read for correctness

2026-09-15. A review of the formulas and algorithms in `maploc` (Pollen
PR 202, our worktree branch `maploc-study`), module by module, asked for
by the user after the wake-up work: *are the SLAM's maths and algorithms
right?* Italian copy: `maploc-math-review.it.md`.

The short answer: **the geometry is right everywhere it was checked** —
SE(2) composition and inverse, the scan-matcher's Jacobians and normal
equations, the distance transform, the pose-graph linearisation, the
loop-closure frame bookkeeping, the odometry's anchor model. What is
*not* right is not a formula but a **model**: the judge that says "this
pose agrees with the map" is blind in three ways that a formula cannot
see, and every alias we chased for a week lives in that blindness.
Findings 1–3 below are those; 4–6 are smaller.

## Verified correct

| module | what was checked | verdict |
|---|---|---|
| `pose_graph.rs` | `compose`, `inverse` (−Rᵀt, −θ), `between` (Rₐᵀ(t_b−t_a), θ_b−θ_a), `wrap_pi` | correct; round-trip tested |
| `pipeline.rs::observe_odom` | odometry delta taken in the *previous* body frame and re-applied in the tracked frame | correct SE(2) dead reckoning |
| `scan_matcher.rs` | Hector-style GN on the distance field: bilinear d and ∇d at cell centres (the −0.5 offset matches `world_to_cell`'s floor), ∂e/∂θ = R′(θ)b, H = JᵀJ, g = Jᵀr, step −H⁻¹g, prior as (1/σ²) on the diagonal and (x−p)/σ² on the gradient, residual reported at the *final* pose | correct |
| `grid.rs` | log-odds ±0.85/−0.40 clamped at ±4; Bresenham free/hit; Felzenszwalb 1-D parabola envelope with the +∞ skip, applied rows then columns, √ then ×cell | correct |
| `submap.rs` | scan in the body frame with per-beam origins; integration through `world_to_local(anchor, body)`; a raw scan kept only if it touched the grid | correct |
| `accumulator.rs` | vote per world cell across *distinct* frames, 3×3 neighbourhood to forgive lattice lines, survivors re-expressed in the median frame's body frame | correct |
| `optimizer.rs` | Jacobians of `between`: J_a = [[−c, −s, pred.y], [s, −c, −pred.x], [0, 0, −1]], J_b = [[c, s, 0], [−s, c, 0], [0, 0, 1]] — checked by hand; Huber as an IRLS weight δ/e on Ω; fixed nodes pinned by zeroing rows/columns with a unit diagonal; H Δ = −b | correct |
| `loop_closer.rs` | scan in the older submap's local frame via `between(older_anchor, new_anchor ∘ pose_in_submap)`, match there, implied new anchor = `older ∘ result ∘ pose_in_submap⁻¹`, consensus by mean xy and circular-mean yaw, edge = `between(older, corrected_new)` | correct |
| `mapper.rs::tracking_correction` (ours) | 2×2 eigen-decomposition of the translational Hessian, weak eigenvector (b, λ−a), projection of the correction off it, yaw stiffness gate, body-frame delta = Rᵀ(pose)·d | correct |
| `odometry/src/lib.rs` | anchor-foot model: the lowest sole corner holds its world xy, trunk = anchor − R·(trunk→contact); switch after N confirming ticks; yaw straight from the IMU quaternion | correct (the IMU yaw drifts on hardware; that is the sensor's, not the formula's) |

## Finding 1 — the judge does not look along the beam

`relocalize::score_pose` — the function behind the watchdog, every
candidate confirmation (`check_candidate`), the hypothesis scoring and
`unique_at` — scores a pose by **the mean distance from each beam's
endpoint to the nearest wall**, over cells the map has observed. It
never asks whether the beam *passed through* a wall on its way there.
A pose that puts the sensor in the next room, with every beam crossing
the shared wall to land on the far wall of the room it is actually in,
scores as well as the truth. That is the mirror image, the bedroom for
the office, the corridor's 180° twin: none of them contradicts an
endpoint test.

The search (`score_offsets`) and the particle filter do have a
see-through test — but only **at the ray's midpoint**, one sample, and
only against a confident wall (`see_through_fp` 300, three net hits).
So a candidate is *nominated* by a judge with one eye and *confirmed*
by a judge with none. A wall crossed at a third of the ray is invisible
to both.

**Fix, mathematically motivated:** score a beam by endpoint distance
*and* by ray consistency — walk the ray at a few fractions (¼, ½, ¾)
and, if any sample lands in a confident wall cell that is not the
endpoint's own wall (the graze exemption the search already has), count
the beam as a contradiction at the clamp. Apply the same rule in
`score_pose`, `score_offsets` and the MCL so nomination and confirmation
agree on what a match is.

## Finding 2 — off-map beams are forgiven by one judge, charged by the other

`score_offsets` and the MCL score an endpoint that leaves the grid at
the full clamp ("skipping it let poses that throw beams off the map
compete on a cherry-picked subset" — their own comment). `score_pose`
*skips* it. For the watchdog that is right: exploring past the rendered
map must not read as a kidnap. For a candidate on a saved map it is
wrong: a pose at the house's edge that throws half its beams into
nothing is judged on the half that fits. The judge needs to know which
question it is answering; today it answers both the same lenient way.

## Finding 3 — free space is thrown away

`kinematics::tof::flatten` keeps only `Zone::Hit` beams. A floor return
— a beam that reached the floor at 0.8 m — proves 0.8 m of free floor
and is **dropped**. The map's free cells come only from the free stretch
of wall-hit rays, so the middle of a room stays *unknown* until a wall
beam happens to sweep it. This is where the explorer's "only 0.08 m of
known floor ahead and then unmapped space" refusals come from, why the
frontier cost is what it is, and part of why journeys wander: the duck
does not know the floor it has already seen. The fix is small in shape
— a beam with `hit: false` integrated as free cells only
(`integrate_ray` already takes `hit_is_occupied`) — and touches
`Scan`, `flatten`, the accumulator (free beams do not vote) and the
matcher (free beams do not match). Worth doing on the bench first: more
free space also changes the frontier picture the explorer plans on.

## Finding 4 — odometry edges have one confidence whatever their length

`pipeline.rs` chains submap nodes with `information_from_sigmas(odom_sigma_xy, odom_sigma_yaw)`, the same for a submap opened 0.5 m from the last and one 3 m away. Odometry error grows with travel; a per-edge σ proportional to the travelled distance (σ₀·√d or σ₀·d) is the standard model and would let loop closures bend long chains more than short ones. Not a bug — the submap travel rule bounds the range — but it is part of why "loop closures on map noise walk the pose off" (`upstream-asks.md` §1): every edge is told the same thing.

## Finding 5 — the still-window composite over-weights the near wall

The accumulator merges *every surviving beam of every frame* into the composite (a 100-frame stand contributes the same wall 100 times), then `integrate_scan_weighted` inks it `passes` more times. Log-odds saturate at ±4 after five hits, so the map itself is unharmed; but every residual computed on the composite (`score_pose`, the matcher, the loop closer) is a mean over beams, and 100 copies of the wall the head happened to point at dominate 5 copies of the corner it swept past. A per-cell de-duplication before scoring (keep one beam per endpoint cell, or weight by 1/count) would make the residual mean what it says.

## Finding 6 — small things

- `score_pose` returns `mean_residual_m: 0.0` when `n_observed == 0`. Every caller checks `n_observed` first, so it is harmless today; a `NaN` or `INFINITY` would be honest.
- `tracking_correction` decimates to 512 beams and `loop_closer` to `max_probe_beams`, both by stride; on a composite ordered frame by frame a stride picks whole frames, not a spread of bearings. Uniform stride over a frame-ordered list is fine only because frames overlap; a bearing-sorted decimation would be strictly better.
- `align.rs` inherits Findings 1–2 through `relocalize_against_grid`: the map-to-map answer that fitted the office to the bedroom had no ray test either. Its floor-on-wall penalty (`wall_penalty`) is the same idea from the other side and is what put the truth first on the bench; the ray test is its complement.

## What to do, in order

1. Finding 1, the ray test in the one judge (`score_pose`), measured on the bench recordings (the kidnap and boot relocalisations) and on the wake-up round: a mirror image that crosses a wall must now fail.
2. Finding 2 alongside it, as a flag on `score_pose` (`off_map_counts`), true for candidates on a saved map, false for the watchdog.
3. Finding 3, free-space beams, on the bench first (map quality, frontier count), then the journeys.
4. Findings 4–5 as knobs, measured on the replay bench like every other change.
