"""Per-room coverage: python3 rooms.py <frame.json> <explore.log> [<explore.log> ...]"""
import re, json, base64, sys
floors=[("cucina (NO)",-4,-1,0.5,3),("bagno (SE)",0.5,4,-3,-1),("corridoio N",-0.4,0,-0.7,3),("corridoio S",-0.4,0,-3,-1.4),("ovest (SO, resto)",-4,-0.4,-3,3),("est (NE, resto)",0,4,-3,3)]
def room_of(px,py):
    for n,x0,x1,y0,y1 in floors:
        if x0<=px<=x1 and y0<=py<=y1: return n
def track(log):
    return [(float(m.group(1)),float(m.group(2))) for l in open(log) for m in [re.search(r"truth=\(([-\d.]+), ([-\d.]+)", l)] if m]
j=json.load(open(sys.argv[1])); f=j.get('frame',j); raw=base64.b64decode(f['cells']); R,C,cm=f['rows'],f['cols'],f['cell_m']
free={n:0 for n,*_ in floors}
for r in range(R):
    for c in range(C):
        if raw[r*C+c]==1:
            n=room_of(f['x_min']+(c+.5)*cm, f['y_min']+(r+.5)*cm)
            if n: free[n]+=1
logs=sys.argv[2:]
print(f"{'zona':18} {'area m²':>8} {'mappata m²':>11} {'%':>4}  visitata " + " / ".join(l.split('/')[-1] for l in logs))
tot_a=tot_m=0
for n,x0,x1,y0,y1 in floors:
    area=(x1-x0)*(y1-y0); mapped=free[n]*cm*cm; tot_a+=area; tot_m+=mapped
    vis=" / ".join("sì" if any(room_of(*p)==n for p in track(l)) else "no" for l in logs)
    print(f"{n:18} {area:8.2f} {mapped:11.2f} {100*mapped/area:4.0f}  {vis}")
print(f"{'totale':18} {tot_a:8.2f} {tot_m:11.2f} {100*tot_m/tot_a:4.0f}")
