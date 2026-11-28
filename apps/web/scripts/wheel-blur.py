"""Make the lucky draw wheel's rotational motion blur (design/lucky-draw.md §3).

The disk art (src/features/lucky-draw/assets/wheel-disk.webp) averaged over N copies rotated
about its centre across SPREAD degrees, in linear light so the gold highlights smear the way a
camera would see them. The result, wheel-disk-blur.webp, sits on the turning disk and fades out
as the wheel slows (lucky-draw.css `ld-wheel-blur`): what the eye expects of a fast wheel instead of
a sharp disk jumping ~12 degrees a frame.

Run from apps/web (Pillow + NumPy):  python scripts/wheel-blur.py
"""

from pathlib import Path

import numpy as np
from PIL import Image

ASSETS = Path(__file__).resolve().parent.parent / "src" / "features" / "lucky-draw" / "assets"
SOURCE = ASSETS / "wheel-disk.webp"
TARGET = ASSETS / "wheel-disk-blur.webp"
# About one frame of the fastest spin at 60 Hz (angles.ts `SPIN`: ~12 degrees a frame at its peak).
SPREAD = 10.0
COPIES = 41
QUALITY = 80


def main() -> None:
    disk = Image.open(SOURCE).convert("RGB")
    linear = np.zeros((disk.height, disk.width, 3))
    for i in range(COPIES):
        angle = -SPREAD / 2 + SPREAD * i / (COPIES - 1)
        turned = disk.rotate(angle, resample=Image.Resampling.BICUBIC, fillcolor=disk.getpixel((0, 0)))
        linear += (np.asarray(turned, dtype=np.float64) / 255.0) ** 2.2
    linear /= COPIES
    srgb = np.clip(linear ** (1 / 2.2) * 255.0 + 0.5, 0, 255).astype(np.uint8)
    Image.fromarray(srgb, "RGB").save(TARGET, "WEBP", quality=QUALITY, method=6)
    print(f"{TARGET.name}: {disk.width}x{disk.height}, {TARGET.stat().st_size // 1024} KB")


if __name__ == "__main__":
    main()
