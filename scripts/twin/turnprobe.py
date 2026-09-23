"""Turning from a standstill: vx 0 and vyaw from 0.7 to 1.5, each trial from the
apartment's most open spot (2.06, 0.05), the duck carried there by the simulator's
`kidnap` so every trial starts alike.

    turnprobe.py <robotd.sock> <sim_port> <label> [reps]

2026-09-23, fork and daemon-v0.14.4 alike: nothing below the dead zone (2-4 deg/s
at 0.7 and 0.9), 30 deg/s at +1.2, 50-60 deg/s at +-1.5, the body within 4 cm.
"""
import json, socket, sys, time, math
path, port, label = sys.argv[1], int(sys.argv[2]), sys.argv[3]; reps = int(sys.argv[4]) if len(sys.argv) > 4 else 2
c = socket.socket(socket.AF_UNIX); c.connect(path); cf = c.makefile("w")
b = socket.socket(); b.settimeout(3); b.connect(("127.0.0.1", port)); bf = b.makefile("rw")
bf.write(json.dumps({"op":"hello","protocol":1,"joints":15})+"\n"); bf.flush(); bf.readline()
SPOT = (2.06, 0.05)
def read():
    bf.write('{"op":"read"}\n'); bf.flush(); r = json.loads(bf.readline()); w, x, y, z = r["imu"]["quat"]
    return r["trunk"][0], r["trunk"][1], math.atan2(2*(w*z + x*y), 1 - 2*(y*y + z*z)), r["trunk_z"]
def send(vx, vyaw): cf.write(json.dumps({"jsonrpc":"2.0","method":"robot.move","params":{"vx":vx,"vy":0.0,"vyaw":vyaw}})+"\n"); cf.flush()
def hold(secs):
    t = time.time()
    while time.time() - t < secs: send(0.0, 0.0); time.sleep(0.1)
for rep in range(reps):
    for vyaw in [0.7, -0.7, 0.9, -0.9, 1.2, -1.2, 1.5, -1.5]:
        hold(1.5); x, y, h, _ = read()
        bf.write(json.dumps({"op":"kidnap","dx":SPOT[0]-x,"dy":SPOT[1]-y,"dyaw":-h})+"\n"); bf.flush(); bf.readline()
        hold(2.5)
        x0, y0, h0, _ = read(); t0 = time.time(); turned = 0.0; prev = h0
        while time.time() - t0 < 3.0:
            send(0.0, vyaw); time.sleep(0.05)
            _, _, h, _ = read(); turned += math.remainder(h - prev, math.tau); prev = h
        x1, y1, _, tz = read()
        print(f"{label}\tvyaw {vyaw:+.1f}\tturned {math.degrees(turned):+6.0f} deg in 3 s ({math.degrees(turned)/3:+5.0f} deg/s)\tbody moved {math.hypot(x1-x0, y1-y0):.2f} m" + ("" if tz > 0.08 else "\tFALL"), flush=True)
hold(2)
