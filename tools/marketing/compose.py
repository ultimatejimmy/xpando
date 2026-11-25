"""Turns raw captures into the final images and GIFs.

Each raw capture is just the app's web view. This wraps it in a Windows 11 style window
(title bar, rounded corners, soft shadow) and, unless --bare is given, places it on a backdrop.

  python compose.py still  <raw.png>   <out.png>  [--scheme light|dark] [--bare]
  python compose.py gif    <frames dir> <out.gif>  [--scheme light|dark] [--width 1100] [--bare]
  python compose.py poster <frames dir> <out.png> [--index -1] ...   (one frame as a still)
"""
import argparse
import json
import os
import sys

from PIL import Image, ImageChops, ImageDraw, ImageFilter, ImageFont

HERE = os.path.dirname(os.path.abspath(__file__))
ICON = os.path.join(HERE, "..", "..", "xpando-app", "src-tauri", "icons", "32x32.png")
FONT = "C:/Windows/Fonts/segoeui.ttf"

THEME = {
    "light": dict(bar=(243, 243, 243), text=(26, 26, 26), border=(205, 208, 212), bg0=(232, 244, 234), bg1=(222, 232, 247), flat=(228, 238, 238)),
    "dark": dict(bar=(32, 32, 32), text=(240, 240, 240), border=(70, 72, 76), bg0=(14, 28, 20), bg1=(16, 22, 38), flat=(18, 26, 28)),
}
BAR_H = 32
RADIUS = 9
PAD = 56


def font(size):
    try:
        return ImageFont.truetype(FONT, size)
    except OSError:
        return ImageFont.load_default()


TITLE = {"text": "xpando", "icon": True, "frameless": False}


def window(shot: Image.Image, scheme: str) -> Image.Image:
    """The screenshot inside a window frame, as RGBA with transparent rounded corners."""
    if TITLE["frameless"]:  # a popup with no title bar: the capture already has its own rounded corners
        return shot.convert("RGBA")
    t = THEME[scheme]
    w, h = shot.size
    s = max(1, round(w / 1280))  # work in device pixels when the capture was taken at 2x
    bar_h, r = BAR_H * s, RADIUS * s
    win = Image.new("RGBA", (w, h + bar_h), t["bar"] + (255,))
    win.paste(shot.convert("RGB"), (0, bar_h))
    d = ImageDraw.Draw(win)
    # icon + title
    if TITLE["icon"] and os.path.exists(ICON):
        ic = Image.open(ICON).convert("RGBA").resize((16 * s, 16 * s), Image.LANCZOS)
        win.alpha_composite(ic, (12 * s, (bar_h - 16 * s) // 2))
    d.text(((36 if TITLE["icon"] else 14) * s, bar_h // 2), TITLE["text"], font=font(12 * s), fill=t["text"], anchor="lm")
    # caption buttons: minimize, maximize, close (left to right)
    c = t["text"]
    lw = max(1, s)
    mid = bar_h // 2
    x_close = w - 23 * s
    x_max = x_close - 46 * s
    x_min = x_close - 92 * s
    d.line([(x_min - 5 * s, mid), (x_min + 5 * s, mid)], fill=c, width=lw)
    d.rectangle([x_max - 5 * s, mid - 5 * s, x_max + 5 * s, mid + 5 * s], outline=c, width=lw)
    d.line([(x_close - 5 * s, mid - 5 * s), (x_close + 5 * s, mid + 5 * s)], fill=c, width=lw)
    d.line([(x_close - 5 * s, mid + 5 * s), (x_close + 5 * s, mid - 5 * s)], fill=c, width=lw)
    # hairline border and rounded corners
    d.rectangle([0, 0, w - 1, h + bar_h - 1], outline=t["border"], width=max(1, s))
    mask = Image.new("L", win.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, win.size[0] - 1, win.size[1] - 1], radius=r, fill=255)
    out = Image.new("RGBA", win.size, (0, 0, 0, 0))
    out.paste(win, (0, 0), mask)
    return out


def backdrop(size, scheme, flat=False):
    t = THEME[scheme]
    if flat:
        return Image.new("RGB", size, t["flat"])
    w, h = size
    # a diagonal two-colour wash, built small and scaled up so it stays smooth
    sw, sh = 64, 40
    base = Image.new("RGB", (sw, sh))
    px = base.load()
    for y in range(sh):
        for x in range(sw):
            k = (x / (sw - 1) * 0.65 + y / (sh - 1) * 0.35)
            px[x, y] = tuple(round(a + (b - a) * k) for a, b in zip(t["bg0"], t["bg1"]))
    return base.resize(size, Image.BICUBIC)


def place(win: Image.Image, scheme, flat=False, shadow=True) -> Image.Image:
    s = max(1, round(win.size[0] / 1280))
    pad = PAD * s
    size = (win.size[0] + pad * 2, win.size[1] + pad * 2)
    bg = backdrop(size, scheme, flat).convert("RGBA")
    if shadow:
        sh = Image.new("RGBA", size, (0, 0, 0, 0))
        ImageDraw.Draw(sh).rounded_rectangle(
            [pad, pad + 14 * s, pad + win.size[0], pad + win.size[1] + 14 * s], radius=(16 if TITLE["frameless"] else RADIUS * s),
            fill=(0, 0, 0, 120 if scheme == "dark" else 70))
        sh = sh.filter(ImageFilter.GaussianBlur(22 * s))
        bg.alpha_composite(sh)
    bg.alpha_composite(win, (pad, pad))
    return bg.convert("RGB")


def frame_image(png, scheme, bare, flat, width=None):
    shot = Image.open(png)
    win = window(shot, scheme)
    img = win.convert("RGBA") if bare else place(win, scheme, flat)
    if width and img.size[0] != width:
        img = img.resize((width, round(img.size[1] * width / img.size[0])), Image.LANCZOS)
    return img


def save_png(img, out):
    os.makedirs(os.path.dirname(out) or ".", exist_ok=True)
    img.save(out, optimize=True)
    print("wrote", out, f"{os.path.getsize(out) / 1024:.0f} KB")


def build_gif(frames_dir, out, scheme, width, bare, flat=True, colors=256):
    names = sorted(f for f in os.listdir(frames_dir) if f.endswith(".png"))
    durations = json.load(open(os.path.join(frames_dir, "frames.json")))
    imgs = [frame_image(os.path.join(frames_dir, n), scheme, bare, flat, width).convert("RGB") for n in names]
    # One palette for the whole clip (sampled from several frames) so colours never flicker.
    step = max(1, len(imgs) // 8)
    sample = imgs[::step][:8]
    w, h = imgs[0].size
    sheet = Image.new("RGB", (w, h * len(sample)))
    for i, im in enumerate(sample):
        sheet.paste(im, (0, i * h))
    pal = sheet.quantize(colors=colors, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE)
    q = [im.quantize(palette=pal, dither=Image.Dither.NONE) for im in imgs]
    os.makedirs(os.path.dirname(out) or ".", exist_ok=True)
    q[0].save(out, save_all=True, append_images=q[1:], duration=durations, loop=0, optimize=False, disposal=1)
    print("wrote", out, f"{len(q)} frames {os.path.getsize(out) / 1024 / 1024:.2f} MB")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("mode", choices=["still", "gif", "poster"])
    ap.add_argument("src")
    ap.add_argument("out")
    ap.add_argument("--scheme", default="light", choices=["light", "dark"])
    ap.add_argument("--bare", action="store_true")
    ap.add_argument("--width", type=int, default=None)
    ap.add_argument("--index", type=int, default=-1)
    ap.add_argument("--colors", type=int, default=256)
    ap.add_argument("--title", default="xpando")
    ap.add_argument("--noicon", action="store_true")
    ap.add_argument("--frameless", action="store_true")
    a = ap.parse_args()
    TITLE["text"], TITLE["icon"], TITLE["frameless"] = a.title, not a.noicon, a.frameless
    if a.mode == "still":
        save_png(frame_image(a.src, a.scheme, a.bare, False, a.width), a.out)
    elif a.mode == "poster":
        names = sorted(f for f in os.listdir(a.src) if f.endswith(".png"))
        save_png(frame_image(os.path.join(a.src, names[a.index]), a.scheme, a.bare, False, a.width), a.out)
    else:
        build_gif(a.src, a.out, a.scheme, a.width, a.bare, colors=a.colors)


if __name__ == "__main__":
    main()
