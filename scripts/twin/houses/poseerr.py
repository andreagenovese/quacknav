"""poseerr.py <nav.sock> <sim_port> <out.tsv>: map pose vs simulator truth every 5 s, until the sockets go."""
import json, socket, sys, time, math
nav, port, out = sys.argv[1], int(sys.argv[2]), open(sys.argv[3], "a", buffering=1)
def ask(req):
    s = socket.socket(socket.AF_UNIX); s.settimeout(5); s.connect(nav); f = s.makefile("rw"); f.write(json.dumps(req)+"\n"); f.flush(); return json.loads(f.readline())
while True:
    try:
        b = socket.socket(); b.settimeout(3); b.connect(("127.0.0.1", port)); bf = b.makefile("rw")
        bf.write(json.dumps({"op":"hello","protocol":1,"joints":15})+"\n"); bf.flush(); bf.readline()
        while True:
            r = ask({"jsonrpc":"2.0","id":1,"method":"nav.call","params":{"name":"robot.map_status","args":{}}})["result"]
            bf.write('{"op":"read"}\n'); bf.flush(); t = json.loads(bf.readline())["trunk"]
            p = r.get("pose") or {}; e = r.get("explore") or {}
            if p and r.get("tracking"):
                out.write(f"{time.time():.0f}\t{p['x']:.3f}\t{p['y']:.3f}\t{t[0]:.3f}\t{t[1]:.3f}\t{math.hypot(p['x']-t[0], p['y']-t[1]):.3f}\t{e.get('state')}\t{e.get('goal')}\n")
            elif p:
                # The pose the mapper does not vouch for, and the truth: was
                # the candidate it would not believe the right one?
                with open(sys.argv[3] + ".untracked", "a") as u:
                    u.write(f"{time.time():.0f}\t{p['x']:.3f}\t{p['y']:.3f}\t{t[0]:.3f}\t{t[1]:.3f}\t{math.hypot(p['x']-t[0], p['y']-t[1]):.3f}\n")
            time.sleep(5)
    except Exception:
        time.sleep(5)
