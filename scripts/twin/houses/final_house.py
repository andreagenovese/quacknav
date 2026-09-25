"""final_house.py <name> <scene> <state> <port> <truth> <out> [session_s] [max_sessions] [rounds]

The final protocol on one house, on the twin:
  1. progressive exploration from nothing, a session of session_s at a time,
     until the house is done or max_sessions have run; coverage per room
     (truth) and the share the duck reports after each;
  2. not done: "exploration complete", declared by the user;
  3. a restart: home, the map frozen, no exploring;
  4. rounds: home, then the go_to tour of the truth's goals (on the frozen map);
  5. the A/B: the same rounds on main's binary (maploc localize), the same map
     and book;
  and the drop book scored against the true holes.
"""
import json, os, re, shutil, subprocess, sys, time

name, scene, state, port, truth_p, out = sys.argv[1:7]
session_s = float(sys.argv[7]) if len(sys.argv) > 7 else 1800.0
max_sessions = int(sys.argv[8]) if len(sys.argv) > 8 else 4
rounds = int(sys.argv[9]) if len(sys.argv) > 9 else 3
HERE = os.path.dirname(os.path.abspath(__file__))
S = os.environ.get("TWIN_WORK", "/tmp/quack-twin-work")  # outputs, and tgt/ for evaluate builds
sys.argv = ["run_house.py", name, scene, state, port, truth_p, out, "0", "0"]
ROUNDS = rounds
exec(open(f"{HERE}/run_house.py").read().split("# ── 1. the exploration")[0])
rounds = ROUNDS  # the exec'd prefix sets its own `rounds` (0) from the argv it was given
ROUNDS_ONLY = os.environ.get("ROUNDS_ONLY") == "1"

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




def home(limit=1200):
    t0 = time.time()
    keys = ("exploring on from where", "the map frozen, navigating", "the map is frozen; navigating", "standing down", "the pose is confirmed on the saved map")
    while time.time() - t0 < limit:
        log = navlog()
        hit = [k for k in keys if k in log]
        if hit:
            err = ""
            try:
                last = open(f"{out}/pose.tsv").read().strip().splitlines()[-1].split("\t"); err = f"{float(last[5]):.2f}"
            except Exception:
                pass
            return hit[0], time.time() - t0, err
        time.sleep(5)
    return "timeout", time.time() - t0, ""


def tour(tag):
    arrived, times = 0, []
    for k, (gx, gy) in truth["goals"].items():
        call("robot.go_to", {"x": gx, "y": gy, "max_s": 300}); t1 = time.time()
        while time.time() - t1 < 330:
            e = call("robot.map_status").get("explore", {})
            if e.get("state") != "running": break
            time.sleep(5)
        e = call("robot.map_status").get("explore", {})
        ok = "arrived" in str(e.get("reason"))
        arrived += ok
        if ok: times.append(time.time() - t1)
        say(f"  {tag} go_to {k} ({gx:+.2f},{gy:+.2f}): {e.get('state')} in {time.time()-t1:.0f} s, legs {e.get('legs')}, refusals {e.get('refusals')} — {e.get('reason')}")
        if "seated or fallen" in str(e.get("reason")): break
    return arrived, times


if not ROUNDS_ONLY:
    for f in ("ground.json", "places.json", "maploc.session"):
        try: os.remove(f"{state}/{f}")
        except FileNotFoundError: pass
    shutil.rmtree(f"{state}/maps", ignore_errors=True); shutil.rmtree(f"{state}/rec", ignore_errors=True)

# 1. progressive exploration
done = ROUNDS_ONLY
for k in range(1, (0 if ROUNDS_ONLY else max_sessions) + 1):
    if k == 1:
        sp = boot(WIPE="on", MAPLOC_MODE="stop_and_scan", HOMECOMING="off")
        time.sleep(15)
        call("robot.map_explore", {"max_s": session_s, "save_as": name})
    else:
        sp = boot(WIPE="on", MAPLOC_MODE="stop_and_scan", HOMECOMING="on", RESUME="on", EXPLORE_S=str(int(session_s)))
        v, t, err = home()
        say(f"{name} session {k}: homecoming {v} in {t:.0f} s, pose vs truth {err} m")
        if v != "exploring on from where":
            down(sp, f"s{k}")
            if "frozen" in v: done = True; break
            continue
    e = wait_explore(session_s + 400)
    log = navlog()
    p = call("robot.map_status").get("house")
    say(f"{name} session {k}: {e.get('reason')}; legs {e.get('legs')}, refusals {e.get('refusals')}, falls {falls(log)}; the duck reports {json.dumps(p)}")
    time.sleep(5)
    down(sp, f"s{k}")
    say(f"{name} session {k}: known floor per room (truth) {truth_coverage(k)}")
    prog = (json.load(open(f"{state}/ground.json")).get(f"{name}.progress") or {}) if os.path.exists(f"{state}/ground.json") else {}
    if prog.get("done"):
        done = True
        say(f"{name}: the house is done after {k} sessions")
        break

# 2. not done: the user declares it complete (in ROUNDS_ONLY, when the book says it is not)
if ROUNDS_ONLY:
    b = json.load(open(f"{out}/ground-after-explore.json"))
    done = bool((b.get(f"{name}.progress") or {}).get("done"))
    if not done:
        json.dump(b, open(f"{state}/ground.json", "w"))
        shutil.rmtree(f"{state}/maps", ignore_errors=True); shutil.copytree(f"{out}/maps-after-explore", f"{state}/maps")
if not done:
    sp = boot(WIPE="on", MAPLOC_MODE="stop_and_scan", HOMECOMING="on", RESUME="off")
    v, t, err = home()
    time.sleep(10)
    r = call("robot.map_explore", {"complete": True}, timeout=200)
    say(f"{name}: 'exploration complete' by the user: {json.dumps({k: r.get(k) for k in ('complete', 'percent_mapped', 'frozen')})} ({r.get('error', '')})")
    down(sp, "complete")
if ROUNDS_ONLY and done:
    # From the map and book the exploration left.
    book_after_explore = json.load(open(f"{out}/ground-after-explore.json"))
    json.dump(book_after_explore, open(f"{state}/ground.json", "w"))
    shutil.rmtree(f"{state}/maps", ignore_errors=True); shutil.copytree(f"{out}/maps-after-explore", f"{state}/maps")
else:
    book_after_explore = json.load(open(f"{state}/ground.json")) if os.path.exists(f"{state}/ground.json") else {}
    json.dump(book_after_explore, open(f"{out}/ground-after-explore.json", "w"))
    shutil.copytree(f"{state}/maps", f"{out}/maps-after-explore", dirs_exist_ok=True)
n, real, near, phantom, cover = score_book(book_after_explore)
say(f"{name}: drop book {n}: rim {real}, near {near}, phantom {len(phantom)} {phantom}; rim covered {cover}")

# 3 + 4. restarts: home, frozen, no exploring; the tour
new_arr, new_t, new_n, new_falls = 0, [], 0, 0
for rnd in range(1, rounds + 1):
    sp = boot(WIPE="on", MAPLOC_MODE="stop_and_scan", HOMECOMING="on", RESUME="on", EXPLORE_S=str(int(session_s)))
    v, t, err = home()
    time.sleep(15)
    exploring = call("robot.map_status").get("explore", {}).get("state") == "running"
    say(f"{name} round {rnd}: homecoming {v} in {t:.0f} s, pose vs truth {err} m; exploring after the restart: {exploring}")
    if "frozen" in v and not exploring:
        a, ts = tour("new"); new_arr += a; new_t += ts; new_n += len(truth["goals"])
    log = navlog(); new_falls += falls(log)
    say(f"{name} round {rnd}: falls {falls(log)}")
    down(sp, f"r{rnd}")

# 5. the A/B: main's binary, localize, the same map and book
main_arr, main_t, main_n, main_falls = 0, [], 0, 0
json.dump(book_after_explore, open(f"{state}/ground.json", "w"))
shutil.rmtree(f"{state}/maps", ignore_errors=True); shutil.copytree(f"{out}/maps-after-explore", f"{state}/maps")
T_new = T
T = os.environ.get("AB_REPO", f"{S}/wt-main") + "/scripts/twin"
for rnd in range(1, rounds + 1):
    sp = boot(WIPE="on", MAPLOC_MODE="localize", HOMECOMING="on")
    t0 = time.time(); v = "timeout"
    while time.time() - t0 < 900:
        if "pose is confirmed" in navlog(): v = "confirmed"; break
        if "standing down" in navlog(): v = "gave up"; break
        time.sleep(5)
    say(f"{name} main round {rnd}: homecoming {v} in {time.time()-t0:.0f} s")
    if v == "confirmed":
        a, ts = tour("main"); main_arr += a; main_t += ts; main_n += len(truth["goals"])
    log = navlog(); main_falls += falls(log)
    say(f"{name} main round {rnd}: falls {falls(log)}")
    down(sp, f"main-r{rnd}")
T = T_new
med = lambda v: sorted(v)[len(v) // 2] if v else float("nan")
say(f"{name}: SUMMARY new {new_arr}/{new_n} arrived, median {med(new_t):.0f} s, falls {new_falls}; main {main_arr}/{main_n}, median {med(main_t):.0f} s, falls {main_falls}")
say(f"{name}: DONE")
