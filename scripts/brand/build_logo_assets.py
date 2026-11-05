"""Builds the web logo assets from the spa's real logo.

    pip install pillow numpy potracer
    python scripts/brand/build_logo_assets.py

Source: `docs for reference/logo Viên phúc y đường transparent.png` (1024², not in git). The artwork
lives in its alpha channel; the "glow" is only the RGB of fully transparent pixels. So `alpha > 127`
is the exact line art: it is traced with potrace (no redrawing), then split into parts by connected
component. The logo is always used WHOLE (EM 2026-11-05 round 3: never strip or recombine
parts): lotus, ring, script wordmark, yin-yang with leaves and the curved tagline. Outputs:

- apps/web/src/assets/brand/emblem.svg — the whole logo, black, used as a CSS mask so the app
  recolours it with tokens (gold on the maroon chrome, maroon on cream).
- apps/web/public/ favicon.svg, favicon-32.png, apple-touch-icon.png, icons/icon-*.png — the whole
  logo in gold on the chrome maroon.
"""

import os
import sys

import numpy as np
import potrace
from PIL import Image, ImageDraw

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
LOGO = os.path.join("docs for reference", "logo Viên phúc y đường transparent.png")
# The reference docs sit in the main checkout only; an agent worktree is three levels below it.
SOURCE = sys.argv[1] if len(sys.argv) > 1 else next(
    (p for p in (os.path.join(ROOT, LOGO), os.path.join(ROOT, "..", "..", "..", LOGO)) if os.path.exists(p)),
    os.path.join(ROOT, LOGO),
)
SVG_OUT = os.path.join(ROOT, "apps", "web", "src", "assets", "brand")
PUBLIC = os.path.join(ROOT, "apps", "web", "public")

MAROON = (0x5A, 0x1D, 0x18)  # tokens.css --chrome-bg (light)
GOLD = (0xE3, 0xB4, 0x55)  # tokens.css --chrome-accent


def num(v):
    return ("%.1f" % v).rstrip("0").rstrip(".")


def to_svg(ink, box):
    x0, y0, x1, y1 = box
    # potrace traces the False pixels of the bitmap it is given, hence the inversion.
    curves = potrace.Bitmap(~ink[y0:y1, x0:x1]).trace(
        turdsize=4,
        turnpolicy=potrace.POTRACE_TURNPOLICY_MINORITY,
        alphamax=1.0,
        opticurve=True,
        opttolerance=0.4,
    )
    d = []
    for curve in curves:
        d.append("M%s %s" % (num(curve.start_point.x), num(curve.start_point.y)))
        for seg in curve.segments:
            if seg.is_corner:
                d.append("L%s %s %s %s" % (num(seg.c.x), num(seg.c.y), num(seg.end_point.x), num(seg.end_point.y)))
            else:
                d.append(
                    "C%s %s %s %s %s %s"
                    % (num(seg.c1.x), num(seg.c1.y), num(seg.c2.x), num(seg.c2.y), num(seg.end_point.x), num(seg.end_point.y))
                )
        d.append("Z")
    return x1 - x0, y1 - y0, "".join(d)


def bbox(ink, pad):
    ys, xs = np.nonzero(ink)
    return (xs.min() - pad, ys.min() - pad, xs.max() + 1 + pad, ys.max() + 1 + pad)


def write_svg(name, ink, pad):
    w, h, d = to_svg(ink, bbox(ink, pad))
    path = os.path.join(SVG_OUT, name)
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 %d %d"><path d="%s"/></svg>\n' % (w, h, d))
    print(name, os.path.getsize(path))
    return w, h, d


def square(alpha):
    ys, xs = np.nonzero(alpha)
    crop = alpha[ys.min() : ys.max() + 1, xs.min() : xs.max() + 1]
    side = max(crop.shape)
    sq = np.zeros((side, side), np.uint8)
    oy, ox = (side - crop.shape[0]) // 2, (side - crop.shape[1]) // 2
    sq[oy : oy + crop.shape[0], ox : ox + crop.shape[1]] = crop
    return Image.fromarray(sq, "L")


def png_icon(logo, size, logo_frac, radius_frac, name):
    img = Image.new("RGBA", (size, size), MAROON + (255,))
    if radius_frac:
        big = Image.new("L", (size * 4, size * 4), 0)
        ImageDraw.Draw(big).rounded_rectangle((0, 0, size * 4 - 1, size * 4 - 1), radius=int(size * 4 * radius_frac), fill=255)
        img.putalpha(big.resize((size, size), Image.LANCZOS))
    m = int(round(size * logo_frac))
    gold = Image.new("RGBA", (m, m), GOLD + (255,))
    gold.putalpha(logo.resize((m, m), Image.LANCZOS))
    img.alpha_composite(gold, ((size - m) // 2, (size - m) // 2))
    path = os.path.join(PUBLIC, name)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    # A small palette: flat two-colour art, ~10x smaller than RGBA.
    img.quantize(48, method=Image.FASTOCTREE, dither=Image.NONE).save(path, optimize=True)
    print(name, os.path.getsize(path))


def main():
    alpha = np.array(Image.open(SOURCE).convert("RGBA"))[..., 3]
    ink = alpha > 127
    os.makedirs(SVG_OUT, exist_ok=True)
    os.makedirs(PUBLIC, exist_ok=True)

    w, h, d = write_svg("emblem.svg", ink, 6)

    # Favicon: the whole logo in gold on a rounded maroon tile (colours baked in: a favicon cannot use tokens).
    side = max(w, h) * 1.16
    fav = (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {s} {s}">'
        '<rect width="{s}" height="{s}" rx="{r}" fill="#5a1d18"/>'
        '<path fill="#e3b455" transform="translate({tx} {ty})" d="{d}"/></svg>\n'
    ).format(s=num(side), r=num(side * 0.2), tx=num((side - w) / 2), ty=num((side - h) / 2), d=d)
    with open(os.path.join(PUBLIC, "favicon.svg"), "w", encoding="utf-8", newline="\n") as fh:
        fh.write(fav)
    print("favicon.svg", os.path.getsize(os.path.join(PUBLIC, "favicon.svg")))

    logo = square(np.where(ink, alpha, 0).astype(np.uint8))
    png_icon(logo, 192, 0.8, 0, "icons/icon-192.png")
    png_icon(logo, 512, 0.8, 0, "icons/icon-512.png")
    # Maskable: launchers crop to a circle of 80% diameter, so the art stays inside ~62%.
    png_icon(logo, 512, 0.62, 0, "icons/icon-maskable-512.png")
    png_icon(logo, 180, 0.78, 0, "apple-touch-icon.png")
    png_icon(logo, 32, 0.94, 0.18, "favicon-32.png")


if __name__ == "__main__":
    main()
