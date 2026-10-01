"""release_body.py <tag> <repo> <out.md>: the GitHub release's text.

docs/release-notes-<tag>.md with its relative links made absolute, to the
file at the tag (https://github.com/<repo>/blob/<tag>/...): on the release
page a relative link resolves against /releases/ and leads nowhere. No
notes file for the tag: the tag's name alone. CI's `release` job runs it.
"""
import os
import posixpath
import re
import sys

tag, repo, out = sys.argv[1:4]
ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
notes = os.path.join(ROOT, "docs", f"release-notes-{tag}.md")
if not os.path.exists(notes):
    open(out, "w").write(tag + "\n")
    sys.exit(0)

base = f"https://github.com/{repo}/blob/{tag}/"


def absolute(m):
    path, _, frag = m.group(1).partition("#")
    full = posixpath.normpath(posixpath.join("docs", path))
    return "](" + base + full + ("#" + frag if frag else "") + ")"


text = open(notes, encoding="utf-8").read()
open(out, "w", encoding="utf-8").write(re.sub(r"\]\(((?![a-z]+:|#)[^)\s]+)\)", absolute, text))
