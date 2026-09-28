"""wake_bench.py <name> <scene> <state> <port> <truth> <out> <maps_dir> <book.json> [limit_s]

Wake-ups on the twin from spots across the house (the user's, 2026-09-28:
the duck is almost always carried and switched on somewhere else): for
each spawn the saved map and its book are put back, the twin is booted
with the duck there (`MICRODUCK_START`), and the homecoming is timed and
judged against the truth — right (confirmed within 0.30 m and 20°),
wrong (confirmed further off), never (not confirmed in `limit_s`).

Spawns: the truth's goals, each with its own heading (0°, 90°, 180°,
-90° in turn); `WAKE_SPAWNS="x,y,yaw_rad;..."` overrides them.

Kept per wake-up in <out>: the recording (`wake-<i>.mdlg`), the pose
sampler's files (`wake-<i>.pose.tsv[.untracked]`) and the navd log, for
the replay bench (`trajectory` with `MAP_SESSION` and `MAP_LOAD_AT_S`:
the load's offset into the recording is in `wakes.json`).
"""
import json, math, os, re, shutil, sys, time

name, scene, state, port, truth_p, out, maps_dir, book_p = sys.argv[1:9]
limit_s = float(sys.argv[9]) if len(sys.argv) > 9 else 480.0
HERE = os.path.dirname(os.path.abspath(__file__))
S = os.environ.get("TWIN_WORK", "/tmp/quack-twin-work")
sys.argv = ["run_house.py", name, scene, state, port, truth_p, out, "0", "0"]
exec(open(f"{HERE}/run_house.py").read().split("# ── 1. the exploration")[0])

RIGHT_M, RIGHT_RAD = 0.30, math.radians(20)
HOME_KEYS = ("the map frozen, navigating", "the map is frozen; navigating", "the pose is confirmed on the saved map")
GAVE_UP = ("standing down",)


def spawns():
    if os.environ.get("WAKE_SPAWNS"):
        return [tuple(float(v) for v in s.split(",")) for s in os.environ["WAKE_SPAWNS"].split(";") if s.strip()]
    yaws = (0.0, math.pi / 2, math.pi, -math.pi / 2)
    return [(x, y, yaws[i % 4]) for i, (x, y) in enumerate(truth["goals"].values())]


def last_pose_row():
    try:
        rows = [r.split("\t") for r in open(f"{out}/pose.tsv").read().strip().splitlines()]
        return rows[-1] if rows else None
    except FileNotFoundError:
        return None


def wake(i, x, y, yaw):
    for f in ("pose.tsv", "pose.tsv.untracked"):
        try: os.remove(f"{out}/{f}")
        except FileNotFoundError: pass
    shutil.rmtree(f"{state}/maps", ignore_errors=True); shutil.copytree(maps_dir, f"{state}/maps")
    shutil.copy(book_p, f"{state}/ground.json")
    shutil.rmtree(f"{state}/rec", ignore_errors=True)
    sp = boot(WIPE="on", MAPLOC_MODE="stop_and_scan", HOMECOMING="on", RESUME="on", MICRODUCK_START=f"{x},{y},{yaw}")
    t0 = time.time(); verdict, t = "never", limit_s
    while time.time() - t0 < limit_s:
        log = navlog()
        if any(k in log for k in HOME_KEYS):
            t = time.time() - t0; verdict = "confirmed"; break
        if any(k in log for k in GAVE_UP):
            t = time.time() - t0; verdict = "never"; break
        time.sleep(3)
    time.sleep(6)  # a pose sample after the confirmation
    row = last_pose_row()
    err = yaw_err = float("nan")
    if row and len(row) > 9:
        err = float(row[5])
        yaw_err = abs(math.atan2(math.sin(float(row[8]) - float(row[9])), math.cos(float(row[8]) - float(row[9]))))
    if verdict == "confirmed":
        verdict = "right" if err <= RIGHT_M and yaw_err <= RIGHT_RAD else "wrong"
    log = navlog()
    # The load's offset into the recording, for the replay bench.
    ts = lambda k: next((l[:26] for l in log.splitlines() if k in l), None)
    rec_at, load_at = ts("maploc: recording session"), ts("map adopted from the library")
    load_s = None
    if rec_at and load_at:
        from datetime import datetime
        load_s = (datetime.fromisoformat(load_at) - datetime.fromisoformat(rec_at)).total_seconds()
    recs = sorted(f for f in os.listdir(f"{state}/rec") if f.endswith(".mdlg")) if os.path.isdir(f"{state}/rec") else []
    down(sp, f"wake-{i}")
    if recs:
        shutil.copy(f"{state}/rec/{recs[-1]}", f"{out}/wake-{i}.mdlg")
    for f in ("pose.tsv", "pose.tsv.untracked"):
        if os.path.exists(f"{out}/{f}"):
            shutil.copy(f"{out}/{f}", f"{out}/wake-{i}.{f}")
    r = {"i": i, "spawn": [x, y, round(math.degrees(yaw))], "verdict": verdict, "s": round(t), "err_m": round(err, 3),
         "yaw_err_deg": round(math.degrees(yaw_err), 1), "load_at_s": load_s, "falls": falls(log)}
    say(f"{name} wake {i} at ({x:+.2f},{y:+.2f},{math.degrees(yaw):+.0f}°): {verdict} in {t:.0f} s, error {err:.2f} m {math.degrees(yaw_err):.0f}°, falls {r['falls']}")
    return r


results = []
for i, (x, y, yaw) in enumerate(spawns(), 1):
    results.append(wake(i, x, y, yaw))
    json.dump(results, open(f"{out}/wakes.json", "w"), indent=1)
n = len(results)
right = [r for r in results if r["verdict"] == "right"]
med = lambda v: sorted(v)[len(v) // 2] if v else float("nan")
say(f"{name}: WAKES right {len(right)}/{n}, wrong {sum(r['verdict'] == 'wrong' for r in results)}, never {sum(r['verdict'] == 'never' for r in results)}; median to right {med([r['s'] for r in right]):.0f} s")
say(f"{name}: DONE")
