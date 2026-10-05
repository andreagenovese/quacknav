"""gate.py: whether a pilot retrained on the duck's calibration may fly.

    gate.py --report RUN/report.json --new RUN/pilot.json [--old OLD.json] --out DIR

The rules, on the test bench of finalize.py (the calibrated simulator):
no fall, ever; at least as many arrivals as the stick; no more than 2
points under the pilot it replaces. Passed: DIR/pilot.json, and how to put
it on the duck. Not passed: why, and what flies meanwhile (the old pilot if
it passes the same rules on the calibrated simulator, else the stick).
"""
import argparse
import json
import shutil
import sys


def row(summary, brain):
    return next((r for r in summary if r["family"] == "ALL" and r["brain"] == brain), None)


def passes(r, stick, margin_to=None):
    why = []
    if r["fell"]:
        why.append(f"{r['fell']} falls")
    if r["arrived"] < stick["arrived"]:
        why.append(f"{r['arrived']} arrivals against the stick's {stick['arrived']}")
    if margin_to is not None and r["arrived"] < margin_to["arrived"] - 0.02 * r["n"]:
        why.append(f"{r['arrived']} arrivals against the old pilot's {margin_to['arrived']}")
    return why


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--report", required=True)
    ap.add_argument("--new", required=True)
    ap.add_argument("--old")
    ap.add_argument("--out", required=True)
    args = ap.parse_args()
    rep = json.load(open(args.report))
    test = rep["test"]
    stick = row(test, "stick")
    new = row(test, "pilot")
    old = (row(test, "old-pilot") or next((r for r in test if r["family"] == "ALL" and r["brain"].endswith("/pilot")), None)) if args.old else None
    if rep.get("reckless_falls"):
        sys.exit(f"the shields let a reckless brain fall {rep['reckless_falls']} times on the calibrated simulator: nothing flies until that is fixed")
    lines = [f"stick: {stick['arrived']}/{stick['n']} arrived, {stick['fell']} falls",
             f"new pilot: {new['arrived']}/{new['n']} arrived, {new['fell']} falls"]
    if old:
        lines.append(f"old pilot: {old['arrived']}/{old['n']} arrived, {old['fell']} falls")
    print("\n".join(lines))
    why = passes(new, stick, old)
    if not why:
        shutil.copy(args.new, f"{args.out}/pilot.json")
        print(f"""
PASSED: {args.out}/pilot.json may fly.
On the duck:
  scp {args.out}/pilot.json microduck@<duck>:/tmp/pilot.json
  ssh microduck@<duck> 'sudo install -m 0644 /tmp/pilot.json /var/lib/quack-nav/pilot.json'
  then QK_RL_POLICY=/var/lib/quack-nav/pilot.json in the knobs (quack-control's page, or a
  line in /var/lib/quack-nav/knobs.env) and `sudo systemctl restart quack-navd`.""")
        return
    print("\nNOT PASSED: " + "; ".join(why))
    if old and not passes(old, stick):
        print("The old pilot passes on the calibrated simulator: it keeps flying.")
    else:
        print("Nothing passes on the calibrated simulator: fly the stick (QK_RL_POLICY unset).")
    sys.exit(1)


if __name__ == "__main__":
    main()
