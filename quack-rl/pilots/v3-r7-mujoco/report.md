# Pilot report: rl-runs/r7-mj

Checkpoint: update 119 of ppo, calibration rl-runs/calib-mj/calib.json.

Parity: rl-runs/r7-mj/pilot.json: 5 probes, the same move 5/5, largest logit difference 4.29e-6; ONNX: largest logit difference 3.34e-06.

## Test bench (seeds from 200000, 60 per family)

```
family     brain             n arrive%  hole   tip   off  tout  fail    secs  bumps  mbump   rim_m
ALL        expert          420    97.9     0     0     0     9     0    64.3   0.29   0.00   0.132
ALL        pilot           420    96.0     0     2     0    15     0    66.4   0.74   0.02   0.052
ALL        stick           420    93.1     0    17     0    12     0    70.6   4.66   0.04   0.074
clutter    expert           60   100.0     0     0     0     0     0    51.6   0.00   0.00  -1.000
clutter    pilot            60   100.0     0     0     0     0     0    57.4   1.85   0.00  -1.000
clutter    stick            60    91.7     0     3     0     2     0    60.6   7.22   0.00  -1.000
corners    expert           60   100.0     0     0     0     0     0    96.9   0.02   0.00  -1.000
corners    pilot            60   100.0     0     0     0     0     0    95.5   0.70   0.00  -1.000
corners    stick            60    98.3     0     1     0     0     0    95.9   0.23   0.00  -1.000
doorway    expert           60    98.3     0     0     0     1     0    68.9   0.88   0.00  -1.000
doorway    pilot            60    85.0     0     2     0     7     0    77.8   1.45   0.00  -1.000
doorway    stick            60    85.0     0     6     0     3     0    79.3  10.75   0.00  -1.000
low        expert           60   100.0     0     0     0     0     0    50.7   0.05   0.00  -1.000
low        pilot            60   100.0     0     0     0     0     0    49.3   0.12   0.00  -1.000
low        stick            60    98.3     0     0     0     1     0    55.8   0.17   0.00  -1.000
mixed      expert           60    88.3     0     0     0     7     0    71.9   1.05   0.00   0.132
mixed      pilot            60    86.7     0     0     0     8     0    85.8   1.07   0.02   0.052
mixed      stick            60    80.0     0     6     0     6     0    93.0  14.25   0.10   0.187
movers     expert           60   100.0     0     0     0     0     0    48.8   0.00   0.02  -1.000
movers     pilot            60   100.0     0     0     0     0     0    45.8   0.00   0.13  -1.000
movers     stick            60    98.3     0     1     0     0     0    54.3   0.00   0.20  -1.000
stairwell  expert           60    98.3     0     0     0     1     0    62.0   0.00   0.00   0.151
stairwell  pilot            60   100.0     0     0     0     0     0    57.7   0.00   0.00   0.117
stairwell  stick            60   100.0     0     0     0     0     0    60.0   0.00   0.00   0.074
```

## Shields: reckless brains (60 per family)

Falls into a hole: **0** (`hole`; `tip` is tipping over against something).

```
family     brain             n arrive%  hole   tip   off  tout  fail    secs  bumps  mbump   rim_m
ALL        reckless-back   420     0.0     0     4     0   416     0     0.0   0.10   0.30   0.270
ALL        reckless-random  420    20.2     0     0     0   335     0   159.1   1.09   0.09   0.078
ALL        reckless-straight  420    55.7     0     4     0   182     0   122.1   1.46   0.06   0.080
clutter    reckless-back    60     0.0     0     0     0    60     0     0.0   0.27   0.00  -1.000
clutter    reckless-random   60     8.3     0     0     0    55     0   110.3   2.07   0.00  -1.000
clutter    reckless-straight   60    46.7     0     1     0    31     0   110.7   2.53   0.00  -1.000
corners    reckless-back    60     0.0     0     0     0    60     0     0.0   0.00   0.00  -1.000
corners    reckless-random   60    33.3     0     0     0    40     0   235.0   0.53   0.00  -1.000
corners    reckless-straight   60   100.0     0     0     0     0     0   141.8   0.38   0.00  -1.000
doorway    reckless-back    60     0.0     0     0     0    60     0     0.0   0.05   0.00  -1.000
doorway    reckless-random   60     5.0     0     0     0    57     0   195.1   1.90   0.00  -1.000
doorway    reckless-straight   60    28.3     0     2     0    41     0   144.5   2.28   0.00  -1.000
low        reckless-back    60     0.0     0     0     0    60     0     0.0   0.00   0.00  -1.000
low        reckless-random   60    26.7     0     0     0    44     0   118.9   0.50   0.00  -1.000
low        reckless-straight   60    38.3     0     0     0    37     0   108.9   0.40   0.00  -1.000
mixed      reckless-back    60     0.0     0     2     0    58     0     0.0   0.03   0.43   0.338
mixed      reckless-random   60    11.7     0     0     0    53     0   138.7   2.30   0.15   0.141
mixed      reckless-straight   60    21.7     0     1     0    46     0   168.3   4.42   0.07   0.080
movers     reckless-back    60     0.0     0     2     0    58     0     0.0   0.38   1.68  -1.000
movers     reckless-random   60    11.7     0     0     0    53     0    96.7   0.27   0.45  -1.000
movers     reckless-straight   60    60.0     0     0     0    24     0    98.6   0.18   0.37  -1.000
stairwell  reckless-back    60     0.0     0     0     0    60     0     0.0   0.00   0.00   0.270
stairwell  reckless-random   60    45.0     0     0     0    33     0   153.3   0.03   0.00   0.078
stairwell  reckless-straight   60    95.0     0     0     0     3     0   110.0   0.00   0.00   0.086
```
