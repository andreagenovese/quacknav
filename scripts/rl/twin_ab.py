"""twin_ab.py: the pilot against the stick on the MuJoCo twin.

    twin_ab.py OUT_DIR PILOT.json [--house casa_ingombra] [--rounds 2] [--arms stick,pilot]

Per arm, a fresh twin (scripts/twin/twin.sh, the viewer on): quack-navd
with the oracle drawing the house's truth as a mapper would
(`QK_ORACLE_AS_MAPPED`: holes and the inside of furniture unknown) and the
true pose — so what the scene has and the truth has not (casa_ingombra's
bag, basket, box, chair legs, toy) is what the map does not know — the true
holes' rims on the books (`QK_ORACLE_BOOK`), and `QK_RL_TRACE` recording
every leg; the pilot arm with `QK_RL_POLICY`. Then `ROUNDS` tours of the truth's goals, each a
`robot.go_to`, judged on the simulator's trunk: arrived within 0.4 m, the
time, a fall (the trunk under 7 cm, or the journey's own word).

Writes OUT_DIR/<arm>/{journeys.jsonl, traces/, twin logs} and
OUT_DIR/summary.md. Needs what twin.sh needs (MICRODUCK at daemon-v0.15.0,
MICRODUCK_RL, POLICY_DIR).
"""
import argparse
import json
import math
import os
import re
import shutil
import socket
import subprocess
import time

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
# The twin (and so the quack-navd binary) of another checkout: TWIN_REPO.
TWIN = os.path.join(os.environ.get("TWIN_REPO", REPO), "scripts", "twin", "twin.sh")
HOUSES = os.path.join(REPO, "scripts", "twin", "houses")
STATE = os.environ.get("STATE", "/tmp/quack-twin")
PORT = int(os.environ.get("PORT", "7872"))


def nav(name, args=None, timeout=10):
    s = socket.socket(socket.AF_UNIX)
    s.settimeout(timeout)
    s.connect(f"{STATE}/nav.sock")
    f = s.makefile("rw")
    f.write(json.dumps({"jsonrpc": "2.0", "id": 1, "method": "nav.call", "params": {"name": name, "args": args or {}}}) + "\n")
    f.flush()
    for line in f:
        m = json.loads(line)
        if m.get("id") == 1:
            return m.get("result", m.get("error"))


def trunk():
    b = socket.socket()
    b.settimeout(3)
    b.connect(("127.0.0.1", PORT))
    bf = b.makefile("rw")
    bf.write(json.dumps({"op": "hello", "protocol": 1, "joints": 15}) + "\n")
    bf.flush()
    bf.readline()
    bf.write('{"op":"read"}\n')
    bf.flush()
    t = json.loads(bf.readline())["trunk"]
    b.close()
    return t


def say(out, text):
    print(text, flush=True)
    with open(os.path.join(out, "ab.log"), "a") as f:
        f.write(text + "\n")


def arm(out, name, house, pilot, rounds, only=None, book=True, main_oracle=False):
    d = os.path.join(out, name)
    os.makedirs(os.path.join(d, "traces"), exist_ok=True)
    subprocess.run([TWIN, "down"], capture_output=True)
    for f in ("ground.json", "places.json", "maploc.session", "knobs.env"):
        try:
            os.remove(f"{STATE}/{f}")
        except FileNotFoundError:
            pass
    os.makedirs(STATE, exist_ok=True)
    truth = json.load(open(f"{HOUSES}/{house}.truth.json"))
    knobs = [f"QK_ORACLE_WALLS={HOUSES}/{house}.toml", f"QK_ORACLE_HOLES={HOUSES}/{house}.truth.json",
             f"QK_ORACLE_POSE=127.0.0.1:{PORT}"]
    # main's oracle knows walls, holes (drawn as walls) and the pose only.
    if not main_oracle:
        knobs += ["QK_ORACLE_AS_MAPPED=1", f"QK_RL_TRACE={d}/traces"]
        if book:
            knobs.append(f"QK_ORACLE_BOOK={HOUSES}/{house}.truth.json")
    if pilot:
        knobs.append(f"QK_RL_POLICY={pilot}")
    with open(f"{STATE}/knobs.env", "w") as f:
        f.write("\n".join(knobs) + "\n")
    scene = os.path.join(os.environ["MICRODUCK_RL"], "src/mjlab_microduck/robot/microduck", f"scene_{house}.xml")
    env = dict(os.environ, SCENE=scene, VIEWER=os.environ.get("VIEWER", "on"), HOMECOMING="off", WIPE="on", MAPLOC_MODE="stop_and_scan")

    def port_busy():
        c = socket.socket()
        c.settimeout(0.3)
        busy = c.connect_ex(("127.0.0.1", PORT)) == 0
        c.close()
        return busy

    def boot():
        subprocess.run([TWIN, "down"], capture_output=True)
        # The simulator lets its port go a few seconds after it is told
        # to stop: `up` refuses a busy port (a reboot after a fall did).
        t = time.time()
        while port_busy() and time.time() - t < 60:
            time.sleep(1)
        up = subprocess.run([TWIN, "up"], env=env, capture_output=True, text=True)
        say(out, f"[{name}] {up.stdout.strip().splitlines()[0] if up.stdout else up.stderr.strip()[:200]}")
        time.sleep(3)
        subprocess.run([TWIN, "enable"], env=env, capture_output=True)
        time.sleep(25)

    boot()
    rows = []
    for rnd in range(rounds):
        for goal, (gx, gy) in truth["goals"].items():
            if only and goal not in only:
                continue
            # The mapper vouches for the pose first (a loaded map relocalizes;
            # standing is what it needs), as final_house.py's tour waits.
            tw = time.time()
            while time.time() - tw < 180 and not (nav("robot.map_status") or {}).get("tracking"):
                time.sleep(3)
            t0 = time.time()
            r = nav("robot.go_to", {"x": gx, "y": gy, "max_s": 300})
            if not isinstance(r, dict) or "message" in r or r.get("error") or r.get("started") is not True:
                rows.append({"round": rnd, "goal": goal, "refused": json.dumps(r)[:200]})
                say(out, f"[{name}] r{rnd} {goal}: refused {json.dumps(r)[:160]}")
                time.sleep(5)
                continue
            time.sleep(3)
            while time.time() - t0 < 330:
                e = (nav("robot.map_status") or {}).get("explore", {})
                if e.get("state") != "running":
                    break
                time.sleep(3)
            e = (nav("robot.map_status") or {}).get("explore", {})
            t = trunk()
            err = math.hypot(t[0] - gx, t[1] - gy)
            down = t[2] < 0.07 or "seated or fallen" in str(e.get("reason"))
            m = re.search(r"arrived at \(([-\d.]+), ([-\d.]+)\)", str(e.get("reason")))
            said = bool(m) and abs(float(m[1]) - gx) < 0.05 and abs(float(m[2]) - gy) < 0.05
            row = {"round": rnd, "goal": goal, "secs": round(time.time() - t0, 1), "state": e.get("state"), "reason": e.get("reason"),
                   "legs": e.get("legs"), "truth": [round(v, 3) for v in t[:3]], "err_m": round(err, 3), "said_arrived": said,
                   "arrived": said and err <= 0.4, "fell": down}
            rows.append(row)
            with open(os.path.join(d, "journeys.jsonl"), "a") as f:
                f.write(json.dumps(row) + "\n")
            say(out, f"[{name}] r{rnd} {goal}: {'ARRIVED' if row['arrived'] else 'not arrived'} in {row['secs']:.0f} s, {err:.2f} m off, legs {row['legs']}{' FELL' if down else ''} — {e.get('reason')}")
            if down:
                # A fall ends the journey, not the arm: the twin boots
                # again at home and the tour goes on (a fall is counted).
                for f in ("navd.log",):
                    if os.path.exists(f"{STATE}/{f}"):
                        shutil.copy(f"{STATE}/{f}", os.path.join(d, f"navd-fall-{rnd}-{goal}.log"))
                boot()
    for f in ("navd.log", "robotd.log", "body.log"):
        if os.path.exists(f"{STATE}/{f}"):
            shutil.copy(f"{STATE}/{f}", os.path.join(d, f))
    subprocess.run([TWIN, "down"], capture_output=True)
    return rows


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("out")
    ap.add_argument("pilot")
    ap.add_argument("--house", default="casa_ingombra")
    ap.add_argument("--rounds", type=int, default=2)
    ap.add_argument("--arms", default="stick,pilot")
    ap.add_argument("--main-oracle", action="store_true", help="only main's oracle knobs (walls, holes as walls, pose)")
    ap.add_argument("--no-book", action="store_true", help="no true rims on the books: the holes unbooked, the guard alone")
    ap.add_argument("--goals", help="only these of the truth's goals, comma-separated")
    args = ap.parse_args()
    os.makedirs(args.out, exist_ok=True)
    pilot = os.path.abspath(args.pilot)
    res = {}
    for a in args.arms.split(","):
        res[a] = arm(args.out, a, args.house, pilot if a == "pilot" else None, args.rounds, args.goals.split(",") if args.goals else None, not args.no_book, args.main_oracle)
    with open(os.path.join(args.out, "summary.md"), "w") as f:
        f.write(f"# Twin A/B on {args.house}: the stick and the pilot\n\n| arm | journeys | arrived | fell | mean s (arrived) | refused |\n|---|---|---|---|---|---|\n")
        for a, rows in res.items():
            done = [r for r in rows if "refused" not in r]
            arr = [r for r in done if r["arrived"]]
            f.write(f"| {a} | {len(done)} | {len(arr)} | {sum(r['fell'] for r in done)} | {sum(r['secs'] for r in arr) / max(1, len(arr)):.0f} | {len(rows) - len(done)} |\n")
    print(open(os.path.join(args.out, "summary.md")).read())


if __name__ == "__main__":
    main()
