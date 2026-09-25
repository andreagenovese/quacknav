"""modes_test.py <name> <scene> <state> <port> <truth> <out>: the user's modes on a map in progress.
resume + "how far along", "exploration complete", hybrid journeys on the frozen map,
a restart that must not explore, and a fresh map replacing the saved one only when it saves."""
import json, os, re, sys, time

name, scene, state, port, truth_p, out = sys.argv[1:7]
HERE = os.path.dirname(os.path.abspath(__file__))
S = os.environ.get("TWIN_WORK", "/tmp/quack-twin-work")  # outputs, and tgt/ for evaluate builds
sys.argv = ["run_house.py", name, scene, state, port, truth_p, out, "0", "0"]
exec(open(f"{HERE}/run_house.py").read().split("# ── 1. the exploration")[0])
SESS = f"{state}/maps/{name}.session"


def house():
    return call("robot.map_status").get("house")


def wait_home(limit=900):
    t0 = time.time()
    while time.time() - t0 < limit:
        log = navlog()
        for key in ("exploring on from where", "the map frozen, navigating", "the map is frozen; navigating", "standing down", "pose is confirmed"):
            if key in log and (key != "pose is confirmed" or time.time() - t0 > 20):
                if key == "pose is confirmed":
                    time.sleep(8); log = navlog()
                return next(k for k in ("exploring on from where", "the map frozen, navigating", "the map is frozen; navigating", "standing down", "pose is confirmed") if k in log), time.time() - t0
        time.sleep(5)
    return "timeout", time.time() - t0


def goto(k, gx, gy):
    call("robot.go_to", {"x": gx, "y": gy, "max_s": 300}); t1 = time.time()
    while time.time() - t1 < 330:
        e = call("robot.map_status").get("explore", {})
        if e.get("state") != "running": break
        time.sleep(5)
    e = call("robot.map_status").get("explore", {})
    say(f"  go_to {k}: {e.get('state')} in {time.time()-t1:.0f} s, legs {e.get('legs')}, refusals {e.get('refusals')} — {e.get('reason')}")


# 1. resume, and "how far along?" while it explores
sp = boot(WIPE="on", MAPLOC_MODE="stop_and_scan", HOMECOMING="on", RESUME="on", EXPLORE_S="600")
v, t = wait_home()
say(f"1. boot: {v} in {t:.0f} s; house {json.dumps(house())}")
time.sleep(180)
say(f"1. after 3 min exploring: house {json.dumps(house())}")
# 2. "esplorazione completata"
r = call("robot.map_explore", {"complete": True}, timeout=90)
say(f"2. complete: {json.dumps(r)[:400]}")
time.sleep(5)
f = call("robot.map_status")
say(f"2. after: house {json.dumps(f.get('house'))}, explore {f.get('explore', {}).get('state')}")
r = call("robot.map_explore", {})
say(f"2. explore again (must refuse): {json.dumps(r)[:300]}")
# 3. hybrid journeys on the frozen map
before = navlog().count("runs onto floor the map does not know")
for k, (gx, gy) in list(truth["goals"].items())[:3]:
    goto(k, gx, gy)
log = navlog()
say(f"3. hybrid: guarded legs onto unknown floor {log.count('runs onto floor the map does not know') - before}, blind legs {log.count('blind leg')}; falls {falls(log)}")
down(sp, "modes-a")
# 4. restart: home, frozen, no exploring
sp = boot(WIPE="on", MAPLOC_MODE="stop_and_scan", HOMECOMING="on", RESUME="on", EXPLORE_S="600")
v, t = wait_home()
time.sleep(20)
f = call("robot.map_status")
say(f"4. restart: {v} in {t:.0f} s; explore {f.get('explore', {}).get('state')}; house {json.dumps(f.get('house'))}; exploring started: {'exploring on from where' in navlog()}")
goto("home", 0.0, 0.0)
# 5. fresh map: the saved one replaced only when the session saves
m0 = os.path.getmtime(SESS)
r = call("robot.map_explore", {"fresh": True, "confirmed": True, "max_s": 120}, timeout=60)
say(f"5. fresh: {json.dumps(r)[:250]}; the saved map still the old one: {os.path.getmtime(SESS) == m0}")
t0 = time.time()
while time.time() - t0 < 400:
    e = call("robot.map_status").get("explore", {})
    if e.get("state") not in ("running", None): break
    time.sleep(10)
f = call("robot.map_status")
say(f"5. after the fresh session: {f.get('explore', {}).get('reason')}; saved map replaced: {os.path.getmtime(SESS) != m0}; house {json.dumps(f.get('house'))}; falls {falls(navlog())}")
down(sp, "modes-b")
say("DONE")
