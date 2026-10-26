"""A journey of quack-rl's paper twin as a GIF: the stick's walk and the
pilot's side by side, from `rl_trace`'s JSON (the house, the route of the
last plan, the duck, the people and pets where they stood, the contacts).

    rl_trace --seed N --family twin:casa_gente --pilot P.json --calib C.json --out t.json
    python3 scripts/rl/trace_gif.py t.json casa_gente.gif

Walls and furniture the map knows in grey, things it does not in orange,
holes black; people blue, pets purple, the duck yellow with its nose, the
goal a green ring, a contact a red dot for 1.5 s. Needs Pillow only."""
import json, math, sys
from PIL import Image, ImageDraw, ImageFont

d = json.load(open(sys.argv[1]))
s = d["scenario"]; w = s["world"]; bx = s["bias"]
x0, x1, y0, y1 = w["bounds"]
PX = 55; M = 14
W, H = int((x1 - x0) * PX) + 2 * M, int((y1 - y0) * PX) + 2 * M + 18
def P(x, y): return (M + (x - bx[0] - x0) * PX, M + 18 + (y1 - (y - bx[1])) * PX)
def R(dr, x0_, x1_, y0_, y1_, fill, outline=None):
    a, b = P(x0_, y1_), P(x1_, y0_); dr.rectangle([a, b], fill=fill, outline=outline)
def C(dr, x, y, r, fill, outline=None):
    a, b = P(x - r, y + r), P(x + r, y - r); dr.ellipse([a, b], fill=fill, outline=outline)
runs = [r for r in d["runs"] if r["name"] in ("stick", "pilot")]
runs.sort(key=lambda r: r["name"] != "stick")
try: font = ImageFont.truetype("/System/Library/Fonts/Helvetica.ttc", 13)
except Exception: font = ImageFont.load_default()
STEP = 3  # every third track point: 0.6 s a frame
frames = []
n = max(len(r["track"]) for r in runs)
for k in range(0, n, STEP):
    im = Image.new("RGB", (W * 2 + 8, H), (250, 248, 244)); dr = ImageDraw.Draw(im)
    for j, r in enumerate(runs):
        ox = j * (W + 8)
        def PP(x, y, ox=ox): p = P(x, y); return (p[0] + ox, p[1])
        # The house: what the map knows in grey, what it does not in orange, the holes black.
        for rc in w["rects"]:
            a, b = PP(rc["x0"], rc["y1"]), PP(rc["x1"], rc["y0"])
            dr.rectangle([a, b], fill=(120, 120, 120) if rc.get("mapped", True) else (235, 150, 60))
        for h in w["holes"]:
            a, b = PP(h["x0"], h["y1"]), PP(h["x1"], h["y0"]); dr.rectangle([a, b], fill=(20, 20, 20))
        # The route of the last plan, the goal.
        if r["route"]:
            dr.line([PP(*q) for q in r["route"]], fill=(150, 190, 240), width=2)
        g = s["goal"]; p = PP(g[0], g[1])
        dr.ellipse([p[0] - 6, p[1] - 6, p[0] + 6, p[1] + 6], outline=(30, 160, 60), width=3)
        i = min(k, len(r["track"]) - 1)
        t, x, y, yaw = r["track"][i]
        # The track so far, the people and the cat, the duck.
        dr.line([PP(q[1], q[2]) for q in r["track"][: i + 1 : 2]] or [PP(x, y)] * 2, fill=(200, 170, 60), width=1)
        for m, (mx, my) in zip(r["movers"], r["mover_track"][i]):
            a, b = PP(mx - m["r"], my + m["r"]), PP(mx + m["r"], my - m["r"])
            dr.ellipse([a, b], fill=(60, 110, 220) if m["height"] > 0.5 else (170, 120, 200))
        a, b = PP(x - 0.11, y + 0.11), PP(x + 0.11, y - 0.11)
        dr.ellipse([a, b], fill=(240, 200, 30), outline=(90, 70, 0))
        dr.line([PP(x, y), PP(x + 0.16 * math.cos(yaw), y + 0.16 * math.sin(yaw))], fill=(90, 70, 0), width=3)
        for c in r["contacts"]:
            if 0 <= t - c[0] < 1.5:
                p = PP(c[2], c[3]); dr.ellipse([p[0] - 5, p[1] - 5, p[0] + 5, p[1] + 5], fill=(230, 40, 40))
        res = r["result"]
        label = f'{r["name"]}  t={t:5.1f} s  {res["outcome"] if t >= r["track"][-1][0] else ""}  people bumps {sum(1 for c in r["contacts"] if c[1]=="mover" and c[0] <= t)}'
        dr.text((ox + M, 4), label, fill=(30, 30, 30), font=font)
    frames.append(im.quantize(colors=64, method=Image.Quantize.FASTOCTREE))
frames[0].save(sys.argv[2], save_all=True, append_images=frames[1:], duration=70, loop=0, optimize=False)
print(sys.argv[2], len(frames), "frames", f"{W*2+8}x{H}")
