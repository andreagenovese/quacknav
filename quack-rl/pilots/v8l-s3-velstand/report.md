# Pilot report: rl-runs/calib-v8l/s3/train

Checkpoint: update 129 of ppo, calibration rl-runs/calib-v8l/calib.json.

Parity: rl-runs/calib-v8l/s3/train/pilot.json: 5 probes, the same move 5/5, largest logit difference 2.86e-6; ONNX: largest logit difference 3.34e-06.

## Test bench (seeds from 200000, 60 per family)

```
family     brain             n arrive%  hole   tip   off  tout  fail    secs  bumps  mbump   rim_m
ALL        expert          420    98.8     0     6     0     5     0    79.3   1.42   0.02   0.057
ALL        old-pilot       420    98.3     0     8     0     7     0    72.5   2.20   0.02   0.063
ALL        pilot           420    98.6     0     6     0     5     1    71.4   1.47   0.03   0.062
ALL        stick           420    98.3     0    18     1     6     0    77.6   3.08   0.07   0.084
clutter    expert           60   100.0     0     1     0     0     0    64.6   0.63   0.00  -1.000
clutter    old-pilot        60   100.0     0     3     0     0     0    61.9   5.03   0.00  -1.000
clutter    pilot            60   100.0     0     0     0     0     0    59.9   2.22   0.00  -1.000
clutter    stick            60    96.7     0     7     0     2     0    67.8   7.73   0.00  -1.000
corners    expert           60   100.0     0     0     0     0     0   106.5   0.18   0.00  -1.000
corners    old-pilot        60   100.0     0     0     0     0     0    98.9   0.13   0.00  -1.000
corners    pilot            60   100.0     0     0     0     0     0    98.9   0.00   0.00  -1.000
corners    stick            60   100.0     0     0     0     0     0   102.7   0.00   0.00  -1.000
doorway    expert           60    96.7     0     0     0     2     0    87.7   4.97   0.00  -1.000
doorway    old-pilot        60    96.7     0     1     0     2     0    81.3   3.70   0.00  -1.000
doorway    pilot            60    95.0     0     1     0     3     0    77.6   3.70   0.00  -1.000
doorway    stick            60    96.7     0     4     1     1     0    86.6   3.88   0.00  -1.000
low        expert           60   100.0     0     0     0     0     0    58.8   0.12   0.00  -1.000
low        old-pilot        60   100.0     0     0     0     0     0    53.4   0.15   0.00  -1.000
low        pilot            60   100.0     0     0     0     0     0    53.8   0.03   0.00  -1.000
low        stick            60   100.0     0     0     0     0     0    60.6   0.00   0.00  -1.000
mixed      expert           60    96.7     0     5     0     2     0    98.3   4.03   0.13   0.196
mixed      old-pilot        60    93.3     0     4     0     4     0    91.5   6.28   0.00   0.198
mixed      pilot            60    95.0     0     4     0     2     1    89.7   4.35   0.02   0.137
mixed      stick            60    95.0     0     7     0     3     0    97.6   9.92   0.17   0.169
movers     expert           60   100.0     0     0     0     0     0    61.0   0.00   0.00  -1.000
movers     old-pilot        60   100.0     0     0     0     0     0    51.4   0.07   0.13  -1.000
movers     pilot            60   100.0     0     1     0     0     0    53.1   0.00   0.22  -1.000
movers     stick            60   100.0     0     0     0     0     0    58.1   0.00   0.30  -1.000
stairwell  expert           60    98.3     0     0     0     1     0    79.2   0.00   0.00   0.057
stairwell  old-pilot        60    98.3     0     0     0     1     0    70.9   0.00   0.00   0.063
stairwell  pilot            60   100.0     0     0     0     0     0    68.2   0.00   0.00   0.062
stairwell  stick            60   100.0     0     0     0     0     0    71.0   0.00   0.00   0.084
```

## Shields: reckless brains (60 per family)

Falls into a hole: **0** (`hole`; `tip` is tipping over against something).

```
family     brain             n arrive%  hole   tip   off  tout  fail    secs  bumps  mbump   rim_m
ALL        reckless-back   420    85.7     0    25     2    58     0   171.3   3.82   0.21   0.031
ALL        reckless-random  420    87.9     0    19     1    50     0   169.6   3.19   0.09   0.077
ALL        reckless-straight  420    94.5     0    24     1    18     4   121.3   3.88   0.07   0.097
clutter    reckless-back    60    80.0     0    10     2    10     0   161.4  12.25   0.00  -1.000
clutter    reckless-random   60    81.7     0     6     0    11     0   152.1   9.05   0.00  -1.000
clutter    reckless-straight   60    91.7     0     9     0     4     1   117.1   7.18   0.00  -1.000
corners    reckless-back    60    98.3     0     1     0     1     0   261.0   0.50   0.00  -1.000
corners    reckless-random   60   100.0     0     2     0     0     0   251.0   0.50   0.00  -1.000
corners    reckless-straight   60    98.3     0     1     0     0     1   159.0   1.13   0.00  -1.000
doorway    reckless-back    60    78.3     0     5     0    13     0   183.3   6.43   0.00  -1.000
doorway    reckless-random   60    80.0     0     5     1    11     0   180.1   7.30   0.00  -1.000
doorway    reckless-straight   60    90.0     0     5     1     5     0   138.3   6.98   0.00  -1.000
low        reckless-back    60    96.7     0     1     0     2     0   142.3   0.80   0.00  -1.000
low        reckless-random   60    91.7     0     1     0     5     0   140.9   0.12   0.00  -1.000
low        reckless-straight   60    96.7     0     1     0     1     1    98.4   1.07   0.00  -1.000
mixed      reckless-back    60    58.3     0     5     0    25     0   185.2   6.73   0.25   0.215
mixed      reckless-random   60    71.7     0     4     0    17     0   190.2   4.85   0.15   0.199
mixed      reckless-straight   60    85.0     0     8     0     8     1   154.6  10.63   0.08   0.173
movers     reckless-back    60    90.0     0     3     0     6     0   134.7   0.05   1.20  -1.000
movers     reckless-random   60    90.0     0     1     0     6     0   135.4   0.48   0.45  -1.000
movers     reckless-straight   60   100.0     0     0     0     0     0    91.6   0.10   0.43  -1.000
stairwell  reckless-back    60    98.3     0     0     0     1     0   134.0   0.00   0.00   0.031
stairwell  reckless-random   60   100.0     0     0     0     0     0   136.1   0.00   0.00   0.077
stairwell  reckless-straight   60   100.0     0     0     0     0     0    96.1   0.08   0.00   0.097
```
