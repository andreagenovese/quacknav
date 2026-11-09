"""findme_bench.py <name> <scene> <state> <port> <truth> <out> <maps_dir> <book.json> [limit_s]

Does the duck find itself again after a fall or a carry? On the twin, the
saved map frozen (the navigation's boot), the duck at home; then, trial by
trial, at the truth's goals in turn:

  carry      standing idle, carried to the goal's spot and turned: by a
             hand (the simulator's `carry`: lifted, carried, set down on its
             feet; robotd's pick-up detector sees it as on the duck), or with
             `FINDME_CARRY=teleport` moved there in one step (`kidnap`, the
             daemon none the wiser);
  fall       shoved where it stands until it falls (the simulator's `shove`);
  fallcarry  shoved, and while down carried to the goal's spot, set down
             as it lay.

Then a go_to the next goal, as a user would ask, and the verdict, on the
first pose it trusts after it stopped trusting the old one: `right`
(within 0.30 m and 20° of the truth), `wrong` (further off), `never` (not
trusted again within `limit_s`), `down` (it did not get up); `unnoticed`
when it never stopped trusting a pose that is wrong (still off 60 s on),
`right` with 0 s when the pose it kept was right. Per trial: seconds to the trusted pose, the
error, the go_to's end. `FINDME="carry,fall,fallcarry"` picks the kinds
(all by default), `FINDME_N` the trials per kind (the goals, by default),
`FINDME_SHOVE` the push (JSON: vx, vy m/s, wx, wy rad/s). A duck still
lost at a trial's end is booted home again for the next.
"""
import json, math, os, re, shutil, socket, subprocess, sys, time

name, scene, state, port, truth_p, out, maps_dir, book_p = sys.argv[1:9]
port = int(port)
limit_s = float(sys.argv[9]) if len(sys.argv) > 9 else 300.0
REPO = os.environ.get("QN_REPO", os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..")))
T, HERE = f"{REPO}/scripts/twin", os.path.dirname(os.path.abspath(__file__))
truth = json.load(open(truth_p))
os.makedirs(out, exist_ok=True)
base = dict(os.environ, SCENE=scene, STATE=state, PORT=str(port), POSEERR_DT=os.environ.get("POSEERR_DT", "1"))
NAV = f"{state}/nav.sock"
RIGHT_M, RIGHT_RAD = 0.30, math.radians(20)
HOME_KEYS = ("the map frozen, navigating", "the map is frozen; navigating", "the pose is confirmed on the saved map")
SHOVE = json.loads(os.environ.get("FINDME_SHOVE", '{"vy": 1.2, "wx": 8.0}'))
CARRY = os.environ.get("FINDME_CARRY", "hand")
DOWN_Z = 0.08
goals = list(truth["goals"].values())


def say(*a):
    print(*a, flush=True)


def call(tool, args=None, timeout=60):
    cmd = ["python3", f"{T}/call.py", NAV, tool, json.dumps(args or {})]
    try:
        r = json.loads(subprocess.run(cmd, capture_output=True, text=True, timeout=timeout).stdout.strip())
        return r if isinstance(r, dict) else {}
    except Exception as e:
        return {"error": str(e)}


def navlog():
    try:
        return re.sub(r"\x1b\[[0-9;]*m", "", open(f"{state}/navd.log", errors="ignore").read())
    except FileNotFoundError:
        return ""


def twin(cmd, **env):
    return subprocess.run([f"{T}/twin.sh", cmd], capture_output=True, text=True, env=dict(base, **env))


def boot(**env):
    twin("down"); time.sleep(4)
    if os.path.exists(f"{state}/navd.log"):
        os.remove(f"{state}/navd.log")
    r = twin("up", **env)
    if r.returncode != 0:
        say("  twin up failed:", r.stderr.strip()[-300:])
        return None
    time.sleep(3); twin("enable")
    return subprocess.Popen(["python3", f"{HERE}/poseerr.py", NAV, str(port), f"{out}/pose.tsv"],
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, env=base)


def down(sampler, tag):
    if sampler:
        sampler.kill()
    if os.path.exists(f"{state}/navd.log"):
        shutil.copy(f"{state}/navd.log", f"{out}/navd-{tag}.log")
    twin("down")


class Sim:
    def __init__(self):
        self.b = socket.socket(); self.b.settimeout(3); self.b.connect(("127.0.0.1", port)); self.f = self.b.makefile("rw")
        self.op({"op": "hello", "protocol": 1, "joints": 15})

    def op(self, req):
        self.f.write(json.dumps(req) + "\n"); self.f.flush(); return json.loads(self.f.readline())

    def pose(self):
        r = self.op({"op": "read"}); w, x, y, z = r["imu"]["quat"]
        return r["trunk"][0], r["trunk"][1], math.atan2(2 * (w * z + x * y), 1 - 2 * (y * y + z * z)), r["trunk_z"]


def home():
    shutil.rmtree(f"{state}/maps", ignore_errors=True); shutil.copytree(maps_dir, f"{state}/maps")
    shutil.copy(book_p, f"{state}/ground.json")
    sp = boot(WIPE="on", MAPLOC_MODE="localize", HOMECOMING="on")
    if sp is None:
        return None
    t0 = time.time()
    while time.time() - t0 < 600:
        if any(k in navlog() for k in HOME_KEYS):
            return sp
        time.sleep(3)
    say(f"{name}: the duck did not find itself at boot")
    down(sp, "boot-failed")
    return None


def rows_since(t0):
    try:
        rows = [r.split("\t") for r in open(f"{out}/pose.tsv").read().strip().splitlines()]
    except FileNotFoundError:
        return []
    return [r for r in rows if len(r) > 9 and float(r[0]) >= t0]


def untracked_since(t0):
    try:
        rows = [r.split("\t") for r in open(f"{out}/pose.tsv.untracked").read().strip().splitlines()]
    except FileNotFoundError:
        return []
    return [float(r[0]) for r in rows if r and float(r[0]) >= t0]


def judge(r):
    err = float(r[5])
    yaw_err = abs(math.remainder(float(r[8]) - float(r[9]), math.tau))
    return ("right" if err <= RIGHT_M and yaw_err <= RIGHT_RAD else "wrong"), err, yaw_err


def lost_at(t0):
    """When the duck stopped trusting its pose after t0: an untracked
    sample, or a hole in the tracked ones (the sampler runs every second)."""
    marks = untracked_since(t0)
    prev = t0
    for r in rows_since(t0):
        if float(r[0]) - prev > 4:
            marks.append(prev + 1)
            break
        prev = float(r[0])
    else:
        if time.time() - prev > 4:
            marks.append(prev + 1)
    return min(marks) if marks else None


def trial(sim, k, kind, spot, nxt):
    sx, sy = spot
    yaw_to = math.remainder(k * math.pi / 2 + math.pi / 4, math.tau)
    call("robot.go_to", {"stop": True})
    time.sleep(8)
    if kind in ("fall", "fallcarry"):
        # Up to three pushes, each half as hard again, until it is down.
        for push in range(3):
            sim.op(dict({"op": "shove"}, **{k: v * 1.5 ** push for k, v in SHOVE.items()}))
            t = time.time()
            while time.time() - t < 4 and sim.pose()[3] >= DOWN_Z:
                time.sleep(0.1)
            if sim.pose()[3] < DOWN_Z:
                break
    fell = sim.pose()[3] < DOWN_Z
    if kind in ("carry", "fallcarry"):
        x, y, h, _ = sim.pose()
        dx, dy, dyaw = sx - x, sy - y, math.remainder(yaw_to - h, math.tau)
        if CARRY == "teleport":
            sim.op({"op": "kidnap", "dx": dx, "dy": dy, "dyaw": dyaw, "tilt": True})
        else:
            # A walking pace for the hand, and a second each to lift and set down.
            secs = max(3.0, math.hypot(dx, dy) / 0.5)
            sim.op({"op": "carry", "dx": dx, "dy": dy, "dyaw": dyaw, "secs": secs, "upright": kind == "carry"})
            time.sleep(secs + 2.5)
    t0 = time.time()
    time.sleep(10)
    asked = call("robot.go_to", {"x": nxt[0], "y": nxt[1], "max_s": limit_s})
    say(f"  go_to answered: {json.dumps(asked)[:300]}")
    verdict, found_s, err, yaw_err, end = "never", None, float("nan"), float("nan"), None
    while time.time() - t0 < limit_s + 30:
        lost = lost_at(t0 + 1)
        if lost is not None:
            again = rows_since(lost)
            if again:
                verdict, err, yaw_err = judge(again[0])
                found_s = float(again[0][0]) - t0
                break
        elif time.time() - t0 > 60:
            # Never stopped trusting: the pose it kept, 60 s on.
            kept = rows_since(t0 + 50)
            if kept:
                verdict, err, yaw_err = judge(kept[-1])
                if verdict == "right":
                    found_s = 0.0
                else:
                    verdict = "unnoticed"
                break
        time.sleep(1)
    if verdict == "never" and sim.pose()[3] < DOWN_Z:
        verdict = "down"
    t1 = time.time()
    while time.time() - t1 < limit_s:
        e = call("robot.map_status").get("explore") or {}
        if e.get("state") not in ("running", "relocalizing", "searching"):
            end = f"{e.get('state')}: {e.get('reason')}"
            break
        time.sleep(3)
    r = {"k": k, "kind": kind, "spot": [round(sx, 2), round(sy, 2)], "verdict": verdict,
         "found_s": None if found_s is None else round(found_s), "err_m": round(err, 3),
         "yaw_err_deg": round(math.degrees(yaw_err), 1), "fell": fell, "go_to": end}
    say(f"{name} {kind} {k} at ({sx:+.2f},{sy:+.2f}): {verdict}"
        + (f" in {found_s:.0f} s, error {err:.2f} m {math.degrees(yaw_err):.0f}°" if found_s is not None else "")
        + f"; fell {fell}; go_to {end}")
    return r


kinds = os.environ.get("FINDME", "carry,fall,fallcarry").split(",")
n = int(os.environ.get("FINDME_N", str(len(goals))))
results, sp, sim = [], None, None
for kind in kinds:
    for k in range(n):
        if sp is None:
            sp = home(); sim = Sim() if sp else None
            if sp is None:
                break
        r = trial(sim, k, kind, goals[k % len(goals)], goals[(k + 1) % len(goals)])
        r["falls_log"] = navlog().count("maploc: robot fell")
        try:
            r["pickups_log"] = open(f"{state}/robotd.log", errors="ignore").read().count("picked up")
        except FileNotFoundError:
            r["pickups_log"] = None
        results.append(r)
        json.dump(results, open(f"{out}/findme.json", "w"), indent=1)
        if not call("robot.map_status").get("tracking"):
            down(sp, f"{kind}-{k}"); sp = None
if sp:
    down(sp, "end")
for kind in kinds:
    rs = [r for r in results if r["kind"] == kind]
    right = sorted(r["found_s"] for r in rs if r["verdict"] == "right")
    med = right[len(right) // 2] if right else None
    say(f"{name}: FINDME {kind} right {len(right)}/{len(rs)}, wrong {sum(r['verdict'] == 'wrong' for r in rs)}, "
        f"unnoticed {sum(r['verdict'] == 'unnoticed' for r in rs)}, "
        f"never {sum(r['verdict'] == 'never' for r in rs)}, down {sum(r['verdict'] == 'down' for r in rs)}; median to right {med} s")
say(f"{name}: DONE")
