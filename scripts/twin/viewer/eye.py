"""The duck's head camera on the twin, answered the way mediad answers it.

On the duck, Pollen's `mediad` serves one raw frame per request on a local
unix socket (`/run/mediad/media.sock`, 0660 group `robot`): a JSON-RPC line
`{"jsonrpc":"2.0","id":N,"method":"media.frame"}`, answered with one
JSON-RPC line whose result is the header — width, height, format "UYVY",
bytes, captured_at_unix_us, rotate — followed by exactly `bytes` raw UYVY
bytes. `hello` is answered too (duck_ipc_proto at daemon-v0.15.0, API 37).
This module is that endpoint for the twin, on `$STATE/media.sock`, so
quack-control's `mediad` adapter is the same for the twin and the duck.

**It never blocks the simulation.** A request sets a flag; the sim loop,
between two steps, copies MjData into a private one (mj_copyData: ~15 us
on casa_grande) and goes on; the render — update_scene, render, UYVY —
happens on this module's own thread with its own mujoco.Renderer (~15 ms
at 640x360 on an M-series Mac, off the sim thread). Nothing is rendered
while nobody asks.

The geometry is Pollen's twin camera's (microduck_rl `sim/camera.py` on
develop): 640x360, the `360p30` rung, and `rotate` 90 because the model's
head camera is rolled a quarter turn like the real mount — mediad says 90
for both, and the consumer turns the picture.
"""
from __future__ import annotations

import json
import os
import socket
import threading
import time

import mujoco
import numpy as np

WIDTH = 640
HEIGHT = 360
ROTATE = 90
API_VERSION = 37  # duck_ipc_proto at daemon-v0.15.0
# mediad's FRAME_TIMEOUT is 500 ms; the sim loop runs a pass every 20 ms.
COPY_TIMEOUT_S = 0.5
CLIENT_TIMEOUT_S = 5.0
MAX_REQUEST = 4096


def to_uyvy(rgb: np.ndarray) -> bytes:
    """RGB to packed UYVY, BT.601 limited range (the ISP's convention and
    the one mediad's `uyvy` crate inverts), chroma averaged per pair."""
    f = rgb.astype(np.float32)
    r, g, b = f[..., 0], f[..., 1], f[..., 2]
    y = 16.0 + 0.257 * r + 0.504 * g + 0.098 * b
    u = 128.0 - 0.148 * r - 0.291 * g + 0.439 * b
    v = 128.0 + 0.439 * r - 0.368 * g - 0.071 * b
    out = np.empty((rgb.shape[0], rgb.shape[1] // 2, 4), dtype=np.uint8)
    out[..., 0] = np.clip((u[:, 0::2] + u[:, 1::2]) / 2, 0, 255)
    out[..., 1] = np.clip(y[:, 0::2], 0, 255)
    out[..., 2] = np.clip((v[:, 0::2] + v[:, 1::2]) / 2, 0, 255)
    out[..., 3] = np.clip(y[:, 1::2], 0, 255)
    return out.tobytes()


class Eye:
    """`media.frame` on a unix socket, from the first duck's head camera."""

    def __init__(self, model: mujoco.MjModel, camera_name: str, path: str):
        self.model = model
        self.camera = mujoco.mj_name2id(model, mujoco.mjtObj.mjOBJ_CAMERA, camera_name)
        if self.camera < 0:
            raise ValueError(f"the model has no camera {camera_name!r}")
        self.path = path
        self.snap = mujoco.MjData(model)
        self.cv = threading.Condition()
        self.wanted = False
        self.generation = 0
        self.captured_us = 0
        self.renderer = None  # made on the serving thread: a GL context is per thread
        self.render_ms = 0.0

    def start(self) -> "Eye":
        try:
            os.unlink(self.path)
        except FileNotFoundError:
            pass
        self.listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.listener.bind(self.path)
        os.chmod(self.path, 0o660)
        self.listener.listen(4)
        threading.Thread(target=self._serve, name="eye", daemon=True).start()
        print(f"== camera: media.frame on {self.path} ({WIDTH}x{HEIGHT}, rotate {ROTATE})", flush=True)
        return self

    def tick(self, data: mujoco.MjData) -> None:
        """Called by the sim loop between steps: a copy when one is wanted."""
        if not self.wanted:
            return
        with self.cv:
            mujoco.mj_copyData(self.snap, self.model, data)
            self.captured_us = time.time_ns() // 1000
            self.wanted = False
            self.generation += 1
            self.cv.notify_all()

    def _frame(self) -> tuple[dict, bytes] | None:
        with self.cv:
            seen = self.generation
            self.wanted = True
            if not self.cv.wait_for(lambda: self.generation != seen, COPY_TIMEOUT_S):
                self.wanted = False
                return None
            captured = self.captured_us
            # The copy is ours until the next request, and requests are
            # served one at a time on this thread.
        t = time.perf_counter()
        if self.renderer is None:
            self.renderer = mujoco.Renderer(self.model, height=HEIGHT, width=WIDTH)
        self.renderer.update_scene(self.snap, camera=self.camera)
        pixels = to_uyvy(self.renderer.render())
        self.render_ms = (time.perf_counter() - t) * 1000.0
        header = {"width": WIDTH, "height": HEIGHT, "format": "UYVY", "bytes": len(pixels),
                  "captured_at_unix_us": captured, "rotate": ROTATE}
        return header, pixels

    def _serve(self) -> None:
        served = 0
        while True:
            conn, _ = self.listener.accept()
            with conn:
                conn.settimeout(CLIENT_TIMEOUT_S)
                try:
                    if self._handle(conn):
                        served += 1
                        if served == 1 or served % 100 == 0:
                            print(f"== camera: {served} frames served, last render {self.render_ms:.1f} ms", flush=True)
                except (OSError, ValueError):
                    pass

    def _handle(self, conn: socket.socket) -> bool:
        reader = conn.makefile("rb")
        while True:
            line = reader.readline(MAX_REQUEST + 1)
            if not line:
                return False
            if len(line) > MAX_REQUEST:
                _reply(conn, None, error=(-32602, "request is too large"))
                return False
            try:
                request = json.loads(line)
            except ValueError as e:
                _reply(conn, None, error=(-32700, str(e)))
                return False
            rid, method = request.get("id"), request.get("method")
            if method == "hello":
                _reply(conn, rid, result={"api_version": API_VERSION, "daemon_version": None, "revision": None})
                continue
            if method != "media.frame":
                _reply(conn, rid, error=(-32601, f"{method} is not served by the twin's camera"))
                continue
            frame = self._frame()
            if frame is None:
                _reply(conn, rid, error=(-32603, "no frame arrived within the capture timeout"))
                return False
            header, pixels = frame
            _reply(conn, rid, result=header)
            conn.sendall(pixels)
            return True


def _reply(conn: socket.socket, rid, result=None, error=None) -> None:
    body = {"jsonrpc": "2.0", "id": rid}
    if error is not None:
        body["error"] = {"code": error[0], "message": error[1]}
    else:
        body["result"] = result
    conn.sendall(json.dumps(body, separators=(",", ":")).encode() + b"\n")
