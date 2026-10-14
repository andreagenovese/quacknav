"""build.py <recording dir> <out dir> [--max N] [--voxel M] [--truth] [--keep-sky]

A textured 3D house from what record.py kept: the head camera's frames,
the pose on the map, the camera's and the depth sensor's place on the
body, and the 8x8 depth sensor's ranges. Off board (a Mac or a PC while
the duck charges); nothing here runs on the duck.

1. **Depth from one picture.** Depth Anything V2 Small (Apache-2.0; the
   Base and Large weights are non-commercial and not used) gives each
   frame a relative inverse depth, up to an unknown scale and shift. The
   picture is turned upright first (the head camera is mounted a quarter
   turn round, mediad's `rotate`), as the model was trained on upright
   pictures.
2. **Metres from the depth sensor.** The sensor's 64 returns, carried into
   the camera through the two poses on the body, are where the true depth
   is known: `1/z = a * d + b` is fitted to them, frame by frame, robustly.
   A frame with too few returns in view, or a fit that does not hold,
   is left out.
3. **Fused.** Each frame's metric depth goes into a truncated signed
   distance volume (TSDF) on the map's frame, with the camera's colours
   averaged in: a surface many frames agree on stays, a single frame's
   mistake is outvoted.
4. **Drawn.** The surface (the volume's zero crossings, shaded by its
   normal) is splatted in isometric views, a dollhouse cut at
   CUT_Z_M, and from the top.

`--truth` builds on the simulator's poses instead of the map's, for
measuring what the map's pose costs.

Outputs in <out dir>: iso_*.png, top.png, cloud.ply (the surface as
coloured points), house3d.bin (the same for quack-control's 3D view;
write_house3d says how) and fit.tsv (each frame's scale fit).
"""
from __future__ import annotations

import json
import math
import os
import sys
import time

import numpy as np
from PIL import Image, ImageDraw, ImageFont

FOVY_DEG = 45.0  # the twin's head camera (microduck_rl's model, no fovy: MuJoCo's default)
MAX_DEPTH_M = 2.2  # beyond this a single picture's depth is not trusted
MIN_DEPTH_M = 0.12
TRUNC_M = 0.06
Z_RANGE = (-0.06, 1.0)
CUT_Z_M = 0.55
MIN_FIT_POINTS = 8
# The twin's walls are 0.5 m high with a sky over them, to which a single
# picture gives a depth like any other; a duck's house has a ceiling.
# Blue above the horizon is not fused (`--keep-sky` fuses it).
SKY_MASK = True
# Where a single picture's depth jumps (an edge), the pixels in between
# belong to neither side: dropped when the depth changes by more than this
# share from one pixel to the next.
EDGE_JUMP = 0.04
TOF_MAX_MM = 3500
# robotd's `frames.camera` is the optical frame (z ahead, x right, y down
# for an upright picture); the raw picture is turned a quarter round
# (mediad's `rotate` 90): its x runs down the world, its y across. As
# columns, the raw picture's axes in a camera that looks down its own -z
# (measured on the twin, its camera turned to face forward:
# viewer/body_with_map.py's face_cameras_forward).
SITE_TO_CAM = np.array([[0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]])


def quat_R(q):
    w, x, y, z = q
    return np.array([
        [1 - 2 * (y * y + z * z), 2 * (x * y - w * z), 2 * (x * z + w * y)],
        [2 * (x * y + w * z), 1 - 2 * (x * x + z * z), 2 * (y * z - w * x)],
        [2 * (x * z - w * y), 2 * (y * z + w * x), 1 - 2 * (x * x + y * y)],
    ])


def Rz(a):
    c, s = math.cos(a), math.sin(a)
    return np.array([[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]])


def uyvy_rgb(raw, w, h):
    a = np.frombuffer(raw, np.uint8).reshape(h, w // 2, 4).astype(np.float32)
    u, y0, v, y1 = a[..., 0], a[..., 1], a[..., 2], a[..., 3]
    y = np.empty((h, w), np.float32)
    y[:, 0::2], y[:, 1::2] = y0, y1
    u = np.repeat(u, 2, axis=1) - 128.0
    v = np.repeat(v, 2, axis=1) - 128.0
    c = 1.164 * (y - 16.0)
    rgb = np.stack([c + 1.596 * v, c - 0.392 * u - 0.813 * v, c + 2.017 * u], -1)
    return np.clip(rgb, 0, 255).astype(np.uint8)


def trunk_pose(rec, truth=False):
    """The trunk on the map: the map's x, y, yaw; the IMU's tilt; the
    odometry's height. `truth`: the simulator's, for measuring only."""
    if truth:
        t = np.array(rec["truth"]["trunk"][:3], float)
        return quat_R(rec["truth"]["quat"]), t
    p = rec["pose"]
    Ri = quat_R(rec["imu_quat"])
    tilt = Rz(-math.atan2(Ri[1, 0], Ri[0, 0])) @ Ri
    R = Rz(p["yaw"]) @ tilt
    t = np.array([p["x"], p["y"], rec["odom"]["position"][2]])
    return R, t


def on_map(R, t, frame):
    return R @ quat_R(frame["quat"]), t + R @ np.array(frame["pos"])


BEAMS = None


def beams():
    """The 8x8 sensor's beam directions in its own frame (kinematics::tof)."""
    global BEAMS
    if BEAMS is None:
        fov = 45.0
        half = math.radians(fov / 2 - fov / 8 / 2)
        step = 2 * half / 7
        out = []
        for i in range(64):
            el = half - (i // 8) * step
            az = half - (i % 8) * step
            out.append([math.cos(el) * math.cos(az), math.cos(el) * math.sin(az), math.sin(el)])
        BEAMS = np.array(out)
    return BEAMS


class Camera:
    def __init__(self, w, h):
        self.w, self.h = w, h
        self.f = (h / 2) / math.tan(math.radians(FOVY_DEG) / 2)

    def project(self, Rc, c, P):
        """World points -> (u, v, z): pixel column, row, depth ahead."""
        q = (P - c) @ Rc  # camera coordinates
        z = -q[:, 2]
        with np.errstate(divide="ignore", invalid="ignore"):
            u = self.w / 2 + self.f * q[:, 0] / z
            v = self.h / 2 - self.f * q[:, 1] / z
        return u, v, z


def upright_turns(Rc):
    """np.rot90 turns that put the world's up at the top of the picture."""
    up = Rc.T @ np.array([0.0, 0.0, 1.0])
    dx, dy = up[0], -up[1]  # in the picture: x right, y down
    if abs(dx) > abs(dy):
        return 1 if dx > 0 else 3
    return 0 if dy < 0 else 2


class Depth:
    def __init__(self):
        import torch
        from transformers import AutoImageProcessor, AutoModelForDepthEstimation

        name = "depth-anything/Depth-Anything-V2-Small-hf"
        self.torch = torch
        self.dev = "mps" if torch.backends.mps.is_available() else "cpu"
        self.proc = AutoImageProcessor.from_pretrained(name)
        self.model = AutoModelForDepthEstimation.from_pretrained(name).to(self.dev)
        self.model.train(False)

    def relative(self, rgb, k):
        up = np.ascontiguousarray(np.rot90(rgb, k))
        inp = self.proc(images=Image.fromarray(up), return_tensors="pt").to(self.dev)
        with self.torch.no_grad():
            d = self.model(**inp).predicted_depth
        d = self.torch.nn.functional.interpolate(d[:, None], size=up.shape[:2], mode="bilinear", align_corners=False)
        return np.ascontiguousarray(np.rot90(d[0, 0].cpu().numpy(), -k))


def unusable(rgb, depth, cam, Rc, sky=True):
    """Pixels not to fuse: the twin's sky, and depth edges."""
    gy, gx = np.gradient(depth)
    bad = np.hypot(gx, gy) > EDGE_JUMP * depth
    if sky:
        v, u = np.mgrid[0:cam.h, 0:cam.w]
        ray = np.stack([(u + 0.5 - cam.w / 2) / cam.f, -(v + 0.5 - cam.h / 2) / cam.f, -np.ones(u.shape)], -1)
        up = ray @ Rc[2]  # the world's z of each pixel's ray
        r, g, b = (rgb[..., i].astype(int) for i in range(3))
        bad |= (up > 0.0) & (b > r + 25) & (b > g + 5)
    return bad


def fit_scale(disp, cam, Rc, c, rec, Rt, tt):
    """Fit 1/z = a*d + b on the depth sensor's returns seen by the camera."""
    tof = rec.get("tof")
    if not tof:
        return None, 0
    Rs, ts = on_map(Rt, tt, rec["frames"]["tof"])
    r = np.array(tof["distance_mm"], float) / 1000.0
    ok = np.isin(np.array(tof["status"]), [5, 9]) & (r > 0.05) & (r < TOF_MAX_MM / 1000.0)
    if ok.sum() < MIN_FIT_POINTS:
        return None, int(ok.sum())
    P = ts + (beams()[ok] @ Rs.T) * r[ok, None]
    u, v, z = cam.project(Rc, c, P)
    half = 12  # about half a zone's footprint, in pixels
    ins = (z > MIN_DEPTH_M) & (u >= half) & (u < cam.w - half) & (v >= half) & (v < cam.h - half)
    if ins.sum() < MIN_FIT_POINTS:
        return None, int(ins.sum())
    d = np.array([np.median(disp[int(vv) - half:int(vv) + half, int(uu) - half:int(uu) + half])
                  for uu, vv in zip(u[ins], v[ins])])
    y = 1.0 / z[ins]
    keep = np.ones(len(d), bool)
    for _ in range(4):
        A = np.stack([d[keep], np.ones(keep.sum())], 1)
        (a, b), *_ = np.linalg.lstsq(A, y[keep], rcond=None)
        res = np.abs(a * d + b - y) / y
        keep = res < max(0.08, 2.5 * np.median(res))
        if keep.sum() < MIN_FIT_POINTS:
            return None, int(keep.sum())
    zz = 1.0 / np.maximum(a * d[keep] + b, 1e-3)
    err = float(np.median(np.abs(zz - z[ins][keep]) / z[ins][keep]))
    if a <= 0 or err > 0.08:
        return None, int(keep.sum())
    return (float(a), float(b), err), int(keep.sum())


class Volume:
    def __init__(self, lo, hi, voxel):
        self.lo, self.voxel = np.array(lo, float), voxel
        self.shape = tuple(int(math.ceil((h - l) / voxel)) for l, h in zip(lo, hi))
        n = int(np.prod(self.shape))
        self.tsdf = np.ones(n, np.float32)
        self.w = np.zeros(n, np.float32)
        self.rgb = np.zeros((n, 3), np.float32)
        idx = np.indices(self.shape).reshape(3, -1).T
        self.centres = (self.lo + (idx + 0.5) * voxel).astype(np.float32)

    def integrate(self, cam, Rc, c, depth, rgb):
        u, v, z = cam.project(Rc, c, self.centres)
        ok = (z > MIN_DEPTH_M) & (z < MAX_DEPTH_M + TRUNC_M) & (u >= 0) & (u < cam.w) & (v >= 0) & (v < cam.h)
        i = np.nonzero(ok)[0]
        ui, vi = u[i].astype(int), v[i].astype(int)
        d = depth[vi, ui]
        sdf = d - z[i]
        good = (d > MIN_DEPTH_M) & (d < MAX_DEPTH_M) & (sdf > -TRUNC_M)
        i, sdf, ui, vi, d = i[good], sdf[good], ui[good], vi[good], d[good]
        t = np.minimum(1.0, sdf / TRUNC_M)
        wn = 1.0 / np.maximum(d, 0.3)
        w0 = self.w[i]
        self.tsdf[i] = (self.tsdf[i] * w0 + t * wn) / (w0 + wn)
        near = np.abs(sdf) < TRUNC_M
        j = i[near]
        wj = wn[near]
        self.rgb[j] = (self.rgb[j] * self.w[j, None] + rgb[vi[near], ui[near]] * wj[:, None]) / (self.w[j, None] + wj[:, None])
        self.w[i] = w0 + wn

    def surface(self, min_w=1.5):
        T = self.tsdf.reshape(self.shape)
        W = self.w.reshape(self.shape)
        seen = W >= min_w
        surf = np.zeros(self.shape, bool)
        for ax in range(3):
            a = [slice(None)] * 3
            b = [slice(None)] * 3
            a[ax], b[ax] = slice(0, -1), slice(1, None)
            ta, tb = T[tuple(a)], T[tuple(b)]
            both = seen[tuple(a)] & seen[tuple(b)]
            cross = both & (np.sign(ta) != np.sign(tb))
            surf[tuple(a)] |= cross & (np.abs(ta) <= np.abs(tb))
            surf[tuple(b)] |= cross & (np.abs(ta) > np.abs(tb))
        Tg = np.where(seen, T, 1.0)
        g = np.stack(np.gradient(Tg), -1)
        idx = np.nonzero(surf.reshape(-1))[0]
        n = g.reshape(-1, 3)[idx]
        n /= np.linalg.norm(n, axis=1, keepdims=True) + 1e-9
        return self.centres[idx], self.rgb[idx], n


def render(P, C, N, az_deg, el_deg, size=(1600, 1100), voxel=0.025, cut=None, title="", path=None):
    if cut is not None:
        m = P[:, 2] <= cut
        P, C, N = P[m], C[m], N[m]
    az, el = math.radians(az_deg), math.radians(el_deg)
    back = np.array([math.cos(el) * math.cos(az), math.cos(el) * math.sin(az), math.sin(el)])
    right = np.array([-math.sin(az), math.cos(az), 0.0])
    up = np.cross(back, right)
    x, y, depth = P @ right, P @ up, P @ back
    W, H = size
    span = max(np.ptp(x) / (W - 80), np.ptp(y) / (H - 120))
    s = 1.0 / span
    px = ((x - x.min()) * s + 40).astype(int)
    py = (H - 40 - (y - y.min()) * s).astype(int)
    light = np.array([0.3, 0.5, 0.8])
    light /= np.linalg.norm(light)
    lam = 0.55 + 0.45 * np.abs(N @ light)
    col = np.clip(C * lam[:, None], 0, 255).astype(np.uint8)
    img = np.full((H, W, 3), 245, np.uint8)
    zb = np.full((H, W), -np.inf)
    r = max(1, int(math.ceil(voxel * s * 0.75)))
    for dy in range(-r, r + 1):
        for dx in range(-r, r + 1):
            xx, yy = px + dx, py + dy
            ok = (xx >= 0) & (xx < W) & (yy >= 0) & (yy < H)
            xx, yy, dd, cc = xx[ok], yy[ok], depth[ok], col[ok]
            # The nearest point of each pixel: sort far to near, the last write wins.
            o = np.argsort(dd)
            xx, yy, dd, cc = xx[o], yy[o], dd[o], cc[o]
            flat = yy * W + xx
            last = np.unique(flat[::-1], return_index=True)[1]
            sel = len(flat) - 1 - last
            nearer = dd[sel] > zb[yy[sel], xx[sel]]
            sel = sel[nearer]
            img[yy[sel], xx[sel]] = cc[sel]
            zb[yy[sel], xx[sel]] = dd[sel]
    if path is not None and len(path):
        # The duck's way, drawn over everything (where the camera stood).
        qx = ((path @ right - x.min()) * s + 40).astype(int)
        qy = (H - 40 - (path @ up - y.min()) * s).astype(int)
        for ox in (-1, 0, 1):
            for oy in (-1, 0, 1):
                ok = (qx + ox >= 0) & (qx + ox < W) & (qy + oy >= 0) & (qy + oy < H)
                img[qy[ok] + oy, qx[ok] + ox] = (220, 30, 30)
    im = Image.fromarray(img)
    if title:
        ImageDraw.Draw(im).text((24, 16), title, fill=(40, 40, 40), font=ImageFont.load_default(size=26))
    return im


def write_ply(path, P, C):
    with open(path, "w") as f:
        f.write(f"ply\nformat ascii 1.0\nelement vertex {len(P)}\nproperty float x\nproperty float y\nproperty float z\n"
                "property uchar red\nproperty uchar green\nproperty uchar blue\nend_header\n")
        for p, c in zip(P, C.astype(np.uint8)):
            f.write(f"{p[0]:.3f} {p[1]:.3f} {p[2]:.3f} {c[0]} {c[1]} {c[2]}\n")


def write_house3d(path, P, C, N, meta):
    """The surface for quack-control's 3D view, small enough for a phone.

    `QH3D`, a u16 version (1), a u16 header length, the header (JSON:
    `points`, `voxel_m`, `map`, `frames`, `built`, `path`), padded with
    spaces to a multiple of 4; then each point in 12 bytes, little-endian:
    i16 x, y, z in millimetres on the map's frame, u8 r, g, b, and the
    surface's normal as i8 x, y, z (times 127).
    """
    head = json.dumps(dict(meta, points=len(P)), separators=(",", ":")).encode()
    head += b" " * (-(8 + len(head)) % 4)
    rec = np.zeros(len(P), dtype=[("p", "<i2", 3), ("c", "u1", 3), ("n", "i1", 3)])
    rec["p"] = np.clip(np.round(P * 1000), -32767, 32767)
    rec["c"] = np.clip(np.round(C), 0, 255)
    rec["n"] = np.clip(np.round(N * 127), -127, 127)
    with open(path, "wb") as f:
        f.write(b"QH3D" + np.array([1, len(head)], "<u2").tobytes() + head + rec.tobytes())


def main():
    args = sys.argv[1:]
    opts = {"--max": None, "--voxel": "0.025"}
    for k in list(opts):
        if k in args:
            i = args.index(k)
            opts[k] = args[i + 1]
            del args[i:i + 2]
    truth = "--truth" in args
    if truth:
        args.remove("--truth")
    sky = SKY_MASK and "--keep-sky" not in args
    if "--keep-sky" in args:
        args.remove("--keep-sky")
    rec_dir, out = args[0], args[1]
    voxel = float(opts["--voxel"])
    os.makedirs(out, exist_ok=True)
    recs = [json.loads(l) for l in open(f"{rec_dir}/frames.jsonl")]
    recs = [r for r in recs if (r.get("truth") if truth else (r.get("pose") and r.get("tracking")))]
    if opts["--max"]:
        recs = recs[: int(opts["--max"])]
    poses = []
    for r in recs:
        Rt, tt = trunk_pose(r, truth)
        Rs, c = on_map(Rt, tt, r["frames"]["camera"])
        poses.append((Rt, tt, Rs @ SITE_TO_CAM, c))
    cs = np.array([p[3] for p in poses])
    lo = [cs[:, 0].min() - MAX_DEPTH_M, cs[:, 1].min() - MAX_DEPTH_M, Z_RANGE[0]]
    hi = [cs[:, 0].max() + MAX_DEPTH_M, cs[:, 1].max() + MAX_DEPTH_M, Z_RANGE[1]]
    vol = Volume(lo, hi, voxel)
    print(f"{len(recs)} frames with a pose; volume {vol.shape} at {voxel} m", flush=True)
    depth_model = Depth()
    fits = open(f"{out}/fit.tsv", "w")
    fits.write("n\tpoints\ta\tb\terr\n")
    used = 0
    for r, (Rt, tt, Rc, c) in zip(recs, poses):
        m = r["media"]
        cam = Camera(m["width"], m["height"])
        rgb = uyvy_rgb(open(f"{rec_dir}/{r['n']:05d}.uyvy", "rb").read(), m["width"], m["height"])
        k = upright_turns(Rc)
        disp = depth_model.relative(rgb, k)
        fit, npts = fit_scale(disp, cam, Rc, c, r, Rt, tt)
        if fit is None:
            fits.write(f"{r['n']}\t{npts}\t\t\t\n")
            continue
        a, b, err = fit
        fits.write(f"{r['n']}\t{npts}\t{a:.5f}\t{b:.5f}\t{err:.3f}\n")
        depth = 1.0 / np.maximum(a * disp + b, 1e-3)
        depth[unusable(rgb, depth, cam, Rc, sky)] = 0.0
        vol.integrate(cam, Rc, c, depth, rgb.astype(np.float32))
        if used == 0:
            Image.fromarray(np.ascontiguousarray(np.rot90(rgb, k))).save(f"{out}/frame0.png")
        used += 1
        if used % 25 == 0:
            print(f"  {used} fused of {r['n'] + 1}", flush=True)
    fits.close()
    print(f"fused {used} of {len(recs)} frames", flush=True)
    P, C, N = vol.surface()
    print(f"surface: {len(P)} voxels", flush=True)
    write_ply(f"{out}/cloud.ply", P, C)
    np.savez_compressed(f"{out}/surface.npz", P=P, C=C, N=N)
    way = np.array([[p[3][0], p[3][1], 0.0] for p in poses])
    maps = [r.get("map") for r in recs if r.get("map")]
    write_house3d(f"{out}/house3d.bin", P, C, N, {
        "voxel_m": voxel, "map": max(set(maps), key=maps.count) if maps else None, "frames": used,
        "built": time.strftime("%Y-%m-%dT%H:%M:%S"),
        # Where the camera stood, every few frames: the duck's way through the house.
        "path": [[round(float(x), 2), round(float(y), 2)] for x, y, _ in way[:: max(1, len(way) // 300)]],
    })
    for name, az in (("ne", 45), ("nw", 135), ("sw", 225), ("se", 315)):
        render(P, C, N, az, 35.264, voxel=voxel, cut=CUT_Z_M,
               title=f"isometric from {name.upper()}, cut at {CUT_Z_M} m - {used} frames fused", path=way).save(f"{out}/iso_{name}.png")
    render(P, C, N, 270, 89.9, voxel=voxel, cut=CUT_Z_M, title="from above", path=way).save(f"{out}/top.png")


if __name__ == "__main__":
    main()
