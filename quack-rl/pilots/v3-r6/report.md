# Pilot report: rl-runs/r6

Checkpoint: update 539 of ppo, calibration the paper twin's defaults.

Parity: rl-runs/r6/pilot.json: 5 probes, the same move 5/5, largest logit difference 2.86e-6; ONNX: largest logit difference 2.86e-06.

## Test bench (seeds from 200000, 60 per family)

```
family     brain             n arrive%  hole   tip   off  tout  fail    secs  bumps  mbump   rim_m
ALL        expert          420    94.3     0     1     0    21     2    73.3   0.48   0.01   0.102
ALL        pilot           420    91.4     0     7     0    27     2    74.6   1.20   0.05   0.070
ALL        stick           420    80.0     0    72     0     5     7    78.1  12.63   0.29   0.046
clutter    expert           60    95.0     0     0     0     3     0    57.2   0.05   0.00  -1.000
clutter    pilot            60    85.0     0     5     0     4     0    63.0   3.17   0.00  -1.000
clutter    stick            60    61.7     0    20     0     3     0    68.6  32.00   0.00  -1.000
corners    expert           60    96.7     0     0     0     1     1   106.3   0.07   0.00  -1.000
corners    pilot            60    96.7     0     0     0     0     2   102.8   0.20   0.00  -1.000
corners    stick            60    91.7     0     1     0     0     4   108.4   0.08   0.00  -1.000
doorway    expert           60    90.0     0     1     0     5     0    79.6   0.87   0.00  -1.000
doorway    pilot            60    83.3     0     0     0    10     0    88.2   1.93   0.00  -1.000
doorway    stick            60    65.0     0    21     0     0     0    89.3  29.62   0.00  -1.000
low        expert           60    96.7     0     0     0     2     0    58.1   0.00   0.00  -1.000
low        pilot            60   100.0     0     0     0     0     0    56.3   0.23   0.00  -1.000
low        stick            60    90.0     0     6     0     0     0    63.4   1.67   0.00  -1.000
mixed      expert           60    86.7     0     0     0     8     0    86.9   2.40   0.08   0.102
mixed      pilot            60    75.0     0     2     0    13     0    97.5   2.75   0.10   0.198
mixed      stick            60    60.0     0    22     0     1     1    98.0  24.00   1.03   0.187
movers     expert           60   100.0     0     0     0     0     0    59.5   0.00   0.00  -1.000
movers     pilot            60   100.0     0     0     0     0     0    51.8   0.10   0.27  -1.000
movers     stick            60    96.7     0     2     0     0     0    56.2   1.03   1.00  -1.000
stairwell  expert           60    95.0     0     0     0     2     1    67.1   0.00   0.00   0.125
stairwell  pilot            60   100.0     0     0     0     0     0    69.8   0.00   0.00   0.070
stairwell  stick            60    95.0     0     0     0     1     2    70.9   0.00   0.00   0.046
```

## Shields: reckless brains (60 per family)

Falls into a hole: **0** (`hole`; `tip` is tipping over against something).

```
family     brain             n arrive%  hole   tip   off  tout  fail    secs  bumps  mbump   rim_m
ALL        reckless-back   420     0.0     0     3     0   410     7     0.0   0.10   0.29   0.207
ALL        reckless-random  420     8.1     0    15     0   361    10   169.5   3.19   0.09   0.124
ALL        reckless-straight  420    46.7     0    16     0   196    12   139.2   5.25   0.07   0.104
clutter    reckless-back    60     0.0     0     1     0    59     0     0.0   0.22   0.00  -1.000
clutter    reckless-random   60     1.7     0     4     0    55     0    96.4   4.82   0.00  -1.000
clutter    reckless-straight   60    38.3     0     4     0    32     1   125.3   8.88   0.00  -1.000
corners    reckless-back    60     0.0     0     0     0    54     6     0.0   0.00   0.00  -1.000
corners    reckless-random   60    16.7     0     2     0    43     5   236.7   4.53   0.00  -1.000
corners    reckless-straight   60    86.7     0     3     0     2     3   171.7   5.22   0.00  -1.000
doorway    reckless-back    60     0.0     0     0     0    60     0     0.0   0.15   0.00  -1.000
doorway    reckless-random   60     3.3     0     1     0    56     1   158.8   4.23   0.00  -1.000
doorway    reckless-straight   60    21.7     0     2     0    45     0   170.1   7.52   0.00  -1.000
low        reckless-back    60     0.0     0     0     0    60     0     0.0   0.07   0.00  -1.000
low        reckless-random   60    10.0     0     2     0    52     0   130.2   1.68   0.00  -1.000
low        reckless-straight   60    40.0     0     0     0    35     1   115.2   3.82   0.00  -1.000
mixed      reckless-back    60     0.0     0     1     0    59     0     0.0   0.13   0.45   0.333
mixed      reckless-random   60     3.3     0     1     0    55     2   164.5   4.23   0.17   0.184
mixed      reckless-straight   60    26.7     0     5     0    39     0   168.8   7.95   0.15   0.176
movers     reckless-back    60     0.0     0     1     0    59     0     0.0   0.15   1.60  -1.000
movers     reckless-random   60     5.0     0     4     0    52     1   130.8   1.03   0.43  -1.000
movers     reckless-straight   60    43.3     0     1     0    33     0    96.6   1.57   0.35  -1.000
stairwell  reckless-back    60     0.0     0     0     0    59     1     0.0   0.00   0.00   0.207
stairwell  reckless-random   60    16.7     0     1     0    48     1   148.0   1.82   0.00   0.124
stairwell  reckless-straight   60    70.0     0     1     0    10     7   125.7   1.82   0.00   0.104
```
