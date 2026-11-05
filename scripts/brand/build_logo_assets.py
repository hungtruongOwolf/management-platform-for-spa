"""Builds the web logo assets from the spa's real logo.

    pip install pillow numpy scipy potracer
    python scripts/brand/build_logo_assets.py

Source: `docs for reference/logo Viên phúc y đường transparent.png` (1024², not in git). The artwork
lives in its alpha channel; the "glow" is only the RGB of fully transparent pixels. So `alpha > 127`
is the exact line art: it is traced with potrace (no redrawing), then split into parts by connected
component. The logo is always used WHOLE (EM 2026-11-05 round 3: never strip or recombine
parts): lotus, ring, script wordmark, yin-yang with leaves and the curved tagline. Outputs:

- apps/web/src/assets/brand/emblem.svg — the whole logo, black, used as a CSS mask so the app
  recolours it with tokens (gold on the maroon chrome, maroon on cream).
- apps/web/public/ apple-touch-icon.png, icons/icon-*.png (PWA) — the whole logo in gold on the
  chrome maroon.
- apps/web/public/ favicon.svg, favicon.ico (16/32/48), favicon-16.png, favicon-32.png — the
  browser tab only: the simplified MARK, ring + lotus (EM 2026-11-05, round-4 answers: "dùng bản rút
  gọn (hoa sen + vòng tròn) chỉ cho icon tab, còn mọi chỗ khác giữ logo đầy đủ"). The two parts are
  the logo's own traced components (the largest = ring, the next = lotus); the lotus is scaled
  and centred in the ring so it reads at 16 px. Nothing is redrawn.

Tab-size legibility: the line art is ~4 source px thick, so at 16-48 px it fades into the maroon.
Small icons fill the tile almost edge to edge and thicken the strokes first (a max filter on the
alpha in the PNGs, a same-colour stroke in the SVG). Bump ICON_VERSION when the art changes:
browsers cache favicons hard, and index.html links them with `?v=ICON_VERSION`.
"""

import os
import sys

import numpy as np
import potrace
from PIL import Image, ImageDraw, ImageFilter
from scipy import ndimage

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
# Cache-busting version in apps/web/index.html icon links (`?v=`); bump when the art changes.
ICON_VERSION = 5
# Stroke growth (source px, odd) per icon size: enough to survive downsampling, not enough to blob.
GROW = {180: 5}
# Same for the tab mark (its source is the ~800 px ring, so it shrinks more than the logo).
MARK_GROW = {16: 31, 32: 21, 48: 15}
# Lotus size inside the ring, relative to its size in the logo.
MARK_LOTUS_SCALE = 1.55
# SVG favicon stroke (source px): Chrome rasterises it at 16/32 px in the tab.
SVG_STROKE = 14


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


def mark(ink):
    """Ring + lotus of the logo on a square canvas, the lotus scaled and centred in the ring."""
    lab, n = ndimage.label(ink)
    areas = ndimage.sum(ink, lab, range(1, n + 1))
    ring_id, lotus_id = (np.argsort(-areas) + 1)[:2]
    ring, lotus = lab == ring_id, lab == lotus_id
    ys, xs = np.nonzero(ring)
    side = int(max(ys.max() - ys.min(), xs.max() - xs.min())) + 1
    out = Image.new("L", (side, side), 0)
    out.paste(Image.fromarray((ring[ys.min() : ys.min() + side, xs.min() : xs.min() + side] * 255).astype(np.uint8)), (0, 0))
    ly, lx = np.nonzero(lotus)
    petals = Image.fromarray((lotus[ly.min() : ly.max() + 1, lx.min() : lx.max() + 1] * 255).astype(np.uint8))
    petals = petals.resize((int(petals.width * MARK_LOTUS_SCALE), int(petals.height * MARK_LOTUS_SCALE)), Image.LANCZOS)
    out.paste(petals, ((side - petals.width) // 2, (side - petals.height) // 2), petals)
    # A clear border, so the tracer (and its bbox padding) never starts at a negative index.
    return np.pad(np.array(out) > 127, 8)


def tile(logo, size, logo_frac, radius_frac, grow_by=None):
    """The art in gold on a maroon (optionally rounded) square, strokes grown for small sizes."""
    grow = (grow_by or GROW).get(size, 1)
    if grow > 1:
        logo = logo.filter(ImageFilter.MaxFilter(grow))
    img = Image.new("RGBA", (size, size), MAROON + (255,))
    if radius_frac:
        big = Image.new("L", (size * 4, size * 4), 0)
        ImageDraw.Draw(big).rounded_rectangle((0, 0, size * 4 - 1, size * 4 - 1), radius=int(size * 4 * radius_frac), fill=255)
        img.putalpha(big.resize((size, size), Image.LANCZOS))
    m = int(round(size * logo_frac))
    gold = Image.new("RGBA", (m, m), GOLD + (255,))
    gold.putalpha(logo.resize((m, m), Image.LANCZOS))
    img.alpha_composite(gold, ((size - m) // 2, (size - m) // 2))
    return img


def png_icon(logo, size, logo_frac, radius_frac, name, grow_by=None):
    img = tile(logo, size, logo_frac, radius_frac, grow_by)
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

    write_svg("emblem.svg", ink, 6)

    # Tab favicon: the ring + lotus mark in gold on a rounded maroon tile (colours baked in: a
    # favicon cannot use tokens), 6 % margin.
    tab = mark(ink)
    w, h, d = to_svg(tab, bbox(tab, 2))
    side = max(w, h) * 1.06
    fav = (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {s} {s}">'
        '<rect width="{s}" height="{s}" rx="{r}" fill="#5a1d18"/>'
        '<path fill="#e3b455" stroke="#e3b455" stroke-width="{sw}" stroke-linejoin="round" '
        'transform="translate({tx} {ty})" d="{d}"/></svg>\n'
    ).format(s=num(side), r=num(side * 0.18), sw=SVG_STROKE, tx=num((side - w) / 2), ty=num((side - h) / 2), d=d)
    with open(os.path.join(PUBLIC, "favicon.svg"), "w", encoding="utf-8", newline="\n") as fh:
        fh.write(fav)
    print("favicon.svg", os.path.getsize(os.path.join(PUBLIC, "favicon.svg")))

    logo = square(np.where(ink, alpha, 0).astype(np.uint8))
    png_icon(logo, 192, 0.8, 0, "icons/icon-192.png")
    png_icon(logo, 512, 0.8, 0, "icons/icon-512.png")
    # Maskable: launchers crop to a circle of 80% diameter, so the art stays inside ~62%.
    png_icon(logo, 512, 0.62, 0, "icons/icon-maskable-512.png")
    png_icon(logo, 180, 0.78, 0, "apple-touch-icon.png")
    tab_img = Image.fromarray((tab * 255).astype(np.uint8))
    png_icon(tab_img, 32, 0.94, 0.18, "favicon-32.png", MARK_GROW)
    png_icon(tab_img, 16, 0.94, 0.18, "favicon-16.png", MARK_GROW)
    # favicon.ico: what Windows, bookmarks and old tabs ask for at /favicon.ico (16, 32, 48 inside).
    icons = [tile(tab_img, n, 0.94, 0.18, MARK_GROW) for n in (16, 32, 48)]
    ico = os.path.join(PUBLIC, "favicon.ico")
    icons[2].save(ico, format="ICO", sizes=[(16, 16), (32, 32), (48, 48)], append_images=icons[:2])
    print("favicon.ico", os.path.getsize(ico))


if __name__ == "__main__":
    main()
