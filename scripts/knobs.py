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
left out. A second, cruder reading — every "QK_…"/"MAPLOC_…" literal in the
Rust sources outside comments — must be covered by the list, or the run
fails: the parser once swallowed a read standing close after another and
lost four knobs, and a check of the generator against itself cannot see
that.
"""
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SKIP = {"RUST_LOG", "HOME", "PATH", "USER", "TMPDIR", "CARGO_MANIFEST_DIR", "PYTHONPATH", "UPDATE_GOLDEN"}
CALL = re.compile(
    r'(?P<fn>env::var_os|env::var|env::qk|env_switch|\bswitch|\bknob|\benvf32|\benvf)\(\s*"(?P<name>[A-Z][A-Z0-9_]+)"'
)


def balanced(text, open_at):
    """The text inside the parenthesis opened at `open_at`."""
    depth = 0
    for k in range(open_at, len(text)):
        if text[k] == "(":
            depth += 1
        elif text[k] == ")":
            depth -= 1
            if depth == 0:
                return text[open_at + 1 : k]
    return text[open_at + 1 :]


def semantics(fn, stmt, before):
    if fn == "env_switch":
        return "1 on, 0 off, else the mode's own"
    if fn == "switch":
        return "1 on, 0 off, else the caller's default"
    if fn in ("knob", "envf", "envf32"):
        d = re.match(r'\(\s*"[A-Z0-9_]+"\s*,\s*', stmt)
        if d:
            open_at = stmt.find("(")
            arg = balanced(stmt, open_at).split(",", 1)[1].strip()
            return f"number (default {arg})"
        return "number"
    arms = re.findall(r'(?:Some|Ok)\("([^"]+)"\)', stmt)
    if re.search(r"\bmatch\s*$", before) or len(arms) >= 2:
        return "one of " + ", ".join(dict.fromkeys(arms)) + ", else the default"
    if ".parse" in stmt:
        u = stmt.find("unwrap_or(")
        if u >= 0:
            d = " ".join(balanced(stmt, u + len("unwrap_or")).split())
            converted = ".map(" in stmt[stmt.find(".parse") : u]
            return f"number (default {d}{', after conversion' if converted else ''})"
        return "number (unset: none)"
    if re.search(r'!=\s*"0"', stmt):
        return "on unless 0"
    if re.search(r'==\s*"1"', stmt):
        return "on only if 1" + (" (2: more)" if '"2"' in stmt else "")
    if re.match(r'\(\s*"[A-Z0-9_]+"\s*\)\s*\.(is_some|is_ok)\(\)', stmt):
        return "set = on (any value)"
    if fn == "env::var_os":
        return "a path, or a value"
    return "a value"


def comment_above(lines, i):
    """The comment right above line i; else, when the read sits within the
    first lines of a short function, that function's doc."""
    def block_above(j):
        out = []
        while j >= 0 and re.match(r"\s*(//|#\[)", lines[j]):
            if lines[j].strip().startswith("//"):
                out.insert(0, re.sub(r"^\s*//[/!]?\s?", "", lines[j]))
            j -= 1
        return out

    block = block_above(i - 1)
    if not block:
        k = i
        fn_line = r"\s*(pub(\([a-z]+\))?\s+)?fn\s"
        while k >= 0 and i - k <= 6 and not re.match(fn_line, lines[k]):
            k -= 1
        if k >= 0 and i - k <= 6 and re.match(fn_line, lines[k]):
            indent = lines[k][: len(lines[k]) - len(lines[k].lstrip())]
            ends_soon = any(lines[j] == indent + "}" for j in range(i, min(len(lines), i + 9)))
            if ends_soon:
                block = block_above(k - 1)
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
                reads = list(CALL.finditer(src))
                for n_, m in enumerate(reads):
                    fnn, name = m.group("fn"), m.group("name")
                    if fnn in ("env::qk", "env_switch"):
                        name = "QK_" + name
                    if name in SKIP:
                        continue
                    line = src[: m.start()].count("\n")
                    stop = reads[n_ + 1].start() if n_ + 1 < len(reads) else len(src)
                    stmt = src[src.find("(", m.start()) : stop][:400]
                    semi = stmt.find(";")
                    stmt = stmt if semi < 0 else stmt[:semi]
                    # Nor past the end of the item it sits in: a read that
                    # ends a struct literal ran on into the next function.
                    item_end = stmt.find("\n}")
                    stmt = stmt if item_end < 0 else stmt[:item_end]
                    before = src[max(0, m.start() - 40) : m.start()]
                    e = found.setdefault(name, {"where": [], "how": set(), "doc": ""})
                    if rel not in e["where"]:
                        e["where"].append(rel)
                    e["how"].add(semantics(fnn, stmt, before))
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


def raw_names():
    """Every "QK_…" / "MAPLOC_…" string literal in the Rust sources outside
    comments, and every name given to `qk`/`env_switch`: a second reading of
    the code, not by the parser above, that the list must cover."""
    names = {}
    for top in ("quack-nav", "quack-duck", "maploc"):
        for dp, dn, fn in os.walk(os.path.join(ROOT, top)):
            dn[:] = [d for d in dn if d != "target"]
            for f in fn:
                if not f.endswith(".rs"):
                    continue
                rel = os.path.relpath(os.path.join(dp, f), ROOT)
                for n, line in enumerate(open(os.path.join(dp, f), encoding="utf-8"), 1):
                    code = line.split("//", 1)[0]
                    for m in re.finditer(r'"((?:QK|MAPLOC)_[A-Z0-9_]+)"', code):
                        names.setdefault(m.group(1), f"{rel}:{n}")
                    for m in re.finditer(r'(?:env::qk|env_switch)\(\s*"([A-Z0-9_]+)"', code):
                        names.setdefault("QK_" + m.group(1), f"{rel}:{n}")
    return names


def main():
    found = collect()
    missed = {n: w for n, w in raw_names().items() if n not in found and not n.startswith("QK_ENV_TEST")}
    if missed:
        print("read by the code but not in the list (the parser missed them):")
        for n, w in sorted(missed.items()):
            print(f"  {n}  {w}")
        sys.exit(1)
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
