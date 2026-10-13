"""Things that move on the twin: people and pets as mocap bodies.

A house's truth.json may list `movers` (scripts/twin/houses/gen.py writes
casa_viva's); the scene has a mocap body `mv_<name>` for each. This moves
them, a few centimetres a step, from body_with_map.py's loop:

  - `linger`: stands at `at` (a doorway). Once the duck has been within
    `near_m` (LINGER_NEAR_M unless the house says), `linger_s` seconds more, then walks to `away`; when the
    duck is farther than RETURN_FAR_M for RETURN_AFTER_S, back to `at`.
  - `patrol`: walks its `path` there and back, pausing `pause_s` at each
    end or corner.

Every kind waits while the duck is in its way (nearer than YIELD_M ahead
of it): a person does not walk into a duck. A duck walking into one is the
duck's bump, printed (`== movers: bump <name>`) for the A/B to count.
`TWIN_MOVERS=<truth.json>` turns them on;
`TWIN_MOVERS_SEED` fixes their dice.
"""
from __future__ import annotations

import json
import math
import os
import random

LINGER_NEAR_M = 0.9
RETURN_FAR_M = 2.5
RETURN_AFTER_S = 10.0
YIELD_M = 0.35


class Mover:
    def __init__(self, spec, mocap_id, rng):
        self.spec, self.id, self.rng = spec, mocap_id, rng
        self.r = spec["r"]
        if spec["kind"] == "linger":
            self.pos = list(spec["at"])
            self.state, self.until = "at", None
        else:
            self.pos = list(spec["path"][0])
            self.leg, self.dir = 1, 1
            self.until = 0.0
        self.far_since = None

    def _toward(self, goal, dt, duck):
        dx, dy = goal[0] - self.pos[0], goal[1] - self.pos[1]
        d = math.hypot(dx, dy)
        if d < 1e-3:
            return True
        ux, uy = dx / d, dy / d
        # The duck ahead and near: wait for it.
        if duck is not None:
            ox, oy = duck[0] - self.pos[0], duck[1] - self.pos[1]
            if math.hypot(ox, oy) < self.r + YIELD_M and ox * ux + oy * uy > 0:
                return False
        step = min(d, self.spec["speed"] * dt)
        self.pos[0] += ux * step
        self.pos[1] += uy * step
        return step >= d - 1e-9

    def tick(self, t, dt, duck):
        s = self.spec
        near = duck is not None and math.hypot(duck[0] - self.pos[0], duck[1] - self.pos[1]) < s.get("near_m", LINGER_NEAR_M)
        if s["kind"] == "linger":
            if self.state == "at":
                if near:
                    self.state, self.until = "lingering", t + self.rng.uniform(*s["linger_s"])
            elif self.state == "lingering":
                if t >= self.until:
                    self.state = "leaving"
            elif self.state == "leaving":
                if self._toward(s["away"], dt, duck):
                    self.state, self.far_since = "away", None
            elif self.state == "away":
                far = duck is None or math.hypot(duck[0] - s["at"][0], duck[1] - s["at"][1]) > RETURN_FAR_M
                self.far_since = (self.far_since or t) if far else None
                if self.far_since is not None and t - self.far_since >= RETURN_AFTER_S:
                    self.state = "returning"
            elif self.state == "returning":
                if self._toward(s["at"], dt, duck):
                    self.state = "at"
            return
        if t < self.until:
            return
        path = s["path"]
        if self._toward(path[self.leg], dt, duck):
            self.until = t + self.rng.uniform(*s["pause_s"])
            nxt = self.leg + self.dir
            if not 0 <= nxt < len(path):
                if len(path) > 2 and path[0] != path[-1] and s.get("loop", len(path) > 2):
                    nxt = 0
                else:
                    self.dir = -self.dir
                    nxt = self.leg + self.dir
            self.leg = nxt


class Movers:
    """The house's movers, moved from the simulator's loop (`tick`)."""

    def __init__(self, model, truth_path):
        import mujoco

        specs = json.load(open(truth_path)).get("movers", [])
        seed = os.environ.get("TWIN_MOVERS_SEED")
        rng = random.Random(int(seed) if seed else None)
        self.items = []
        for s in specs:
            body = mujoco.mj_name2id(model, mujoco.mjtObj.mjOBJ_BODY, f"mv_{s['name']}")
            if body < 0 or model.body_mocapid[body] < 0:
                print(f"== movers: no mocap body mv_{s['name']} in the scene", flush=True)
                continue
            m = Mover(s, int(model.body_mocapid[body]), rng)
            m.geom = mujoco.mj_name2id(model, mujoco.mjtObj.mjOBJ_GEOM, f"mv_{s['name']}")
            m.touching = False
            self.items.append(m)
        self.last = None
        self.bumps = 0
        print(f"== movers: {', '.join(m.spec['name'] for m in self.items) or 'none'}", flush=True)

    def tick(self, data, duck):
        t = float(data.time)
        dt = 0.0 if self.last is None else max(0.0, t - self.last)
        self.last = t
        touched = set()
        for c in data.contact[: data.ncon]:
            touched.add(int(c.geom1))
            touched.add(int(c.geom2))
        for m in self.items:
            m.tick(t, dt, duck)
            data.mocap_pos[m.id][0] = m.pos[0]
            data.mocap_pos[m.id][1] = m.pos[1]
            # Only the duck touches a mocap body: the world's static
            # geometry and a mocap body never make a contact.
            now = m.geom in touched
            if now and not m.touching:
                self.bumps += 1
                print(f"== movers: bump {m.spec['name']} t={t:.1f} ({self.bumps})", flush=True)
            m.touching = now
