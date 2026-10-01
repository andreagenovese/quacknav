"""knobs.py [--check]: every environment variable the code reads, from the code.

Writes docs/knobs.md and docs/knobs.it.md: per variable, where it is read,
how its value is taken (a number with its default, on unless 0, on only if
1, 1 on else off with the default when unset, set = on, a path...), and the
comment the code gives it. Also quack-nav/src/knobs.json: the knobs
quack-navd itself reads (`QK_*` and `MAPLOC_*` outside the examples and the
tests), machine-readable — name, type, default, where, doc — which the
daemon serves as `nav.knobs` to a client that edits them. `--check`
writes nothing and fails when the files are not what the code says — CI
runs it, so a knob added, renamed or dropped updates the list with it.

What counts as reading one: `std::env::var("NAME")` / `var_os`, and the
helpers that take a name — `quack_duck::env::qk("X")` and `env_switch("X")`
(both read `QK_X`), `switch("NAME")`, `knob("NAME", default)`, `envf`,
`envf32` — in quack-nav, quack-duck and maploc; `os.environ` in the twin's
Python. Variables of the build and the shell (`RUST_LOG`, `HOME`...) are
left out. A second, cruder reading — every "QK_…"/"MAPLOC_…" literal in the
Rust sources outside comments, and every name handed to `env::var`,
`var_os`, `envf`, `envf32`, `switch` or `knob` line by line — must be
covered by the list, or the run fails: the parser once swallowed a read standing close after another and
lost four knobs, and a check of the generator against itself cannot see
that.
"""
import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SKIP = {"SYSTEMD_EXEC_PID", "RUST_LOG", "HOME", "PATH", "USER", "TMPDIR", "CARGO_MANIFEST_DIR", "PYTHONPATH", "UPDATE_GOLDEN"}
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
            conv = stmt[stmt.find(".parse") : u]
            if ".map(" in conv:
                # The default in the knob's own units: a plain scale in the
                # conversion (`ms * 1e6` to ns) is undone on it.
                k = re.search(r"\*\s*([0-9][0-9_.eE+-]*)\)", conv)
                try:
                    return f"number (default {float(d.replace('_', '')) / float(k[1].replace('_', '')):g})"
                except (TypeError, ValueError, ZeroDivisionError):
                    return f"number (default {d}, after conversion)"
            return f"number (default {d})"
        return "number (unset: none)"
    if re.search(r'!=\s*"0"', stmt):
        return "on unless 0"
    m = re.search(r'\.map\(\|\w+\|\s*\w+\s*==\s*"1"\)\s*\.unwrap_or\(', stmt)
    if m:
        d = " ".join(balanced(stmt, m.end() - 1).split())
        return f"1 on, else off; unset: {d}"
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


def comment_naming(lines, name):
    """The sentence of a comment elsewhere in the file that names the knob
    in backticks — a module's doc listing its knobs — when the comment
    above the read says nothing."""
    com = [re.match(r"\s*//[/!]?\s?(.*)", ln) for ln in lines]
    for k, m in enumerate(com):
        if not (m and "`" + name in m.group(1)):
            continue
        a = k
        while a > 0 and com[a - 1] and com[a - 1].group(1).strip() and not com[a].group(1).lstrip().startswith("- "):
            a -= 1
        b = k
        while b + 1 < len(com) and com[b + 1] and com[b + 1].group(1).strip() and not com[b + 1].group(1).lstrip().startswith("- "):
            b += 1
        text = " ".join(com[x].group(1).strip() for x in range(a, b + 1)).lstrip("- ")
        for sentence in re.split(r"(?<=[.;])\s+", text):
            if "`" + name in sentence:
                return sentence[:220] + ("…" if len(sentence) > 220 else "")
    return ""


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
                        e["doc"] = comment_above(lines, line) or comment_naming(lines, name)
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
        "On the duck they live in `/var/lib/quack-nav/knobs.env`, read by the unit\n"
        "(`EnvironmentFile=-`) at every start. A client edits them without a shell:\n"
        "`nav.knobs` on the nav socket lists quack-navd's own (machine-readable,\n"
        "`quack-nav/src/knobs.json`, generated with this page) and writes the file,\n"
        "`nav.restart` applies them — quack-control's page has an editor for both.\n"
        "Every knob needs that restart: the environment is read at the process's start.\n\n"
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
        "Sull'anatra stanno in `/var/lib/quack-nav/knobs.env`, letto dall'unit\n"
        "(`EnvironmentFile=-`) a ogni avvio. Un client le modifica senza una shell:\n"
        "`nav.knobs` sul socket nav elenca quelle di quack-navd (leggibili da una\n"
        "macchina, `quack-nav/src/knobs.json`, generato con questa pagina) e scrive il\n"
        "file, `nav.restart` le applica — la pagina di quack-control ha un editor per\n"
        "entrambe. Ogni manopola richiede quel riavvio: l'ambiente si legge all'avvio\n"
        "del processo.\n\n"
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
        return "numero (default" + h[len("number (default") :].replace(", after conversion", ", dopo la conversione")
    if h == "number":
        return "numero"
    if h.startswith("1 on, else off; unset: "):
        return "1 accesa, altrimenti spenta; assente: " + h[len("1 on, else off; unset: ") :]
    return HOW_IT.get(h, h)


def render(found, lang):
    order = sorted(found, key=lambda n: (0 if n.startswith("QK_") else 1 if n.startswith("MAPLOC_") else 2, n))
    out = [HEAD[lang]]
    for n in order:
        e = found[n]
        hows = {readable(h) for h in e["how"]}
        how = "; ".join(sorted(hows if lang == "en" else {how_it(h) for h in hows}))
        where = ", ".join(f"`{w}`" for w in e["where"][:3]) + (" …" if len(e["where"]) > 3 else "")
        doc = e["doc"].replace("|", "\\|")
        out.append(f"| `{n}` | {how} | {where} | {doc} |\n")
    return "".join(out)


TWIN_ONLY = ("QK_ORACLE_",)


def daemon_file(rel):
    """A source quack-navd is built from: not a bench, not a test."""
    parts = rel.split(os.sep)
    return parts[-1].endswith(".rs") and "examples" not in parts and "tests" not in parts


def constant(name):
    """A numeric `const NAME: T = value;` in the daemon's sources, or None."""
    pat = re.compile(r"\bconst\s+" + re.escape(name) + r"\s*:\s*\w+\s*=\s*([-0-9_.eE+]+)\s*;")
    for top in ("quack-nav", "quack-duck", "maploc"):
        for dp, dn, fn in os.walk(os.path.join(ROOT, top)):
            dn[:] = [d for d in dn if d != "target"]
            for f in sorted(fn):
                if f.endswith(".rs"):
                    m = pat.search(open(os.path.join(dp, f), encoding="utf-8").read())
                    if m:
                        return m.group(1).replace("_", "")
    return None


def resolved(d):
    """A default as a number: its constants replaced by their values, and
    `if mode { a } else { b }` said as "b (mode: a)"."""
    d = re.sub(r"\b[A-Z][A-Z0-9_]{2,}\b", lambda m: constant(m.group(0)) or m.group(0), d)
    m = re.fullmatch(r"if (\w+) \{ (\S+) \} else \{ (\S+) \}", d.strip())
    return f"{m.group(3)} ({m.group(1)}: {m.group(2)})" if m else d


def readable(h):
    """A reading as the docs say it: a default named by a constant given as
    its value."""
    m = re.fullmatch(r"number \(default (.+?)(, after conversion)?\)", h)
    return f"number (default {resolved(m.group(1))}{m.group(2) or ''})" if m else h


def typed(hows):
    """How a client edits a knob: its type — number, switch ("0"/"1"),
    choice (one of `options`), flag (set to "1" is on, unset off) or text —
    the default it would show, and what a switch means unset."""
    for h in sorted(hows):
        m = re.match(r"number \(default (.+)\)$", h)
        if m:
            return {"type": "number", "default": resolved(m.group(1).replace(", after conversion", ""))}
        if h == "number" or h.startswith("number (unset"):
            return {"type": "number", "default": None}
        if h.startswith("1 on, 0 off"):
            return {"type": "switch", "default": None, "unset": h.split(", else ", 1)[1]}
        if h == "on unless 0":
            return {"type": "switch", "default": "1"}
        if h == "on only if 1 (2: more)":
            return {"type": "choice", "default": "0", "options": ["0", "1", "2"]}
        if h.startswith("on only if 1"):
            return {"type": "switch", "default": "0"}
        if h.startswith("1 on, else off; unset: "):
            return {"type": "switch", "default": "1" if h.endswith("true") else "0" if h.endswith("false") else None}
        if h == "set = on (any value)":
            # Any value is on, "0" too: on is "1", off is unset.
            return {"type": "flag", "default": None, "unset": "off"}
    return {"type": "text", "default": None}


def machine(found):
    """The knobs quack-navd reads, as quack-nav/src/knobs.json holds them."""
    out = []
    for n in sorted(found):
        e = found[n]
        where = [w for w in e["where"] if daemon_file(w)]
        if not (n.startswith("QK_") or n.startswith("MAPLOC_")) or not where:
            continue
        # The oracle's knobs replace the map, the holes or the pose with the
        # twin's truth (src/oracle.rs): measuring tools, not the duck's.
        if n.startswith(TWIN_ONLY):
            continue
        k = {"name": n, "group": n.split("_", 1)[0], "read_as": "; ".join(sorted(readable(h) for h in e["how"]))}
        k.update(typed(e["how"]))
        k["where"] = where
        k["doc"] = e["doc"]
        out.append(k)
    return json.dumps({"generated_by": "scripts/knobs.py", "knobs": out}, indent=1, ensure_ascii=False) + "\n"


def raw_names():
    """Every "QK_…" / "MAPLOC_…" string literal in the Rust sources outside
    comments, every name given to `qk`/`env_switch`, and every name read
    with `env::var`/`var_os`/`envf`/`envf32`/`switch`/`knob`: a second
    reading of the code, line by line and not by the parser above, that the
    list must cover."""
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
                    for m in re.finditer(r'(?:env::var_os|env::var|\benvf32|\benvf|\bswitch|\bknob)\(\s*"([A-Z][A-Z0-9_]+)"', code):
                        if m.group(1) not in SKIP:
                            names.setdefault(m.group(1), f"{rel}:{n}")
    return names


def main():
    found = collect()
    missed = {n: w for n, w in raw_names().items() if n not in found and not n.startswith("QK_ENV_TEST")}
    if missed:
        print("read by the code but not in the list (the parser missed them):")
        for n, w in sorted(missed.items()):
            print(f"  {n}  {w}")
        sys.exit(1)
    targets = {
        "en": os.path.join(ROOT, "docs", "knobs.md"),
        "it": os.path.join(ROOT, "docs", "knobs.it.md"),
        "json": os.path.join(ROOT, "quack-nav", "src", "knobs.json"),
    }
    stale = []
    for lang, path in targets.items():
        text = machine(found) if lang == "json" else render(found, lang)
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
