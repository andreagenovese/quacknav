"""oracle_book.py <truth.json> <name> <book in> <book out>

The drop book of the oracle (branch `oracle`): the true holes' rims, a
point every 5 cm along each edge at the books' radius (0.10 m), in place
of what the exploration booked; the lanes dropped (they are the trail of
a walk the oracle did not need), the progress kept.
"""
import json
import sys

truth, name, book_in, book_out = sys.argv[1:5]
holes = json.load(open(truth)).get("holes", [])
book = json.load(open(book_in))
rim = []
for x0, x1, y0, y1 in holes:
    xa, xb, ya, yb = min(x0, x1), max(x0, x1), min(y0, y1), max(y0, y1)
    n_x, n_y = max(1, round((xb - xa) / 0.05)), max(1, round((yb - ya) / 0.05))
    for i in range(n_x + 1):
        x = xa + (xb - xa) * i / n_x
        rim += [[round(x, 3), ya, 0.10], [round(x, 3), yb, 0.10]]
    for j in range(1, n_y):
        y = ya + (yb - ya) * j / n_y
        rim += [[xa, round(y, 3), 0.10], [xb, round(y, 3), 0.10]]
book[name] = rim
book[f"{name}.lanes"] = []
json.dump(book, open(book_out, "w"))
print(f"{name}: {len(holes)} holes, {len(rim)} rim points")
