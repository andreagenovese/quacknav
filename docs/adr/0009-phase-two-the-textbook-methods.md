# ADR 0009 — Phase two: the textbook methods, on the duck we have

Date: 2026-09-25. Status: accepted.

## Context

The preview release (docs/results.md, 5 of 7 criteria) was reached by
measuring, on the twins, every failure the duck showed and writing a
rule against it: the drop book with its radii, the string pulled at most
0.6 m, the centred aim, the refusal counters, the no-go spots, the valley
test. Each rule is measured, and each is a patch on one failure.

Looked at against the standard literature, the layers are not equal.
`maploc` already is a textbook design — a small Cartographer: submaps, a
Levenberg-Marquardt scan matcher that returns its own Hessian, an SE(2)
pose graph relaxed by Gauss-Newton over information matrices, a particle
filter. And robotd's odometry already fuses what the duck has: the
contact anchors of the legs for position, the IMU's integrated yaw for
heading. The hand-made rules sit above, in quack-nav, where the textbook
has a method for each of them:

| Rule today | The method it stands for |
|---|---|
| The valley test (slide the pose 0.30 m in 8 directions) | Degeneracy from the scan matcher's Hessian (Zhang, Kaess & Singh 2016) |
| A pose is a point, confirmed or not | A pose with a covariance, propagated by odometry and updated by the scan (EKF) |
| The string pull, `route_heading_anew`, fixed turns | A path-tracking controller: Regulated Pure Pursuit (Nav2) |
| The centred aim, inflation as a wall | A layered costmap with a decaying inflation cost |
| The drop book and its radii | An occupancy layer for drops, in log-odds, with a beam sensor model |
| The boot search's hypotheses and chords | Augmented MCL (KLD sampling, random injection), branch-and-bound global match |
| Refusal counters, no-go spots | A behaviour tree with recoveries |
| Frontier chosen by cost | Frontier chosen by information gain against cost |

The hardware stays what it is: an 8×8 ToF of about 45° and a few metres,
two IMUs, the legs' contacts, a camera and an NPU on board. Methods that
need a 360° scan, dense depth or a GPU are out.

## Decision

**Phase two replaces rules with methods, one at a time, and only when the
numbers say so.** Each step is measured the way the release was — paper
twin, then MuJoCo, then an A/B against `main` on the criteria of
docs/results.md — and goes in if it improves them, or matches them while
removing hand-made rules. A step that does neither is reverted, and the
measurement is written down anyway.

The order, each step standing on the ones before:

0. **Measurement first.** No method is swapped without a way to show it
   is better:
   - ATE and RPE in the standard form (TUM trajectories, `evo`-compatible);
   - golden routes on fixed scenes;
   - property tests on the safety rules;
   - deterministic replay of `.mdlg` recordings as a regression;
   - CI on every push, the paper twin included.
1. **A pose with its uncertainty.**
   - A 3×3 covariance on the tracked pose, grown by odometry between
     windows (noise per metre walked and per radian turned) and shrunk by
     each scan match (Σ = σ²·H⁻¹).
   - Degeneracy from the Hessian's eigenvalues, measured against the
     valley test before it replaces it.
   - A resumed session inks nothing until the covariance and the
     corrections have settled: the consistency check that casa_arredata's
     second session lacked.
2. A layered costmap and Regulated Pure Pursuit.
3. Drops as an occupancy layer in log-odds.
4. Relocalization as Augmented MCL, with branch-and-bound global matching.
5. A behaviour tree with recoveries.
6. Exploration by information gain.
7. Bringup on the duck: systemd, watchdog, safe stop, log rotation.
8. The camera, last and measured: an AprilTag at the charging dock, then
   place recognition on the NPU.

**Not in phase two:** full visual SLAM or VIO (ORB-SLAM3, VINS). On a
walking duck the camera shakes at every step, and the on-board CPU would
pay a great deal for what steps 1–4 give more cheaply. ROS 2 is not
adopted either: robotd's JSON-RPC stays the interface, and a ROS bridge
remains something anyone can build on the socket.

## Consequences

The work lives on the branch `phase-2` until a step is measured better.
(2026-09-30: `phase-2`'s work, steps 0 and 1, is on `main`. The shadow
map of 2026-09-29 — a lost duck's map of its walk, asked map-to-map where
it fits in the saved one, no particle filter — is a step toward step 4's
global relocalization, not the MCL itself.)
docs/results.md gains ATE and RPE columns from step 0 on, so every later
step reports in the numbers other systems publish. `map_status` gains
the pose's uncertainty with step 1, and the homecoming, the resume and
the explorer can ask how sure the duck is, not only whether it is.

The rules that a method replaces are removed, not kept beside it: two
mechanisms for one decision is how the explorer grew its counters. Where
a rule encodes something the method does not know — the rim is 0.15 m
because the gait creeps — it becomes a parameter of the method, named
for what was measured.
