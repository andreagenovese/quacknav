# Calibration

1 traces, 1653 events: 1389 legs, 264 stands of 1.5 s or more, 33 maps.

| number | prior | fitted | n | how |
|---|---|---|---|---|
| `back_speed` | 0.0800 | — | 0 | median speed of the back-offs — too few samples (0 < 8): prior kept |
| `bump_fall_p` | 0.0060 | **0.0091** | 55 | 0 falls in 55 bumps (forward steps that barely moved with something within 0.25 m ahead) |
| `odom_xy_per_m` | 0.0500 | — | 0 | odometry against the map's pose over 183 walks between stands: the difference (10.0 mm) is within the map pose's own noise — not identifiable, prior kept |
| `phantom_any_p` | 0.0020 | **0.0000** | 218635 | drops on known floor (0.3 m of floor all round) per floor row judged, at the stands |
| `phantom_p` | 0.1000 | — | 0 | low furniture's phantoms are not told apart from the others in a trace: prior kept |
| `pose_jitter_m` | 0.0100 | — | 0 | not identifiable from the traces: prior kept |
| `post_fall_p` | 0.0300 | **0.0455** | 55 | bump_fall_p × the prior's ratio of posts to boxes (a trace does not tell what was met) |
| `pulse_gain_mean` | 0.8500 | **0.7466** | 558 | median of (yaw turned) / (yaw_per_unit × vyaw × secs) over the curving steps ≤ 0.8 s |
| `pulse_gain_sd` | 0.6500 | **0.4171** | 558 | their spread (MAD) |
| `short_returns` | 0.0000 | — | 48660 | returns more than 0.3 m short of the map at the stands: 48660 of them, 286 lone (noise), the rest things the map does not have |
| `speed_at_03` | 0.1140 | **0.1228** | 1059 | median forward speed of the free steps, by the map's pose, scaled to vx 0.3 |
| `speed_sd` | 0.1000 | **0.0000** | 1059 | their spread (MAD), relative, less the map pose's jitter at both ends |
| `spur_median_m` | 0.4500 | **1.1635** | 286 | their median distance |
| `spur_p` | 0.0050 | **0.0005** | 286 | lone returns more than 0.3 m short of the map's ray (not back at that point of the world a second later), per zone and frame at the stands |
| `spur_sigma` | 0.8000 | **0.5099** | 286 | their spread, in log (half the 16-84 % span) |
| `stand_keep` | 0.5000 | — | 0 | not identifiable from the traces (the map's own error is not seen): prior kept |
| `straight_veer` | -0.0500 | **-0.0125** | 176 | median yaw rate of the straight steps (|vyaw| < 0.1) |
| `tof_dropout` | 0.0200 | — | 0 | near (under 1.4 m) several rows meet a wall and a column's silence says little of one zone: prior kept |
| `tof_dropout_far` | 0.2000 | **0.2782** | 16080 | columns whose ray meets a mapped wall 1.4-1.95 m off (one row reaches it) with no return there |
| `tof_hz` | 15.0000 | **13.8313** | 19804 | 1 / the median gap between frames |
| `tof_low_walk_m` | 0.0900 | — | 0 | not identifiable from the traces: prior kept |
| `tof_range_bias` | 0.0000 | **-0.0746** | 35321 | median of (range − the map's ray) at the stands, inliers within 0.3 m |
| `tof_range_sd` | 0.0200 | **0.0415** | 35321 | their spread (MAD) |
| `turn_left_rad_s` | 0.8901 | **0.5982** | 49 | median rate of the turns in place to the left (vyaw ≥ 1.45), by odometry over the turn's duration |
| `turn_right_rad_s` | 1.0123 | **0.7620** | 55 | the same to the right |
| `turn_sd` | 0.1000 | **0.3300** | 104 | their spread, relative |
| `yaw_per_unit` | 0.6500 | — | 0 | median yaw rate per unit of vyaw over curving legs longer than 0.8 s (the pilot's and the stick's steps are 0.6 s: rarely seen) — too few samples (0 < 8): prior kept |

Replay of 1176 legs through the gait model (no noise): distance RMSE 0.0234 m with the prior, 0.0248 m with the fit; yaw RMSE 0.1911 rad with the prior, 0.1664 rad with the fit.
