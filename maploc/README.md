# maploc (vendored)

Submap pose-graph SLAM and relocalization over the head ToF, from Pollen
Robotics' `pollen-robotics/microduck` (PR 127, by apirrone), with the
changes of the `maploc-quacknav` branch (commit `16070fd`) on top. Carried
here so `quack-navd` can host the mapper itself against the released
robotd (daemon-v0.16.1, API 41) — see `NOTICE`.

`flat` (behind the `kinematics` feature) is the depth frame as a 2D scan;
in the fork it was `kinematics::tof::Reprojector::flatten`. It also gives
the same frame's beams in 3D, for `band`.

`band` is not Pollen's: a 2.5D layer of what stands at the duck's own
height (ten 2.5 cm slices from 0.05 to 0.30 m per 5 cm cell), kept by
submap and saved beside the session; a return near a wall the submap's
own 2D grid has is left to the map. `examples/band` builds it from a
recording (or draws a saved `<name>.session` with its band), and `scripts/band/band_vs_truth.py` scores it against a twin
scene's geometry.

Checked on 2026-09-22: `examples/evaluate` over a twin recording gives
byte-identical output and map here and in the fork.
