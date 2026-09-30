"""knobs.py [--check]: every environment variable the code reads, from the code.

Writes docs/knobs.md and docs/knobs.it.md: per variable, where it is read,
how its value is taken (a number with its default, on unless 0, on only if
1, set = on, a path...), and the comment the code gives it. `--check`
writes nothing and fails when the files are not what the code says — CI
runs it, so a knob added, renamed or dropped updates the list with it.

What counts as reading one: `std::env::var("NAME")` / `var_os`, and the
helpers that take a name — `quack_duck::env::qk("X")` and `env_switch("X")`
(both read `QK_X`), `switch("NAME")`, `knob("NAME", default)`, `envf`,
`envf32` — in quack-nav, quack-duck and maploc; `os.environ` in the twin's
Python. Variables of the build and the shell (`RUST_LOG`, `HOME`...) are
left out.
"""
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SKIP = {"RUST_LOG", "HOME", "PATH", "USER", "TMPDIR", "CARGO_MANIFEST_DIR", "PYTHONPATH", "UPDATE_GOLDEN"}
CALL = re.compile(
    r'(?P<fn>env::var_os|env::var|env::qk|env_switch|\bswitch|\bknob|\benvf32|\benvf)\(\s*"(?P<name>[A-Z][A-Z0-9_]+)"(?P<rest>[^;{]{0,160})'
)


def semantics(fn, rest, tail):
    if fn in ("env::qk",) or fn == "env_switch":
        if fn == "env_switch":
            return "1 on, 0 off, else the mode's own"
    if fn == "switch":
        return "1 on, 0 off, else the caller's default"
    if fn in ("knob", "envf", "envf32"):
        d = re.match(r"\s*,\s*([^)]+)\)", rest)
        return f"number (default {d.group(1).strip()})" if d else "number"
    t = tail[:160]
    d = re.search(r"unwrap_or\(\s*([^)]+?)\s*\)", t)
    if ".parse" in t[:90]:
        return f"number (default {d.group(1)})" if d else "number (unset: off)"
    if re.search(r'!=\s*"0"', t[:110]):
        return "on unless 0"
    if re.search(r'==\s*"1"|Some\("1"\)|Ok\("1"\)', t[:110]):
        return "on only if 1" + (" (2: more)" if '"2"' in t[:110] else "")
    if re.match(r"\s*\)\s*\.(is_some|is_ok)\(\)", t):
        return "set = on (any value)"
    if fn == "env::var_os":
        return "a path, or a value"
    return "a value"


def comment_above(lines, i):
    """The comment block right above line i, or above the fn that holds it."""
    j = i - 1
    block = []
    while j >= 0 and re.match(r"\s*//", lines[j]):
        block.insert(0, re.sub(r"^\s*//[/!]?\s?", "", lines[j]))
        j -= 1
    if not block:
        k = i
        while k >= 0 and not re.match(r"\s*(pub(\([a-z]+\))?\s+)?fn\s", lines[k]):
            k -= 1
        j = k - 1
        while j >= 0 and re.match(r"\s*(//|#\[)", lines[j]):
            if lines[j].strip().startswith("//"):
                block.insert(0, re.sub(r"^\s*//[/!]?\s?", "", lines[j]))
            j -= 1
    text = " ".join(x.strip() for x in block if x.strip())
    parts = re.split(r"(?<=\.)\s", text) if text else []
    first = ""
    for part in parts:
        first = (first + " " + part).strip()
        if len(first) >= 40:
            break
    return first[:220] + ("…" if len(first) > 220 else "")


def collect():
    found = {}
    for top in ("quack-nav", "quack-duck", "maploc"):
        for dp, dn, fn in os.walk(os.path.join(ROOT, top)):
            dn[:] = [d for d in dn if d != "target"]
            for f in sorted(fn):
                if not f.endswith(".rs"):
                    continue
                p = os.path.join(dp, f)
                rel = os.path.relpath(p, ROOT)
                src = open(p, encoding="utf-8").read()
                lines = src.split("\n")
                for m in CALL.finditer(src):
                    fnn, name = m.group("fn"), m.group("name")
                    if fnn in ("env::qk", "env_switch"):
                        name = "QK_" + name
                    if name in SKIP:
                        continue
                    line = src[: m.start()].count("\n")
                    tail = src[m.start("rest") :]
                    e = found.setdefault(name, {"where": [], "how": set(), "doc": ""})
                    if rel not in e["where"]:
                        e["where"].append(rel)
                    e["how"].add(semantics(fnn, m.group("rest"), tail))
                    if not e["doc"]:
                        e["doc"] = comment_above(lines, line)
    for dp, dn, fn in os.walk(os.path.join(ROOT, "scripts")):
        dn[:] = [d for d in dn if d != "__pycache__"]
        for f in sorted(fn):
            if not f.endswith(".py") or f == "knobs.py":
                continue
            p = os.path.join(dp, f)
            rel = os.path.relpath(p, ROOT)
            src = open(p, encoding="utf-8").read()
            for m in re.finditer(r'os\.environ(?:\.get\(\s*|\[\s*)["\']([A-Z][A-Z0-9_]+)["\']', src):
                name = m.group(1)
                if name in SKIP:
                    continue
                e = found.setdefault(name, {"where": [], "how": set(), "doc": ""})
                if rel not in e["where"]:
                    e["where"].append(rel)
                e["how"].add("a value (script)")
    return found


HEAD = {
    "en": (
        "# The knobs\n\n"
        "Every environment variable the code reads, generated from the code by\n"
        "`scripts/knobs.py` (CI checks it is current — do not edit by hand).\n"
        "The `QK_*` knobs are quack-nav's, `MAPLOC_*` maploc's; the rest are the\n"
        "benches' (`maploc/examples`, `quack-nav/examples`) and the twin's scripts.\n"
        "The knobs once named `QUACKSAT_*` are `QK_*` since 2026-09-30.\n\n"
        "| Variable | Read as | Where | What the code says |\n|---|---|---|---|\n"
    ),
    "it": (
        "# Le manopole\n\n"
        "Ogni variabile d'ambiente che il codice legge, generata dal codice da\n"
        "`scripts/knobs.py` (il CI controlla che sia aggiornata — non modificarla a\n"
        "mano). Le `QK_*` sono di quack-nav, le `MAPLOC_*` di maploc; le altre dei\n"
        "banchi (`maploc/examples`, `quack-nav/examples`) e degli script del\n"
        "gemello. Le manopole che si chiamavano `QUACKSAT_*` sono `QK_*` dal\n"
        "2026-09-30. La descrizione è il commento del codice (in inglese).\n\n"
        "| Variabile | Letta come | Dove | Cosa dice il codice |\n|---|---|---|---|\n"
    ),
}
HOW_IT = {
    "on unless 0": "accesa salvo 0",
    "on only if 1": "accesa solo con 1",
    "on only if 1 (2: more)": "accesa con 1 (2: di più)",
    "set = on (any value)": "presente = accesa (qualsiasi valore)",
    "a path, or a value": "un percorso, o un valore",
    "a value": "un valore",
    "a value (script)": "un valore (script)",
    "number (unset: off)": "numero (assente: spenta)",
    "1 on, 0 off, else the mode's own": "1 accesa, 0 spenta, altrimenti quella del modo",
    "1 on, 0 off, else the caller's default": "1 accesa, 0 spenta, altrimenti il default del chiamante",
}


def how_it(h):
    if h.startswith("number (default"):
        return "numero (default" + h[len("number (default") :]
    if h == "number":
        return "numero"
    return HOW_IT.get(h, h)


def render(found, lang):
    order = sorted(found, key=lambda n: (0 if n.startswith("QK_") else 1 if n.startswith("MAPLOC_") else 2, n))
    out = [HEAD[lang]]
    for n in order:
        e = found[n]
        how = "; ".join(sorted(e["how"] if lang == "en" else {how_it(h) for h in e["how"]}))
        where = ", ".join(f"`{w}`" for w in e["where"][:3]) + (" …" if len(e["where"]) > 3 else "")
        doc = e["doc"].replace("|", "\\|")
        out.append(f"| `{n}` | {how} | {where} | {doc} |\n")
    return "".join(out)


def main():
    found = collect()
    targets = {"en": os.path.join(ROOT, "docs", "knobs.md"), "it": os.path.join(ROOT, "docs", "knobs.it.md")}
    stale = []
    for lang, path in targets.items():
        text = render(found, lang)
        if "--check" in sys.argv:
            if not os.path.exists(path) or open(path, encoding="utf-8").read() != text:
                stale.append(path)
        else:
            open(path, "w", encoding="utf-8").write(text)
    if stale:
        print("stale, run scripts/knobs.py:", *stale)
        sys.exit(1)
    print(f"{len(found)} knobs")


if __name__ == "__main__":
    main()
