"""paper_twin_gate.py <paper_twin binary> <world.json> <out dir>

The paper twin as a gate: the same seeds on every push, so the numbers move
only when the code does. Two benches:

  explore  40 seeds of 1200 s from nothing: no fall, and the mean coverage
           no lower than the bar (12 seeds were too few: their mean moved by
           2.5 points between two builds whose 40-seed means agree);
  go_to    30 seeds on the known world with the books (the journey bench),
           to the goal beyond house2's stairwell: no fall, and at least the
           bar of arrivals.

The bars are what the code achieved when they were set, less a margin for
the noise; raise them when a change improves the numbers, and say so in the
commit. Lowering one is a decision, not a fix. Last set 2026-09-25 with the
layered costmap (ADR 0009, step 2a).
"""
import re
import subprocess
import sys

BARS = {
    "explore_runs": 40,
    "explore_budget_s": 1200,
    # measured 37.8 (sd 5.2 a run): two standard errors of margin.
    "explore_mean_cover_pct": 36.0,
    "goto_runs": 30,
    # measured 30 / 30 on macOS (26 before the layered costmap); one of
    # margin for another platform's libm (the twin is chaotic: a
    # last-digit difference in a sine moves a seed).
    "goto_arrived": 29,
}
GOAL = "-2.64,-2.12"

binary, world, out = sys.argv[1], sys.argv[2], sys.argv[3]
failed = []


def run(args):
    p = subprocess.run([binary, world, *args], capture_output=True, text=True)
    if p.returncode != 0:
        print(p.stdout[-2000:], p.stderr[-2000:])
        sys.exit(f"paper_twin exited {p.returncode}")
    return p.stdout + p.stderr


text = run([f"{out}/explore", "--runs", str(BARS["explore_runs"]), "--budget", str(BARS["explore_budget_s"])])
rows = [l.split() for l in text.splitlines() if re.match(r"^\s*\d+\s+\d+\s+\S+", l)]
covers = [float(r[6]) for r in rows]
falls = sum("FELL" in l for l in text.splitlines())
mean_cover = sum(covers) / max(len(covers), 1)
print(f"explore: {len(rows)} runs, {falls} falls, mean coverage {mean_cover:.1f} % (bar {BARS['explore_mean_cover_pct']})")
if len(rows) != BARS["explore_runs"]:
    failed.append(f"explore: {len(rows)} summary rows, expected {BARS['explore_runs']}")
if falls:
    failed.append(f"explore: {falls} falls")
if mean_cover < BARS["explore_mean_cover_pct"]:
    failed.append(f"explore: mean coverage {mean_cover:.1f} % under the bar")

text = run([f"{out}/goto", "--runs", str(BARS["goto_runs"]), "--goto", GOAL, "--books", "--known"])
lines = [l for l in text.splitlines() if l.startswith("goto seed")]
arrived = sum("Done — arrived" in l for l in lines)
falls = sum("FELL" in l for l in lines)
print(f"go_to: {arrived}/{len(lines)} arrived, {falls} falls (bar {BARS['goto_arrived']})")
for l in lines:
    if "Done — arrived" not in l:
        print("  " + l.split(";")[0])
if falls:
    failed.append(f"go_to: {falls} falls")
if arrived < BARS["goto_arrived"]:
    failed.append(f"go_to: {arrived} arrivals under the bar of {BARS['goto_arrived']}")

if failed:
    sys.exit("paper twin gate failed:\n  " + "\n  ".join(failed))
print("paper twin gate: passed")
