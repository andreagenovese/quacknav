"""finalize.py: a trained run made ready to fly, and measured.

    finalize.py --run DIR [--calib FILE] [--compare OLD_PILOT.json] [--seeds 60]

From DIR/best.pt: DIR/pilot.json (what quack-navd loads, `QK_RL_POLICY`) with
probe observations, checked by `rl_pilot_check` (this build computes what
training computed); DIR/pilot.onnx, checked against torch with onnxruntime;
then the test bench — seeds never used to train or to choose the
checkpoint (from 200000) — the stick, the expert and the pilot (and
`--compare`, an older pilot) through quack-navd's own loop; and the shields'
bench: reckless brains (random, always back, always straight) that must
never fall. Writes DIR/report.md and DIR/report.json.
"""
import argparse
import json
import os
import subprocess
import sys

import numpy as np
import torch

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import pilot as P  # noqa: E402


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--run", required=True)
    ap.add_argument("--calib")
    ap.add_argument("--compare")
    ap.add_argument("--seeds", type=int, default=60)
    ap.add_argument("--reckless-seeds", type=int, default=60)
    args = ap.parse_args()
    net, meta = P.load(os.path.join(args.run, "best.pt"))
    rng = np.random.default_rng(0)
    # Probe observations: what the bench's own journeys look like.
    env = P.VecEnv(8, seed=777, level=3, calib=args.calib, spread="none")
    probe = []
    for _ in range(20):
        obs = env.last[0]
        probe.append(obs[rng.integers(0, env.n)])
        with torch.no_grad():
            env.step(net.logits(torch.as_tensor(obs)).argmax(-1).numpy())
    env.close()
    probe = np.asarray(probe[::4], dtype=np.float32)
    pj = os.path.join(args.run, "pilot.json")
    P.export_json(net, pj, dict(meta, finalized=True), probe=probe)
    check = subprocess.run([P.binary("rl_pilot_check"), pj], capture_output=True, text=True)
    print(check.stdout.strip() or check.stderr.strip())
    if check.returncode != 0:
        sys.exit("rl_pilot_check failed")
    onnx_path = os.path.join(args.run, "pilot.onnx")
    P.export_onnx(net, onnx_path)
    import onnxruntime as ort

    sess = ort.InferenceSession(onnx_path)
    got = sess.run(None, {"obs": probe})[0]
    with torch.no_grad():
        want = net.logits(torch.as_tensor(probe)).numpy()
    onnx_err = float(np.abs(got - want).max())
    print(f"onnx: largest logit difference {onnx_err:.2e}")
    extra = ["--stick", "--expert"]
    if args.compare:
        extra += ["--pilot", args.compare]
    text, summary = P.evaluate(pj, seeds=args.seeds, seed0=200000, calib=args.calib, extra=extra)
    print(text)
    rtext, rsummary = P.evaluate(None, seeds=args.reckless_seeds, seed0=300000, calib=args.calib, extra=["--reckless", "random", "--reckless", "back", "--reckless", "straight"])
    print(rtext)
    reckless_falls = sum(r["fell"] for r in rsummary if r["family"] == "ALL")
    report = {"run": args.run, "meta": meta, "calib": args.calib, "test": summary, "reckless": rsummary, "reckless_falls": reckless_falls, "onnx_max_diff": onnx_err, "parity": check.stdout.strip()}
    with open(os.path.join(args.run, "report.json"), "w") as f:
        json.dump(report, f, indent=1)
    with open(os.path.join(args.run, "report.md"), "w") as f:
        f.write(f"# Pilot report: {args.run}\n\n")
        calib = args.calib or "the paper twin's defaults"
        f.write(f"Checkpoint: update {meta.get('update')} of {meta.get('stage')}, calibration {calib}.\n\n")
        f.write(f"Parity: {check.stdout.strip()}; ONNX: largest logit difference {onnx_err:.2e}.\n\n")
        f.write(f"## Test bench (seeds from 200000, {args.seeds} per family)\n\n```\n{text}```\n\n")
        f.write(f"## Shields: reckless brains ({args.reckless_seeds} per family)\n\nFalls: **{reckless_falls}**.\n\n```\n{rtext}```\n")
    print(f"report: {os.path.join(args.run, 'report.md')}")
    if reckless_falls:
        sys.exit("a reckless brain fell: the shields do not hold")


if __name__ == "__main__":
    main()
