#!/usr/bin/env python3
"""The band against the twin's own geometry.

    $MICRODUCK_RL/.venv/bin/python scripts/band/band_vs_truth.py <out_dir> <scene.xml> [png]

<out_dir> is what `cargo run -p maploc --example band` wrote (map.u8,
band.u8, band.json). Every static geom of the scene is sorted by where it
stands against the duck's height: IN the band (something between 0.05 and
0.30 m: the duck meets it), or ABOVE it (all of it over 0.33 m: the duck
walks under). For each, the share of its footprint the band calls blocked
and the share the 2D map calls wall; then the band's blocked cells that no
geometry in the band explains (phantoms). With a png path, a picture: the
2D map in grey, the band's blocked cells red (from the floor) or orange (a
beam seen under them), the in-band footprints cyan, the above-band green (boxes drawn for
cylinders too).
"""
import json
import struct
import sys
import zlib

import mujoco
import numpy as np

BAND = (0.05, 0.30)
ABOVE = 0.33


def geoms(scene):
    m = mujoco.MjModel.from_xml_path(scene)
    d = mujoco.MjData(m)
    mujoco.mj_forward(m, d)
    out = []
    for g in range(m.ngeom):
        b = m.geom_bodyid[g]
        # The world's own geoms and free bodies at rest (the duck is not in a
        # house scene's world body).
        if b != 0 and m.body_jntnum[m.body_rootid[b]] > 0:
            continue
        name = mujoco.mj_id2name(m, mujoco.mjtObj.mjOBJ_GEOM, g) or f"geom{g}"
        t, size, pos = m.geom_type[g], m.geom_size[g], d.geom_xpos[g]
        R = d.geom_xmat[g].reshape(3, 3)
        if t == mujoco.mjtGeom.mjGEOM_BOX:
            half = np.abs(R) @ size
        elif t == mujoco.mjtGeom.mjGEOM_CYLINDER:
            half = np.array([size[0], size[0], size[1]])
        else:
            continue
        lo, hi = pos - half, pos + half
        if hi[2] <= 0.005 or lo[2] < -0.05:  # floors, rugs, stairs below
            continue
        if any(k in name for k in ("floor", "rug")):
            continue
        out.append((name, lo, hi, t == mujoco.mjtGeom.mjGEOM_CYLINDER))
    return out


def main():
    out_dir, scene = sys.argv[1], sys.argv[2]
    png = sys.argv[3] if len(sys.argv) > 3 else None
    meta = json.load(open(f"{out_dir}/band.json"))
    rows, cols, cell = meta["rows"], meta["cols"], meta["cell_m"]
    x0, y0 = meta["x_min"], meta["y_min"]
    grid = np.fromfile(f"{out_dir}/map.u8", np.uint8).reshape(rows, cols)
    band = np.fromfile(f"{out_dir}/band.u8", np.uint8).reshape(rows, cols)
    blocked = band >= 2
    over = band >= 8
    xs = x0 + (np.arange(cols) + 0.5) * cell
    ys = y0 + (np.arange(rows) + 0.5) * cell
    X, Y = np.meshgrid(xs, ys)

    def footprint(lo, hi, cyl, grow=0.0):
        if cyl:
            c = (lo + hi) / 2
            return np.hypot(X - c[0], Y - c[1]) <= (hi[0] - lo[0]) / 2 + grow
        return (X >= lo[0] - grow) & (X <= hi[0] + grow) & (Y >= lo[1] - grow) & (Y <= hi[1] + grow)

    print(f"{'object':<18} {'z (m)':>11}  {'where':<6} {'cells':>5} {'seen':>5} {'band':>5} {'2D wall':>7}")
    rows_out = []
    solids = []
    for name, lo, hi, cyl in geoms(scene):
        in_band = lo[2] < BAND[1] and hi[2] > BAND[0]
        if in_band:
            solids.append((lo, hi))
        if name.startswith("wall_"):
            continue
        above = lo[2] >= ABOVE
        if not (in_band or above):
            continue
        fp = footprint(lo, hi, cyl)
        n = int(fp.sum())
        if n == 0:
            continue
        seen = fp & (band > 0)
        ns = int(seen.sum())
        share = lambda m: f"{100 * (m & seen).sum() / ns:4.0f}%" if ns else "   - "
        where = "in" if in_band else "above"
        rows_out.append((name, lo, hi, cyl, where))
        print(f"{name:<18} {lo[2]:5.2f}-{hi[2]:4.2f}  {where:<6} {n:5d} {ns:5d} {share(blocked)} {share(grid == 2):>7}")
    # How far each blocked cell (and each 2D wall cell) is from the nearest
    # geometry in the band, the outer walls included.
    def dist(mask):
        px, py = X[mask], Y[mask]
        d = np.full(px.shape, np.inf)
        for lo, hi in solids:
            dx = np.maximum(np.maximum(lo[0] - px, px - hi[0]), 0)
            dy = np.maximum(np.maximum(lo[1] - py, py - hi[1]), 0)
            d = np.minimum(d, np.hypot(dx, dy))
        return d
    for label, mask in (("band blocked", blocked), ("2D wall", grid == 2)):
        d = dist(mask)
        if label == "band blocked":
            far = d > 0.08
        print(f"{label:<13} {len(d):5d} cells from the nearest geometry: median {100 * np.median(d):.1f} cm, "
              f"within 5 cm {100 * (d <= 0.05).mean():.0f} %, within 10 cm {100 * (d <= 0.10).mean():.0f} %")
    print(f"blocked cells {int(blocked.sum())}, with a beam under {int(over.sum())}; "
          f"farther than 8 cm from any geometry in the band {int(far.sum())}")
    if png:
        img = np.full((rows, cols, 3), 60, np.uint8)
        img[grid == 1] = (225, 225, 225)
        img[grid == 2] = (20, 20, 20)
        img[blocked] = (220, 40, 40)
        img[over] = (255, 150, 0)
        s = 6
        img = np.repeat(np.repeat(img[::-1], s, 0), s, 1)
        for name, lo, hi, cyl, where in rows_out:
            col = (0, 200, 220) if where == "in" else (40, 200, 60)
            c0 = int(round((lo[0] - x0) / cell * s))
            c1 = int(round((hi[0] - x0) / cell * s))
            r0 = int(round((rows - (hi[1] - y0) / cell) * s))
            r1 = int(round((rows - (lo[1] - y0) / cell) * s))
            c0, r0 = max(c0, 0), max(r0, 0)
            img[r0:r0 + 2, c0:c1] = col
            img[r1 - 2:r1, c0:c1] = col
            img[r0:r1, c0:c0 + 2] = col
            img[r0:r1, c1 - 2:c1] = col
        write_png(png, img)
        print(png)


def write_png(path, img):
    h, w, _ = img.shape
    raw = b"".join(b"\x00" + img[y].tobytes() for y in range(h))
    def chunk(tag, data):
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
                + chunk(b"IDAT", zlib.compress(raw, 6)) + chunk(b"IEND", b""))

if __name__ == "__main__":
    main()
