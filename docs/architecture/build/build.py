"""Builds architecture.pdf and architecture.it.pdf from the .md pages and the SVGs
in img/ (pandoc + Chrome headless; the fonts come from Google Fonts, so online):

    python3 docs/architecture/build/build.py [en|it]
"""
import pathlib
import re
import subprocess
import sys
import tempfile

D = pathlib.Path(__file__).resolve().parent.parent
CHROME = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
FONTS = ("https://fonts.googleapis.com/css2?family=Anton"
         "&family=DM+Sans:opsz,wght@9..40,400;9..40,600;9..40,700"
         "&family=DM+Mono:wght@400;500&display=block")
LANG = {"en": ("architecture", "MICRODUCK · NAVIGATION", "quack-nav · architecture"),
        "it": ("architecture.it", "MICRODUCK · NAVIGAZIONE", "quack-nav · architettura")}


def inline_svg(m):
    # inline, so the page's fonts reach the drawings
    return f'<figure class="fig">{(D / m.group(1)).read_text()}</figure>'


def tiles(m):
    cells = re.findall(r"<td[^>]*>(.*?)</td>", m.group(0), flags=re.S)
    colors = ["y", "o", "c", "p", "l", "y"]
    out = "".join(f'<div class="tile {colors[i % 6]}">{c}</div>' for i, c in enumerate(cells))
    return f'<div class="tiles">{out}</div>'


def build(lang):
    stem, kicker, title = LANG[lang]
    body = subprocess.run(["pandoc", str(D / f"{stem}.md"), "-f", "gfm", "-t", "html5", "--no-highlight"],
                          capture_output=True, text=True, check=True).stdout
    body = re.sub(r'<img src="(img/[^"]+\.svg)"[^>]*/?>', inline_svg, body)
    body = re.sub(r'<p>(<figure class="fig">.*?</figure>)</p>', r'\1', body, flags=re.S)
    # a heading stays with the figure or the code that follows it
    body = re.sub(r'(<h[23][^>]*>(?:(?!</h[23]>).)*</h[23]>\s*)(<figure class="fig">.*?</figure>)',
                  r'<div class="keep">\1\2</div>', body, flags=re.S)
    body = re.sub(r'(<h3[^>]*>(?:(?!</h3>).)*</h3>\s*(?:<p>(?:(?!</p>).)*</p>\s*)?)(<pre(?:(?!</pre>).)*</pre>)',
                  r'<div class="keep">\1\2</div>', body, flags=re.S)
    # the numbers table becomes tiles
    body = re.sub(r'(?<=</h2>\n)<table>.*?</table>', tiles, body, count=1, flags=re.S)
    # the title and the first line become the hero
    body = re.sub(r'^<h1[^>]*>(.*?)</h1>\s*<p>(.*?)</p>',
                  rf'<header class="hero"><div class="kicker">{kicker}</div><h1>\1</h1><p class="tag">\2</p></header>',
                  body, count=1, flags=re.S)
    css = (D / "build" / "style.css").read_text()
    page = (f'<!doctype html><html lang="{lang}"><head><meta charset="utf-8"><title>{title}</title>'
            f'<link href="{FONTS}" rel="stylesheet"><style>{css}</style></head>'
            f'<body><main>{body}</main></body></html>')
    with tempfile.TemporaryDirectory() as tmp:
        html = pathlib.Path(tmp) / f"{stem}.html"
        html.write_text(page)
        pdf = D / f"{stem}.pdf"
        subprocess.run([CHROME, "--headless=new", "--disable-gpu", "--no-pdf-header-footer",
                        "--virtual-time-budget=8000", f"--print-to-pdf={pdf}", html.as_uri()],
                       check=True, capture_output=True)
    print(pdf)


for lang in sys.argv[1:] or list(LANG):
    build(lang)
