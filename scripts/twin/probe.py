"""What a mapper outside robotd gets: robot.state and tof.stream, side by side.

    probe.py <robotd.sock> <tof.sock> [secs]

Rates, the fields maploc needs, and how far apart the two clocks put a depth
frame and the nearest state (they share CLOCK_MONOTONIC since API v24).
"""
import json, socket, sys, threading, time

RS, TS = sys.argv[1], sys.argv[2]
SECS = float(sys.argv[3]) if len(sys.argv) > 3 else 4.0

def stream(path, method, out):
    s = socket.socket(socket.AF_UNIX); s.connect(path); f = s.makefile("rw")
    f.write(json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": {}}) + "\n"); f.flush()
    t0 = time.time()
    while time.time() - t0 < SECS:
        line = f.readline()
        if not line:
            break
        m = json.loads(line)
        if "method" in m:
            out.append(m["params"])

states, frames = [], []
a = threading.Thread(target=stream, args=(RS, "robot.subscribe", states))
b = threading.Thread(target=stream, args=(TS, "tof.stream", frames))
a.start(); b.start(); a.join(); b.join()
states = [p for p in states if "odom" in p]
print(f"robot.state {len(states) / SECS:.1f} Hz, tof.stream {len(frames) / SECS:.1f} Hz")
if states:
    x = states[-1]
    print("  odom", x["odom"], " t_ns", x.get("t_ns"), " imu", "yes" if x.get("imu") else "no")
    print("  head measured (joints 5..8)", [round(v, 3) for v in x["joints"][5:9]], " commanded", x["head"])
    print("  gravity", x["safety"]["gravity"], " policy", x["policy"], " move.applied", x["move"]["applied"])
if states and frames:
    gaps = [min(abs(fr["t_ns"] - s["t_ns"]) for s in states) / 1e6 for fr in frames[-20:]]
    print(f"  tof.t_ns to the nearest state.t_ns, last 20 frames: max {max(gaps):.1f} ms")
