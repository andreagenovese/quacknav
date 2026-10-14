# From pollen-robotics/microduck PR 202's sim-maploc/ (Peter Schade, Apache-2.0),
# with Andrea Genovese's changes on the fork's maploc-study branch (the plan,
# the cliff guard's view, re-seating, QUACK_NAV_SOCKET). Copied here so the
# twin needs only public repositories; see scripts/twin/README.md.
"""body_server, plus a maploc overlay drawn into the viewer's `user_scn`.

Run with mjpython. This does not modify microduck_rl: it imports the real
body_server and replaces its `run` with the same loop plus three additions —

  1. keep the viewer handle (upstream it is a local that nothing can reach),
  2. a background thread subscribing to robotd's `robot.map` stream,
  3. rebuild `viewer.user_scn` from the newest MapFrame just before `sync()`,
     on the main thread, because `sync()` copies that scene and a socket
     thread writing it would race the copy,
  4. with `--media-socket PATH`, the head camera answered as mediad's
     `media.frame` on that socket (eye.py): rendered only when asked, on
     its own thread, from a copy of the data the loop makes between steps.
  5. with `TWIN_MOVERS=<truth.json>`, the house's people and pets moved
     between steps (movers.py; casa_viva's). `--headless` runs all of it
     without the window.

    mjpython body_with_map.py --port 7871 --ducks 1 --keyframe SIT \
        --scene .../scene_apartment.xml --robot-socket /tmp/dsm/a.sock
"""
from __future__ import annotations

import json
import math
import os
import sys
import time

sys.path.insert(0, "/Users/schade/Pollen/microduck_rl/src")
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))  #

import maploc_overlay as mo  # noqa: E402
from mjlab_microduck.sim import body_server as bs  # noqa: E402

ROBOT_SOCKET = "/tmp/dsm/a.sock"
for i, a in enumerate(sys.argv):
    if a == "--robot-socket":
        ROBOT_SOCKET = sys.argv[i + 1]
        del sys.argv[i:i + 2]
        break
MEDIA_SOCKET = None
for i, a in enumerate(sys.argv):
    if a == "--media-socket":
        MEDIA_SOCKET = sys.argv[i + 1]
        del sys.argv[i:i + 2]
        break


def _truth(world):
    """Where the first duck really is: (x, y, yaw) from the free joint.

    The overlay uses it to notice that the map underneath it has been
    swapped for a saved one, whose origin is somewhere else entirely.
    """
    if not world.bodies:
        return None
    trunk = world.bodies[0].trunk
    q = world.data.qpos
    x, y = float(q[trunk]), float(q[trunk + 1])
    w, qx, qy, qz = (float(v) for v in q[trunk + 3:trunk + 7])
    yaw = math.atan2(2.0 * (w * qz + qx * qy), 1.0 - 2.0 * (qy * qy + qz * qz))
    return (x, y, yaw)


def face_cameras_forward(world) -> None:
    """Turn each duck's head camera to look where its depth sensor looks.

    microduck_rl's MJCF (develop, 2026-10-12) puts `head_camera` looking
    straight back, 180 degrees from the `tof` site; the duck itself is
    fine (robotd's `frames.camera` looks ahead). Pollen know: since
    2026-09-02 their own body_server's camera (`sim/camera.py`, apirrone)
    turns it at load, and they chose to leave the MJCF alone. The old
    body_server this wrapper runs has no such turn, so it is done here:
    half a turn about the camera's own x, the same as theirs, which keeps
    the picture a quarter turn round as mediad's `rotate` 90 says.

    Only a camera that still looks away from its sensor is turned, so a
    model or a body_server that has already done it is left alone rather
    than turned back. `TWIN_CAMERA_BACKWARD=1` leaves every camera as is.
    """
    if os.environ.get("TWIN_CAMERA_BACKWARD") == "1":
        return
    import mujoco
    import numpy as np

    data = mujoco.MjData(world.model)
    mujoco.mj_forward(world.model, data)
    for body in world.bodies:
        cam = mujoco.mj_name2id(world.model, mujoco.mjtObj.mjOBJ_CAMERA, f"{body.prefix}head_camera")
        tof = mujoco.mj_name2id(world.model, mujoco.mjtObj.mjOBJ_SITE, f"{body.prefix}tof")
        if cam < 0 or tof < 0:
            continue
        looks = -data.cam_xmat[cam].reshape(3, 3)[:, 2]
        if looks @ data.site_xmat[tof].reshape(3, 3)[:, 0] > 0:
            continue
        q = np.zeros(4)
        mujoco.mju_mulQuat(q, world.model.cam_quat[cam].copy(), np.array([0.0, 1.0, 0.0, 0.0]))
        world.model.cam_quat[cam] = q


def run(world, headless: bool) -> None:
    viewer = None
    if not headless:
        import mujoco.viewer
        viewer = mujoco.viewer.launch_passive(
            world.model, world.data, show_left_ui=False, show_right_ui=False
        )

    face_cameras_forward(world)
    eye = None
    if MEDIA_SOCKET and world.bodies:
        import eye as eye_mod
        try:
            eye = eye_mod.Eye(world.model, f"{world.bodies[0].prefix}head_camera", MEDIA_SOCKET).start()
        except (ValueError, OSError) as e:
            print(f"== camera: none on the twin ({e})", flush=True)

    movers = None
    if os.environ.get("TWIN_MOVERS"):
        import movers as movers_mod
        movers = movers_mod.Movers(world.model, os.environ["TWIN_MOVERS"])

    source = mo.MapSource(ROBOT_SOCKET).start()
    # quack-nav's plan (route, aim, goal, booked drops), asked of quack-navd's
    # own socket (QUACK_NAV_SOCKET, which twin.sh always sets). The old path
    # through quacksat's MCP proxy (QUACK_NAV_MCP, QUACK_NAV_MCP_TOKEN) was
    # removed 2026-09-30.
    plans = mo.PlanSource(os.environ.get("QUACK_NAV_SOCKET")).start()

    # A map to draw when robotd's own is still empty. The live stream stays
    # authoritative: as soon as a MapFrame arrives with real cells in it, that
    # is what gets drawn. See the report -- maploc did not fill one in time.
    fallback = None
    fb_path = os.environ.get("MAP_FALLBACK")
    if fb_path:
        blob = json.load(open(fb_path))
        fallback = mo.MapFrame.from_params(blob["frame"])
        fb_trail = [tuple(p) for p in blob.get("trail", [])]
        print(f"== fallback map: {fallback.caption()}", flush=True)
    trail: list[tuple[float, float]] = []
    reseats = 0
    last_seq = -1
    last_report = 0.0

    dt = world.model.opt.timestep
    batch = max(1, round(0.020 / dt))
    period = batch * dt
    passes_per_frame = max(1, round((1.0 / 30.0) / period))
    step = 0
    next_step = time.perf_counter()
    try:
        while True:
            world.step(batch)
            if movers is not None:
                duck = _truth(world)
                movers.tick(world.data, duck[:2] if duck else None)
            if eye is not None:
                eye.tick(world.data)
            if viewer is not None and not viewer.is_running():
                break
            next_step += period
            slack = next_step - time.perf_counter()
            if slack > 0.002:
                time.sleep(slack)
            elif slack < -0.25:
                next_step = time.perf_counter()
            step += 1
            if viewer is not None and step % passes_per_frame == 0:
                frame, status = source.latest()
                # Draw the live map as soon as it has a single known cell. An
                # early map is 40-odd cells, and gating above that showed
                # nothing at all while maploc was plainly working.
                known = 0 if frame is None else int((frame.cells != 0).sum())
                if known == 0:
                    if fallback is not None:
                        with viewer.lock():
                            mo.draw(viewer.user_scn, fallback, trail=fb_trail)
                    viewer.sync()
                    continue
                if frame is not None:
                    if frame.seq != last_seq:
                        last_seq = frame.seq
                        if frame.tracking:
                            trail.append((frame.x, frame.y))
                        now = time.perf_counter()
                        if now - last_report > 2.0:
                            last_report = now
                            print("== map " + frame.caption(), flush=True)
                    with viewer.lock():
                        mo.draw(viewer.user_scn, frame, trail=trail, truth=_truth(world), plan=plans.latest())
                    # A map exchanged under the overlay takes the trail's
                    # meaning with it: those are map coordinates, and they
                    # now name somewhere the duck has never been.
                    if mo.reseats != reseats:
                        reseats = mo.reseats
                        trail.clear()
                viewer.sync()
    except KeyboardInterrupt:
        pass
    finally:
        if viewer is not None:
            viewer.close()


bs.run = run
bs.main()
