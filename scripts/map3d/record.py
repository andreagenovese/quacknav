"""record.py <state dir> <out dir> [seconds] [--sim-port PORT]

What the 3D textured map is built from, recorded beside a running duck
(the twin, or the duck itself) through the daemons' own sockets only:

  - mediad's `media.frame` (`<state>/media.sock`; `/run/mediad/media.sock`
    on the duck): one raw UYVY frame;
  - robotd's `robot.state` stream: `frames.camera` and `frames.tof` (the
    camera and the depth sensor in the trunk frame), the IMU's attitude,
    the trunk's height, the joints, the applied move;
  - tofd's `tof.stream`: the 8x8 ranges and their status;
  - quack-navd's `robot.map_status`: the pose on the map (x, y, yaw).

A frame is taken only while the duck stands (no move applied for
STILL_S), at most one each EVERY_S: a walking body bounces, and the pose
and the picture would not belong together. Each capture is
`<n>.uyvy` (the raw frame) and one line of `frames.jsonl`.

`--sim-port` adds the simulator's truth (the trunk's pose) to each line,
for measuring only: nothing built from the recording reads it.
"""
from __future__ import annotations

import json
import os
import socket
import sys
import threading
import time

STILL_S = 0.6
EVERY_S = 0.5


def connect(path, timeout=5.0):
    s = socket.socket(socket.AF_UNIX)
    s.settimeout(timeout)
    s.connect(path)
    return s


class Stream(threading.Thread):
    """The newest message of a subscription, kept by a thread."""

    def __init__(self, path, method, params):
        super().__init__(daemon=True)
        self.path, self.method, self.params = path, method, params
        self.latest, self.at = None, 0.0
        self.lock = threading.Lock()

    def run(self):
        while True:
            try:
                s = connect(self.path, 30.0)
                f = s.makefile("rw")
                f.write(json.dumps({"jsonrpc": "2.0", "id": 0, "method": self.method, "params": self.params}) + "\n")
                f.flush()
                for line in f:
                    m = json.loads(line)
                    if "params" in m:
                        with self.lock:
                            self.latest, self.at = m["params"], time.monotonic()
            except (OSError, ValueError):
                time.sleep(1.0)

    def get(self):
        with self.lock:
            return self.latest, self.at


def nav_status(state):
    s = connect(f"{state}/nav.sock")
    f = s.makefile("rw")
    f.write(json.dumps({"jsonrpc": "2.0", "id": 1, "method": "nav.call",
                        "params": {"name": "robot.map_status", "args": {}}}) + "\n")
    f.flush()
    return json.loads(f.readline()).get("result") or {}


def media_frame(state):
    s = connect(f"{state}/media.sock")
    f = s.makefile("rwb")
    f.write((json.dumps({"jsonrpc": "2.0", "id": 1, "method": "media.frame"}) + "\n").encode())
    f.flush()
    head = json.loads(f.readline())["result"]
    raw = f.read(head["bytes"])
    return head, raw


class Truth:
    """The simulator's trunk pose (measuring only)."""

    def __init__(self, port):
        b = socket.create_connection(("127.0.0.1", port), timeout=3)
        self.f = b.makefile("rw")
        self.f.write(json.dumps({"op": "hello", "protocol": 1, "joints": 15}) + "\n")
        self.f.flush()
        self.f.readline()

    def read(self):
        self.f.write('{"op":"read"}\n')
        self.f.flush()
        body = json.loads(self.f.readline())
        return {"trunk": body["trunk"], "quat": body["imu"]["quat"]}


def main():
    args = sys.argv[1:]
    port = None
    if "--sim-port" in args:
        i = args.index("--sim-port")
        port = int(args[i + 1])
        del args[i:i + 2]
    state, out = args[0], args[1]
    seconds = float(args[2]) if len(args) > 2 else 300.0
    os.makedirs(out, exist_ok=True)
    robot = Stream(f"{state}/robotd.sock", "robot.subscribe", {"hz": 20})
    tof = Stream(f"{state}/tof.sock", "tof.stream", {})
    robot.start()
    tof.start()
    truth = Truth(port) if port else None
    log = open(f"{out}/frames.jsonl", "a", buffering=1)
    n = len(os.listdir(out)) // 2
    t0 = time.monotonic()
    moved_at = time.monotonic()
    last_take = 0.0
    while time.monotonic() - t0 < seconds:
        time.sleep(0.05)
        st, _ = robot.get()
        if st is None:
            continue
        if any(abs(v) > 1e-6 for v in st["move"]["applied"]) or st["safety"].get("fallen"):
            moved_at = time.monotonic()
            continue
        now = time.monotonic()
        if now - moved_at < STILL_S or now - last_take < EVERY_S:
            continue
        try:
            nav = nav_status(state)
            head, raw = media_frame(state)
        except (OSError, ValueError, KeyError) as e:
            print(f"skipped a frame: {e}", flush=True)
            continue
        st_after, _ = robot.get()
        if any(abs(v) > 1e-6 for v in st_after["move"]["applied"]):
            moved_at = time.monotonic()
            continue
        tf, _ = tof.get()
        pose = nav.get("pose")
        rec = {
            "n": n, "t": round(now - t0, 3),
            "media": head,
            "pose": pose, "tracking": nav.get("tracking"),
            "map": (nav.get("house") or {}).get("map"),
            "uncertainty": nav.get("pose_uncertainty"),
            "frames": st["frames"], "imu_quat": st["imu"]["quat"],
            "odom": st["odom"], "joints": st["joints"], "policy": st["policy"],
            "state_t_ns": st["t_ns"],
            "tof": tf and {k: tf[k] for k in ("distance_mm", "status", "rows", "cols", "t_ns")},
        }
        if truth:
            rec["truth"] = truth.read()
        with open(f"{out}/{n:05d}.uyvy", "wb") as fr:
            fr.write(raw)
        log.write(json.dumps(rec) + "\n")
        n += 1
        last_take = now
        if n % 20 == 0:
            print(f"{n} frames, {now - t0:.0f} s", flush=True)
    print(f"done: {n} frames in {out}", flush=True)


if __name__ == "__main__":
    main()
