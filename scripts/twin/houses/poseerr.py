"""poseerr.py <nav.sock> <sim_port> <out.tsv>: map pose vs simulator truth every
POSEERR_DT seconds (5 by default), until the sockets go.

Columns: time, map x, map y, true x, true y, distance, explore state, goal,
map yaw, true yaw (radians; the last two since 2026-09-25, so older files
end at the goal), and the mapper's covariance rebuilt from map_status's
pose_uncertainty — xx, xy, yy (m²), yaw·yaw (rad²), empty from a mapper that
keeps none (since 2026-09-26: does the sigma the duck believes follow the
error it has?). traj_metrics.py turns a file into ATE, RPE and NEES."""
import json, socket, sys, time, math, os
nav, port, out = sys.argv[1], int(sys.argv[2]), open(sys.argv[3], "a", buffering=1)
DT = float(os.environ.get("POSEERR_DT", "5"))
def true_yaw(q):
    w, x, y, z = q
    return math.atan2(2 * (w * z + x * y), 1 - 2 * (y * y + z * z))
def ask(req):
    s = socket.socket(socket.AF_UNIX); s.settimeout(5); s.connect(nav); f = s.makefile("rw"); f.write(json.dumps(req)+"\n"); f.flush(); return json.loads(f.readline())
while True:
    try:
        b = socket.socket(); b.settimeout(3); b.connect(("127.0.0.1", port)); bf = b.makefile("rw")
        bf.write(json.dumps({"op":"hello","protocol":1,"joints":15})+"\n"); bf.flush(); bf.readline()
        while True:
            r = ask({"jsonrpc":"2.0","id":1,"method":"nav.call","params":{"name":"robot.map_status","args":{}}})["result"]
            bf.write('{"op":"read"}\n'); bf.flush(); body = json.loads(bf.readline()); t = body["trunk"]
            tyaw = true_yaw(body["imu"]["quat"])
            p = r.get("pose") or {}; e = r.get("explore") or {}
            if p and r.get("tracking"):
                u = r.get("pose_uncertainty") or {}
                sig = "\t\t\t"
                if u:
                    a, M, m = math.radians(u["along_deg"]), u["xy_m"], u["xy_minor_m"]
                    c, sn = math.cos(a), math.sin(a)
                    xx, xy, yy = M*M*c*c + m*m*sn*sn, (M*M - m*m)*c*sn, M*M*sn*sn + m*m*c*c
                    sig = f"{xx:.6f}\t{xy:.6f}\t{yy:.6f}\t{math.radians(u['yaw_deg'])**2:.6f}"
                out.write(f"{time.time():.1f}\t{p['x']:.3f}\t{p['y']:.3f}\t{t[0]:.3f}\t{t[1]:.3f}\t{math.hypot(p['x']-t[0], p['y']-t[1]):.3f}\t{e.get('state')}\t{e.get('goal')}\t{p.get('yaw', float('nan')):.3f}\t{tyaw:.3f}\t{sig}\n")
            elif p:
                # The pose the mapper does not vouch for, and the truth: was
                # the candidate it would not believe the right one?
                with open(sys.argv[3] + ".untracked", "a") as u:
                    u.write(f"{time.time():.0f}\t{p['x']:.3f}\t{p['y']:.3f}\t{t[0]:.3f}\t{t[1]:.3f}\t{math.hypot(p['x']-t[0], p['y']-t[1]):.3f}\n")
            time.sleep(DT)
    except Exception:
        time.sleep(5)
