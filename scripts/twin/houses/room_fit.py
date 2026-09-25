"""room_fit.py <frame.json> <truth.toml> <truth.json>

How a map sits on the house, room by room: for the whole map and for each
room of the truth, the map's wall cells against the true walls as they are
(mean and p90 distance), and the rigid motion (rotation, shift) that fits
them best, with what is left after it.

A map can score a low mean over the whole house and still have its rooms
turned against each other: casa_arredata's map after its second session
(2026-09-25) sat +0.45° on the house as a whole, with its bedroom +2.8° and
its office -2.0° — 15 cm at a room's edge, where the duck, right on the map,
was 0.3 m off in the house. The per-room rotation is the number that shows
it. `frame.json` from `maploc/examples/dump_frame --as-navd`.
"""
import base64
import json
import math
import sys
import tomllib

import numpy as np


def wall_cells(path):
    f = json.load(open(path))
    f = f.get("frame", f)
    c = np.frombuffer(base64.b64decode(f["cells"]), dtype=np.uint8).reshape(f["rows"], f["cols"])
    r, k = np.nonzero(c == 2)
    return np.c_[f["x_min"] + (k + 0.5) * f["cell_m"], f["y_min"] + (r + 0.5) * f["cell_m"]]


def truth_points(path):
    pts = []
    for x1, y1, x2, y2 in tomllib.load(open(path, "rb"))["walls"]:
        n = max(2, int(math.hypot(x2 - x1, y2 - y1) / 2))
        for t in np.linspace(0, 1, n):
            pts.append(((x1 + t * (x2 - x1)) / 100, (y1 + t * (y2 - y1)) / 100))
    return np.array(pts)


def nearest(a, b):
    return np.sqrt(((a[:, None, :] - b[None, :, :]) ** 2).sum(-1).min(1))


def icp(src, dst, iters=40):
    """Rigid (R, t) taking src onto dst, trimmed to the nearer 80 %."""
    R, t = np.eye(2), np.zeros(2)
    for _ in range(iters):
        cur = src @ R.T + t
        d = ((cur[:, None, :] - dst[None, :, :]) ** 2).sum(-1)
        j = d.argmin(1)
        dist = np.sqrt(d[np.arange(len(cur)), j])
        keep = dist < max(0.2, np.percentile(dist, 80))
        a, b = cur[keep], dst[j[keep]]
        ma, mb = a.mean(0), b.mean(0)
        u, _, vt = np.linalg.svd((a - ma).T @ (b - mb))
        ri = vt.T @ np.diag([1, np.sign(np.linalg.det(vt.T @ u.T))]) @ u.T
        R, t = ri @ R, ri @ t + mb - ri @ ma
    return math.degrees(math.atan2(R[1, 0], R[0, 0])), t, nearest(src @ R.T + t, dst)


def main():
    frame, toml, rooms_json = sys.argv[1:4]
    cells, walls = wall_cells(frame), truth_points(toml)
    rng = np.random.default_rng(0)
    sample = cells[rng.choice(len(cells), min(len(cells), 2000), replace=False)]
    rows = [("house", sample, walls)]
    for name, (x0, x1, y0, y1) in json.load(open(rooms_json))["rooms"].items():
        inside = lambda p, m: (p[:, 0] > min(x0, x1) - m) & (p[:, 0] < max(x0, x1) + m) & (p[:, 1] > min(y0, y1) - m) & (p[:, 1] < max(y0, y1) + m)
        rows.append((name, cells[inside(cells, 0.15)], walls[inside(walls, 0.4)]))
    print(f"{'':<11} {'cells':>5}  {'as is: mean / p90':>18}  {'best fit: rotation, shift':>28}  {'then mean':>9}")
    for name, src, dst in rows:
        if len(src) < 30 or len(dst) < 10:
            print(f"{name:<11} {len(src):>5}  too few wall cells")
            continue
        d0 = nearest(src, dst)
        ang, t, d = icp(src, dst)
        print(f"{name:<11} {len(src):>5}  {d0.mean()*100:6.1f} / {np.percentile(d0, 90)*100:5.1f} cm   {ang:+6.2f}°, ({t[0]*100:+5.1f}, {t[1]*100:+5.1f}) cm   {d.mean()*100:6.1f} cm")


if __name__ == "__main__":
    main()
