#!/usr/bin/env bash
# calibrate.sh OUT_DIR PILOT_RUN TRACE.jsonl...: the pilot tuned to the duck.
#
# 1. rl_calib fits the simulator's numbers to the duck's traces
#    (QK_RL_TRACE): OUT_DIR/calib.json and calib.md;
# 2. the pilot of PILOT_RUN (its best.pt) goes on training in the simulator
#    with those numbers, varied narrowly around them;
# 3. finalize.py: the new pilot's file, its checks, the test bench on the
#    calibrated simulator against the stick and the old pilot, the shields;
# 4. gate.py: it flies only if it never falls and is no worse than either.
#
# UPDATES (default 150) sets the training's length; GAIT (alpha, velstand)
# the walk the traces are of — the pilot then flies that walk only.
set -euo pipefail
cd "$(dirname "$0")/../.."
OUT=$1; RUN=$2; shift 2
PY=scripts/rl/.venv/bin/python
cargo build --release -p quack-rl
mkdir -p "$OUT/train"
target/release/rl_calib --out "$OUT" "$@"
# GAIT=velstand: the traces are of that walk (quack-navd's [gait] profile).
if [[ -n "${GAIT:-}" ]]; then
  $PY - "$OUT/calib.json" "$GAIT" <<'PYEOF'
import json, sys
p, g = sys.argv[1], sys.argv[2]
v = json.load(open(p))
(v["calib"] if "calib" in v else v)["gait"] = g
json.dump(v, open(p, "w"), indent=1)
PYEOF
fi
cp "$RUN/pilot.json" "$OUT/old-pilot.json"
$PY scripts/rl/train.py ppo --out "$OUT/train" --init "$RUN/best.pt" --calib "$OUT/calib.json" \
    --spread calibrated --updates "${UPDATES:-150}" --bc0 0.2 --bc-decay 60
$PY scripts/rl/finalize.py --run "$OUT/train" --calib "$OUT/calib.json" --compare "$OUT/old-pilot.json"
$PY scripts/rl/gate.py --report "$OUT/train/report.json" --new "$OUT/train/pilot.json" --old "$OUT/old-pilot.json" --out "$OUT"
