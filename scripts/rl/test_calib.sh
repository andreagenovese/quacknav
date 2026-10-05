#!/usr/bin/env bash
# test_calib.sh: the calibration checked end to end, before any duck.
# A simulated duck with deliberately wrong numbers (testdata/fake-duck.json)
# drives journeys with QK_RL_TRACE on — the stick and a reckless brain, so
# that back-offs are traced too — and rl_calib must find its numbers again
# within the tolerances below.
set -euo pipefail
cd "$(dirname "$0")/../.."
cargo build --release -p quack-rl
T=$(mktemp -d)
QK_RL_TRACE=$T/traces target/release/rl_eval --seeds 3 --stick --threads 1 \
    --calib scripts/rl/testdata/fake-duck.json --families clutter,doorway,corners > /dev/null
QK_RL_TRACE=$T/traces target/release/rl_eval --seeds 3 --reckless random --threads 1 \
    --calib scripts/rl/testdata/fake-duck.json --families clutter,movers,stairwell > /dev/null
target/release/rl_calib --out "$T/out" "$T"/traces/*.jsonl > /dev/null
python3 - "$T/out/calib.json" scripts/rl/testdata/fake-duck.json <<'PY'
import json, sys
fit = json.load(open(sys.argv[1]))
want = json.load(open(sys.argv[2]))
# number: (absolute tolerance), measured against the fake duck's truth.
tol = {"speed_at_03": 0.008, "straight_veer": 0.01, "pulse_gain_mean": 0.1, "turn_left_rad_s": 0.05,
       "turn_right_rad_s": 0.05, "back_speed": 0.01, "tof_hz": 0.5, "tof_range_bias": 0.03}
bad = []
for k, t in tol.items():
    got = fit["calib"][k]
    ok = abs(got - want[k]) <= t and fit["fit"][k]["fitted"]
    print(f"{k:18s} truth {want[k]:8.4f}  fitted {got:8.4f}  {'ok' if ok else 'OFF'}")
    if not ok:
        bad.append(k)
r = fit["replay"]
print(f"replay yaw RMSE {r['prior_rmse_yaw_rad']:.4f} -> {r['fit_rmse_yaw_rad']:.4f} rad")
if r["fit_rmse_yaw_rad"] >= r["prior_rmse_yaw_rad"]:
    bad.append("replay")
sys.exit(f"calibration off: {bad}" if bad else 0)
PY
echo "calibration test: passed"
