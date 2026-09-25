"""pt_bench.py <paper_twin binary> <out dir> [label] [--runs N] [--explore K]

The paper twin as a bench for a change to the route or its following:
a go_to to a point in every part of the apartment, N seeds each, on the
known world with the books (the journey bench), and K explorations from
nothing. What comes out, per goal and in total:

  arrived   journeys that ended at the goal
  falls     journeys that ended in a hole (must be 0)
  time      median seconds of a journey that arrived
  detour    walked / planned: 1.0 is the route as planned
  refusals  mean per journey (the guards saying no)
  hole, box the nearest the body came to a hole's edge and a box's face
            (walls are boxes), the worst journey and the median

Everything is written to <out>/bench.json as well, to diff against another
build's (`--compare a.json b.json`). The environment reaches the twin, so a
`QK_*` knob measures a variant: `QK_RPP=1 pt_bench.py ... rpp`.
"""
import json
import os
import re
import statistics as st
import subprocess
import sys

# Each at least 0.35 m from any box, wall or hole (the first pick for
# "east" was inside the bed).
GOALS = {
    "west": (-2.64, -2.12),
    "kitchen": (-2.5, 2.1),
    "corridor_n": (-0.2, 1.5),
    "corridor_s": (-0.2, -2.2),
    "east": (2.2, 2.35),
    "bath": (2.2, -2.0),
}
LINE = re.compile(
    r"goto seed (\d+): (\w+) — .*?planned ([\d.]+) m, walked ([\d.]+) m in (\d+) s, (\d+) legs, (\d+) refusals, "
    r"ended ([\d.]+) m away, nearest hole ([\d.inf]+) m, nearest box ([\d.inf]+) m(, FELL)?")
WORLD = os.path.join(os.path.dirname(__file__), "../../quack-nav/examples/apartment.world.json")


def run_goal(binary, out, name, goal, runs):
    args = [binary, WORLD, f"{out}/{name}", "--runs", str(runs), "--goto", f"{goal[0]},{goal[1]}", "--books", "--known"]
    p = subprocess.run(args, capture_output=True, text=True)
    rows = []
    for l in (p.stdout + p.stderr).splitlines():
        m = LINE.search(l)
        if m:
            rows.append({
                "seed": int(m[1]), "arrived": m[2] == "Done" and "arrived" in l, "planned_m": float(m[3]),
                "walked_m": float(m[4]), "secs": int(m[5]), "legs": int(m[6]), "refusals": int(m[7]),
                "end_m": float(m[8]), "hole_m": float(m[9]), "box_m": float(m[10]), "fell": bool(m[11]),
            })
    return rows


def summary(rows):
    ok = [r for r in rows if r["arrived"]]
    med = lambda v: st.median(v) if v else float("nan")
    return {
        "journeys": len(rows), "arrived": len(ok), "falls": sum(r["fell"] for r in rows),
        "time_s": med([r["secs"] for r in ok]),
        "detour": med([r["walked_m"] / r["planned_m"] for r in ok if r["planned_m"] > 0]),
        "refusals": st.mean([r["refusals"] for r in rows]) if rows else float("nan"),
        "hole_min_m": min((r["hole_m"] for r in rows), default=float("nan")),
        "hole_med_m": med([r["hole_m"] for r in rows]),
        "box_min_m": min((r["box_m"] for r in rows), default=float("nan")),
        "box_med_m": med([r["box_m"] for r in rows]),
    }


def show(label, s):
    print(f"  {label:<11} {s['arrived']:>3}/{s['journeys']:<3} falls {s['falls']}  time {s['time_s']:5.0f} s  detour {s['detour']:.2f}  "
          f"refusals {s['refusals']:5.1f}  hole min {s['hole_min_m']:.2f} med {s['hole_med_m']:.2f}  box min {s['box_min_m']:.2f} med {s['box_med_m']:.2f}")


def main():
    if sys.argv[1] == "--compare":
        a, b = json.load(open(sys.argv[2])), json.load(open(sys.argv[3]))
        for key in list(GOALS) + ["total"]:
            print(key)
            show(a["label"], a["goals"][key] if key != "total" else a["total"])
            show(b["label"], b["goals"][key] if key != "total" else b["total"])
        return
    binary, out = sys.argv[1], sys.argv[2]
    label = sys.argv[3] if len(sys.argv) > 3 and not sys.argv[3].startswith("--") else "build"
    runs = int(sys.argv[sys.argv.index("--runs") + 1]) if "--runs" in sys.argv else 15
    explore = int(sys.argv[sys.argv.index("--explore") + 1]) if "--explore" in sys.argv else 0
    os.makedirs(out, exist_ok=True)
    result = {"label": label, "runs": runs, "goals": {}, "env": {k: v for k, v in os.environ.items() if k.startswith("QK_")}}
    every = []
    print(f"{label}: {runs} seeds per goal")
    for name, goal in GOALS.items():
        rows = run_goal(binary, out, name, goal, runs)
        every += rows
        result["goals"][name] = summary(rows)
        show(name, result["goals"][name])
    result["total"] = summary(every)
    show("total", result["total"])
    if explore:
        p = subprocess.run([binary, WORLD, f"{out}/explore", "--runs", str(explore), "--budget", "1200"], capture_output=True, text=True)
        text = p.stdout + p.stderr
        rows = [l.split() for l in text.splitlines() if re.match(r"^\s*\d+\s+\d+\s+\S+", l)]
        covers = [float(r[6]) for r in rows]
        result["explore"] = {"runs": len(rows), "falls": text.count("FELL"), "mean_cover_pct": st.mean(covers) if covers else float("nan")}
        print(f"  explore     {len(rows)} runs, falls {result['explore']['falls']}, mean coverage {result['explore']['mean_cover_pct']:.1f} %")
    json.dump(result, open(f"{out}/bench.json", "w"), indent=1)


if __name__ == "__main__":
    main()
