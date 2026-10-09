# Pilot report: rl-runs/calib-vel/train

Checkpoint: update 109 of ppo, calibration rl-runs/calib-vel/calib.json.

Parity: rl-runs/calib-vel/train/pilot.json: 5 probes, the same move 5/5, largest logit difference 3.34e-6; ONNX: largest logit difference 2.86e-06.

## Test bench (seeds from 200000, 60 per family)

```
family     brain             n arrive%  hole   tip   off  tout  fail    secs  bumps  mbump   rim_m
ALL        expert          420    97.1     0     2     0    10     0    64.7   0.21   0.00   0.132
ALL        old-pilot       420    94.0     0    12     0    13     0    65.1   0.87   0.02   0.129
ALL        pilot           420    95.2     0     4     0    16     0    64.9   0.70   0.04   0.074
ALL        stick           420    94.8     0    18     0     4     0    73.2   2.59   0.11   0.091
clutter    expert           60    98.3     0     0     0     1     0    50.9   0.00   0.00  -1.000
clutter    old-pilot        60    95.0     0     3     0     0     0    55.7   3.47   0.00  -1.000
clutter    pilot            60    96.7     0     2     0     0     0    54.1   1.63   0.00  -1.000
clutter    stick            60    95.0     0     3     0     0     0    65.4   1.55   0.00  -1.000
corners    expert           60   100.0     0     0     0     0     0    98.9   0.00   0.00  -1.000
corners    old-pilot        60   100.0     0     0     0     0     0    95.6   0.00   0.00  -1.000
corners    pilot            60    98.3     0     1     0     0     0    94.8   0.63   0.00  -1.000
corners    stick            60   100.0     0     0     0     0     0    97.5   0.02   0.00  -1.000
doorway    expert           60    96.7     0     0     0     2     0    69.7   0.42   0.00  -1.000
doorway    old-pilot        60    88.3     0     1     0     6     0    74.5   1.05   0.00  -1.000
doorway    pilot            60    91.7     0     0     0     5     0    72.4   1.60   0.00  -1.000
doorway    stick            60    88.3     0     5     0     2     0    82.8   6.93   0.00  -1.000
low        expert           60   100.0     0     0     0     0     0    51.8   0.07   0.00  -1.000
low        old-pilot        60   100.0     0     0     0     0     0    50.0   0.08   0.00  -1.000
low        pilot            60   100.0     0     0     0     0     0    51.0   0.08   0.00  -1.000
low        stick            60   100.0     0     0     0     0     0    59.3   0.00   0.00  -1.000
mixed      expert           60    85.0     0     2     0     7     0    70.9   0.98   0.02   0.132
mixed      old-pilot        60    75.0     0     8     0     7     0    79.8   1.48   0.08   0.203
mixed      pilot            60    80.0     0     1     0    11     0    79.1   0.92   0.10   0.155
mixed      stick            60    80.0     0    10     0     2     0    94.8   9.60   0.32   0.167
movers     expert           60   100.0     0     0     0     0     0    50.1   0.00   0.00  -1.000
movers     old-pilot        60   100.0     0     0     0     0     0    45.8   0.00   0.03  -1.000
movers     pilot            60   100.0     0     0     0     0     0    46.3   0.00   0.20  -1.000
movers     stick            60   100.0     0     0     0     0     0    55.0   0.00   0.48  -1.000
stairwell  expert           60   100.0     0     0     0     0     0    61.2   0.00   0.00   0.165
stairwell  old-pilot        60   100.0     0     0     0     0     0    58.8   0.00   0.00   0.129
stairwell  pilot            60   100.0     0     0     0     0     0    60.4   0.00   0.00   0.074
stairwell  stick            60   100.0     0     0     0     0     0    62.3   0.00   0.00   0.091
```

## Shields: reckless brains (60 per family)

Falls into a hole: **0** (`hole`; `tip` is tipping over against something).

```
family     brain             n arrive%  hole   tip   off  tout  fail    secs  bumps  mbump   rim_m
ALL        reckless-back   420     0.0     0     2     0   418     0     0.0   0.18   0.41   0.295
ALL        reckless-random  420    27.9     0     5     0   298     0   171.0   1.01   0.06   0.111
ALL        reckless-straight  420    61.9     0     8     0   152     0   122.0   1.81   0.06   0.111
clutter    reckless-back    60     0.0     0     0     0    60     0     0.0   0.27   0.00  -1.000
clutter    reckless-random   60    16.7     0     1     0    49     0   170.6   1.65   0.00  -1.000
clutter    reckless-straight   60    48.3     0     1     0    30     0   117.3   3.88   0.00  -1.000
corners    reckless-back    60     0.0     0     0     0    60     0     0.0   0.00   0.00  -1.000
corners    reckless-random   60    56.7     0     1     0    25     0   244.5   0.95   0.00  -1.000
corners    reckless-straight   60   100.0     0     0     0     0     0   149.3   0.45   0.00  -1.000
doorway    reckless-back    60     0.0     0     0     0    60     0     0.0   0.07   0.00  -1.000
doorway    reckless-random   60     6.7     0     0     0    56     0   133.2   1.37   0.00  -1.000
doorway    reckless-straight   60    31.7     0     3     0    38     0   147.0   4.10   0.00  -1.000
low        reckless-back    60     0.0     0     0     0    60     0     0.0   0.00   0.00  -1.000
low        reckless-random   60    20.0     0     0     0    48     0   121.6   0.70   0.00  -1.000
low        reckless-straight   60    61.7     0     1     0    22     0   104.7   0.57   0.00  -1.000
mixed      reckless-back    60     0.0     0     1     0    59     0     0.0   0.55   1.12   0.340
mixed      reckless-random   60     5.0     0     3     0    54     0   130.5   1.83   0.22   0.121
mixed      reckless-straight   60    38.3     0     3     0    34     0   147.0   3.47   0.18   0.123
movers     reckless-back    60     0.0     0     1     0    59     0     0.0   0.37   1.77  -1.000
movers     reckless-random   60    23.3     0     0     0    46     0   109.9   0.55   0.20  -1.000
movers     reckless-straight   60    61.7     0     0     0    23     0    99.7   0.17   0.23  -1.000
stairwell  reckless-back    60     0.0     0     0     0    60     0     0.0   0.00   0.00   0.295
stairwell  reckless-random   60    66.7     0     0     0    20     0   151.6   0.00   0.00   0.111
stairwell  reckless-straight   60    91.7     0     0     0     5     0   102.2   0.02   0.00   0.111
```
