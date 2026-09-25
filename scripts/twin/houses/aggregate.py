"""aggregate.py: the final protocol's numbers, per house, as markdown tables."""
import json, os, re, statistics as st, sys

H = os.environ.get("TWIN_WORK", "/tmp/quack-twin-work")
HOUSES = [("house2", "apartment", "final-apart"), ("casa_arredata", "casa_arredata", "final-arredata"), ("casa_libera", "casa_libera", "final-libera")]


def area_weighted(cov, rooms):
    tot = sum((b - a) * (d - c) for a, b, c, d in rooms.values())
    return sum(cov.get(k, 0) * (b - a) * (d - c) for k, (a, b, c, d) in rooms.items()) / tot


def read(path):
    try:
        return open(path).read()
    except FileNotFoundError:
        return ""


rows = []
for label, name, base in HOUSES:
    truth = json.load(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), f"{name}.truth.json")))
    explore = read(f"{H}/{base}.txt")
    rounds = read(f"{H}/{base}-rounds.txt") or explore
    sessions = []
    for k in range(1, 5):
        rep = re.search(rf"session {k}: [^\n]*the duck reports (\{{[^\n]*\}})", explore)
        cov = re.search(rf"session {k}: known floor per room \(truth\) (\{{[^\n]*\}})", explore)
        home = re.search(rf"session {k}: homecoming ([^\n]*?) in (\d+) s", explore)
        if not (rep or cov or home):
            continue
        c = eval(cov.group(1)) if cov else {}
        sessions.append({
            "k": k,
            "reported": json.loads(rep.group(1)).get("percent_mapped") if rep else None,
            "truth": round(100 * area_weighted(c, truth["rooms"])) if c else None,
            "home": (home.group(1), int(home.group(2))) if home else None,
            "falls": int(re.search(rf"session {k}: [^\n]*falls (\d+)", explore).group(1)) if re.search(rf"session {k}: [^\n]*falls (\d+)", explore) else 0,
        })
    book = re.findall(r"drop book (\d+): rim (\d+), near (\d+), phantom (\d+)", rounds or explore)
    new_home = re.findall(r"round \d: homecoming ([^\n]*?) in (\d+) s, pose vs truth ([\d.]+) m; exploring after the restart: (\w+)", rounds)
    new_go = re.findall(r"  new go_to \S+ [^\n]*?: \w+ in (\d+) s[^\n]*— ([^\n]*)", rounds)
    main_home = re.findall(r"main round \d: homecoming (\w+) in (\d+) s", rounds)
    main_go = re.findall(r"  main go_to \S+ [^\n]*?: \w+ in (\d+) s[^\n]*— ([^\n]*)", rounds)
    new_falls = sum(int(x) for x in re.findall(r"\n\w+ round \d: falls (\d+)", "\n" + rounds))
    main_falls = sum(int(x) for x in re.findall(r"main round \d: falls (\d+)", rounds))
    arr = lambda g: [int(t) for t, why in g if "arrived" in why]
    rows.append((label, sessions, book[-1] if book else None, new_home, new_go, main_home, main_go, new_falls, main_falls, arr))

print("## Exploring, a session of 30 minutes at a time\n")
print("| House | Session | Homecoming | Reported | Truth | Falls |\n|---|---|---|---|---|---|")
for label, sessions, *_ in rows:
    for s in sessions:
        h = f"{s['home'][0]} ({s['home'][1]} s)" if s["home"] else "—"
        print(f"| {label} | {s['k']} | {h} | {s['reported']} % | {s['truth']} % | {s['falls']} |")
print("\n## The drop book after exploring\n")
print("| House | Drops | On the rim (≤ 10 cm) | Near (≤ 20 cm) | Phantom (> 20 cm) |\n|---|---|---|---|---|")
for label, _, book, *_ in rows:
    if book:
        print(f"| {label} | {book[0]} | {book[1]} | {book[2]} | {book[3]} |")
print("\n## Coming home and going places on the mapped house (A/B)\n")
print("| House | Build | Homecomings confirmed | Pose vs truth | go_to arrived | Median | Falls |\n|---|---|---|---|---|---|---|")
for label, _, _, nh, ng, mh, mg, nf, mf, arr in rows:
    errs = [float(e) for _, _, e, _ in nh]
    a = arr(ng); b = arr(mg)
    print(f"| {label} | new | {sum('frozen' in v for v, *_ in nh)}/{len(nh)} | {min(errs) if errs else '—'}–{max(errs) if errs else '—'} m | {len(a)}/{len(ng)} | {st.median(a) if a else '—'} s | {nf} |")
    print(f"| {label} | main | {sum(v == 'confirmed' for v, _ in mh)}/{len(mh)} | — | {len(b)}/{len(mg)} | {st.median(b) if b else '—'} s | {mf} |")
