# Pilot report: rl-runs/calib-mj/train

Checkpoint: update 139 of ppo, calibration rl-runs/calib-mj/calib.json.

Parity: rl-runs/calib-mj/train/pilot.json: 5 probes, the same move 5/5, largest logit difference 1.91e-6; ONNX: largest logit difference 1.91e-06.

## Test bench (seeds from 200000, 60 per family)

```
family     brain             n arrive%  hole   tip   off  tout  fail    secs  bumps  mbump   rim_m
ALL        expert          420    98.3     0     0     0     7     0    63.1   0.53   0.00   0.132
ALL        old-pilot       420    90.0     0     2     0    40     0    65.5   0.86   0.02   0.079
ALL        pilot           420    96.2     0     1     0    15     0    67.3   1.03   0.03   0.112
ALL        stick           420    82.4     0    59     0    12     3    70.0  20.07   0.26   0.090
clutter    expert           60   100.0     0     0     0     0     0    51.8   0.03   0.00  -1.000
clutter    old-pilot        60    91.7     0     0     0     5     0    61.9   2.17   0.00  -1.000
clutter    pilot            60    96.7     0     0     0     2     0    57.3   2.52   0.00  -1.000
clutter    stick            60    58.3     0    18     0     7     0    62.3  44.62   0.00  -1.000
corners    expert           60   100.0     0     0     0     0     0    96.8   0.02   0.00  -1.000
corners    old-pilot        60    98.3     0     1     0     0     0    96.2   0.58   0.00  -1.000
corners    pilot            60    98.3     0     1     0     0     0    94.0   0.65   0.00  -1.000
corners    stick            60    98.3     0     1     0     0     0    96.0   1.87   0.00  -1.000
doorway    expert           60    98.3     0     0     0     1     0    68.2   0.90   0.00  -1.000
doorway    old-pilot        60    71.7     0     0     0    17     0    67.5   0.70   0.00  -1.000
doorway    pilot            60    88.3     0     0     0     7     0    78.7   1.93   0.00  -1.000
doorway    stick            60    76.7     0    14     0     0     0    86.4  44.25   0.00  -1.000
low        expert           60   100.0     0     0     0     0     0    50.0   0.08   0.00  -1.000
low        old-pilot        60   100.0     0     0     0     0     0    51.0   0.33   0.00  -1.000
low        pilot            60   100.0     0     0     0     0     0    49.4   0.22   0.00  -1.000
low        stick            60    93.3     0     3     0     1     0    54.7   3.90   0.00  -1.000
mixed      expert           60    90.0     0     0     0     6     0    68.6   2.70   0.00   0.132
mixed      old-pilot        60    68.3     0     1     0    18     0    83.1   2.23   0.08   0.093
mixed      pilot            60    90.0     0     0     0     6     0    91.7   1.87   0.00   0.154
mixed      stick            60    60.0     0    20     0     4     0    84.1  44.53   1.32   0.190
movers     expert           60   100.0     0     0     0     0     0    46.9   0.00   0.02  -1.000
movers     old-pilot        60   100.0     0     0     0     0     0    45.2   0.00   0.08  -1.000
movers     pilot            60   100.0     0     0     0     0     0    45.4   0.00   0.23  -1.000
movers     stick            60    95.0     0     3     0     0     0    49.4   1.32   0.52  -1.000
stairwell  expert           60   100.0     0     0     0     0     0    60.2   0.00   0.00   0.162
stairwell  old-pilot        60   100.0     0     0     0     0     0    60.0   0.00   0.00   0.079
stairwell  pilot            60   100.0     0     0     0     0     0    58.9   0.00   0.00   0.112
stairwell  stick            60    95.0     0     0     0     0     3    61.4   0.00   0.00   0.090
```

## Shields: reckless brains (60 per family)

Falls into a hole: **0** (`hole`; `tip` is tipping over against something).

```
family     brain             n arrive%  hole   tip   off  tout  fail    secs  bumps  mbump   rim_m
ALL        reckless-back   420     0.0     0     4     0   416     0     0.0   0.09   0.29   0.266
ALL        reckless-random  420    15.5     0     5     0   349     1   167.0   1.00   0.06   0.110
ALL        reckless-straight  420    53.3     0     7     0   187     2   125.7   1.77   0.10   0.080
clutter    reckless-back    60     0.0     0     0     0    60     0     0.0   0.27   0.00  -1.000
clutter    reckless-random   60    11.7     0     3     0    50     0   144.9   2.38   0.00  -1.000
clutter    reckless-straight   60    38.3     0     2     0    35     0   118.2   2.82   0.00  -1.000
corners    reckless-back    60     0.0     0     0     0    60     0     0.0   0.00   0.00  -1.000
corners    reckless-random   60    26.7     0     0     0    44     0   252.2   0.58   0.00  -1.000
corners    reckless-straight   60   100.0     0     0     0     0     0   148.4   0.48   0.00  -1.000
doorway    reckless-back    60     0.0     0     0     0    60     0     0.0   0.05   0.00  -1.000
doorway    reckless-random   60     0.0     0     1     0    59     0     0.0   1.80   0.00  -1.000
doorway    reckless-straight   60    23.3     0     2     0    44     0   132.8   2.33   0.00  -1.000
low        reckless-back    60     0.0     0     0     0    60     0     0.0   0.03   0.00  -1.000
low        reckless-random   60    15.0     0     0     0    51     0   107.1   0.38   0.00  -1.000
low        reckless-straight   60    45.0     0     0     0    33     0   105.2   0.47   0.00  -1.000
mixed      reckless-back    60     0.0     0     2     0    58     0     0.0   0.12   0.55   0.338
mixed      reckless-random   60     3.3     0     1     0    57     0   173.7   1.42   0.10   0.137
mixed      reckless-straight   60    23.3     0     3     0    42     1   149.4   6.08   0.28   0.080
movers     reckless-back    60     0.0     0     2     0    58     0     0.0   0.13   1.45  -1.000
movers     reckless-random   60    21.7     0     0     0    47     0   117.3   0.37   0.32  -1.000
movers     reckless-straight   60    50.0     0     0     0    30     0   107.1   0.22   0.40  -1.000
stairwell  reckless-back    60     0.0     0     0     0    60     0     0.0   0.00   0.00   0.266
stairwell  reckless-random   60    30.0     0     0     0    41     1   165.0   0.03   0.00   0.110
stairwell  reckless-straight   60    93.3     0     0     0     3     1   116.6   0.00   0.00   0.124
```
