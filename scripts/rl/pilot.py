"""The pilot's learning, shared by train.py, export.py and calibrate.sh.

- VecEnv: quack-rl's `rl_env` over a pipe (see its module doc for the
  protocol): N journeys through quack-navd's own loop, the learner's move
  on every leg.
- Net: the actor (what flies on the duck) and the critic (training only),
  with the observation's normalisation frozen into the checkpoint.
"""
import os
import struct
import subprocess

import numpy as np
import torch
import torch.nn as nn

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
FAMILIES = ["clutter", "doorway", "stairwell", "corners", "movers", "low", "mixed"]
OUTCOMES = {1: "arrived", 2: "arrived_off", 3: "fell", 4: "timeout", 5: "failed"}


def binary(name):
    return os.path.join(ROOT, "target", "release", name)


class VecEnv:
    def __init__(self, envs, seed=1, level=0, calib=None, spread="wide", focus=None):
        args = [binary("rl_env"), "--envs", str(envs), "--seed", str(seed), "--level", str(level), "--spread", spread]
        if focus:
            args += ["--focus", focus]
        if calib:
            args += ["--calib", calib]
        self.p = subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE, bufsize=0)
        self.n, self.obs_dim, self.n_actions, self.obs_version = struct.unpack("<IIII", self._read(16))
        self.last = self._recv()

    def _read(self, k):
        buf = bytearray()
        while len(buf) < k:
            chunk = self.p.stdout.read(k - len(buf))
            if not chunk:
                raise RuntimeError("rl_env ended")
            buf += chunk
        return bytes(buf)

    def _recv(self):
        n, d = self.n, self.obs_dim
        obs = np.frombuffer(self._read(4 * n * d), dtype="<f4").reshape(n, d).copy()
        rew = np.frombuffer(self._read(4 * n), dtype="<f4").copy()
        done = np.frombuffer(self._read(n), dtype=np.uint8).astype(bool)
        expert = np.frombuffer(self._read(n), dtype=np.uint8).astype(np.int64)
        outcome = np.frombuffer(self._read(n), dtype=np.uint8).copy()
        family = np.frombuffer(self._read(n), dtype=np.uint8).copy()
        return obs, rew, done, expert, outcome, family

    def step(self, actions):
        self.p.stdin.write(b"A" + bytes(np.asarray(actions, dtype=np.uint8)))
        self.last = self._recv()
        return self.last

    def set_level(self, level):
        self.p.stdin.write(b"L" + bytes([level]))

    def close(self):
        try:
            self.p.stdin.write(b"Q")
            self.p.stdin.flush()
        except Exception:
            pass
        self.p.kill()


def mlp(i, h, o):
    return nn.Sequential(nn.Linear(i, h), nn.Tanh(), nn.Linear(h, h), nn.Tanh(), nn.Linear(h, o))


class Net(nn.Module):
    def __init__(self, obs_dim, n_actions, hidden=256):
        super().__init__()
        self.actor = mlp(obs_dim, hidden, n_actions)
        self.critic = mlp(obs_dim, hidden, 1)
        self.register_buffer("mean", torch.zeros(obs_dim))
        self.register_buffer("std", torch.ones(obs_dim))
        self.hidden = hidden
        with torch.no_grad():
            self.actor[-1].weight.mul_(0.01)

    def norm(self, x):
        return ((x - self.mean) / self.std.clamp_min(1e-6)).clamp(-10, 10)

    def logits(self, x):
        return self.actor(self.norm(x))

    def value(self, x):
        return self.critic(self.norm(x)).squeeze(-1)


def save(net, path, meta):
    torch.save({"state": net.state_dict(), "obs_dim": net.mean.numel(), "n_actions": net.actor[-1].out_features, "hidden": net.hidden, "meta": meta}, path)


def load(path):
    ck = torch.load(path, map_location="cpu", weights_only=False)
    net = Net(ck["obs_dim"], ck["n_actions"], ck.get("hidden", 256))
    net.load_state_dict(ck["state"])
    return net, ck.get("meta", {})


class Tally:
    """Journeys ended, per family and outcome."""

    def __init__(self):
        self.rows = []

    def add(self, done, outcome, family):
        for d, o, f in zip(done, outcome, family):
            if d and o:
                self.rows.append((int(f), int(o)))

    def summary(self, last=None):
        rows = self.rows[-last:] if last else self.rows
        if not rows:
            return "no journey ended yet"
        n = len(rows)
        arr = sum(1 for _, o in rows if o == 1) / n
        fell = sum(1 for _, o in rows if o == 3)
        per = []
        for k, name in enumerate(FAMILIES):
            fr = [o for f, o in rows if f == k]
            if fr:
                per.append(f"{name[:5]} {100 * sum(1 for o in fr if o == 1) / len(fr):.0f}")
        return f"{n} journeys: arrived {100 * arr:.1f} %, fell {fell} | " + " ".join(per)


def export_json(net, path, meta, probe=None):
    """The actor as quack_nav::rlnav::PilotFile: what quack-navd loads
    (`QK_RL_POLICY`). `probe`: a few observations whose logits go along, for
    `rl_pilot_check` to compare with its own arithmetic."""
    import json

    layers = []
    linears = [m for m in net.actor if isinstance(m, nn.Linear)]
    for k, lin in enumerate(linears):
        layers.append({
            "w": lin.weight.detach().cpu().numpy().astype(float).tolist(),
            "b": lin.bias.detach().cpu().numpy().astype(float).tolist(),
            "act": "tanh" if k < len(linears) - 1 else "none",
        })
    meta = dict(meta)
    if probe is not None:
        x = torch.as_tensor(np.asarray(probe, dtype=np.float32))
        with torch.no_grad():
            lg = net.logits(x).numpy()
        meta["probe"] = {"obs": np.asarray(probe, dtype=float).tolist(), "logits": lg.astype(float).tolist()}
    out = {
        "format": "quack-pilot",
        "obs_version": int(meta["obs_version"]),
        "obs_dim": int(net.mean.numel()),
        "n_actions": int(linears[-1].out_features),
        "obs_mean": net.mean.cpu().numpy().astype(float).tolist(),
        "obs_std": net.std.clamp_min(1e-6).cpu().numpy().astype(float).tolist(),
        "layers": layers,
        "meta": meta,
    }
    with open(path, "w") as f:
        json.dump(out, f)


def export_onnx(net, path):
    """The actor with its normalisation, as ONNX: obs[N, OBS_DIM] -> logits."""

    class Actor(nn.Module):
        def __init__(self, n):
            super().__init__()
            self.n = n

        def forward(self, x):
            return self.n.logits(x)

    dummy = torch.zeros(1, net.mean.numel())
    torch.onnx.export(Actor(net).eval(), (dummy,), path, input_names=["obs"], output_names=["logits"],
                      dynamic_axes={"obs": {0: "n"}, "logits": {0: "n"}}, opset_version=17, dynamo=False)


def evaluate(pilot_json, seeds=30, seed0=100000, level=3, calib=None, extra=()):
    """rl_eval on held-out seeds: per family and overall, the pilot and
    whatever `extra` asks for (`--stick`, `--expert`). Returns the summary rows."""
    import json
    import tempfile

    with tempfile.NamedTemporaryFile(suffix=".json", delete=False) as t:
        out = t.name
    args = [binary("rl_eval"), "--seeds", str(seeds), "--seed0", str(seed0), "--level", str(level), "--threads", "10", "--out", out]
    if pilot_json:
        args += ["--pilot", pilot_json]
    if calib:
        args += ["--calib", calib]
    args += list(extra)
    text = subprocess.run(args, capture_output=True, text=True, check=True).stdout
    with open(out) as f:
        summary = json.load(f)["summary"]
    os.unlink(out)
    return text, summary
