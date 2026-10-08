"""The walk's numbers the navigation's gait model needs, measured on the twin.

    gaitprobe.py <robotd.sock> <body port> <label> [reps]

In an open room (scene_arena.xml), each trial from the middle, the duck carried
there by the simulator's `kidnap`: the straight leg's speed and veer (vx 0.3,
10 s), the yaw rate per unit of yaw command while walking (vx 0.3, vyaw
+-0.3 / +-0.6, 6 s), a turn in place (vx 0, vyaw +-1.0 / +-1.5, 2 s) and its
coast — how far it goes on in the 2 s after the command stops. robot.move
straight to robotd: no trim, no gain (what `[gait]` corrects is measured raw).
"""
import json, math, socket, sys, time

path, port, label = sys.argv[1], int(sys.argv[2]), sys.argv[3]
reps = int(sys.argv[4]) if len(sys.argv) > 4 else 2
c = socket.socket(socket.AF_UNIX); c.connect(path); cf = c.makefile("w")
b = socket.socket(); b.settimeout(3); b.connect(("127.0.0.1", port)); bf = b.makefile("rw")
bf.write(json.dumps({"op": "hello", "protocol": 1, "joints": 15}) + "\n"); bf.flush(); bf.readline()
SPOT = (0.0, 0.0)


def read():
    bf.write('{"op":"read"}\n'); bf.flush(); r = json.loads(bf.readline()); w, x, y, z = r["imu"]["quat"]
    return r["trunk"][0], r["trunk"][1], math.atan2(2 * (w * z + x * y), 1 - 2 * (y * y + z * z)), r["trunk_z"]


def send(vx, vyaw):
    cf.write(json.dumps({"jsonrpc": "2.0", "method": "robot.move", "params": {"vx": vx, "vy": 0.0, "vyaw": vyaw}}) + "\n"); cf.flush()


def hold(secs):
    t = time.time()
    while time.time() - t < secs:
        send(0.0, 0.0); time.sleep(0.1)


def reset():
    hold(1.5); x, y, h, _ = read()
    bf.write(json.dumps({"op": "kidnap", "dx": SPOT[0] - x, "dy": SPOT[1] - y, "dyaw": -h}) + "\n"); bf.flush(); bf.readline()
    hold(2.5)


def run(vx, vyaw, secs):
    """Command for `secs`; the yaw turned, the distance, and the yaw the 2 s after added."""
    x0, y0, h0, _ = read(); prev = h0; turned = 0.0; t0 = time.time()
    while time.time() - t0 < secs:
        send(vx, vyaw); time.sleep(0.05)
        _, _, h, _ = read(); turned += math.remainder(h - prev, math.tau); prev = h
    x1, y1, _, _ = read(); moved = math.hypot(x1 - x0, y1 - y0); during = turned
    t1 = time.time()
    while time.time() - t1 < 2.0:
        send(0.0, 0.0); time.sleep(0.05)
        _, _, h, _ = read(); turned += math.remainder(h - prev, math.tau); prev = h
    tz = read()[3]
    return during, turned - during, moved, tz


rows = []
for rep in range(reps):
    for name, vx, vyaw, secs in [("straight", 0.3, 0.0, 10.0),
                                 ("arc", 0.3, 0.3, 6.0), ("arc", 0.3, -0.3, 6.0),
                                 ("arc", 0.3, 0.6, 6.0), ("arc", 0.3, -0.6, 6.0),
                                 ("spin", 0.0, 1.0, 2.0), ("spin", 0.0, -1.0, 2.0),
                                 ("spin", 0.0, 1.5, 2.0), ("spin", 0.0, -1.5, 2.0)]:
        reset()
        during, coast, moved, tz = run(vx, vyaw, secs)
        row = {"label": label, "trial": name, "vx": vx, "vyaw": vyaw, "secs": secs,
               "yaw_deg": round(math.degrees(during), 1), "coast_deg": round(math.degrees(coast), 1),
               "m": round(moved, 3), "fell": tz <= 0.08}
        if name == "straight":
            row["m_per_s"] = round(moved / secs, 3)
        if name == "arc":
            row["rate_per_unit"] = round(during / secs / vyaw, 3)
        if name == "spin":
            row["deg_per_s"] = round(math.degrees(during) / secs, 1)
        rows.append(row)
        print(json.dumps(row), flush=True)
hold(2)
