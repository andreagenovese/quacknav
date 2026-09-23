"""Who has the head: the commanded head_yaw/head_roll at 2 Hz.

    headwatch.py <robotd.sock> <secs> [sway]

With `sway`, also sends quacksat's thinking pose meanwhile (yaw 0.15 sin,
roll 0.10, 10 Hz) and recentres at the end — the sweep should stand aside.
"""
import json, math, socket, sys, threading, time

path, secs = sys.argv[1], float(sys.argv[2])
s = socket.socket(socket.AF_UNIX); s.connect(path); f = s.makefile("rw")
f.write(json.dumps({"jsonrpc": "2.0", "id": 1, "method": "robot.subscribe", "params": {"hz": 2}}) + "\n"); f.flush()

def head(cf, yaw, roll):
    cf.write(json.dumps({"jsonrpc": "2.0", "method": "robot.head", "params": {
        "neck_pitch": 0, "head_pitch": 0, "head_yaw": yaw, "head_roll": roll}}) + "\n"); cf.flush()

if len(sys.argv) > 3:
    def pump():
        c = socket.socket(socket.AF_UNIX); c.connect(path); cf = c.makefile("w"); t0 = time.time()
        while time.time() - t0 < secs:
            head(cf, 0.15 * math.sin(2 * math.pi * (time.time() - t0) / 2.4), 0.10); time.sleep(0.1)
        head(cf, 0.0, 0.0)
    threading.Thread(target=pump, daemon=True).start()

t0, out = time.time(), []
while time.time() - t0 < secs:
    m = json.loads(f.readline())
    if m.get("method") == "robot.state":
        h = m["params"]["head"]; out.append(f"{h[2]:+.2f}/{h[3]:+.2f}")
print("commanded yaw/roll:", " ".join(out))
