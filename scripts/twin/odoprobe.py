"""robotd's odometry against the simulator's truth, for a walk.

    odoprobe.py <robotd.sock> <body port> <label> [reps]

In the arena, from the middle each time: a straight leg (vx 0.3, 10 s), arcs
(vx 0.3, vyaw +-0.5, 8 s) and spins (vyaw +-1.5, 3 s). Per trial, the distance
and the turn odometry (`robot.state.odom`) gives against the truth (the trunk
in the simulator): the scale the mapper's dead reckoning works with.
"""
import json, math, socket, sys, threading, time

path, port, label = sys.argv[1], int(sys.argv[2]), sys.argv[3]
reps = int(sys.argv[4]) if len(sys.argv) > 4 else 2
# Where each trial starts and faces (env SPOT="x,y,yaw"; the arena's middle by default).
import os
SPOT = tuple(float(v) for v in os.environ.get("SPOT", "0,0,0").split(","))
c = socket.socket(socket.AF_UNIX); c.connect(path); cf = c.makefile("w")
b = socket.socket(); b.settimeout(3); b.connect(("127.0.0.1", port)); bf = b.makefile("rw")
bf.write(json.dumps({"op": "hello", "protocol": 1, "joints": 15}) + "\n"); bf.flush(); bf.readline()

odom = {}


def follow():
    s = socket.socket(socket.AF_UNIX); s.connect(path); f = s.makefile("rw")
    f.write(json.dumps({"jsonrpc": "2.0", "id": 1, "method": "robot.subscribe", "params": {}}) + "\n"); f.flush()
    for line in f:
        p = json.loads(line).get("params")
        if p and "odom" in p:
            odom["pos"] = p["odom"]["position"]; odom["yaw"] = p["odom"]["yaw"]


threading.Thread(target=follow, daemon=True).start()


def truth():
    bf.write('{"op":"read"}\n'); bf.flush(); r = json.loads(bf.readline()); w, x, y, z = r["imu"]["quat"]
    return r["trunk"][0], r["trunk"][1], math.atan2(2 * (w * z + x * y), 1 - 2 * (y * y + z * z)), r["trunk_z"]


def send(vx, vyaw):
    cf.write(json.dumps({"jsonrpc": "2.0", "method": "robot.move", "params": {"vx": vx, "vy": 0.0, "vyaw": vyaw}}) + "\n"); cf.flush()


def hold(secs):
    t = time.time()
    while time.time() - t < secs:
        send(0.0, 0.0); time.sleep(0.1)


def reset():
    hold(1.5); x, y, h, _ = truth()
    bf.write(json.dumps({"op": "kidnap", "dx": SPOT[0] - x, "dy": SPOT[1] - y, "dyaw": math.remainder(SPOT[2] - h, math.tau)}) + "\n"); bf.flush(); bf.readline()
    hold(2.5)


time.sleep(1.0)
for rep in range(reps):
    trials = [("steps", 0.3, 0.0, 12.0), ("straight", 0.3, 0.0, 10.0), ("arc", 0.3, 0.5, 8.0), ("arc", 0.3, -0.5, 8.0),
              ("spin", 0.0, 1.5, 3.0), ("spin", 0.0, -1.5, 3.0)]
    if os.environ.get("ONLY"):
        trials = [t for t in trials if t[0] == os.environ["ONLY"]]
    for name, vx, vyaw, secs in trials:
        reset()
        tx0, ty0, th0, _ = truth(); o0, oy0 = list(odom["pos"]), odom["yaw"]
        t = time.time(); tturn = 0.0; oturn = 0.0; prev_t, prev_o = th0, oy0
        while time.time() - t < secs:
            # "steps": the stick's legs, a 0.6 s step then a 1.2 s stand.
            on = name != "steps" or (time.time() - t) % 1.8 < 0.6
            send(vx if on else 0.0, vyaw if on else 0.0); time.sleep(0.05)
            _, _, h, _ = truth(); tturn += math.remainder(h - prev_t, math.tau); prev_t = h
            oturn += math.remainder(odom["yaw"] - prev_o, math.tau); prev_o = odom["yaw"]
        hold(2.0)
        tx1, ty1, _, tz = truth(); o1 = odom["pos"]
        td = math.hypot(tx1 - tx0, ty1 - ty0); od = math.hypot(o1[0] - o0[0], o1[1] - o0[1])
        row = {"label": label, "trial": name, "vyaw": vyaw, "truth_m": round(td, 3), "odom_m": round(od, 3),
               "scale": round(od / td, 3) if td > 0.05 else None,
               "truth_deg": round(math.degrees(tturn), 1), "odom_deg": round(math.degrees(oturn), 1), "fell": tz <= 0.08}
        print(json.dumps(row), flush=True)
hold(1)
