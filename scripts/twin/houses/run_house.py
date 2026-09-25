"""run_house.py <name> <scene.xml> <state dir> <port> <truth.json> <out dir> <rounds> [explore_s]

On the MuJoCo twin (scripts/twin/twin.sh): a full exploration from a fresh
state, the map saved as <name>, its drop book scored against the true holes,
then <rounds> boots with homecoming + a go_to tour of the truth's goals.
Every step is printed as it ends; logs are copied into <out dir>.
"""
import json, os, math, re, shutil, subprocess, sys, time

REPO = os.environ.get("QN_REPO", os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", ".."))); T = f"{REPO}/scripts/twin"
HERE = os.path.dirname(os.path.abspath(__file__))
S = os.environ.get("TWIN_WORK", "/tmp/quack-twin-work")  # outputs, and tgt/ for evaluate builds
name, scene, state, port, truth_p, out, rounds = sys.argv[1:8]
port, rounds = int(port), int(rounds)
explore_s = float(sys.argv[8]) if len(sys.argv) > 8 else 2400.0
truth = json.load(open(truth_p))
os.makedirs(out, exist_ok=True)
base = dict(os.environ, SCENE=scene, STATE=state, PORT=str(port),
            MICRODUCK=os.environ["MICRODUCK"], MICRODUCK_RL=os.environ["MICRODUCK_RL"],
            POLICY_DIR=os.environ["POLICY_DIR"])
NAV, ROBOTD = f"{state}/nav.sock", f"{state}/robotd.sock"


def say(*a):
    print(*a, flush=True)


def call(tool, args=None, sock=NAV, kind=None, timeout=60):
    cmd = ["python3", f"{T}/call.py"] + ([kind] if kind else []) + [sock, tool, json.dumps(args or {})]
    try:
        o = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout).stdout.strip()
        return json.loads(o)
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
    r = twin("up", **env)
    if r.returncode != 0:
        say("  twin up failed:", r.stderr.strip()[-300:])
        return None
    time.sleep(3); twin("enable")
    sampler = subprocess.Popen(["python3", f"{HERE}/poseerr.py", NAV, str(port), f"{out}/pose.tsv"],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return sampler


def down(sampler, tag):
    if sampler:
        sampler.kill()
    shutil.copy(f"{state}/navd.log", f"{out}/navd-{tag}.log") if os.path.exists(f"{state}/navd.log") else None
    twin("down")


def falls(log):
    return log.count("maploc: robot fell")


def hole_dist(x, y):
    best = math.inf
    for (x0, x1, y0, y1) in truth["holes"]:
        dx = max(x0 - x, 0, x - x1); dy = max(y0 - y, 0, y - y1)
        best = min(best, math.hypot(dx, dy))
    return best


def score_book(book):
    drops = book.get(name, [])
    real = sum(hole_dist(d[0], d[1]) <= 0.10 for d in drops)
    near = sum(0.10 < hole_dist(d[0], d[1]) <= 0.20 for d in drops)
    phantom = [d for d in drops if hole_dist(d[0], d[1]) > 0.20]
    # How much of each hole's rim has a drop within 0.15 m.
    cover = []
    for (x0, x1, y0, y1) in truth["holes"]:
        pts, k = [], 0.0
        per = 2 * ((x1 - x0) + (y1 - y0))
        while k < per:
            if k < x1 - x0: p = (x0 + k, y0)
            elif k < (x1 - x0) + (y1 - y0): p = (x1, y0 + k - (x1 - x0))
            elif k < 2 * (x1 - x0) + (y1 - y0): p = (x1 - (k - (x1 - x0) - (y1 - y0)), y1)
            else: p = (x0, y1 - (k - 2 * (x1 - x0) - (y1 - y0)))
            pts.append(p); k += 0.05
        seen = sum(any(math.hypot(p[0] - d[0], p[1] - d[1]) <= 0.15 for d in drops) for p in pts)
        cover.append(round(seen / len(pts), 2))
    return len(drops), real, near, phantom, cover


def coverage(pgm_path):
    """Share of each room's cells the map knows as floor, from evaluate's PGM
    (0 wall, 230 floor, 160 unknown, 70 the truth's walls drawn over)."""
    try:
        b = open(pgm_path, "rb").read(); hdr, px = b.split(b"\n", 1); w, h = map(int, hdr.split()[1:3])
    except Exception:
        return {}
    cols = [j for i in range(h) for j in range(w) if px[i * w + j] == 70]
    rows = [i for i in range(h) for j in range(w) if px[i * w + j] == 70]
    if not cols:
        return {}
    C = 0.05; x0 = 4.06 - (max(cols) + 0.5) * C; ytop = 3.06 + (min(rows) + 0.5) * C
    out_ = {}
    for room, (ax, bx, ay, by) in truth.get("rooms", {}).items():
        free = known = 0
        x = ax + 0.15
        while x < bx - 0.15:
            y = ay + 0.15
            while y < by - 0.15:
                if hole_dist(x, y) > 0.1:
                    j, i = int((x - x0) / C), int((ytop - y) / C)
                    if 0 <= i < h and 0 <= j < w:
                        v = px[i * w + j]; free += 1; known += v in (230, 0, 70)
                y += C
            x += C
        out_[room] = round(known / max(free, 1), 2)
    return out_


# ── 1. the exploration, from nothing ─────────────────────────────────────
if explore_s > 0:
    for f in ("ground.json", "places.json", "maploc.session"):
        try: os.remove(f"{state}/{f}")
        except FileNotFoundError: pass
    shutil.rmtree(f"{state}/maps", ignore_errors=True); shutil.rmtree(f"{state}/rec", ignore_errors=True)
    sp = boot(WIPE="on", MAPLOC_MODE="stop_and_scan", HOMECOMING="off")
    time.sleep(15)
    t0 = time.time()
    say(f"{name}: explore", json.dumps(call("robot.map_explore", {"max_s": explore_s}))[:160])
    marks = [m for m in (1200, 2400, 3600) if m < explore_s]
    while time.time() - t0 < explore_s + 120:
        e = call("robot.map_status").get("explore", {})
        left = explore_s - (time.time() - t0)
        if e and e.get("state") != "running" and "budget" in str(e.get("reason")) and left > 120:
            # robot.map_explore caps a job at an hour: go on exploring the same live map.
            say(f"{name}: explore budget spent at {time.time()-t0:.0f} s; going on for {left:.0f} s more")
            call("robot.map_explore", {"max_s": left}); time.sleep(10); continue
        if e and e.get("state") != "running": break
        if marks and time.time() - t0 >= marks[0]:
            # A copy of the recording so far: its map is the coverage at that minute.
            recs = sorted(os.listdir(f"{state}/rec"))
            if recs:
                shutil.copy(f"{state}/rec/{recs[-1]}", f"{out}/explore-{marks[0] // 60}min.mdlg")
            marks.pop(0)
        time.sleep(10)
    e = call("robot.map_status").get("explore", {})
    log = navlog()
    say(f"{name}: explore {e.get('state')} in {time.time()-t0:.0f} s, legs {e.get('legs')}, refusals {e.get('refusals')} — {e.get('reason')}; falls {falls(log)}")
    r = call("robot.map_save", {"name": name}, timeout=200)
    say(f"{name}: map_save", json.dumps(r)[:200])
    time.sleep(5)
    down(sp, "explore")
    rec = sorted([f for f in os.listdir(f"{state}/rec") if f.endswith(".mdlg")], key=lambda f: os.path.getsize(f"{state}/rec/{f}"))
    if rec:
        shutil.copy(f"{state}/rec/{rec[-1]}", f"{out}/explore.mdlg")
    book = json.load(open(f"{state}/ground.json")) if os.path.exists(f"{state}/ground.json") else {}
    json.dump(book, open(f"{out}/ground-after-explore.json", "w"))
    n, real, near, phantom, cover = score_book(book)
    say(f"{name}: drop book {n}: on the rim (≤0.10 m) {real}, near (≤0.20) {near}, phantom (>0.20) {len(phantom)}; rim covered per hole {cover}")
    for d in phantom:
        say(f"    phantom {d} at {hole_dist(d[0], d[1]):.2f} m from a hole")
    if rec and os.path.exists(truth_p.replace(".truth.json", ".toml")):
        ev = subprocess.run(["cargo", "run", "-q", "-p", "maploc", "--release", "--features", "kinematics", "--example", "evaluate", "--",
                             f"{out}/explore.mdlg", truth_p.replace(".truth.json", ".toml"), f"{out}/eval"],
                            cwd=REPO, capture_output=True, text=True, env=dict(os.environ, CARGO_TARGET_DIR=f"{S}/tgt"))
        open(f"{out}/eval.txt", "w").write(ev.stdout + ev.stderr)
        for l in ev.stdout.splitlines():
            if re.search(r"^session|map walls vs room|tracked vs raw", l):
                say("   ", l.strip()[:200])
        say(f"{name}: known floor per room {coverage(f'{out}/eval/explore_truth.pgm')}")
        for mark in sorted(f for f in os.listdir(out) if f.startswith("explore-") and f.endswith("min.mdlg")):
            tag = mark[len("explore-"):-len(".mdlg")]
            subprocess.run(["cargo", "run", "-q", "-p", "maploc", "--release", "--features", "kinematics", "--example", "evaluate", "--",
                            f"{out}/{mark}", truth_p.replace(".truth.json", ".toml"), f"{out}/eval-{tag}"],
                           cwd=REPO, capture_output=True, text=True, env=dict(os.environ, CARGO_TARGET_DIR=f"{S}/tgt"))
            say(f"{name}: known floor per room at {tag} {coverage(f'{out}/eval-{tag}/' + mark[:-5] + '_truth.pgm')}")


# ── 2. the rounds: homecoming, then the go_to tour ───────────────────────
goals = [(k, v) for k, v in truth["goals"].items()]
for rnd in range(1, rounds + 1):
    sp = boot(WIPE="on", MAPLOC_MODE="localize", HOMECOMING="on")
    t0 = time.time(); verdict = "timeout"
    while time.time() - t0 < 900:
        log = navlog()
        if "pose is confirmed" in log: verdict = "confirmed"; break
        if "standing down" in log: verdict = "gave up"; break
        time.sleep(5)
    err = ""
    if verdict == "confirmed":
        time.sleep(6)
        try:
            last = open(f"{out}/pose.tsv").read().strip().splitlines()[-1].split("	")
            err = f", pose vs truth {float(last[5]):.2f} m"
        except Exception:
            pass
    say(f"{name} round {rnd}: homecoming {verdict} in {time.time()-t0:.0f} s{err}")
    if verdict == "confirmed":
        for k, (gx, gy) in goals:
            call("robot.go_to", {"x": gx, "y": gy, "max_s": 300}); t1 = time.time()
            while time.time() - t1 < 330:
                e = call("robot.map_status").get("explore", {})
                if e.get("state") != "running": break
                time.sleep(5)
            e = call("robot.map_status").get("explore", {}); w = call("robot.where_am_i"); p = w.get("pose") or {}
            say(f"  go_to {k} ({gx:+.2f},{gy:+.2f}): {e.get('state')} in {time.time()-t1:.0f} s, legs {e.get('legs')}, "
                f"refusals {e.get('refusals')}, pose ({p.get('x')},{p.get('y')}) — {e.get('reason')}")
            if "seated or fallen" in str(e.get("reason")):
                break
    log = navlog()
    say(f"{name} round {rnd}: falls {falls(log)}")
    down(sp, f"r{rnd}")
book = json.load(open(f"{state}/ground.json")) if os.path.exists(f"{state}/ground.json") else {}
json.dump(book, open(f"{out}/ground-after-rounds.json", "w"))
n, real, near, phantom, cover = score_book(book)
say(f"{name}: drop book after the rounds {n}: rim {real}, near {near}, phantom {len(phantom)}; rim covered {cover}")
say(f"{name}: DONE")
