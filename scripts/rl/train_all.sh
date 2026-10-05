#!/usr/bin/env bash
# train_all.sh RUN_DIR [train.py options...]: the pilot from nothing —
# imitation of the expert (DAgger), PPO on the reward, then finalize.py
# (the pilot file, its checks, the test bench, the shields' bench).
set -euo pipefail
cd "$(dirname "$0")/../.."
RUN=$1; shift
PY=scripts/rl/.venv/bin/python
CALIB=()
args=("$@")
for ((i = 0; i < ${#args[@]}; i++)); do
  if [[ ${args[i]} == --calib ]]; then CALIB=(--calib "${args[i+1]}"); fi
done
cargo build --release -p quack-rl
mkdir -p "$RUN"
$PY scripts/rl/train.py bc --out "$RUN" "$@"
$PY scripts/rl/train.py ppo --out "$RUN" --init "$RUN/bc.pt" "$@"
$PY scripts/rl/finalize.py --run "$RUN" "${CALIB[@]+"${CALIB[@]}"}"
