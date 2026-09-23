"""One call, one line back.

    call.py <nav.sock> <tool> [json-args]              a quack-navd tool (nav.call)
    call.py --robotd <robotd.sock> <method> [json]     a robotd method
    call.py --map <map.sock> <method> [json]           the map socket (robot.map_*)
"""
import json, socket, sys

args = sys.argv[1:]
kind = "nav"
if args[0] in ("--robotd", "--map"):
    kind = args.pop(0)[2:]
path, name = args[0], args[1]
params = json.loads(args[2]) if len(args) > 2 else {}
s = socket.socket(socket.AF_UNIX); s.connect(path); f = s.makefile("rw")
if kind == "nav":
    req = {"jsonrpc": "2.0", "id": 1, "method": "nav.call", "params": {"name": name, "args": params}}
else:
    req = {"jsonrpc": "2.0", "id": 1, "method": name, "params": params}
f.write(json.dumps(req) + "\n"); f.flush()
for line in f:
    m = json.loads(line)
    if m.get("id") == 1:
        print(json.dumps(m.get("result", m.get("error"))))
        break
