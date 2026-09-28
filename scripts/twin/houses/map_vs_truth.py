"""map_vs_truth.py <frame.json> <truth.toml> <truth.json> [book.json name]

An explored map against the house as it is (the oracle ladder's step 3,
2026-09-28: the navigation and the homecoming frozen on the perfect map,
what is left is the map the exploration draws). Per room of the truth,
and for the house:

  walls    the map's wall cells by their distance to the nearest true solid
           (the truth's boxes from its segments, four to a box: walls and
           furniture): on it (<= 0.10 m),
           thickened (0.10-0.30 m: a wall drawn twice, or drawn fat), phantom
           (> 0.30 m: a wall where the house has none), the phantoms grouped
           into blobs with where they are;
  free     the map's free cells deeper than 8 cm inside a true wall (the
           middle of a 12 cm wall is 6 cm in) — the planner's way through
           it — and, apart, inside furniture (the perfect map draws only
           the furniture's outline);
  faces    the share of the solids' faces toward the room the map has drawn
           (a wall cell within 0.10 m);
  floor    the share of the true floor (not solid, not hole) the map knows as
           free;
  fit      the room's rigid misfit (rotation, shift) from `room_fit`;
  book     with a book: its drops as rim / near / phantom and each hole's rim
           covered, as `run_house.score_book` scores them.

`frame.json` from `maploc/examples/dump_frame --as-navd` (0 unknown, 1 free,
2 wall).
"""
import base64
import json
import math
import os
import sys

import tomllib

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from room_fit import icp, nearest, truth_points  # noqa: E402

ON_M, FAT_M, CELL_SOLID_M = 0.10, 0.30, 0.08


def solids(toml_p):
    """The truth's solids from its segments: every four are one box's
    outline (centimetres). A box 13 cm thin or less is a wall, the rest
    furniture."""
    segs = tomllib.load(open(toml_p, "rb"))["walls"]
    boxes = []
    for i in range(0, len(segs) - 3, 4):
        xs = [v / 100 for s in segs[i : i + 4] for v in (s[0], s[2])]
        ys = [v / 100 for s in segs[i : i + 4] for v in (s[1], s[3])]
        boxes.append((min(xs), max(xs), min(ys), max(ys)))
    walls = [b for b in boxes if min(b[1] - b[0], b[3] - b[2]) <= 0.13]
    furniture = [b for b in boxes if min(b[1] - b[0], b[3] - b[2]) > 0.13]
    return boxes, walls, furniture


def frame(path):
    f = json.load(open(path))
    f = f.get("frame", f)
    c = np.frombuffer(base64.b64decode(f["cells"]), dtype=np.uint8).reshape(f["rows"], f["cols"])
    rr, kk = np.mgrid[0 : f["rows"], 0 : f["cols"]]
    xy = np.stack([f["x_min"] + (kk + 0.5) * f["cell_m"], f["y_min"] + (rr + 0.5) * f["cell_m"]], -1)
    return c, xy, f["cell_m"]


def box_distance(p, boxes):
    """Signed distance of points `p` (N×2) to the union of boxes: negative
    inside a box, positive outside."""
    best = np.full(len(p), np.inf)
    for x0, x1, y0, y1 in boxes:
        dx = np.maximum(np.maximum(x0 - p[:, 0], p[:, 0] - x1), 0.0)
        dy = np.maximum(np.maximum(y0 - p[:, 1], p[:, 1] - y1), 0.0)
        outside = np.hypot(dx, dy)
        inside = -np.minimum(np.minimum(p[:, 0] - x0, x1 - p[:, 0]), np.minimum(p[:, 1] - y0, y1 - p[:, 1]))
        d = np.where((dx == 0) & (dy == 0), inside, outside)
        best = np.minimum(best, d)
    return best


def in_boxes(p, boxes, margin=0.0):
    m = np.zeros(len(p), bool)
    for x0, x1, y0, y1 in boxes:
        m |= (p[:, 0] >= x0 - margin) & (p[:, 0] <= x1 + margin) & (p[:, 1] >= y0 - margin) & (p[:, 1] <= y1 + margin)
    return m


def blobs(points, link=0.10):
    """Groups of points within `link` of each other: (count, centre)."""
    left = list(range(len(points)))
    out = []
    while left:
        seed = [left.pop()]
        group = []
        while seed:
            i = seed.pop()
            group.append(i)
            near = [j for j in left if math.hypot(*(points[i] - points[j])) <= link]
            for j in near:
                left.remove(j)
            seed += near
        out.append((len(group), points[group].mean(0)))
    return sorted(out, key=lambda b: -b[0])


def face_points(boxes, step=0.05):
    pts = []
    for x0, x1, y0, y1 in boxes:
        for x in np.arange(x0, x1 + 1e-9, step):
            pts += [(x, y0), (x, y1)]
        for y in np.arange(y0, y1 + 1e-9, step):
            pts += [(x0, y), (x1, y)]
    return np.array(pts)


def score_book(drops, holes):
    def hole_dist(x, y):
        best = math.inf
        for x0, x1, y0, y1 in holes:
            dx = max(x0 - x, 0.0, x - x1)
            dy = max(y0 - y, 0.0, y - y1)
            best = min(best, math.hypot(dx, dy))
        return best

    real = sum(hole_dist(d[0], d[1]) <= 0.10 for d in drops)
    near = sum(0.10 < hole_dist(d[0], d[1]) <= 0.20 for d in drops)
    phantom = [(round(d[0], 2), round(d[1], 2)) for d in drops if hole_dist(d[0], d[1]) > 0.20]
    cover = []
    for x0, x1, y0, y1 in holes:
        rim = face_points([(x0, x1, y0, y1)])
        seen = sum(any(math.hypot(p[0] - d[0], p[1] - d[1]) <= 0.15 for d in drops) for p in rim)
        cover.append(round(seen / len(rim), 2))
    return len(drops), real, near, phantom, cover


def main():
    frame_p, toml_p, json_p = sys.argv[1:4]
    truth = json.load(open(json_p))
    boxes, wall_boxes, furniture = solids(toml_p)
    holes = [tuple(h) for h in truth["holes"]]
    cells, xy, cell_m = frame(frame_p)
    flat, xyf = cells.ravel(), xy.reshape(-1, 2)
    wall = xyf[flat == 2]
    free = xyf[flat == 1]
    d_wall = box_distance(wall, boxes)
    d_free_wall = box_distance(free, wall_boxes)
    d_free_furn = box_distance(free, furniture) if furniture else np.full(len(free), np.inf)
    true_walls = truth_points(toml_p)
    faces = face_points(boxes)
    rooms = truth["rooms"]

    def in_room(p, r, m=0.0):
        x0, x1, y0, y1 = r
        return (p[:, 0] > min(x0, x1) - m) & (p[:, 0] < max(x0, x1) + m) & (p[:, 1] > min(y0, y1) - m) & (p[:, 1] < max(y0, y1) + m)

    # The floor: a grid over each room, not solid, not hole.
    def floor_of(r):
        x0, x1, y0, y1 = r
        gx, gy = np.meshgrid(np.arange(min(x0, x1) + 0.025, max(x0, x1), 0.05), np.arange(min(y0, y1) + 0.025, max(y0, y1), 0.05))
        p = np.c_[gx.ravel(), gy.ravel()]
        return p[~in_boxes(p, boxes, 0.02) & ~in_boxes(p, holes)]

    def known_free(p):
        if not len(p):
            return 0.0
        c = cells
        f = json.load(open(frame_p))
        f = f.get("frame", f)
        k = ((p[:, 0] - f["x_min"]) / cell_m).astype(int)
        r = ((p[:, 1] - f["y_min"]) / cell_m).astype(int)
        ok = (k >= 0) & (r >= 0) & (k < c.shape[1]) & (r < c.shape[0])
        v = np.zeros(len(p), np.uint8)
        v[ok] = c[r[ok], k[ok]]
        return float((v == 1).mean())

    # Faces toward the floor: a face point with floor a cell out from it.
    def room_faces(r):
        fp = faces[in_room(faces, r, 0.02)]
        keep = []
        for p in fp:
            for dx, dy in ((0.06, 0), (-0.06, 0), (0, 0.06), (0, -0.06)):
                q = np.array([[p[0] + dx, p[1] + dy]])
                if in_room(q, r)[0] and not in_boxes(q, boxes)[0] and not in_boxes(q, holes)[0]:
                    keep.append(p)
                    break
        return np.array(keep).reshape(-1, 2)

    print(f"{'':<10} {'walls':>6} {'on':>5} {'fat':>5} {'phantom':>7} {'free-in-wall':>12} {'-furniture':>10} {'faces':>6} {'floor':>6}   fit (rotation, shift, then mean)")
    rows = [("house", (-1e9, 1e9, -1e9, 1e9))] + list(rooms.items())
    phantom_all = []
    for name, r in rows:
        wm = in_room(wall, r, 0.15)
        fm = in_room(free, r)
        dw = d_wall[wm]
        on, fat, ph = (dw <= ON_M).sum(), ((dw > ON_M) & (dw <= FAT_M)).sum(), (dw > FAT_M).sum()
        fiw = (d_free_wall[fm] < -CELL_SOLID_M).sum()
        fif = (d_free_furn[fm] < -CELL_SOLID_M).sum()
        if name == "house":
            phantom_all = wall[wm][dw > FAT_M]
            fl = np.concatenate([floor_of(rr) for rr in rooms.values()])
            fc = np.concatenate([room_faces(rr) for rr in rooms.values()])
        else:
            fl, fc = floor_of(r), room_faces(r)
        seen = (nearest(fc, wall) <= ON_M).mean() if len(fc) and len(wall) else 0.0
        fit = ""
        src = wall[wm]
        dst = true_walls[in_room(true_walls, r, 0.4)] if name != "house" else true_walls
        if len(src) >= 30 and len(dst) >= 10:
            rng = np.random.default_rng(0)
            src = src[rng.choice(len(src), min(len(src), 1500), replace=False)]
            ang, t, d = icp(src, dst)
            fit = f"{ang:+5.2f}°, ({t[0]*100:+5.1f}, {t[1]*100:+5.1f}) cm, {d.mean()*100:4.1f} cm"
        n = max(len(dw), 1)
        print(f"{name:<10} {len(dw):>6} {on/n:>5.0%} {fat/n:>5.0%} {ph/n:>7.0%} {fiw:>12} {fif:>10} {seen:>6.0%} {known_free(fl):>6.0%}   {fit}")
    big = [b for b in blobs(phantom_all) if b[0] >= 4][:8] if len(phantom_all) < 4000 else []
    if big:
        print("phantom blobs (cells, where):", ", ".join(f"{c} at ({p[0]:+.2f},{p[1]:+.2f})" for c, p in big))
    if len(sys.argv) > 5:
        book = json.load(open(sys.argv[4]))
        n, real, near, phantom, cover = score_book(book.get(sys.argv[5], []), holes)
        print(f"book: {n} drops — rim {real}, near {near}, phantom {len(phantom)} {phantom[:10]}; rims covered {cover}")


if __name__ == "__main__":
    main()
