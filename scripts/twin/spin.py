"""The panorama's turn, measured: a 1 s walking kick (vx 0.3, vyaw 0.7), then
vyaw 0.7 in 0.25 s chunks for 6 s, robot.move at 20 Hz; the yaw from odometry.

    spin.py <robotd.sock> [trials]

2026-09-23: 22-24 deg/s on the released robotd and on the fork alike.
"""
import json, math, socket, sys, threading, time

path = sys.argv[1]
trials = int(sys.argv[2]) if len(sys.argv) > 2 else 3
st = socket.socket(socket.AF_UNIX); st.connect(path); sf = st.makefile("rw")
sf.write(json.dumps({"jsonrpc": "2.0", "id": 1, "method": "robot.subscribe", "params": {"hz": 50}}) + "\n"); sf.flush()
latest = {}

def read():
    while True:
        m = json.loads(sf.readline())
        if m.get("method") == "robot.state":
            latest.update(m["params"])

threading.Thread(target=read, daemon=True).start()
c = socket.socket(socket.AF_UNIX); c.connect(path); cf = c.makefile("w")

def move(vx, vyaw):
    cf.write(json.dumps({"jsonrpc": "2.0", "method": "robot.move", "params": {"vx": vx, "vy": 0.0, "vyaw": vyaw}}) + "\n"); cf.flush()

time.sleep(1)
for trial in range(trials):
    t0 = time.time()
    while time.time() - t0 < 1.0:
        move(0.3, 0.7); time.sleep(0.05)
    y0, t1 = latest["odom"]["yaw"], time.time()
    while time.time() - t1 < 6.0:
        move(0.0, 0.7); time.sleep(0.05)
    d = math.degrees(math.remainder(latest["odom"]["yaw"] - y0, math.tau))
    print(f"trial {trial}: {d:+.0f} deg in 6 s = {d / 6:+.1f} deg/s", flush=True)
    t2 = time.time()
    while time.time() - t2 < 4:
        move(0.0, 0.0); time.sleep(0.1)
