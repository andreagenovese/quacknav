# Pilot report: rl-runs/r3

Checkpoint: update 239 of ppo, calibration the paper twin's defaults.

Parity: rl-runs/r3/pilot.json: 5 probes, the same move 5/5, largest logit difference 3.34e-6; ONNX: largest logit difference 5.25e-06.

## Test bench (seeds from 200000, 60 per family)

```
family     brain             n arrive%  fell   off  tout  fail    secs  bumps  mbump   rim_m
ALL        expert          420    93.1     0     0    26     3    79.9   1.04   0.00   0.136
ALL        pilot           420    94.0     0     0    23     2    72.1   0.68   0.02   0.118
ALL        r2/pilot        420    92.4     0     0    28     4    71.0   0.90   0.03   0.066
ALL        stick           420    89.0     0     0    41     5    82.0  32.23   0.82   0.071
clutter    expert           60    91.7     0     0     5     0    58.5   0.83   0.00  -1.000
clutter    pilot            60    96.7     0     0     2     0    64.8   2.20   0.00  -1.000
clutter    r2/pilot         60    95.0     0     0     3     0    64.3   3.43   0.00  -1.000
clutter    stick            60    73.3     0     0    16     0    77.6  79.28   0.00  -1.000
corners    expert           60    93.3     0     0     2     2   114.1   1.30   0.00  -1.000
corners    pilot            60    98.3     0     0     0     1   100.1   0.07   0.00  -1.000
corners    r2/pilot         60    95.0     0     0     0     3    99.2   0.23   0.00  -1.000
corners    stick            60    91.7     0     0     1     4   103.2   5.05   0.00  -1.000
doorway    expert           60    95.0     0     0     3     0    93.1   2.38   0.00  -1.000
doorway    pilot            60    83.3     0     0     9     1    82.8   0.82   0.00  -1.000
doorway    r2/pilot         60    80.0     0     0    12     0    76.9   1.22   0.00  -1.000
doorway    stick            60    85.0     0     0     8     1   101.2  53.38   0.00  -1.000
low        expert           60    90.0     0     0     6     0    59.9   0.25   0.00  -1.000
low        pilot            60   100.0     0     0     0     0    53.2   0.03   0.00  -1.000
low        r2/pilot         60   100.0     0     0     0     0    53.7   0.13   0.00  -1.000
low        stick            60    98.3     0     0     1     0    65.9   9.47   0.00  -1.000
mixed      expert           60    93.3     0     0     4     0    88.4   2.28   0.00   0.136
mixed      pilot            60    80.0     0     0    12     0    92.2   1.53   0.03   0.144
mixed      r2/pilot         60    80.0     0     0    12     0    92.2   1.27   0.03   0.124
mixed      stick            60    78.3     0     0    13     0   110.7  77.55   0.78   0.071
movers     expert           60    95.0     0     0     3     0    72.6   0.13   0.03  -1.000
movers     pilot            60   100.0     0     0     0     0    54.4   0.10   0.12  -1.000
movers     r2/pilot         60   100.0     0     0     0     0    51.8   0.03   0.20  -1.000
movers     stick            60    98.3     0     0     1     0    57.0   0.87   4.93  -1.000
stairwell  expert           60    93.3     0     0     3     1    71.6   0.10   0.00   0.153
stairwell  pilot            60   100.0     0     0     0     0    63.1   0.00   0.00   0.118
stairwell  r2/pilot         60    96.7     0     0     1     1    65.6   0.00   0.00   0.066
stairwell  stick            60    98.3     0     0     1     0    67.2   0.00   0.00   0.084
```

## Shields: reckless brains (60 per family)

Falls: **0**.

```
family     brain             n arrive%  fell   off  tout  fail    secs  bumps  mbump   rim_m
ALL        reckless-back   420     0.0     0     0   416     4     0.0   5.52   0.40   0.283
ALL        reckless-random  420     8.3     0     0   378     7   170.0   2.11   0.04   0.087
ALL        reckless-straight  420    50.7     0     0   203     4   130.4   4.00   0.07   0.071
clutter    reckless-back    60     0.0     0     0    60     0     0.0   1.22   0.00  -1.000
clutter    reckless-random   60     0.0     0     0    59     1     0.0   2.42   0.00  -1.000
clutter    reckless-straight   60    45.0     0     0    32     1   131.8   6.13   0.00  -1.000
corners    reckless-back    60     0.0     0     0    57     3     0.0  18.98   0.00  -1.000
corners    reckless-random   60    13.3     0     0    49     3   213.7   4.10   0.00  -1.000
corners    reckless-straight   60    93.3     0     0     2     2   162.2   5.13   0.00  -1.000
doorway    reckless-back    60     0.0     0     0    60     0     0.0   1.62   0.00  -1.000
doorway    reckless-random   60     6.7     0     0    55     1   165.7   2.17   0.00  -1.000
doorway    reckless-straight   60    25.0     0     0    44     1   124.6   3.98   0.00  -1.000
low        reckless-back    60     0.0     0     0    60     0     0.0   2.72   0.00  -1.000
low        reckless-random   60     8.3     0     0    55     0   103.1   1.38   0.00  -1.000
low        reckless-straight   60    40.0     0     0    36     0   104.6   1.93   0.00  -1.000
mixed      reckless-back    60     0.0     0     0    59     1     0.0   3.52   0.78   0.298
mixed      reckless-random   60     0.0     0     0    60     0     0.0   3.20   0.13   0.153
mixed      reckless-straight   60    31.7     0     0    41     0   151.9   7.07   0.28   0.188
movers     reckless-back    60     0.0     0     0    60     0     0.0   1.25   2.00  -1.000
movers     reckless-random   60     6.7     0     0    56     0   157.3   0.72   0.17  -1.000
movers     reckless-straight   60    45.0     0     0    33     0   101.4   2.18   0.22  -1.000
stairwell  reckless-back    60     0.0     0     0    60     0     0.0   9.33   0.00   0.283
stairwell  reckless-random   60    23.3     0     0    44     2   173.7   0.78   0.00   0.087
stairwell  reckless-straight   60    75.0     0     0    15     0   113.8   1.55   0.00   0.071
```
