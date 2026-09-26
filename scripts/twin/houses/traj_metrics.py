"""traj_metrics.py <pose.tsv> [--delta 1.0] [--tum out_prefix] [--json]

ATE and RPE from a pose sampler's file (poseerr.py), in the form other
systems publish them:

  ATE  absolute trajectory error: the tracked pose against the truth at each
       sample, as RMSE / mean / median / max of the translation (and of the
       heading, when the file carries it). Reported twice: as recorded — on the
       twin the map frame is the world frame, so this is the error the duck
       lives with — and after the best rigid SE(2) alignment (Horn/Umeyama,
       evo's `--align`), which leaves only the shape of the trajectory.
  RPE  relative pose error over `delta` metres walked (by the truth): for every
       sample i, the first j at least `delta` further along; the error of the
       estimated motion i→j against the true one, translation (m, and % of
       delta) and rotation (deg). Drift, independent of where the frame sits.

Samples are split into segments where the sampler was not tracking or the
twin restarted (a gap in time, or the truth jumping), and RPE never spans two.
`--tum` writes `<prefix>.est.tum` and `<prefix>.gt.tum` (timestamp x y z qx qy
qz qw, one segment after the other) for `evo_ape tum gt est` / `evo_rpe`.
"""
import argparse
import json
import math
import sys

import numpy as np


def load_cov(path):
    """The mapper's covariance per row (xx, xy, yy, yaw·yaw), NaN where the
    file has none (the live sampler's), aligned with `load`."""
    rows = []
    for line in open(path):
        f = line.rstrip("\n").split("\t")
        if len(f) < 6:
            continue
        rows.append([float(v) if v else math.nan for v in f[10:14]] if len(f) >= 14 else [math.nan] * 4)
    return np.array(rows, dtype=float).reshape(-1, 4)


def nees(a, cov, idx):
    """Normalized estimation error squared of the position: e·Σ⁻¹·e, which
    averages 2 for an honest 2-D covariance, and the share of samples inside
    the 95 % ellipse (χ²₂ = 5.991), which should be about 0.95. Lower NEES
    means the covariance is too cautious, higher that it is overconfident."""
    vals, sig = [], []
    for i in idx:
        xx, xy, yy, _ = cov[i]
        if not all(math.isfinite(v) for v in (xx, xy, yy)):
            continue
        s = np.array([[xx, xy], [xy, yy]])
        e = np.array([a[i, 1] - a[i, 4], a[i, 2] - a[i, 5]])
        try:
            vals.append(float(e @ np.linalg.solve(s + np.eye(2) * 1e-10, e)))
        except np.linalg.LinAlgError:
            continue
        sig.append(math.sqrt(max(np.linalg.eigvalsh(s)[-1], 0.0)))
    if not vals:
        return None
    v = np.array(vals)
    return {"mean": float(v.mean()), "median": float(np.median(v)), "inside95": float((v <= 5.991).mean()),
            "n": len(v), "sigma_major_median_m": float(np.median(sig))}


def load(path):
    """Rows of (t, est x, est y, est yaw|nan, true x, true y, true yaw|nan)."""
    rows = []
    for line in open(path):
        f = line.rstrip("\n").split("\t")
        if len(f) < 6:
            continue
        t, mx, my, tx, ty = (float(v) for v in f[:5])
        myaw = float(f[8]) if len(f) > 9 and f[8] not in ("", "None") else math.nan
        tyaw = float(f[9]) if len(f) > 9 and f[9] not in ("", "None") else math.nan
        rows.append((t, mx, my, myaw, tx, ty, tyaw))
    return np.array(rows, dtype=float).reshape(-1, 7)


def segments(a, max_gap_s, max_jump_m):
    """Index ranges [s, e) of continuous tracking."""
    if len(a) == 0:
        return []
    cuts = [0]
    for i in range(1, len(a)):
        gap = a[i, 0] - a[i - 1, 0]
        jump = math.hypot(a[i, 4] - a[i - 1, 4], a[i, 5] - a[i - 1, 5])
        if gap > max_gap_s or jump > max_jump_m:
            cuts.append(i)
    cuts.append(len(a))
    return [(s, e) for s, e in zip(cuts, cuts[1:]) if e - s >= 2]


def wrap(a):
    return (a + math.pi) % (2 * math.pi) - math.pi


def stats(v):
    v = np.asarray(v, dtype=float)
    v = v[np.isfinite(v)]
    if len(v) == 0:
        return None
    return {
        "rmse": float(np.sqrt(np.mean(v**2))),
        "mean": float(np.mean(v)),
        "median": float(np.median(v)),
        "max": float(np.max(v)),
        "n": int(len(v)),
    }


def align_se2(est, gt):
    """R, t minimizing |R·est + t − gt|² (Horn 1987, 2-D)."""
    me, mg = est.mean(axis=0), gt.mean(axis=0)
    h = (est - me).T @ (gt - mg)
    u, _, vt = np.linalg.svd(h)
    d = np.sign(np.linalg.det(vt.T @ u.T))
    r = vt.T @ np.diag([1.0, d]) @ u.T
    return r, mg - r @ me


def rel(p, q):
    """Pose q seen from pose p (x, y, yaw)."""
    dx, dy = q[0] - p[0], q[1] - p[1]
    c, s = math.cos(p[2]), math.sin(p[2])
    return (c * dx + s * dy, -s * dx + c * dy, wrap(q[2] - p[2]))


def rpe(a, segs, delta):
    """Relative errors over `delta` metres of true travel, within segments."""
    et, er = [], []
    have_yaw = np.isfinite(a[:, 3]).all() and np.isfinite(a[:, 6]).all()
    for s, e in segs:
        along = np.concatenate([[0.0], np.cumsum(np.hypot(np.diff(a[s:e, 4]), np.diff(a[s:e, 5])))])
        j = 0
        for i in range(e - s):
            j = max(j, i + 1)
            while j < e - s and along[j] - along[i] < delta:
                j += 1
            if j >= e - s:
                break
            gi, gj = a[s + i], a[s + j]
            if have_yaw:
                g = rel((gi[4], gi[5], gi[6]), (gj[4], gj[5], gj[6]))
                m = rel((gi[1], gi[2], gi[3]), (gj[1], gj[2], gj[3]))
                # The error motion, expressed in the estimate's step frame.
                err = rel(g, m)
                et.append(math.hypot(err[0], err[1]))
                er.append(abs(err[2]))
            else:
                # No heading in the file: compare the displacement vectors'
                # lengths and directions in the world frame (a lower bound).
                gd = np.array([gj[4] - gi[4], gj[5] - gi[5]])
                md = np.array([gj[1] - gi[1], gj[2] - gi[2]])
                et.append(float(np.linalg.norm(md - gd)))
    return et, er


def yaw_to_quat(yaw):
    return (0.0, 0.0, math.sin(yaw / 2), math.cos(yaw / 2))


def write_tum(a, segs, prefix):
    with open(prefix + ".est.tum", "w") as fe, open(prefix + ".gt.tum", "w") as fg:
        for s, e in segs:
            for r in a[s:e]:
                for f, (x, y, yaw) in ((fe, r[1:4]), (fg, r[4:7])):
                    q = yaw_to_quat(0.0 if not math.isfinite(yaw) else yaw)
                    f.write(f"{r[0]:.3f} {x:.4f} {y:.4f} 0 {q[0]} {q[1]} {q[2]:.6f} {q[3]:.6f}\n")


def metrics(path, delta=1.0, max_gap_s=20.0, max_jump_m=0.5):
    a = load(path)
    segs = segments(a, max_gap_s, max_jump_m)
    idx = np.concatenate([np.arange(s, e) for s, e in segs]) if segs else np.array([], dtype=int)
    out = {"file": path, "samples": int(len(idx)), "segments": len(segs), "delta_m": delta}
    if len(idx) == 0:
        return out, a, segs
    est, gt = a[idx][:, 1:3], a[idx][:, 4:6]
    out["ate_m"] = stats(np.linalg.norm(est - gt, axis=1))
    r, t = align_se2(est, gt)
    out["ate_aligned_m"] = stats(np.linalg.norm((est @ r.T + t) - gt, axis=1))
    yaw = wrap(a[idx][:, 3] - a[idx][:, 6])
    if np.isfinite(yaw).any():
        out["ate_yaw_deg"] = stats(np.degrees(np.abs(yaw)))
        # The alignment's rotation, taken out of the heading as well.
        out["ate_yaw_aligned_deg"] = stats(np.degrees(np.abs(wrap(yaw + math.atan2(r[1, 0], r[0, 0])))))
    et, er = rpe(a, segs, delta)
    out["rpe_m"] = stats(et)
    if out["rpe_m"]:
        out["rpe_pct"] = 100.0 * out["rpe_m"]["rmse"] / delta
    if er:
        out["rpe_yaw_deg"] = stats(np.degrees(er))
    n = nees(a, load_cov(path), idx)
    if n:
        out["nees_xy"] = n
    out["travel_m"] = float(sum(
        np.hypot(np.diff(a[s:e, 4]), np.diff(a[s:e, 5])).sum() for s, e in segs))
    return out, a, segs


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("tsv")
    ap.add_argument("--delta", type=float, default=1.0)
    ap.add_argument("--tum")
    ap.add_argument("--json", action="store_true")
    o = ap.parse_args()
    out, a, segs = metrics(o.tsv, o.delta)
    if o.tum:
        write_tum(a, segs, o.tum)
    if o.json:
        json.dump(out, sys.stdout, indent=1)
        print()
        return
    def line(name, s, unit):
        if s:
            print(f"  {name:<14} rmse {s['rmse']:.3f}  mean {s['mean']:.3f}  median {s['median']:.3f}  max {s['max']:.3f} {unit}  (n={s['n']})")
    print(f"{o.tsv}: {out['samples']} samples in {out['segments']} segments, {out.get('travel_m', 0):.1f} m walked")
    line("ATE", out.get("ate_m"), "m")
    line("ATE aligned", out.get("ate_aligned_m"), "m")
    line("ATE yaw", out.get("ate_yaw_deg"), "deg")
    line("ATE yaw alig.", out.get("ate_yaw_aligned_deg"), "deg")
    line(f"RPE/{o.delta:g} m", out.get("rpe_m"), "m")
    if "rpe_pct" in out:
        print(f"  {'':<14} = {out['rpe_pct']:.1f} % of the distance")
    line("RPE yaw", out.get("rpe_yaw_deg"), "deg")
    if "nees_xy" in out:
        n = out["nees_xy"]
        print(f"  {'NEES xy':<14} mean {n['mean']:.2f} (2 is honest)  median {n['median']:.2f}  inside the 95 % ellipse {100*n['inside95']:.0f} %  sigma major median {n['sigma_major_median_m']:.3f} m")


if __name__ == "__main__":
    main()
