"""Builds every final image from the raw captures in out/raw into site/assets (website) and out/wiki (wiki).
   python build.py            all of it
   python build.py gifs       just the GIFs
"""
import os, subprocess, sys
HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
RAW = os.path.join(HERE, "out", "raw")
IMG = os.path.join(ROOT, "site", "assets", "img")
GIF = os.path.join(ROOT, "site", "assets", "gif")
WIKI = os.path.join(HERE, "out", "wiki")
STILLS = ["library", "editor-rich", "macros", "macro-menu", "apps", "works-in", "advanced", "conflicts", "search", "settings", "savings", "new-phrase"]
GIFS = {"create": 1500, "search": 1500, "richtext": 1500, "macros": 1500, "advanced": 1500, "worksin": 1500, "expand": 1300, "popup": 1300}
POPUP_STILLS = ["popup-search", "popup-filter", "popup-commands"]  # the Alt+Space search box: light only, no title bar
EXPAND_TITLE = ["--title", "Re: Quick question about the onboarding timeline - Message", "--noicon"]

def run(*a):
    subprocess.run([sys.executable, os.path.join(HERE, "compose.py"), *a], check=True)

what = sys.argv[1] if len(sys.argv) > 1 else "all"
for d in (IMG, GIF, WIKI): os.makedirs(d, exist_ok=True)
if what in ("all", "stills"):
    for n in STILLS:
        for scheme in ("light", "dark"):
            src = os.path.join(RAW, f"{n}.{scheme}.png")
            if not os.path.exists(src): continue
            run("still", src, os.path.join(IMG, f"{n}.{scheme}.png"), "--scheme", scheme, "--width", "2000")
        src = os.path.join(RAW, f"{n}.light.png")
        if os.path.exists(src): run("still", src, os.path.join(WIKI, f"{n}.png"), "--scheme", "light", "--bare", "--width", "1600")
if what in ("all", "stills"):
    for n in POPUP_STILLS:
        src = os.path.join(RAW, f"{n}.light.png")
        if not os.path.exists(src): continue
        run("still", src, os.path.join(IMG, f"{n}.png"), "--scheme", "light", "--frameless")
        run("still", src, os.path.join(WIKI, f"{n}.png"), "--scheme", "light", "--frameless", "--bare")
if what in ("all", "gifs"):
    for n, w in GIFS.items():
        d = os.path.join(RAW, f"{n}.light")
        if not os.path.isdir(d): continue
        extra = EXPAND_TITLE if n in ("expand", "popup") else []
        run("gif", d, os.path.join(GIF, f"{n}.gif"), "--scheme", "light", "--width", str(w), *extra)
        run("gif", d, os.path.join(WIKI, f"{n}.gif"), "--scheme", "light", "--width", str(w), "--bare", *extra)

# Keep the width/height attributes in the page in step with the real files (they set the aspect
# ratio before an image loads, so a mismatch makes the page jump).
if what in ("all", "stills", "gifs"):
    import re
    from PIL import Image
    page = os.path.join(ROOT, "site", "index.html")
    html = open(page, encoding="utf8").read()
    def size(path):
        with Image.open(os.path.join(ROOT, "site", path)) as im: return im.size
    def fix_img(m):
        w, h = size(m.group(2)); return f'{m.group(1)}{m.group(2)}" width="{w}" height="{h}"'
    html2 = re.sub(r'(<img src=")(assets/[^"]+)" width="\d+" height="\d+"', fix_img, html)
    html2 = re.sub(r'(srcset=")(assets/[^"]+)" width="\d+" height="\d+"', fix_img, html2)
    if html2 != html:
        open(page, "w", encoding="utf8", newline="").write(html2); print("updated image sizes in site/index.html")
