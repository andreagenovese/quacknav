"""cut_mdlg.py <in.mdlg> <seconds> <out.mdlg> [pose.tsv out.truth.tsv]

The first `seconds` of a recording, as a recording of its own (same header,
same epoch, the records whole), and — given the live run's pose sampler file
— the truth rows that fall inside it (the tracked file's and the untracked
one's, merged). A fixture for the replay regression test, small enough to
keep in the repository."""
import struct
import sys

src, seconds, dst = sys.argv[1], float(sys.argv[2]), sys.argv[3]
with open(src, "rb") as f, open(dst, "wb") as o:
    head = f.read(16)
    magic, ver, epoch_ms = struct.unpack("<4sIQ", head)
    assert magic == b"MDLG" and ver == 2, (magic, ver)
    o.write(head)
    n = 0
    while True:
        h = f.read(13)
        if len(h) < 13:
            break
        ts, sid, size = struct.unpack("<QBI", h)
        payload = f.read(size)
        if ts / 1e6 > seconds:
            break
        o.write(h + payload)
        n += 1
print(f"{dst}: {n} records, {seconds:g} s from {src}")
if len(sys.argv) > 5:
    t0, t1 = epoch_ms / 1000, epoch_ms / 1000 + seconds
    rows = []
    for p in (sys.argv[4], sys.argv[4] + ".untracked"):
        try:
            for line in open(p):
                fl = line.rstrip("\n").split("\t")
                if len(fl) >= 5 and t0 <= float(fl[0]) <= t1:
                    # Truth only: time, (map x, map y blanked), true x, true y, ...
                    rows.append([fl[0], "nan", "nan", fl[3], fl[4], "nan", "truth", "None", "nan", fl[9] if len(fl) > 9 else "nan"])
        except FileNotFoundError:
            pass
    rows.sort(key=lambda r: float(r[0]))
    with open(sys.argv[5], "w") as o:
        for r in rows:
            o.write("\t".join(r) + "\n")
    print(f"{sys.argv[5]}: {len(rows)} truth rows")
