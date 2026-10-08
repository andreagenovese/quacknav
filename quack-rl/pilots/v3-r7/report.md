# Pilot report: rl-runs/r7

Checkpoint: update 319 of ppo, calibration the paper twin's defaults.

Parity: rl-runs/r7/pilot.json: 5 probes, the same move 5/5, largest logit difference 3.10e-6; ONNX: largest logit difference 2.38e-06.

## Test bench (seeds from 200000, 60 per family)

```
family     brain             n arrive%  hole   tip   off  tout  fail    secs  bumps  mbump   rim_m
ALL        expert          420    94.0     0     1     0    23     1    73.2   0.59   0.00   0.124
ALL        pilot           420    96.7     0     4     0    10     0    75.2   1.30   0.03   0.074
ALL        stick           420    93.3     0    20     0     7     1    81.3   2.82   0.20   0.075
clutter    expert           60    95.0     0     0     0     3     0    56.6   0.05   0.00  -1.000
clutter    pilot            60   100.0     0     0     0     0     0    64.9   4.07   0.00  -1.000
clutter    stick            60    91.7     0     5     0     0     0    68.3   5.30   0.00  -1.000
corners    expert           60    96.7     0     0     0     1     1   105.3   0.02   0.00  -1.000
corners    pilot            60   100.0     0     0     0     0     0   104.9   0.12   0.00  -1.000
corners    stick            60    98.3     0     0     0     0     1   111.9   0.23   0.00  -1.000
doorway    expert           60    93.3     0     0     0     4     0    81.9   1.70   0.00  -1.000
doorway    pilot            60    91.7     0     0     0     5     0    85.0   1.48   0.00  -1.000
doorway    stick            60    88.3     0     5     0     2     0    94.6   7.60   0.00  -1.000
low        expert           60    96.7     0     0     0     2     0    57.1   0.07   0.00  -1.000
low        pilot            60    96.7     0     1     0     1     0    59.0   0.18   0.00  -1.000
low        stick            60    96.7     0     2     0     0     0    64.6   0.52   0.00  -1.000
mixed      expert           60    85.0     0     1     0     8     0    83.2   2.12   0.02   0.163
mixed      pilot            60    88.3     0     3     0     4     0    95.6   3.28   0.02   0.074
mixed      stick            60    83.3     0     7     0     3     0    98.3   6.08   0.40   0.135
movers     expert           60    96.7     0     0     0     2     0    59.1   0.17   0.02  -1.000
movers     pilot            60   100.0     0     0     0     0     0    52.2   0.00   0.22  -1.000
movers     stick            60    98.3     0     1     0     0     0    62.2   0.00   1.02  -1.000
stairwell  expert           60    95.0     0     0     0     3     0    70.2   0.00   0.00   0.124
stairwell  pilot            60   100.0     0     0     0     0     0    67.3   0.00   0.00   0.112
stairwell  stick            60    96.7     0     0     0     2     0    71.8   0.00   0.00   0.075
```

## Shields: reckless brains (60 per family)

Falls into a hole: **0** (`hole`; `tip` is tipping over against something).

```
family     brain             n arrive%  hole   tip   off  tout  fail    secs  bumps  mbump   rim_m
ALL        reckless-back   420     0.0     0     3     0   417     0     0.0   0.11   0.28   0.227
ALL        reckless-random  420    12.4     0    16     0   350     2   176.8   3.11   0.06   0.075
ALL        reckless-straight  420    40.7     0    19     0   229     1   139.7   5.88   0.19   0.103
clutter    reckless-back    60     0.0     0     1     0    59     0     0.0   0.22   0.00  -1.000
clutter    reckless-random   60     8.3     0     4     0    51     0   149.7   5.92   0.00  -1.000
clutter    reckless-straight   60    26.7     0     6     0    37     1   126.7   8.82   0.00  -1.000
corners    reckless-back    60     0.0     0     0     0    60     0     0.0   0.00   0.00  -1.000
corners    reckless-random   60    25.0     0     2     0    43     0   239.6   4.78   0.00  -1.000
corners    reckless-straight   60    93.3     0     1     0     3     0   170.2   5.32   0.00  -1.000
doorway    reckless-back    60     0.0     0     0     0    60     0     0.0   0.15   0.00  -1.000
doorway    reckless-random   60     1.7     0     2     0    57     0   108.7   3.45   0.00  -1.000
doorway    reckless-straight   60    16.7     0     2     0    48     0   136.3   7.77   0.00  -1.000
low        reckless-back    60     0.0     0     0     0    60     0     0.0   0.00   0.00  -1.000
low        reckless-random   60     3.3     0     0     0    58     0    81.9   1.67   0.00  -1.000
low        reckless-straight   60    20.0     0     1     0    47     0   141.8   4.08   0.00  -1.000
mixed      reckless-back    60     0.0     0     1     0    59     0     0.0   0.10   0.45   0.333
mixed      reckless-random   60     3.3     0     4     0    53     1   254.3   3.70   0.07   0.093
mixed      reckless-straight   60     8.3     0     6     0    49     0   178.5  10.48   0.98   0.107
movers     reckless-back    60     0.0     0     1     0    59     0     0.0   0.33   1.52  -1.000
movers     reckless-random   60    15.0     0     3     0    48     0   124.3   0.75   0.37  -1.000
movers     reckless-straight   60    40.0     0     3     0    33     0   101.4   2.55   0.32  -1.000
stairwell  reckless-back    60     0.0     0     0     0    60     0     0.0   0.00   0.00   0.227
stairwell  reckless-random   60    30.0     0     1     0    40     1   164.1   1.48   0.00   0.075
stairwell  reckless-straight   60    80.0     0     0     0    12     0   123.9   2.12   0.00   0.103
```
