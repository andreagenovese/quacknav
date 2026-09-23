# From pollen-robotics/microduck PR 202's sim-maploc/ (Peter Schade, Apache-2.0),
# with Andrea Genovese's changes on the fork's maploc-study branch (the plan,
# the cliff guard's view, re-seating, QUACK_NAV_SOCKET). Copied here so the
# twin needs only public repositories; see scripts/twin/README.md.
"""Draw a maploc occupancy grid into a MuJoCo passive viewer's `user_scn`.

The map crosses the process boundary as robotd's `robot.map` stream: JSON-RPC
`map.frame` notifications on robotd's unix socket, one per second, carrying a
base64 grid of one byte per cell (0 unknown, 1 free, 2 wall) plus the pose.
Nothing here knows about SLAM; it decodes MapFrame and emits boxes.

Two entry points:
  * `MapSource` — a background thread that subscribes to robotd and keeps the
    latest frame in a lock-protected slot.
  * `draw(user_scn, frame, ...)` — call from the thread that owns `viewer.sync()`.
"""

from __future__ import annotations

import base64
import json
import os
import socket
import threading
from dataclasses import dataclass

import mujoco
import numpy as np

UNKNOWN, FREE, WALL = 0, 1, 2

# Where the overlay floats. The apartment's walls run z in [0, 0.50]; the free
# tiles sit just above the floor and the wall cells just under the real wall
# tops, so the overlay reads against the geometry instead of hiding inside it.
FREE_Z = 0.004
FREE_H = 0.002
WALL_Z = 0.26
WALL_H = 0.26

COLOR_FREE = (0.20, 0.55, 0.95, 0.28)
COLOR_WALL = (1.00, 0.35, 0.15, 0.55)
COLOR_POSE = (1.00, 0.90, 0.10, 0.95)
COLOR_TRAIL = (1.00, 0.20, 0.40, 0.85)
# quacksat's plan, polled from its MCP: the route it means to walk, the
# point the current leg aims at, the goal, and the drops on its books.
COLOR_ROUTE = (0.10, 0.85, 0.20, 0.95)
COLOR_AIM = (0.95, 0.95, 0.10, 0.95)
COLOR_GOAL = (0.10, 0.90, 0.90, 0.80)
COLOR_DROP = (0.95, 0.10, 0.10, 0.70)
# The cliff guard's own view, polled with the plan: obstacle rays, the
# edges it sees, the wedge the head is looking through, its lane ahead.
COLOR_RAY_OBSTACLE = (1.00, 0.55, 0.10, 0.90)
COLOR_RAY_DROP = (1.00, 0.10, 0.10, 0.95)
COLOR_WEDGE = (0.60, 0.60, 0.60, 0.45)
COLOR_LANE = (0.10, 0.85, 0.90, 0.55)
GUARD_LANE_HALF_M = 0.16
GUARD_LANE_LEN_M = 1.0


@dataclass
class MapFrame:
    seq: int
    x: float
    y: float
    yaw: float
    tracking: bool
    x_min: float
    y_min: float
    cell_m: float
    rows: int
    cols: int
    cells: np.ndarray  # uint8, shape (rows, cols), row 0 at y_min
    n_submaps: int
    n_loops: int
    windows: int
    still: bool
    seated: bool

    @staticmethod
    def from_params(p: dict) -> "MapFrame":
        rows, cols = int(p["rows"]), int(p["cols"])
        raw = base64.b64decode(p["cells"])
        if len(raw) != rows * cols:
            raise ValueError(f"cells is {len(raw)} bytes, expected {rows * cols}")
        return MapFrame(
            seq=int(p["seq"]),
            x=float(p["x"]), y=float(p["y"]), yaw=float(p["yaw"]),
            tracking=bool(p["tracking"]),
            x_min=float(p["x_min"]), y_min=float(p["y_min"]),
            cell_m=float(p["cell_m"]),
            rows=rows, cols=cols,
            cells=np.frombuffer(raw, dtype=np.uint8).reshape(rows, cols),
            n_submaps=int(p.get("n_submaps", 0)),
            n_loops=int(p.get("n_loops", 0)),
            windows=int(p.get("windows", 0)),
            still=bool(p.get("still", False)),
            seated=bool(p.get("seated", False)),
        )

    def caption(self) -> str:
        return (
            f"seq {self.seq} · {self.rows}x{self.cols} @ {self.cell_m:.2f} m · "
            f"{self.n_submaps} submaps · {self.n_loops} loops · {self.windows} windows · "
            f"pose ({self.x:+.2f}, {self.y:+.2f}, {np.degrees(self.yaw):+.0f} deg) · "
            f"{'tracking' if self.tracking else 'LOST'}"
        )


class MapSource:
    """Subscribe to robotd's `robot.map` stream on a background thread."""

    def __init__(self, sock_path: str):
        self.sock_path = sock_path
        self._lock = threading.Lock()
        self._frame: MapFrame | None = None
        self._status = "connecting"
        self._stop = threading.Event()
        self.thread = threading.Thread(target=self._run, daemon=True)

    def start(self) -> "MapSource":
        self.thread.start()
        return self

    def latest(self):
        with self._lock:
            return self._frame, self._status

    def _run(self) -> None:
        backoff = 0.5
        while not self._stop.is_set():
            try:
                s = socket.socket(socket.AF_UNIX)
                s.settimeout(10)
                s.connect(self.sock_path)
                f = s.makefile("rw")
                f.write(json.dumps({"jsonrpc": "2.0", "id": 1, "method": "robot.map",
                                    "params": {}}) + "\n")
                f.flush()
                reply = json.loads(f.readline())
                res = reply.get("result", {})
                with self._lock:
                    self._status = (
                        f"subscribed (enabled={res.get('enabled')} mode={res.get('mode')})"
                    )
                backoff = 0.5
                s.settimeout(None)
                for line in f:
                    if self._stop.is_set():
                        break
                    try:
                        msg = json.loads(line)
                    except ValueError:
                        continue
                    if msg.get("method") != "map.frame":
                        continue
                    frame = MapFrame.from_params(msg["params"])
                    with self._lock:
                        self._frame = frame
                        self._status = "streaming"
            except Exception as error:  # noqa: BLE001 - a dead robotd is one reconnect
                with self._lock:
                    self._status = f"reconnecting ({type(error).__name__}: {error})"
            self._stop.wait(backoff)
            backoff = min(backoff * 2, 5.0)


class PlanSource:
    """`robot.map_status`, polled in the background: the planned route, the
    leg's aim, the goal and the drops on the books — all in the map frame,
    drawn through the same origin as the walls. Asked of quack-navd's own
    socket when `nav_socket` is given (the navigation's daemon, 2026-09-22),
    else of quacksat's MCP, which proxies the same tool."""

    def __init__(self, url: str, token: str, period_s: float = 0.5, nav_socket: str | None = None):
        self.url, self.token, self.period = url, token, period_s
        self.nav_socket = nav_socket
        self._lock = threading.Lock()
        self._plan = None

    def start(self):
        threading.Thread(target=self._run, daemon=True, name="plan-source").start()
        return self

    def latest(self):
        with self._lock:
            return self._plan

    def _run(self) -> None:
        import time
        import urllib.request
        body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
                           "params": {"name": "robot_map_status", "arguments": {}}}).encode()
        while True:
            plan = None
            try:
                if self.nav_socket:
                    d = self._ask_navd()
                else:
                    req = urllib.request.Request(self.url, data=body, headers={
                        "Content-Type": "application/json", "Authorization": f"Bearer {self.token}"})
                    with urllib.request.urlopen(req, timeout=2) as r:
                        text = json.load(r)["result"]["content"][0]["text"]
                    d = json.loads(text).get("data") or {}
                e = d.get("explore") or {}
                c = d.get("cliff") or {}
                pose = d.get("pose") or {}
                plan = {"route": [tuple(p) for p in e.get("route") or []],
                        "route_raw": [tuple(p) for p in e.get("route_raw") or []],
                        "aim": tuple(e["aim"]) if e.get("aim") else None,
                        "goal": tuple(e["goal"]) if e.get("goal") else None,
                        "drops": [tuple(p) for p in e.get("local") or []],
                        "running": e.get("state") == "running",
                        "rays": c.get("rays") or [],
                        "half_fov": c.get("half_fov", 0.39),
                        "pose": (pose["x"], pose["y"], pose["yaw"]) if pose else None}
            except Exception:
                plan = None
            with self._lock:
                self._plan = plan
            time.sleep(self.period)

    def _ask_navd(self) -> dict:
        s = socket.socket(socket.AF_UNIX)
        s.settimeout(2)
        try:
            s.connect(self.nav_socket)
            f = s.makefile("rw")
            f.write(json.dumps({"jsonrpc": "2.0", "id": 1, "method": "nav.call",
                                "params": {"name": "robot.map_status", "args": {}}}) + "\n")
            f.flush()
            return json.loads(f.readline()).get("result") or {}
        finally:
            s.close()


def _runs(row: np.ndarray, value: int):
    """Merge equal neighbours into (start, length) runs — far fewer boxes."""
    mask = row == value
    if not mask.any():
        return
    idx = np.flatnonzero(np.diff(np.concatenate(([0], mask.view(np.int8), [0]))))
    for a, b in zip(idx[0::2], idx[1::2]):
        yield int(a), int(b - a)


def _add(scn, type_, size, pos, mat, rgba) -> bool:
    if scn.ngeom >= scn.maxgeom:
        return False
    g = scn.geoms[scn.ngeom]
    mujoco.mjv_initGeom(g, type_, np.asarray(size, dtype=np.float64),
                        np.asarray(pos, dtype=np.float64),
                        np.asarray(mat, dtype=np.float64).reshape(9),
                        np.asarray(rgba, dtype=np.float32))
    g.category = mujoco.mjtCatBit.mjCAT_DECOR
    scn.ngeom += 1
    return True


_EYE = np.eye(3).reshape(9)


# Where the map frame sits in the world, as (x, y, yaw). The map's origin
# is wherever the duck booted, so `MICRODUCK_START` is the right answer at
# boot — but only until the map underneath changes. Loading or adopting a
# saved map (`robot.map_load`, `robot.map_adopt`) replaces the frame with
# the saved map's own, whose origin is wherever *that* run booted, and the
# old offset then draws the whole map and its pose arrow metres away from
# the duck.
#
# So the offset is re-derived at the swap and only there: a map that grows
# gains a submap at a time, one that is exchanged gains hundreds at once.
# Deriving it from the pose in that first frame is honest — it is the
# daemon's own answer to "where am I on this map now", which is what the
# overlay exists to show — and doing it once means every later error stays
# visible as an arrow away from the duck, instead of being defined away.
_origin = None
_submaps = None
# A swap seen, whose place in the world is not settled yet: the pose in the
# first frames of a loaded map is the one the saved run ended at, and means
# nothing until a still window judges it. Drawing on it once put a saved
# flat 88° across a different house.
_unsettled = False
# How many times the map has been exchanged under the overlay. The trail is
# kept by the caller, in map coordinates, so it has to be thrown away when
# those coordinates start meaning somewhere else.
reseats = 0
SWAP_SUBMAPS = 20
RESEAT_M = 1.0


def _spawn():
    sp = os.environ.get("MICRODUCK_START")
    if not sp:
        return (0.0, 0.0, 0.0)
    x, y, yaw = [float(v) for v in sp.split(",")]
    return (x, y, yaw)


def _reseat(frame: MapFrame, truth) -> None:
    """Re-derive the map's place in the world when the map is exchanged."""
    global _origin, _submaps, _unsettled
    if _origin is None:
        _origin = _spawn()
    swapped = _submaps is not None and abs(frame.n_submaps - _submaps) >= SWAP_SUBMAPS
    _submaps = frame.n_submaps
    if swapped:
        _unsettled = True
    if truth is None or not _unsettled:
        return
    # Keep re-deriving until the mapper trusts the pose again, then stop:
    # a map just loaded reports the pose its own last run ended at, and
    # settling on that draws the house at a place nobody is standing.
    if frame.tracking:
        _unsettled = False
    ox, oy, oyaw = _origin
    c, s = np.cos(oyaw), np.sin(oyaw)
    drawn = (ox + c * frame.x - s * frame.y, oy + s * frame.x + c * frame.y)
    if np.hypot(drawn[0] - truth[0], drawn[1] - truth[1]) <= RESEAT_M:
        return
    # truth = origin ∘ believed, solved for origin.
    global reseats
    reseats += 1
    yaw = truth[2] - frame.yaw
    c, s = np.cos(yaw), np.sin(yaw)
    _origin = (truth[0] - (c * frame.x - s * frame.y),
               truth[1] - (s * frame.x + c * frame.y),
               yaw)
    print(f"== map moved under the overlay; drawing it at "
          f"({_origin[0]:+.2f}, {_origin[1]:+.2f}, {np.degrees(_origin[2]):+.0f}°)", flush=True)


def draw(scn, frame: MapFrame, trail=None, show_free=True, truth=None, plan=None) -> int:
    """Rebuild `scn` from one MapFrame. Returns the geom count used.

    Must be called from the thread that owns `viewer.sync()`: `user_scn` is
    copied into the render scene by `sync()`, so mutating it from a socket
    thread races that copy.
    """
    scn.ngeom = 0
    c = frame.cell_m
    half = c / 2.0
    # The map frame is the world frame moved and turned; everything below is
    # transformed by that pose (quacksat's passage test, 2026-09-08).
    _reseat(frame, truth)
    _sx, _sy, _syaw = _origin
    _cs, _sn = np.cos(_syaw), np.sin(_syaw)
    _rot = np.array([[_cs, -_sn, 0], [_sn, _cs, 0], [0, 0, 1]], dtype=np.float64)
    def _w(x, y):
        return (_sx + _cs * x - _sn * y, _sy + _sn * x + _cs * y)
    _rot9 = _rot.reshape(9)

    for r in range(frame.rows):
        y = frame.y_min + (r + 0.5) * c
        row = frame.cells[r]
        if show_free:
            for start, length in _runs(row, FREE):
                x0 = frame.x_min + start * c
                wx, wy = _w(x0 + length * half, y)
                _add(scn, mujoco.mjtGeom.mjGEOM_BOX,
                     (length * half, half, FREE_H),
                     (wx, wy, FREE_Z), _rot9, COLOR_FREE)
        for start, length in _runs(row, WALL):
            x0 = frame.x_min + start * c
            wx, wy = _w(x0 + length * half, y)
            _add(scn, mujoco.mjtGeom.mjGEOM_BOX,
                 (length * half, half, WALL_H),
                 (wx, wy, WALL_Z), _rot9, COLOR_WALL)

    # The tracked path, in the map frame — deliberately separate from odometry,
    # because after a loop closure the two frames differ.
    if trail:
        for (px, py) in trail[::2]:
            wx, wy = _w(px, py)
            _add(scn, mujoco.mjtGeom.mjGEOM_SPHERE, (0.025, 0, 0),
                 (wx, wy, 0.05), _EYE, COLOR_TRAIL)

    # quacksat's plan: the route as a chain of capsules a hand above the
    # floor, the aim as a sphere, the goal as a post, the booked drops as
    # red discs — every one of them in the map frame, through `_w`.
    if plan:
        # Dijkstra's own route, thin and pale, under the pulled one.
        rpts = [_w(px, py) for (px, py) in plan.get("route_raw") or []]
        for (ax, ay), (bx, by) in zip(rpts, rpts[1:]):
            dx, dy = bx - ax, by - ay
            length = float(np.hypot(dx, dy))
            if length < 1e-4:
                continue
            ang = float(np.arctan2(dy, dx))
            ca, sa = np.cos(ang), np.sin(ang)
            mat = np.array([[ca, -sa, 0], [sa, ca, 0], [0, 0, 1]], dtype=np.float64) @ \
                  np.array([[0, 0, 1], [0, 1, 0], [-1, 0, 0]], dtype=np.float64)
            _add(scn, mujoco.mjtGeom.mjGEOM_CAPSULE, (0.006, length / 2.0, 0),
                 ((ax + bx) / 2.0, (ay + by) / 2.0, 0.07), mat.reshape(9), (0.55, 0.85, 0.55, 0.7))
        pts = [_w(px, py) for (px, py) in plan.get("route") or []]
        for (ax, ay), (bx, by) in zip(pts, pts[1:]):
            dx, dy = bx - ax, by - ay
            length = float(np.hypot(dx, dy))
            if length < 1e-4:
                continue
            ang = float(np.arctan2(dy, dx))
            ca, sa = np.cos(ang), np.sin(ang)
            # capsule axis is z: rotate z onto the segment's direction.
            mat = np.array([[ca, -sa, 0], [sa, ca, 0], [0, 0, 1]], dtype=np.float64) @ \
                  np.array([[0, 0, 1], [0, 1, 0], [-1, 0, 0]], dtype=np.float64)
            _add(scn, mujoco.mjtGeom.mjGEOM_CAPSULE, (0.012, length / 2.0, 0),
                 ((ax + bx) / 2.0, (ay + by) / 2.0, 0.08), mat.reshape(9), COLOR_ROUTE)
        for (px, py, r) in plan.get("drops") or []:
            wx, wy = _w(px, py)
            _add(scn, mujoco.mjtGeom.mjGEOM_CYLINDER, (max(r, 0.03), 0.004, 0),
                 (wx, wy, 0.012), _EYE, COLOR_DROP)
        # The guard's view, from the believed pose: a ray to every obstacle
        # it holds, a red segment where an edge begins (edge_min..range),
        # the head's wedge for the newest frame, and the lane it judges a
        # straight leg with.
        bp = plan.get("pose")
        if bp:
            px, py, pyaw = bp
            ox, oy = _w(px, py)
            wyaw = pyaw + _syaw
            def seg(a, b, z, radius, rgba):
                dx, dy = b[0] - a[0], b[1] - a[1]
                length = float(np.hypot(dx, dy))
                if length < 1e-4:
                    return
                ang = float(np.arctan2(dy, dx))
                ca, sa = np.cos(ang), np.sin(ang)
                mat = np.array([[ca, -sa, 0], [sa, ca, 0], [0, 0, 1]], dtype=np.float64) @ \
                      np.array([[0, 0, 1], [0, 1, 0], [-1, 0, 0]], dtype=np.float64)
                _add(scn, mujoco.mjtGeom.mjGEOM_CAPSULE, (radius, length / 2.0, 0),
                     ((a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0, z), mat.reshape(9), rgba)
            def at(bearing, r):
                return (ox + r * np.cos(wyaw + bearing), oy + r * np.sin(wyaw + bearing))
            rays = plan.get("rays") or []
            for f in rays[-3:]:
                for (b, r) in f.get("obstacles") or []:
                    seg((ox, oy), at(b, r), 0.06, 0.006, COLOR_RAY_OBSTACLE)
                for (b, e0, e1) in f.get("drops") or []:
                    seg(at(b, max(e0, 0.05)), at(b, max(e1, e0 + 0.02)), 0.03, 0.012, COLOR_RAY_DROP)
            if rays:
                hy = rays[-1].get("head_yaw", 0.0)
                half = plan.get("half_fov", 0.39)
                for side in (-1, 1):
                    seg((ox, oy), at(hy + side * half, 1.5), 0.04, 0.004, COLOR_WEDGE)
            for side in (-1, 1):
                a = at(np.arctan2(side * GUARD_LANE_HALF_M, 0.0), GUARD_LANE_HALF_M)
                b = at(np.arctan2(side * GUARD_LANE_HALF_M, GUARD_LANE_LEN_M), float(np.hypot(GUARD_LANE_LEN_M, GUARD_LANE_HALF_M)))
                seg(a, b, 0.02, 0.005, COLOR_LANE)
        if plan.get("aim"):
            wx, wy = _w(*plan["aim"])
            _add(scn, mujoco.mjtGeom.mjGEOM_SPHERE, (0.04, 0, 0), (wx, wy, 0.10), _EYE, COLOR_AIM)
        if plan.get("goal"):
            wx, wy = _w(*plan["goal"])
            _add(scn, mujoco.mjtGeom.mjGEOM_CYLINDER, (0.05, 0.30, 0), (wx, wy, 0.30), _EYE, COLOR_GOAL)

    # Pose estimate: a shaft along +x rotated by yaw, so heading is readable.
    ca, sa = np.cos(frame.yaw + _syaw), np.sin(frame.yaw + _syaw)
    mat = np.array([[ca, -sa, 0], [sa, ca, 0], [0, 0, 1]], dtype=np.float64)
    rgba = COLOR_POSE if frame.tracking else (1.0, 0.1, 0.1, 0.95)
    fx, fy = _w(frame.x, frame.y)
    _add(scn, mujoco.mjtGeom.mjGEOM_ARROW, (0.035, 0.035, 0.32),
         (fx, fy, 0.62), (mat @ np.array([[0, 0, 1], [0, 1, 0], [-1, 0, 0]])).reshape(9),
         rgba)
    _add(scn, mujoco.mjtGeom.mjGEOM_SPHERE, (0.07, 0, 0),
         (fx, fy, 0.62), _EYE, rgba)
    return scn.ngeom
