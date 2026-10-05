"""Five-room houses for the twin: `casa_libera` (bare rooms, doors only),
`casa_arredata` (furniture, a corridor, narrow gaps, two holes), and a
seven-room `casa_grande` (furnished, two holes, a corridor turning 90
degrees, nothing that blocks: every door 0.9 m or more, every gap beside
furniture or a hole 0.6 m or more); and `casa_ingombra`, casa_arredata
with things put down since its map was made — a bag and a basket in the
corridor, a box narrowing the living room's door, a chair's legs in the
kitchen, a toy on the way to the bedroom — in the scene and not in its
truth (the pilot's bench on the twin, docs/rl-pilot.md: the oracle draws
the map from the truth, so the map does not know them).

Writes, per house: <robot dir>/<name>.xml and scene_<name>.xml (MuJoCo),
<out>/<name>.toml (maploc evaluate truth, cm), <out>/<name>.world.json
(paper twin), <out>/<name>.truth.json (holes and rooms, for scoring drops).

    python3 gen.py <robot dir> <out dir> [house ...]
"""
import json, sys
from pathlib import Path

ROBOT, OUT = Path(sys.argv[1]), Path(sys.argv[2])
T, H = 0.12, 0.50  # wall thickness and height
X0, X1, Y0, Y1 = -4.0, 4.0, -3.0, 3.0


def bounds(x0, x1, y0, y1):
    """The house's outer walls (the default 8 x 6 m unless a house says)."""
    global X0, X1, Y0, Y1
    X0, X1, Y0, Y1 = x0, x1, y0, y1


def wall_x(name, y, xa, xb, doors):
    """A wall along x at y from xa to xb, minus the door intervals."""
    out, x = [], xa
    for d0, d1 in sorted(doors) + [(xb, xb)]:
        if d0 - x > 1e-6:
            out.append((f"{name}_{len(out)}", x, d0, y - T / 2, y + T / 2, H))
        x = d1
    return out


def wall_y(name, x, ya, yb, doors):
    out, y = [], ya
    for d0, d1 in sorted(doors) + [(yb, yb)]:
        if d0 - y > 1e-6:
            out.append((f"{name}_{len(out)}", x - T / 2, x + T / 2, y, d0, H))
        y = d1
    return out


def outer():
    return [("wall_south", X0 - T / 2, X1 + T / 2, Y0 - T / 2, Y0 + T / 2, H),
            ("wall_north", X0 - T / 2, X1 + T / 2, Y1 - T / 2, Y1 + T / 2, H),
            ("wall_west", X0 - T / 2, X0 + T / 2, Y0, Y1, H),
            ("wall_east", X1 - T / 2, X1 + T / 2, Y0, Y1, H)]


def floor_tiles(holes):
    """The floor rectangle minus the holes, as non-overlapping boxes."""
    xs = sorted({X0 - 0.1, X1 + 0.1} | {h[0] for h in holes} | {h[1] for h in holes})
    ys = sorted({Y0 - 0.1, Y1 + 0.1} | {h[2] for h in holes} | {h[3] for h in holes})
    tiles = []
    for i in range(len(xs) - 1):
        for j in range(len(ys) - 1):
            cx, cy = (xs[i] + xs[i + 1]) / 2, (ys[j] + ys[j + 1]) / 2
            if any(h[0] < cx < h[1] and h[2] < cy < h[3] for h in holes):
                continue
            tiles.append((xs[i], xs[i + 1], ys[j], ys[j + 1]))
    return tiles


def house_libera():
    bounds(-4.0, 4.0, -3.0, 3.0)
    boxes = outer()
    boxes += wall_x("wS", -0.8, X0, X1, [(-3.3, -2.5), (0.5, 1.3), (2.4, 3.2)])
    boxes += wall_y("wAB", -1.4, -0.8, Y1, [(0.6, 1.4)])
    boxes += wall_y("wBC", 1.4, -0.8, Y1, [(1.6, 2.4)])
    boxes += wall_y("wDE", 0.3, Y0, -0.8, [(-2.4, -1.6)])
    rooms = {"A": [-4, -1.4, -0.8, 3], "B": [-1.4, 1.4, -0.8, 3], "C": [1.4, 4, -0.8, 3],
             "D": [-4, 0.3, -3, -0.8], "E": [0.3, 4, -3, -0.8]}
    goals = {"A": [-2.7, 1.8], "C": [2.7, 0.6], "D": [-2.0, -2.0], "E": [2.2, -2.0], "home": [0.0, 0.0]}
    return boxes, [], rooms, goals, []


def house_arredata():
    bounds(-4.0, 4.0, -3.0, 3.0)
    boxes = outer()
    # The corridor, y in [-0.5, 0.5], the whole width; the rooms off it.
    boxes += wall_x("wN", 0.5, X0, X1, [(-2.8, -2.0), (1.0, 1.8)])
    boxes += wall_x("wS", -0.5, X0, X1, [(-3.2, -2.4), (-0.9, -0.2), (3.0, 3.6)])
    boxes += wall_y("wKL", -0.3, 0.5, Y1, [(1.9, 2.6)])
    boxes += wall_y("wBO", -1.3, Y0, -0.5, [])
    boxes += wall_y("wOB", 1.4, Y0, -0.5, [])
    furniture = [
        # kitchen
        ("counter", -3.9, -1.5, 2.4, 2.9, 0.45), ("kit_table", -2.2, -1.4, 1.2, 1.8, 0.40),
        ("fridge", -3.9, -3.4, 0.6, 1.2, 0.50),
        # living: the coffee table 0.5 m from the sofa
        ("sofa", 0.6, 2.4, 2.4, 2.9, 0.40), ("coffee_table", 1.0, 2.0, 1.5, 1.9, 0.30),
        ("bookshelf", -0.2, 0.2, 0.6, 1.4, 0.50),
        # bedroom: 0.6 m between the bed and the wardrobe
        ("bed", -3.9, -2.5, -2.9, -1.4, 0.45), ("armadio", -1.9, -1.4, -2.9, -1.8, 0.50),
        # office
        ("desk", -1.2, 0.2, -2.9, -2.4, 0.45), ("shelf", 0.8, 1.3, -2.9, -1.2, 0.50),
        ("chair", -0.5, -0.1, -2.2, -1.8, 0.25),
        # bath
        ("tub", 2.6, 3.9, -2.9, -2.2, 0.35), ("sink", 3.5, 3.9, -1.9, -1.4, 0.45),
        # a low thing in the corridor, well off every door
        ("shoes", -3.6, -3.4, 0.05, 0.25, 0.07),
    ]
    boxes += furniture
    # The stairwell in the corridor, on the way to the bath's door: the
    # passage beside it 0.49 m (house2's is 0.54). A sunken corner in the
    # living room, against its walls.
    holes = [(2.0, 2.6, -0.44, -0.05), (3.0, 3.8, 2.1, 2.94)]
    rooms = {"kitchen": [-4, -0.3, 0.5, 3], "living": [-0.3, 4, 0.5, 3], "corridor": [-4, 4, -0.5, 0.5],
             "bedroom": [-4, -1.3, -3, -0.5], "office": [-1.3, 1.4, -3, -0.5], "bath": [1.4, 4, -3, -0.5]}
    goals = {"kitchen": [-2.9, 1.9], "living": [1.4, 1.2], "bedroom": [-2.1, -1.1],
             "office": [-0.6, -1.3], "bath": [3.3, -1.2], "home": [0.0, 0.0]}
    return boxes, holes, rooms, goals, ["shoes", "chair"]


def house_ingombra():
    """casa_arredata, and what was put down since (scene only)."""
    boxes, holes, rooms, goals, low = house_arredata()
    unmapped = [
        ("bag", "box", -1.6, -1.3, -0.05, 0.25, 0.20),
        ("basket", "box", -0.5, -0.25, -0.35, -0.1, 0.30),
        ("box_door", "box", 1.05, 1.3, 0.62, 0.85, 0.25),
        ("toy", "box", -2.4, -2.25, -0.95, -0.8, 0.12),
    ] + [(f"chair_leg{k}", "cyl", -2.45 + dx, 1.05 + dy, 0.015, 0.45) for k, (dx, dy) in enumerate([(-0.19, -0.19), (0.19, -0.19), (-0.19, 0.19), (0.19, 0.19)])]
    return boxes, holes, rooms, goals, low, unmapped


def house_grande():
    """Seven rooms, 9 x 7 m, furnished, two holes, and no passage the duck
    cannot take: the house to test the whole stack on, not its limits
    (2026-09-30). The corridor, 1.2 m wide, runs east from the west wall
    and turns 90 degrees south at x 1.3, down to the south wall: the
    kitchen and a long living room north of it, a bedroom and the bath in
    the corner the turn leaves, the study and a second bedroom opening off
    the south leg. The stairwell stands against the living room's north
    wall and a sunken corner in the second bedroom, well off every door."""
    bounds(-4.5, 4.5, -3.5, 3.5)
    boxes = outer()
    # the corridor: y in [-0.6, 0.6] for x < 1.9, then x in [0.7, 1.9] south
    boxes += wall_x("wN", 0.6, X0, X1, [(-3.4, -2.5), (0.0, 1.2)])
    boxes += wall_x("wS", -0.6, X0, 0.7, [(-3.4, -2.5), (-1.0, -0.1)])
    boxes += wall_y("wCW", 0.7, Y0, -0.6, [])
    boxes += wall_y("wCE", 1.9, Y0, 0.6, [(-1.1, -0.2), (-2.6, -1.7)])
    boxes += wall_x("wSB", -1.3, 1.9, X1, [])
    boxes += wall_y("wKL", -1.5, 0.6, Y1, [(1.8, 2.8)])
    boxes += wall_y("wBB", -1.5, Y0, -0.6, [])
    furniture = [
        # kitchen
        ("counter", -4.4, -2.0, 3.0, 3.44, 0.45), ("kit_table", -3.6, -2.8, 1.6, 2.2, 0.40),
        ("fridge", -4.4, -3.9, 0.7, 1.2, 0.50),
        # living room: the stairwell against the north wall
        ("bookshelf", -1.4, -1.0, 0.7, 1.4, 0.50), ("coffee_table", 1.4, 2.0, 1.4, 1.9, 0.30),
        ("sofa", 2.6, 3.2, 1.6, 3.3, 0.40), ("tv_stand", 4.0, 4.4, 2.0, 3.2, 0.50),
        ("armchair", 3.8, 4.4, 0.75, 1.35, 0.40),
        # study, off the corridor's south leg
        ("desk", 3.4, 4.4, 0.05, 0.5, 0.45), ("shelf", 2.4, 3.0, 0.2, 0.5, 0.50),
        ("chair", 3.7, 4.1, -0.55, -0.15, 0.25),
        # bedroom
        ("bed", -4.4, -3.0, -3.4, -1.8, 0.45), ("armadio", -2.1, -1.6, -3.4, -2.4, 0.50),
        # bath
        ("tub", -1.4, 0.6, -3.4, -2.7, 0.35), ("sink", 0.2, 0.6, -1.6, -1.1, 0.45),
        # second bedroom: the bed along its north wall, 0.65 m from the sunken corner
        ("bed2", 2.8, 4.4, -1.95, -1.4, 0.45),
        # corridor: a low shoe rack at the west end, a plant at the south end
        ("shoes", -4.3, -4.0, -0.1, 0.3, 0.07), ("plant", 1.0, 1.6, -3.4, -3.1, 0.50),
    ]
    boxes += furniture
    holes = [(0.0, 0.9, 2.8, 3.44), (3.6, 4.44, -3.44, -2.6)]
    rooms = {"kitchen": [-4.5, -1.5, 0.6, 3.5], "living": [-1.5, 4.5, 0.6, 3.5],
             "corridor": [-4.5, 1.9, -0.6, 0.6], "corridor_s": [0.7, 1.9, -3.5, -0.6],
             "bedroom": [-4.5, -1.5, -3.5, -0.6], "bath": [-1.5, 0.7, -3.5, -0.6],
             "study": [1.9, 4.5, -1.3, 0.6], "bedroom2": [1.9, 4.5, -3.5, -1.3]}
    goals = {"kitchen": [-2.4, 1.3], "living": [1.0, 2.2], "study": [2.8, -0.4], "bedroom": [-2.5, -1.4],
             "bath": [-0.6, -1.8], "bedroom2": [2.6, -2.8], "corridor_end": [1.3, -2.5], "home": [0.0, 0.0]}
    return boxes, holes, rooms, goals, ["shoes", "chair"]


MATS = """  <asset>
    <texture type="2d" name="floor_wood" builtin="checker" rgb1="0.72 0.58 0.38" rgb2="0.62 0.50 0.30" width="256" height="256"/>
    <material name="floor_main" texture="floor_wood" texrepeat="14 14" reflectance="0.04"/>
    <material name="pit_mat" rgba="0.25 0.20 0.18 1"/>
    <material name="wall_warm" rgba="0.94 0.88 0.80 1" reflectance="0.02"/>
    <material name="furn_mat" rgba="0.55 0.38 0.20 1"/>
    <material name="low_mat" rgba="0.92 0.10 0.10 1"/>
  </asset>
"""


def write(name, house):
    boxes, holes, rooms, goals, low = house[:5]
    unmapped = house[5] if len(house) > 5 else []
    g = []
    for k, (x0, x1, y0, y1) in enumerate(floor_tiles(holes)):
        g.append(f'    <geom name="floor_{k}" type="box" size="{(x1-x0)/2:.3f} {(y1-y0)/2:.3f} 0.005" '
                 f'pos="{(x0+x1)/2:.3f} {(y0+y1)/2:.3f} -0.005" material="floor_main" contype="1" conaffinity="1"/>')
    for k, (x0, x1, y0, y1) in enumerate(holes):
        g.append(f'    <geom name="pit_{k}" type="box" size="{(x1-x0)/2:.3f} {(y1-y0)/2:.3f} 0.005" '
                 f'pos="{(x0+x1)/2:.3f} {(y0+y1)/2:.3f} -0.305" material="pit_mat" contype="1" conaffinity="1"/>')
    for (n, x0, x1, y0, y1, h) in boxes:
        mat = "wall_warm" if n.startswith("w") else ("low_mat" if n in low else "furn_mat")
        g.append(f'    <geom name="{n}" type="box" size="{(x1-x0)/2:.3f} {(y1-y0)/2:.3f} {h/2:.3f}" '
                 f'pos="{(x0+x1)/2:.3f} {(y0+y1)/2:.3f} {h/2:.3f}" material="{mat}" contype="1" conaffinity="1"/>')
    for u in unmapped:
        if u[1] == "box":
            n, _, x0, x1, y0, y1, h = u
            g.append(f'    <geom name="{n}" type="box" size="{(x1-x0)/2:.3f} {(y1-y0)/2:.3f} {h/2:.3f}" '
                     f'pos="{(x0+x1)/2:.3f} {(y0+y1)/2:.3f} {h/2:.3f}" material="low_mat" contype="1" conaffinity="1"/>')
        else:
            n, _, x, y, r, h = u
            g.append(f'    <geom name="{n}" type="cylinder" size="{r:.3f} {h/2:.3f}" '
                     f'pos="{x:.3f} {y:.3f} {h/2:.3f}" material="low_mat" contype="1" conaffinity="1"/>')
    (ROBOT / f"{name}.xml").write_text(
        f'<!-- {name}: generated by quack-nav scripts/twin/houses/gen.py. -->\n<mujoco model="{name}">\n'
        + MATS + "  <worldbody>\n" + "\n".join(g) + "\n  </worldbody>\n</mujoco>\n")
    scene = (ROBOT / "scene_apartment.xml").read_text().replace('file="apartment.xml"', f'file="{name}.xml"')
    (ROBOT / f"scene_{name}.xml").write_text(scene)
    segs = []
    for (n, x0, x1, y0, y1, h) in boxes:
        if h <= 0.02:
            continue
        a, b, c, d = x0 * 100, x1 * 100, y0 * 100, y1 * 100
        segs += [f"    [{a:.1f}, {c:.1f}, {b:.1f}, {c:.1f}],  # {n} S", f"    [{b:.1f}, {c:.1f}, {b:.1f}, {d:.1f}],  # {n} E",
                 f"    [{b:.1f}, {d:.1f}, {a:.1f}, {d:.1f}],  # {n} N", f"    [{a:.1f}, {d:.1f}, {a:.1f}, {c:.1f}],  # {n} W"]
    (OUT / f"{name}.toml").write_text(
        f"# Ground truth for {name}, cm and degrees, room frame == MuJoCo world frame.\n"
        "walls = [\n" + "\n".join(segs) + "\n]\n\nstart = [0.0, 0.0, 0.0]\nkidnap = [200.0, -200.0, 180.0]\n")
    keys = ["kitchen", "bath", "corridor_n", "corridor_s", "west", "east"]
    # The paper twin's six room names for the five-room houses, as before;
    # a bigger house keeps its own.
    names = keys if len(rooms) <= len(keys) else list(rooms)
    world = {"source": f"{name}, generated", "bounds": [X0 - 0.3, X1 + 0.3, Y0 - 0.3, Y1 + 0.3], "start": [0.0, 0.0, 0.0],
             "holes": [list(h) for h in holes], "rooms": {k: v for k, v in zip(names, rooms.values())},
             "boxes": [[n, x0, x1, y0, y1] for (n, x0, x1, y0, y1, h) in boxes if h > 0.03], "low": low}
    (OUT / f"{name}.world.json").write_text(json.dumps(world, indent=1))
    (OUT / f"{name}.truth.json").write_text(json.dumps({"holes": holes, "rooms": rooms, "goals": goals,
                                                        "boxes": [[n, x0, x1, y0, y1, h] for (n, x0, x1, y0, y1, h) in boxes],
                                                        "unmapped": [list(u) for u in unmapped]}, indent=1))
    print(name, len(boxes), "boxes,", len(holes), "holes")


OUT.mkdir(parents=True, exist_ok=True)
HOUSES = {"casa_libera": house_libera, "casa_arredata": house_arredata, "casa_grande": house_grande, "casa_ingombra": house_ingombra}
for name in sys.argv[3:] or ["casa_libera", "casa_arredata"]:
    write(name, HOUSES[name]())
