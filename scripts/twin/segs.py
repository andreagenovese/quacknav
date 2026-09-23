"""A `.mdlg` recording's odometry as runs of moving and still.

    segs.py <file.mdlg> [from_s] [to_s]

How the 2026-09-23 slow start was found: the panorama's steps 5-8 "moved"
6.9 s each for 4 degrees in all. Format: maploc/src/record.rs (v2).
"""
import math, struct, sys

data = open(sys.argv[1], "rb").read()
a = float(sys.argv[2]) if len(sys.argv) > 2 else 0.0
b = float(sys.argv[3]) if len(sys.argv) > 3 else float("inf")
assert data[:4] == b"MDLG", "not a .mdlg"
i, ticks = 16, []
while i + 13 <= len(data):
    ts, sid, size = struct.unpack_from("<QBI", data, i); i += 13
    if sid == 2 and size == 45:
        v = struct.unpack_from("<11fB", data, i)
        ticks.append((ts / 1e6, v[2], bool(v[11] & 1)))
    i += size
ticks = [t for t in ticks if a <= t[0] <= b]
start = 0
for k in range(1, len(ticks) + 1):
    if k == len(ticks) or ticks[k][2] != ticks[start][2]:
        t0, y0, moving = ticks[start]; t1, y1, _ = ticks[k - 1]
        if t1 - t0 >= 0.15:
            dy = math.degrees(math.remainder(y1 - y0, math.tau))
            print(f"{t0:8.1f}s  {'MOVE ' if moving else 'still'} {t1 - t0:5.2f}s  yaw {dy:+5.0f}")
        start = k
