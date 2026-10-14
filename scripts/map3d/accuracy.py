"""accuracy.py <build dir> <recording dir> <house>.truth.json

How far the 3D house build.py made lies from the twin's true one: each
surface voxel's distance to the nearest true surface (the floor, outside
the holes, and the faces of the truth's boxes, each standing from the
floor to its height). The map's frame is put on the simulator's by the
rigid fit of the recorded poses onto the truth's (the map starts where
the duck woke, the simulator's origin elsewhere). The twin only.
"""
from __future__ import annotations

import json
import math
import sys

import numpy as np


def rigid_2d(a, b):
    """R, t with b ~ R a + t (least squares, Kabsch in the plane)."""
    ca, cb = a.mean(0), b.mean(0)
    H = (a - ca).T @ (b - cb)
    U, _, Vt = np.linalg.svd(H)
    R = Vt.T @ U.T
    if np.linalg.det(R) < 0:
        Vt[1] *= -1
        R = Vt.T @ U.T
    return R, cb - R @ ca


def box_distance(P, x0, x1, y0, y1, h):
    """Unsigned distance to a solid box's surface (inside: to the nearest face)."""
    lo = np.array([x0, y0, 0.0])
    hi = np.array([x1, y1, h])
    c, e = (lo + hi) / 2, (hi - lo) / 2
    q = np.abs(P - c) - e
    outside = np.linalg.norm(np.maximum(q, 0), axis=1)
    inside = np.minimum(q.max(1), 0)
    return np.abs(outside + inside)


def main():
    build, rec, truth_p = sys.argv[1:4]
    s = np.load(f"{build}/surface.npz")
    P = s["P"].astype(float)
    recs = [json.loads(l) for l in open(f"{rec}/frames.jsonl")]
    pairs = [(r["pose"]["x"], r["pose"]["y"], r["truth"]["trunk"][0], r["truth"]["trunk"][1])
             for r in recs if r.get("pose") and r.get("tracking") and r.get("truth")]
    a = np.array([p[:2] for p in pairs])
    b = np.array([p[2:] for p in pairs])
    R, t = rigid_2d(a, b)
    pose_err = np.linalg.norm(a @ R.T + t - b, axis=1)
    W = P.copy()
    W[:, :2] = P[:, :2] @ R.T + t
    truth = json.load(open(truth_p))
    d = np.abs(W[:, 2])
    for x0, x1, y0, y1 in truth["holes"]:
        inh = (W[:, 0] > x0) & (W[:, 0] < x1) & (W[:, 1] > y0) & (W[:, 1] < y1)
        d[inh] = np.inf
    for _, x0, x1, y0, y1, h in truth["boxes"]:
        d = np.minimum(d, box_distance(W, x0, x1, y0, y1, h))
    ok = np.isfinite(d)
    print(f"poses: rotation {math.degrees(math.atan2(R[1, 0], R[0, 0])):.1f} deg, "
          f"residual median {np.median(pose_err) * 100:.1f} cm, max {pose_err.max() * 100:.1f} cm")
    # The floor is most of the surface and the easiest; walls and things apart.
    for name, sel in (("all", ok), ("floor (z < 5 cm)", ok & (W[:, 2] < 0.05)),
                      ("above the floor", ok & (W[:, 2] >= 0.05))):
        x = d[sel]
        if len(x) == 0:
            continue
        print(f"{name}: {len(x)} voxels, distance to the true surface median {np.median(x) * 100:.1f} cm, "
              f"within 5 cm {np.mean(x < 0.05) * 100:.0f} %, within 10 cm {np.mean(x < 0.10) * 100:.0f} %, "
              f"p90 {np.percentile(x, 90) * 100:.1f} cm")


if __name__ == "__main__":
    main()
