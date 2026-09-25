"""prog_house.py <name> <scene.xml> <state> <port> <truth.json> <out> <sessions> <session_s>

Progressive exploration on the twin: session 1 from nothing, then each
next boot comes home on the saved map (HOMECOMING + RESUME) and explores
on; after each, the progress the duck reports against the truth's.
"""
import json, os, re, shutil, subprocess, sys, time

name, scene, state, port, truth_p, out, sessions, session_s = sys.argv[1:9]
sessions, session_s = int(sessions), float(session_s)
HERE = os.path.dirname(os.path.abspath(__file__))
S = os.environ.get("TWIN_WORK", "/tmp/quack-twin-work")  # outputs, and tgt/ for evaluate builds
sys.argv = ["run_house.py", name, scene, state, port, truth_p, out, "0", "0"]
exec(open(f"{HERE}/run_house.py").read().split("# ── 1. the exploration")[0])


def progress():
    return call("robot.map_status").get("explore", {}).get("progress")


def wait_explore(limit):
    t0 = time.time()
    while time.time() - t0 < limit:
        e = call("robot.map_status").get("explore", {})
        if e and e.get("state") not in ("running", None):
            return e
        time.sleep(10)
    return call("robot.map_status").get("explore", {})


def truth_coverage(tag):
    """The map as saved after this session, rendered by evaluate on the
    session's own recording with the saved session loaded first."""
    recs = sorted(f for f in os.listdir(f"{state}/rec") if f.endswith(".mdlg"))
    sess = f"{state}/maps/{name}.session"
    if not recs or not os.path.exists(sess):
        return {}
    shutil.copy(f"{state}/rec/{recs[-1]}", f"{out}/s{tag}.mdlg")
    shutil.copy(sess, f"{out}/s{tag}.session")
    env = dict(os.environ, CARGO_TARGET_DIR=f"{S}/tgt", MAP_SESSION=f"{out}/s{tag}.session")
    subprocess.run(["cargo", "run", "-q", "-p", "maploc", "--release", "--features", "kinematics", "--example", "evaluate", "--",
                    f"{out}/s{tag}.mdlg", truth_p.replace(".truth.json", ".toml"), f"{out}/eval-s{tag}"],
                   cwd=REPO, capture_output=True, text=True, env=env)
    return coverage(f"{out}/eval-s{tag}/s{tag}_truth.pgm")


for f in ("ground.json", "places.json", "maploc.session"):
    try: os.remove(f"{state}/{f}")
    except FileNotFoundError: pass
shutil.rmtree(f"{state}/maps", ignore_errors=True); shutil.rmtree(f"{state}/rec", ignore_errors=True)

for k in range(1, sessions + 1):
    if k == 1:
        sp = boot(WIPE="on", MAPLOC_MODE="stop_and_scan", HOMECOMING="off")
        time.sleep(15)
        say(f"{name} session 1: explore", json.dumps(call("robot.map_explore", {"max_s": session_s, "save_as": name}))[:120])
    else:
        sp = boot(WIPE="on", MAPLOC_MODE="stop_and_scan", HOMECOMING="on", RESUME="on", EXPLORE_S=str(int(session_s)))
        t0 = time.time(); verdict = "timeout"
        while time.time() - t0 < 1200:
            log = navlog()
            if "exploring on from where" in log: verdict = "home, exploring on"; break
            if "standing down" in log: verdict = "gave up"; break
            if "nothing to explore on" in log: verdict = "nothing left"; break
            time.sleep(5)
        err = ""
        try:
            last = open(f"{out}/pose.tsv").read().strip().splitlines()[-1].split("\t"); err = f", pose vs truth {float(last[5]):.2f} m"
        except Exception:
            pass
        say(f"{name} session {k}: homecoming {verdict} in {time.time()-t0:.0f} s{err}")
        if verdict != "home, exploring on":
            down(sp, f"s{k}"); continue
    e = wait_explore(session_s + 400)
    log = navlog()
    say(f"{name} session {k}: {e.get('state')} — {e.get('reason')}; legs {e.get('legs')}, refusals {e.get('refusals')}, falls {falls(log)}")
    say(f"{name} session {k}: progress the duck reports {json.dumps(progress())}")
    time.sleep(5)
    down(sp, f"s{k}")
    say(f"{name} session {k}: known floor per room (truth) {truth_coverage(k)}")
book = json.load(open(f"{state}/ground.json")) if os.path.exists(f"{state}/ground.json") else {}
n, real, near, phantom, cover = score_book(book)
say(f"{name}: drop book {n}: rim {real}, near {near}, phantom {len(phantom)}; rim covered {cover}")
say(f"{name}: DONE")
