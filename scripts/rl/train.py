"""train.py: the pilot's training (docs/rl-pilot.md).

    train.py bc  --out DIR [--iters 8] [--steps 256] [--envs 128] [--calib FILE] [--spread wide]
    train.py ppo --out DIR --init DIR/bc.pt [--updates 400] [--steps 128] [--envs 128] [--calib FILE] [--spread wide]

`bc`: imitation with DAgger. The teacher (quack-rl's expert, which sees the
truth) drives first; then, iteration by iteration, the pilot drives more and
the teacher labels what the pilot met. The observation's normalisation is
fitted on the first iteration and frozen.

`ppo`: the pilot then learns on its own reward (progress down the true way,
time, bumps, the shield's refusals, the rim's nearness; arrival and falls at
the end), the teacher's labels kept as a fading auxiliary loss. Every
`--eval-every` updates the pilot is exported and benched on held-out
scenarios through quack-navd's own loop (`rl_eval`); the best — no fall,
most arrivals — is kept as DIR/pilot.json.
"""
import argparse
import json
import os
import sys
import time

import numpy as np
import torch
import torch.nn.functional as F

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import pilot as P  # noqa: E402

torch.set_num_threads(4)


def log(out, text):
    print(text, flush=True)
    with open(os.path.join(out, "train.log"), "a") as f:
        f.write(text + "\n")


def calib_gait(path):
    """The walk a calibration is of (its "gait"; alpha when it names none)."""
    if not path:
        return "alpha"
    with open(path) as f:
        v = json.load(f)
    return (v.get("calib", v).get("gait")) or "alpha"


def bench(net, out, tag, args, meta, probe=None):
    path = os.path.join(out, f"{tag}.json")
    P.export_json(net, path, meta, probe=probe)
    text, summary = P.evaluate(path, seeds=args.eval_seeds, calib=args.calib)
    row = next(r for r in summary if r["family"] == "ALL")
    return path, row, text


def cmd_bc(args):
    env = P.VecEnv(args.envs, seed=args.seed, level=3, calib=args.calib, spread=args.spread)
    net = P.Net(env.obs_dim, env.n_actions, args.hidden)
    opt = torch.optim.Adam(net.parameters(), lr=1e-3)
    data_o, data_y = [], []
    tally = P.Tally()
    rng = np.random.default_rng(args.seed)
    for it in range(args.iters):
        beta = max(0.0, 1.0 - it / max(1, args.iters - 1))
        t0 = time.time()
        for _ in range(args.steps):
            obs, rew, done, expert, outcome, family = env.last
            tally.add(done, outcome, family)
            data_o.append(obs.astype(np.float32))
            data_y.append(expert.copy())
            if beta >= 1.0:
                act = expert
            else:
                with torch.no_grad():
                    lg = net.logits(torch.as_tensor(obs))
                mine = torch.distributions.Categorical(logits=lg).sample().numpy() if args.sample else lg.argmax(-1).numpy()
                act = np.where(rng.random(env.n) < beta, expert, mine)
            env.step(act)
        X = np.concatenate(data_o)
        Y = np.concatenate(data_y)
        if it == 0:
            net.mean.copy_(torch.as_tensor(X.mean(0)))
            net.std.copy_(torch.as_tensor(np.maximum(X.std(0), 0.05)))
        Xt, Yt = torch.as_tensor(X), torch.as_tensor(Y)
        n = len(Xt)
        for _ in range(args.epochs):
            perm = torch.randperm(n)
            tot, acc = 0.0, 0.0
            for k in range(0, n, 4096):
                idx = perm[k:k + 4096]
                lg = net.logits(Xt[idx])
                loss = F.cross_entropy(lg, Yt[idx])
                opt.zero_grad()
                loss.backward()
                opt.step()
                tot += loss.item() * len(idx)
                acc += (lg.argmax(-1) == Yt[idx]).float().sum().item()
        log(args.out, f"bc it {it} beta {beta:.2f}: {n} samples, loss {tot / n:.3f}, agree {100 * acc / n:.1f} %, {time.time() - t0:.0f} s | driving: {tally.summary(last=400)}")
    env.close()
    meta = {"obs_version": env.obs_version, "gait": calib_gait(args.calib), "stage": "bc", "iters": args.iters, "samples": int(len(data_y) * env.n), "calib": args.calib, "spread": args.spread}
    P.save(net, os.path.join(args.out, "bc.pt"), meta)
    probe = np.concatenate(data_o)[:: max(1, len(data_o) * env.n // 4)][:4]
    P.export_json(net, os.path.join(args.out, "bc.json"), meta, probe=probe)
    text, summary = P.evaluate(os.path.join(args.out, "bc.json"), seeds=args.eval_seeds, calib=args.calib, extra=("--stick", "--expert"))
    log(args.out, "bc bench (held-out):\n" + text)


def cmd_ppo(args):
    net, meta0 = P.load(args.init)
    env = P.VecEnv(args.envs, seed=args.seed, level=3, calib=args.calib, spread=args.spread, focus=args.focus)
    opt = torch.optim.Adam(net.parameters(), lr=args.lr)
    tally = P.Tally()
    best = None
    T, N = args.steps, env.n
    gamma, lam = 0.99, 0.95
    meta = dict(meta0, stage="ppo", init=args.init, calib=args.calib, spread=args.spread, gait=calib_gait(args.calib))
    for u in range(args.updates):
        t0 = time.time()
        bc_coef = max(args.bc_floor, args.bc0 * max(0.0, 1.0 - u / max(1, args.bc_decay)))
        frac = 1.0 - u / args.updates
        for g in opt.param_groups:
            g["lr"] = args.lr * max(0.1, frac)
        O = np.zeros((T, N, env.obs_dim), np.float32)
        A = np.zeros((T, N), np.int64)
        LP = np.zeros((T, N), np.float32)
        V = np.zeros((T + 1, N), np.float32)
        R = np.zeros((T, N), np.float32)
        D = np.zeros((T, N), np.float32)
        E = np.zeros((T, N), np.int64)
        for t in range(T):
            obs, _, _, expert, _, _ = env.last
            ot = torch.as_tensor(obs)
            with torch.no_grad():
                lg = net.logits(ot)
                v = net.value(ot)
            dist = torch.distributions.Categorical(logits=lg)
            a = dist.sample()
            O[t], A[t], LP[t], V[t], E[t] = obs, a.numpy(), dist.log_prob(a).numpy(), v.numpy(), expert
            obs2, rew, done, _, outcome, family = env.step(a.numpy())
            tally.add(done, outcome, family)
            R[t], D[t] = rew, done.astype(np.float32)
        with torch.no_grad():
            V[T] = net.value(torch.as_tensor(env.last[0])).numpy()
        adv = np.zeros((T, N), np.float32)
        last = np.zeros(N, np.float32)
        for t in reversed(range(T)):
            nonterm = 1.0 - D[t]
            delta = R[t] + gamma * V[t + 1] * nonterm - V[t]
            last = delta + gamma * lam * nonterm * last
            adv[t] = last
        ret = adv + V[:T]
        b_o = torch.as_tensor(O.reshape(T * N, -1))
        b_a = torch.as_tensor(A.reshape(-1))
        b_lp = torch.as_tensor(LP.reshape(-1))
        b_adv = torch.as_tensor(adv.reshape(-1))
        b_ret = torch.as_tensor(ret.reshape(-1))
        b_e = torch.as_tensor(E.reshape(-1))
        b_adv = (b_adv - b_adv.mean()) / (b_adv.std() + 1e-8)
        n = T * N
        stats = []
        for _ in range(args.epochs):
            perm = torch.randperm(n)
            for k in range(0, n, args.minibatch):
                idx = perm[k:k + args.minibatch]
                lg = net.logits(b_o[idx])
                d = torch.distributions.Categorical(logits=lg)
                lp = d.log_prob(b_a[idx])
                ratio = (lp - b_lp[idx]).exp()
                pg = -torch.min(ratio * b_adv[idx], ratio.clamp(1 - args.clip, 1 + args.clip) * b_adv[idx]).mean()
                vl = F.mse_loss(net.value(b_o[idx]), b_ret[idx])
                ent = d.entropy().mean()
                bc = F.cross_entropy(lg, b_e[idx])
                loss = pg + 0.5 * vl - args.ent * ent + bc_coef * bc
                opt.zero_grad()
                loss.backward()
                torch.nn.utils.clip_grad_norm_(net.parameters(), 0.5)
                opt.step()
                stats.append((pg.item(), vl.item(), ent.item(), bc.item()))
        s = np.mean(stats, 0)
        log(args.out, f"ppo {u}: reward/step {R.mean():+.3f}, pg {s[0]:+.3f} v {s[1]:.3f} ent {s[2]:.3f} bc {s[3]:.3f} (x{bc_coef:.2f}), {time.time() - t0:.0f} s | {tally.summary(last=600)}")
        if (u + 1) % args.eval_every == 0 or u == args.updates - 1:
            P.save(net, os.path.join(args.out, "last.pt"), dict(meta, update=u))
            path, row, text = bench(net, args.out, "last", args, dict(meta, update=u), probe=O[0, :4])
            score = (row.get("fell_hole", row["fell"]) == 0, row["arrived"] - 2 * row.get("tipped", 0), -row["mean_secs_arrived"])
            log(args.out, f"bench at {u}: arrived {row['arrived']}/{row['n']}, fell {row['fell']} (holes {row.get('fell_hole')}, tipped {row.get('tipped')}), off {row['arrived_off']}, timeout {row['timeout']}, failed {row['failed']}, secs {row['mean_secs_arrived']:.1f}, bumps {row['bumps_per_ep']:.2f}")
            if best is None or score > best[0]:
                best = (score, u)
                P.save(net, os.path.join(args.out, "best.pt"), dict(meta, update=u, bench=row))
                os.replace(path, os.path.join(args.out, "pilot.json"))
                log(args.out, f"  best so far (update {u})")
    env.close()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=["bc", "ppo"])
    ap.add_argument("--out", required=True)
    ap.add_argument("--init")
    ap.add_argument("--envs", type=int, default=128)
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--calib")
    ap.add_argument("--spread", default="wide")
    ap.add_argument("--focus", help="ppo: half the journeys from these families (comma-separated)")
    ap.add_argument("--hidden", type=int, default=256)
    ap.add_argument("--iters", type=int, default=8)
    ap.add_argument("--steps", type=int, default=None)
    ap.add_argument("--epochs", type=int, default=None)
    ap.add_argument("--sample", action="store_true")
    ap.add_argument("--updates", type=int, default=300)
    ap.add_argument("--lr", type=float, default=3e-4)
    ap.add_argument("--minibatch", type=int, default=4096)
    ap.add_argument("--clip", type=float, default=0.2)
    ap.add_argument("--ent", type=float, default=0.005)
    ap.add_argument("--bc0", type=float, default=0.5)
    ap.add_argument("--bc-decay", type=int, default=150)
    ap.add_argument("--bc-floor", type=float, default=0.0, help="ppo: the expert's labels never weigh less than this")
    ap.add_argument("--eval-every", type=int, default=10)
    ap.add_argument("--eval-seeds", type=int, default=30)
    args = ap.parse_args()
    os.makedirs(args.out, exist_ok=True)
    with open(os.path.join(args.out, f"args-{args.cmd}.json"), "w") as f:
        json.dump(vars(args), f, indent=1)
    if args.cmd == "bc":
        args.steps = args.steps or 256
        args.epochs = args.epochs or 6
        cmd_bc(args)
    else:
        args.steps = args.steps or 128
        args.epochs = args.epochs or 4
        cmd_ppo(args)


if __name__ == "__main__":
    main()
