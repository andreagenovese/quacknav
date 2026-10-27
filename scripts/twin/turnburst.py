"""A turn in place closed on robotd's odometry yaw, as the explorer's stick_turn
closes it, with the truth read from the simulator's bridge: how far past the
closing point the body goes, split into the odometry's lag at the close and the
coast after the stop.

    turnburst.py <robotd.sock> <sim_port> <label> [wants_deg] [chunks_s] [vyaws]

Each trial from the apartment's open spot (2.06, 0.05) by the simulator's
`kidnap`. Prints one line a trial: want, chunk, vyaw, odometry at the close,
truth at the close, truth after 1.5 s still, odometry after, seconds.
"""
import json, math, socket, sys, threading, time

path, port, label = sys.argv[1], int(sys.argv[2]), sys.argv[3]
wants = [float(x) for x in (sys.argv[4] if len(sys.argv) > 4 else "10,20,45,90").split(",")]
chunks = [float(x) for x in (sys.argv[5] if len(sys.argv) > 5 else "0.15,0.05").split(",")]
vyaws = [float(x) for x in (sys.argv[6] if len(sys.argv) > 6 else "1.0,0.6").split(",")]
prewalk = float(sys.argv[7]) if len(sys.argv) > 7 else 0.0  # a straight walk this long, then 0.4 s stopped, before the turn
stopmode = sys.argv[8] if len(sys.argv) > 8 else "zero"
LEAD = 0.10  # TURN_LEAD_RAD; no coast subtracted: the coast is what is measured

# robotd: one socket for moves, one subscribed to robot.state for the odometry
c = socket.socket(socket.AF_UNIX); c.connect(path); cf = c.makefile("w")
s = socket.socket(socket.AF_UNIX); s.connect(path); sf = s.makefile("rw")
sf.write(json.dumps({"jsonrpc": "2.0", "id": 1, "method": "robot.subscribe", "params": {"hz": 50}}) + "\n"); sf.flush()
odom = {"yaw": None, "n": 0}
def pump():
    while True:
        m = json.loads(sf.readline())
        if m.get("method") == "robot.state":
            odom["yaw"] = m["params"]["odom"]["yaw"]; odom["n"] += 1
threading.Thread(target=pump, daemon=True).start()

b = socket.socket(); b.settimeout(3); b.connect(("127.0.0.1", port)); bf = b.makefile("rw")
bf.write(json.dumps({"op": "hello", "protocol": 1, "joints": 15}) + "\n"); bf.flush(); bf.readline()
SPOT = (2.06, 0.05)
def truth():
    bf.write('{"op":"read"}\n'); bf.flush(); r = json.loads(bf.readline()); w, x, y, z = r["imu"]["quat"]
    return r["trunk"][0], r["trunk"][1], math.atan2(2 * (w * z + x * y), 1 - 2 * (y * y + z * z)), r["trunk_z"]
def send(vx, vyaw): cf.write(json.dumps({"jsonrpc": "2.0", "method": "robot.move", "params": {"vx": vx, "vy": 0.0, "vyaw": vyaw}}) + "\n"); cf.flush()
def hold(secs):
    t = time.time()
    while time.time() - t < secs: send(0.0, 0.0); time.sleep(0.05)
def wrap(a): return math.remainder(a, math.tau)

class Unwrap:
    def __init__(self, h0): self.prev, self.total = h0, 0.0
    def feed(self, h): self.total += wrap(h - self.prev); self.prev = h; return self.total

while odom["yaw"] is None: time.sleep(0.05)
print(f"# {label}: want_deg chunk_s vyaw | odom@close truth@close truth_final odom_final | secs", flush=True)
for want in wants:
    for chunk in chunks:
        for vyaw in vyaws:
            for sign in (1.0, -1.0):
                hold(1.0); x, y, h, _ = truth()
                bf.write(json.dumps({"op": "kidnap", "dx": SPOT[0] - x, "dy": SPOT[1] - y, "dyaw": -h}) + "\n"); bf.flush(); bf.readline()
                hold(2.5)
                if prewalk > 0.0:
                    tw = time.time()
                    while time.time() - tw < prewalk: send(0.3, 0.0); time.sleep(0.05)
                    hold(0.4)
                _, _, h0, _ = truth(); ut = Unwrap(h0); uo = Unwrap(odom["yaw"])
                goal = max(math.radians(want) - LEAD, 0.05)
                budget = 2.0 * math.radians(want) / 0.5 + 1.0
                t0 = time.time(); closed = None
                while time.time() - t0 < budget:
                    # one chunk: the command repeated every 50 ms, as a timed move sends it
                    tc = time.time()
                    while time.time() - tc < chunk:
                        send(0.0, vyaw * sign); time.sleep(0.05)
                        ut.feed(truth()[2])
                    turned = uo.feed(odom["yaw"]) * sign
                    if turned >= goal:
                        closed = (turned, ut.total * sign, time.time() - t0); break
                # After the close: zeros at once ("zero"), or silence as the
                # explorer's stand leaves it, the last command in robotd ("silent").
                if stopmode == "zero": send(0.0, 0.0)
                t1 = time.time()
                while time.time() - t1 < 1.5:
                    if stopmode == "zero": send(0.0, 0.0)
                    time.sleep(0.05); ut.feed(truth()[2])
                of = uo.feed(odom["yaw"]) * sign; tf = ut.total * sign; tz = truth()[3]
                if closed is None:
                    print(f"{label}\t{want:4.0f}\t{chunk:.2f}\t{vyaw * sign:+.1f}\tNOT CLOSED in {budget:.1f} s: odom {math.degrees(of):+5.0f} truth {math.degrees(tf):+5.0f}" + ("" if tz > 0.08 else "\tFALL"), flush=True)
                else:
                    oc, tc_, secs = closed
                    print(f"{label}\t{want:4.0f}\t{chunk:.2f}\t{vyaw * sign:+.1f}\t| {math.degrees(oc):5.0f} {math.degrees(tc_):5.0f} {math.degrees(tf):5.0f} {math.degrees(of):5.0f} | {secs:.2f}" + ("" if tz > 0.08 else "\tFALL"), flush=True)
hold(1.0)
