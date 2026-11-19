#!/usr/bin/env python3
"""Generate the app icon in every format the packages need (requires Pillow).

    python3 tools/make_icons.py      # writes packaging/icons/*
"""
import math
import os

from PIL import Image, ImageDraw

OUT = os.path.join(os.path.dirname(__file__), "..", "packaging", "icons")
S = 1024  # master size; drawn at 4x and downsampled for smooth edges
SS = S * 4


def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))


def master():
    img = Image.new("RGBA", (SS, SS), (0, 0, 0, 0))
    # vertical gradient background
    grad = Image.new("RGBA", (SS, SS))
    top, bottom = (24, 92, 140), (10, 30, 64)
    gd = ImageDraw.Draw(grad)
    for y in range(SS):
        gd.line([(0, y), (SS, y)], fill=lerp(top, bottom, y / SS) + (255,))
    mask = Image.new("L", (SS, SS), 0)
    m = 0.06 * SS
    ImageDraw.Draw(mask).rounded_rectangle([m, m, SS - m, SS - m], radius=0.2 * SS, fill=255)
    img.paste(grad, (0, 0), mask)

    d = ImageDraw.Draw(img)
    # mesh nodes (fractions of the canvas)
    nodes = [(0.28, 0.66), (0.50, 0.40), (0.72, 0.62), (0.40, 0.80), (0.64, 0.82)]
    links = [(0, 1), (1, 2), (0, 3), (3, 4), (4, 2), (1, 3), (1, 4), (0, 4)]
    line = (120, 210, 255, 255)
    for a, b in links:
        (x1, y1), (x2, y2) = nodes[a], nodes[b]
        d.line([(x1 * SS, y1 * SS), (x2 * SS, y2 * SS)], fill=line, width=int(0.022 * SS))
    for i, (x, y) in enumerate(nodes):
        r = (0.075 if i == 1 else 0.055) * SS
        d.ellipse([x * SS - r, y * SS - r, x * SS + r, y * SS + r], fill=(255, 255, 255, 255))
        r2 = r * 0.5
        d.ellipse([x * SS - r2, y * SS - r2, x * SS + r2, y * SS + r2], fill=(40, 140, 210, 255))
    # radio waves above the central node
    cx, cy = 0.50 * SS, 0.40 * SS
    for k, rad in enumerate((0.13, 0.20, 0.27)):
        R = rad * SS
        alpha = 255 - k * 60
        d.arc([cx - R, cy - R, cx + R, cy + R], start=225, end=315, fill=(255, 255, 255, alpha), width=int(0.024 * SS))
    return img.resize((S, S), Image.LANCZOS)


def main():
    os.makedirs(OUT, exist_ok=True)
    big = master()
    for n in (16, 32, 48, 64, 128, 256, 512, 1024):
        big.resize((n, n), Image.LANCZOS).save(os.path.join(OUT, f"meshcore-{n}.png"), optimize=True)
    big.save(os.path.join(OUT, "meshcore.ico"), sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
    big.save(os.path.join(OUT, "meshcore.icns"))
    print("icons written to", os.path.abspath(OUT))


if __name__ == "__main__":
    main()
