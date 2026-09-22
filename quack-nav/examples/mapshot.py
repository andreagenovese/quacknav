"""Render the map, the true walls and the duck's true path to a PNG.
    python3 mapshot.py <frame.json> <explore.log|drive.jsonl> <out.png> [title]"""
import json, base64, math, sys, zlib, struct, re
S = __file__.rsplit("/", 1)[0]
frame_path, track_path, out = sys.argv[1], sys.argv[2], sys.argv[3]
title = sys.argv[4] if len(sys.argv) > 4 else ""
j = json.load(open(frame_path)); f = j.get("frame", j)
raw = base64.b64decode(f["cells"]); R, C, cm = f["rows"], f["cols"], f["cell_m"]; x0, y0 = f["x_min"], f["y_min"]
boxes = json.load(open(f"{S}/boxes.json"))
# world window: the apartment plus a margin
X0, X1, Y0, Y1 = -4.3, 4.3, -3.3, 3.3
PX = 60  # px per metre
W, H = int((X1 - X0) * PX), int((Y1 - Y0) * PX)
img = bytearray(b"\xe6\xe6\xe6" * W * H)   # unknown: light grey
def put(px, py, rgb):
    if 0 <= px < W and 0 <= py < H:
        i = (py * W + px) * 3; img[i:i+3] = bytes(rgb)
def world(x, y): return int((x - X0) * PX), int((Y1 - y) * PX)
# map cells
for r in range(R):
    for c in range(C):
        v = raw[r * C + c]
        if v == 0: continue
        rgb = (255, 255, 255) if v == 1 else (30, 30, 30)
        wx, wy = x0 + c * cm, y0 + r * cm
        ax, ay = world(wx, wy + cm); bx, by = world(wx + cm, wy)
        for py in range(ay, by):
            for px in range(ax, bx): put(px, py, rgb)
# true walls and furniture: blue outline
for _, bx0, bx1, by0, by1 in boxes:
    ax, ay = world(bx0, by1); bx, by = world(bx1, by0)
    for px in range(ax, bx + 1): put(px, ay, (60, 110, 220)); put(px, by, (60, 110, 220))
    for py in range(ay, by + 1): put(ax, py, (60, 110, 220)); put(bx, py, (60, 110, 220))
# track
pts = []; marks = []
for l in open(track_path):
    if track_path.endswith(".jsonl"):
        d = json.loads(l)
        if "truth" in d: pts.append((d["truth"][0], d["truth"][1]))
        if "mark" in d: marks.append((d["mark"], d["truth"][0], d["truth"][1]))
    else:
        m = re.search(r"truth=\(([-\d.]+), ([-\d.]+)", l) or re.search(r"truth \(([-\d.]+),([-\d.]+)\)", l)
        if m: pts.append((float(m.group(1)), float(m.group(2))))
def line(a, b, rgb):
    (ax, ay), (bx, by) = world(*a), world(*b); n = max(abs(bx - ax), abs(by - ay), 1)
    for k in range(n + 1):
        px, py = ax + (bx - ax) * k // n, ay + (by - ay) * k // n
        for dx in (-1, 0, 1):
            for dy in (-1, 0, 1): put(px + dx, py + dy, rgb)
for i in range(1, len(pts)):
    t = i / max(1, len(pts) - 1)
    line(pts[i - 1], pts[i], (int(220 - 100 * t), int(40 + 60 * t), int(40 + 180 * t)))  # red → violet with time
def blob(p, rgb, r=6):
    cx, cy = world(*p)
    for dx in range(-r, r + 1):
        for dy in range(-r, r + 1):
            if dx * dx + dy * dy <= r * r: put(cx + dx, cy + dy, rgb)
if pts: blob(pts[0], (0, 170, 0)); blob(pts[-1], (255, 140, 0))
for _, mx, my in marks: blob((mx, my), (200, 0, 200), 5)
# PNG
rows = b"".join(b"\x00" + bytes(img[y * W * 3:(y + 1) * W * 3]) for y in range(H))
def chunk(t, d): return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xffffffff)
png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", W, H, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(rows, 6)) + chunk(b"IEND", b"")
open(out, "wb").write(png)
print(f"{out}: {W}x{H}, {len(pts)} punti di percorso, {len(marks)} segnaposti; verde=partenza, arancio=fine, blu=muri e mobili veri, bianco=pavimento mappato, nero=muri mappati")
