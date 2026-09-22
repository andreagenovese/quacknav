# maploc (vendored)

Submap pose-graph SLAM and relocalization over the head ToF, from Pollen
Robotics' `pollen-robotics/microduck` (PR 127, by apirrone), with the
changes of the `maploc-quacknav` branch (commit `16070fd`) on top. Carried
here so `quack-navd` can host the mapper itself against the released
robotd (daemon-v0.14.4, API 34) — see `NOTICE`.

`flat` (behind the `kinematics` feature) is the depth frame as a 2D scan;
in the fork it was `kinematics::tof::Reprojector::flatten`.

Checked on 2026-09-22: `examples/evaluate` over a twin recording gives
byte-identical output and map here and in the fork.
