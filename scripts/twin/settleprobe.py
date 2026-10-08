"""How long a walk takes to stand still once robotd applies no twist.

    settleprobe.py <robotd.sock> <body port> <label> [reps]

In the arena: walk (vx 0.3, then a turn in place) for 3 s, stop, and follow
for 3 s robotd's applied twist (`robot.state.move.applied`) and the trunk's
true speed in the simulator: when the applied twist reaches zero, and when
the body really is still (under 1 cm/s and 2°/s).
"""
import json, math, socket, sys, threading, time

path, port, label = sys.argv[1], int(sys.argv[2]), sys.argv[3]
reps = int(sys.argv[4]) if len(sys.argv) > 4 else 3
c = socket.socket(socket.AF_UNIX); c.connect(path); cf = c.makefile("w")
b = socket.socket(); b.settimeout(3); b.connect(("127.0.0.1", port)); bf = b.makefile("rw")
bf.write(json.dumps({"op": "hello", "protocol": 1, "joints": 15}) + "\n"); bf.flush(); bf.readline()
state = {}


def follow():
    s = socket.socket(socket.AF_UNIX); s.connect(path); f = s.makefile("rw")
    f.write(json.dumps({"jsonrpc": "2.0", "id": 1, "method": "robot.subscribe", "params": {}}) + "\n"); f.flush()
    for line in f:
        p = json.loads(line).get("params")
        if p and "move" in p:
            state["applied"] = p["move"]["applied"]


threading.Thread(target=follow, daemon=True).start()


def truth():
    bf.write('{"op":"read"}\n'); bf.flush(); r = json.loads(bf.readline()); w, x, y, z = r["imu"]["quat"]
    return r["trunk"][0], r["trunk"][1], math.atan2(2 * (w * z + x * y), 1 - 2 * (y * y + z * z))


def send(vx, vyaw):
    cf.write(json.dumps({"jsonrpc": "2.0", "method": "robot.move", "params": {"vx": vx, "vy": 0.0, "vyaw": vyaw}}) + "\n"); cf.flush()


def hold(secs):
    t = time.time()
    while time.time() - t < secs:
        send(0.0, 0.0); time.sleep(0.1)


time.sleep(1.0)
for rep in range(reps):
    for name, vx, vyaw in [("walk", 0.3, 0.0), ("spin", 0.0, 1.5)]:
        hold(2.0)
        t = time.time()
        while time.time() - t < 3.0:
            send(vx, vyaw); time.sleep(0.05)
        t0 = time.time(); prev = truth(); pt = t0
        zero_at = still_at = None; quiet = 0
        while time.time() - t0 < 3.0:
            send(0.0, 0.0); time.sleep(0.05)
            now = time.time(); cur = truth(); dt = now - pt
            v = math.hypot(cur[0] - prev[0], cur[1] - prev[1]) / dt; w = abs(math.remainder(cur[2] - prev[2], math.tau)) / dt
            prev, pt = cur, now
            if zero_at is None and all(abs(a) < 1e-3 for a in state.get("applied", [1, 1, 1])):
                zero_at = now - t0
            quiet = quiet + 1 if (v < 0.01 and w < math.radians(2)) else 0
            if still_at is None and quiet >= 4:
                still_at = now - t0 - 0.15
        print(json.dumps({"label": label, "trial": name, "twist_zero_s": round(zero_at, 2) if zero_at else None,
                          "body_still_s": round(still_at, 2) if still_at else None}), flush=True)
hold(1)
