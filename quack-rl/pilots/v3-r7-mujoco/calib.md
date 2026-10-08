# Calibration

3 traces, 3016 events: 2684 legs, 332 stands of 1.5 s or more, 44 maps.

| number | prior | fitted | n | how |
|---|---|---|---|---|
| `back_speed` | 0.0800 | — | 0 | median speed of the back-offs — too few samples (0 < 8): prior kept |
| `bump_fall_p` | 0.0060 | **0.0040** | 126 | 0 falls in 126 bumps (forward steps that barely moved with something within 0.25 m ahead) |
| `odom_xy_per_m` | 0.0500 | — | 0 | odometry against the map's pose over 133 walks between stands: the difference (5.4 mm) is within the map pose's own noise — not identifiable, prior kept |
| `phantom_any_p` | 0.0020 | **0.0000** | 190448 | drops on known floor (0.3 m of floor all round) per floor row judged, at the stands |
| `phantom_p` | 0.1000 | — | 0 | low furniture's phantoms are not told apart from the others in a trace: prior kept |
| `pose_jitter_m` | 0.0100 | — | 0 | not identifiable from the traces: prior kept |
| `post_fall_p` | 0.0300 | **0.0198** | 126 | bump_fall_p × the prior's ratio of posts to boxes (a trace does not tell what was met) |
| `pulse_gain_mean` | 0.8500 | **1.3416** | 588 | median of (yaw turned) / (yaw_per_unit × vyaw × secs) over the curving steps ≤ 0.8 s |
| `pulse_gain_sd` | 0.6500 | **0.6028** | 588 | their spread (MAD) |
| `short_returns` | 0.0000 | — | 51146 | returns more than 0.3 m short of the map at the stands: 51146 of them, 238 lone (noise), the rest things the map does not have |
| `speed_at_03` | 0.1140 | **0.1221** | 891 | median forward speed of the free steps, by the map's pose, scaled to vx 0.3 |
| `speed_sd` | 0.1000 | **0.0443** | 891 | their spread (MAD), relative, less the map pose's jitter at both ends |
| `spur_median_m` | 0.4500 | **0.8990** | 238 | their median distance |
| `spur_p` | 0.0050 | **0.0004** | 238 | lone returns more than 0.3 m short of the map's ray (not back at that point of the world a second later), per zone and frame at the stands |
| `spur_sigma` | 0.8000 | **0.3389** | 238 | their spread, in log (half the 16-84 % span) |
| `stand_keep` | 0.5000 | — | 0 | not identifiable from the traces (the map's own error is not seen): prior kept |
| `straight_veer` | -0.0500 | **-0.0519** | 170 | median yaw rate of the straight steps (|vyaw| < 0.1) |
| `tof_dropout` | 0.0200 | — | 0 | near (under 1.4 m) several rows meet a wall and a column's silence says little of one zone: prior kept |
| `tof_dropout_far` | 0.2000 | **0.3816** | 13137 | columns whose ray meets a mapped wall 1.4-1.95 m off (one row reaches it) with no return there |
| `tof_hz` | 15.0000 | **13.3333** | 26136 | 1 / the median gap between frames |
| `tof_low_walk_m` | 0.0900 | — | 0 | not identifiable from the traces: prior kept |
| `tof_range_bias` | 0.0000 | **-0.0715** | 34645 | median of (range − the map's ray) at the stands, inliers within 0.3 m |
| `tof_range_sd` | 0.0200 | **0.0426** | 34645 | their spread (MAD) |
| `turn_left_rad_s` | 0.8901 | **0.8537** | 726 | median rate of the turns in place to the left (vyaw ≥ 1.45), by odometry over the turn's duration |
| `turn_right_rad_s` | 1.0123 | **0.9594** | 723 | the same to the right |
| `turn_sd` | 0.1000 | **0.2430** | 1449 | their spread, relative |
| `yaw_per_unit` | 0.6500 | — | 0 | median yaw rate per unit of vyaw over curving legs longer than 0.8 s (the pilot's and the stick's steps are 0.6 s: rarely seen) — too few samples (0 < 8): prior kept |

Replay of 2346 legs through the gait model (no noise): distance RMSE 0.0190 m with the prior, 0.0193 m with the fit; yaw RMSE 0.1888 rad with the prior, 0.1763 rad with the fit.
