"""What the duck touches: one JSON line per contact with the house.

`TWIN_CONTACTS=<file>` appends a line when a touch between a part of the duck
and anything of the house but the floor ends: the part, the thing, the start's
unix time, how long, the peak normal force, the trunk's true pose at the start,
and where the touch was in the trunk's frame (`fwd_m`, `left_m`). A gap of
`GAP_S` or less is one touch. Read every control tick (20 ms): a touch shorter
than that can be missed. It only reads the simulation.
"""
from __future__ import annotations

import json
import math
import time

import mujoco
import numpy as np

GAP_S = 0.1


class Contacts:
    def __init__(self, model, path: str):
        self.model = model
        self.path = path
        self.force = np.zeros(6)
        self.open: dict[tuple[str, str], dict] = {}
        self.duck_bodies = {b for b in range(1, model.nbody) if model.body_rootid[b] != 0}

    def _name(self, geom: int) -> str:
        return mujoco.mj_id2name(self.model, mujoco.mjtObj.mjOBJ_GEOM, geom) or f"geom{geom}"

    def tick(self, data, truth) -> None:
        """`truth` is the trunk's (x, y, yaw), or None without a duck."""
        if truth is None:
            return
        now = time.time()
        x, y, yaw = truth
        for i in range(data.ncon):
            c = data.contact[i]
            b1, b2 = int(self.model.geom_bodyid[c.geom1]), int(self.model.geom_bodyid[c.geom2])
            d1, d2 = b1 in self.duck_bodies, b2 in self.duck_bodies
            if d1 == d2:
                continue
            duck_body, thing = (b1, self._name(c.geom2)) if d1 else (b2, self._name(c.geom1))
            if thing.startswith("floor"):
                continue
            part = mujoco.mj_id2name(self.model, mujoco.mjtObj.mjOBJ_BODY, duck_body) or f"body{duck_body}"
            mujoco.mj_contactForce(self.model, data, i, self.force)
            n = abs(float(self.force[0]))
            t = self.open.get((part, thing))
            if t is None:
                px, py = float(c.pos[0]) - x, float(c.pos[1]) - y
                t = self.open[(part, thing)] = {
                    "unix_s": round(now, 3), "part": part, "thing": thing, "max_n": 0.0, "last": now,
                    "at": [round(x, 3), round(y, 3), round(yaw, 3)],
                    "fwd_m": round(px * math.cos(yaw) + py * math.sin(yaw), 3),
                    "left_m": round(-px * math.sin(yaw) + py * math.cos(yaw), 3),
                }
            t["last"] = now
            t["max_n"] = max(t["max_n"], n)
        for key in [k for k, t in self.open.items() if now - t["last"] > GAP_S]:
            self._write(self.open.pop(key))

    def close(self) -> None:
        for t in self.open.values():
            self._write(t)
        self.open.clear()

    def _write(self, t: dict) -> None:
        t["secs"] = round(t.pop("last") - t["unix_s"], 3)
        t["max_n"] = round(t["max_n"], 2)
        with open(self.path, "a") as f:
            f.write(json.dumps(t) + "\n")
