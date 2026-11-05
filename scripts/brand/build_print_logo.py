"""Builds a print kit (vector SVG + vector PDF + large PNG) from the spa's only logo file.

    pip install pillow numpy scipy potracer
    python scripts/brand/build_print_logo.py [source.png] [out_dir]

The source is a 1024² PNG whose line art lives in the alpha channel (see build_logo_assets.py). For
print we upsample the alpha 4x with a smooth filter before thresholding, so potrace follows the
intended curve instead of the 1024-px pixel staircase, then trace to Bézier paths. The result is
true vector: SVG and PDF scale to any size (business card to banner). Each file holds the WHOLE logo,
unaltered (ring, lotus, wordmark, yin-yang, leaves, curved tagline), in four colourways.

Fidelity is bounded by the source: fine details smaller than ~1 source pixel may be softened.
A designer's redraw is the only way past that; this kit is the faithful best from the PNG.
"""

import os
import sys

import numpy as np
import potrace
from PIL import Image, ImageDraw, ImageFilter

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
SOURCE = sys.argv[1] if len(sys.argv) > 1 else os.path.join(
    ROOT, "docs for reference", "logo Viên phúc y đường transparent.png"
)
OUT = sys.argv[2] if len(sys.argv) > 2 else os.path.join(ROOT, "docs for reference", "logo-in-an")
SCALE = 4

COLOURWAYS = {
    "vang": "#D9A13A",  # gold for print on dark/maroon or cream stock
    "nau-do": "#7A2620",  # brand maroon (tokens.css --primary)
    "den": "#000000",  # one-colour black (stamps, fax, engraving)
    "trang": "#FFFFFF",  # white, for maroon or photo backgrounds
}


def trace(path):
    alpha = Image.open(path).convert("RGBA").getchannel("A")
    big = alpha.resize((alpha.width * SCALE, alpha.height * SCALE), Image.Resampling.LANCZOS)
    # The source edge carries ~1 px of noise; a blur of about one source pixel removes the wobble
    # without moving the 50 % contour that defines the strokes.
    big = big.filter(ImageFilter.GaussianBlur(radius=SCALE * 0.9))
    ink = np.array(big) > 127
    # potrace traces the False pixels of the bitmap it is given, hence the inversion.
    curves = potrace.Bitmap(~ink).trace(
        turdsize=4 * SCALE * SCALE,
        turnpolicy=potrace.POTRACE_TURNPOLICY_MINORITY,
        alphamax=1.1,
        opticurve=True,
        opttolerance=0.2,
    )
    return curves, big.size


def segments(curves, k):
    """Yields path ops in units scaled by k: ('M', x, y) / ('L', x, y) / ('C', x1, y1, x2, y2, x, y) / ('Z',)."""
    for curve in curves:
        p = curve.start_point
        yield ("M", p.x * k, p.y * k)
        for seg in curve.segments:
            if seg.is_corner:
                yield ("L", seg.c.x * k, seg.c.y * k)
                yield ("L", seg.end_point.x * k, seg.end_point.y * k)
            else:
                yield ("C", seg.c1.x * k, seg.c1.y * k, seg.c2.x * k, seg.c2.y * k,
                       seg.end_point.x * k, seg.end_point.y * k)
        yield ("Z",)


def fmt(v):
    return ("%.2f" % v).rstrip("0").rstrip(".")


def write_svg(curves, size, colour, path):
    w, h = size
    d = []
    for op in segments(curves, 1):
        d.append(op[0] + " ".join(fmt(v) for v in op[1:]))
    with open(path, "w", encoding="utf-8") as f:
        f.write(
            '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 %d %d" width="%d" height="%d">'
            '<title>Viên Phúc Y Đường</title>'
            '<path fill="%s" fill-rule="evenodd" d="%s"/></svg>\n' % (w, h, w // SCALE, h // SCALE, colour, "".join(d))
        )


def write_pdf(curves, size, colour, path, side_mm=200.0):
    """A one-page vector PDF, `side_mm` wide (scales freely in any print tool)."""
    w, h = size
    pt = side_mm / 25.4 * 72
    k = pt / w
    r, g, b = (int(colour[i:i + 2], 16) / 255 for i in (1, 3, 5))
    ops = ["%s %s %s rg" % (fmt(r), fmt(g), fmt(b))]
    for op in segments(curves, k):
        # PDF's y axis points up.
        if op[0] == "M":
            ops.append("%s %s m" % (fmt(op[1]), fmt(pt - op[2])))
        elif op[0] == "L":
            ops.append("%s %s l" % (fmt(op[1]), fmt(pt - op[2])))
        elif op[0] == "C":
            ops.append("%s %s %s %s %s %s c" % (fmt(op[1]), fmt(pt - op[2]), fmt(op[3]), fmt(pt - op[4]),
                                                 fmt(op[5]), fmt(pt - op[6])))
        else:
            ops.append("h")
    ops.append("f*")
    content = "\n".join(ops).encode("ascii")
    objs = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        ("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 %s %s] /Contents 4 0 R >>" % (fmt(pt), fmt(pt))).encode(),
        b"<< /Length %d >>\nstream\n" % len(content) + content + b"\nendstream",
        b"<< /Title (Vien Phuc Y Duong logo) /Producer (build_print_logo.py) >>",
    ]
    out = bytearray(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n")
    offsets = []
    for i, body in enumerate(objs, 1):
        offsets.append(len(out))
        out += b"%d 0 obj\n" % i + body + b"\nendobj\n"
    xref = len(out)
    out += b"xref\n0 %d\n0000000000 65535 f \n" % (len(objs) + 1)
    for o in offsets:
        out += b"%010d 00000 n \n" % o
    out += b"trailer\n<< /Size %d /Root 1 0 R /Info 5 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (len(objs) + 1, xref)
    with open(path, "wb") as f:
        f.write(out)


def flatten(curves, k, steps=24):
    """Polygons (one per curve) from the traced Béziers, scaled by k."""
    polys = []
    for curve in curves:
        pts = [(curve.start_point.x * k, curve.start_point.y * k)]
        for seg in curve.segments:
            if seg.is_corner:
                pts += [(seg.c.x * k, seg.c.y * k), (seg.end_point.x * k, seg.end_point.y * k)]
            else:
                x0, y0 = pts[-1]
                for i in range(1, steps + 1):
                    t = i / steps
                    u = 1 - t
                    x = u**3 * x0 + 3 * u * u * t * seg.c1.x * k + 3 * u * t * t * seg.c2.x * k + t**3 * seg.end_point.x * k
                    y = u**3 * y0 + 3 * u * u * t * seg.c1.y * k + 3 * u * t * t * seg.c2.y * k + t**3 * seg.end_point.y * k
                    pts.append((x, y))
        polys.append(pts)
    return polys


def render_mask(curves, size, px=4096, ss=2):
    """Even-odd raster of the vector, supersampled for antialiasing, so the PNG matches the SVG/PDF."""
    big = px * ss
    acc = np.zeros((big, big), np.bool_)
    for poly in flatten(curves, big / size[0]):
        m = Image.new("1", (big, big), 0)
        ImageDraw.Draw(m).polygon(poly, fill=1)
        acc ^= np.array(m, np.bool_)
    return Image.fromarray(acc.astype(np.uint8) * 255, "L").resize((px, px), Image.Resampling.BOX)


def write_png(mask, colour, path):
    """Large transparent PNG for tools that cannot take vectors (Canva, Zalo, Word)."""
    px = mask.width
    rgb = tuple(int(colour[i:i + 2], 16) for i in (1, 3, 5))
    img = np.zeros((px, px, 4), np.uint8)
    img[..., 0], img[..., 1], img[..., 2] = rgb
    img[..., 3] = np.array(mask)
    Image.fromarray(img, "RGBA").save(path, optimize=True)


def main():
    os.makedirs(OUT, exist_ok=True)
    curves, size = trace(SOURCE)
    mask = render_mask(curves, size)
    for name, colour in COLOURWAYS.items():
        base = os.path.join(OUT, "logo-vien-phuc-y-duong-" + name)
        write_svg(curves, size, colour, base + ".svg")
        write_pdf(curves, size, colour, base + ".pdf")
        write_png(mask, colour, base + "-4096.png")
        print("wrote", base + ".{svg,pdf}", "+ -4096.png")


if __name__ == "__main__":
    main()
