#!/usr/bin/env python3
"""Build every icon from assets/ClaudeBascule.png (black glyph on transparency):
the glyph in white, centred in a squircle of the default accent (coral).

    python3 tools/make-icons.py

Writes app/icons/* (Tauri, Windows .ico, macOS .icns), app/ui/logo.svg and app/ui/squircle.svg
(the in-app mark, coloured by CSS), assets/icon.svg (Linux launcher, any size) and assets/icon-256.png.
assets/logo.svg is the glyph traced from the PNG (vector, crisp at every size).
"""
import math
from pathlib import Path
from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
CORAL = (224, 83, 45, 255)  # #e0532d, the default accent (app.js ACCENTS.coral)
N = 3.0  # |x|^3 + |y|^3 = 1: a full squircle


def squircle_points(size, margin=0.0, steps=720):
    r = size / 2 - margin
    c = size / 2
    pts = []
    for i in range(steps):
        t = 2 * math.pi * i / steps
        ct, st = math.cos(t), math.sin(t)
        x = math.copysign(abs(ct) ** (2 / N), ct)
        y = math.copysign(abs(st) ** (2 / N), st)
        pts.append((c + r * x, c + r * y))
    return pts


def glyph_mask():
    """The glyph's alpha, trimmed to its box."""
    src = Image.open(ROOT / "assets/ClaudeBascule.png").convert("RGBA")
    a = src.getchannel("A")
    return a.crop(a.getbbox())


def icon(size, margin_ratio=0.0, glyph_ratio=0.70):
    ss = 4  # supersampling for smooth edges
    big = size * ss
    canvas = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    margin = big * margin_ratio
    ImageDraw.Draw(canvas).polygon(squircle_points(big, margin), fill=CORAL)
    g = glyph_mask()
    inner = big - 2 * margin
    h = int(inner * glyph_ratio)
    w = int(h * g.width / g.height)
    g = g.resize((w, h), Image.LANCZOS)
    white = Image.new("RGBA", (w, h), (255, 255, 255, 255))
    white.putalpha(g)
    canvas.alpha_composite(white, ((big - w) // 2, (big - h) // 2))
    return canvas.resize((size, size), Image.LANCZOS)


def main():
    icons = ROOT / "app/icons"
    icons.mkdir(parents=True, exist_ok=True)
    # Each size is drawn on its own (supersampled), small ones with a bigger glyph to stay legible.
    sized = lambda s: icon(s, margin_ratio=0.03, glyph_ratio=0.76 if s <= 32 else 0.70)
    sized(512).save(icons / "icon.png")
    for s in (32, 128):
        sized(s).save(icons / f"{s}x{s}.png")
    sized(256).save(icons / "128x128@2x.png")
    ico = [sized(s) for s in (256, 128, 64, 48, 32, 24, 16)]
    ico[0].save(icons / "icon.ico", sizes=[im.size for im in ico], append_images=ico[1:])
    # macOS: the shape sits inside Apple's 824/1024 grid.
    icon(1024, margin_ratio=0.0977).save(icons / "icon.icns")
    # README and release assets.
    assets = ROOT / "assets"
    sized(256).save(assets / "icon-256.png")
    # Vector: the squircle, the glyph (app mark, coloured by CSS) and the whole icon.
    pts = squircle_points(100, 0, 360)
    d = "M" + " L".join(f"{x:.2f} {y:.2f}" for x, y in pts) + " Z"
    (ROOT / "app/ui/squircle.svg").write_text(f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><path d="{d}"/></svg>\n')
    logo = (ROOT / "assets/logo.svg").read_text()
    (ROOT / "app/ui/logo.svg").write_text(logo)
    glyph = logo[logo.index("<path"):logo.index("/>", logo.index("<path")) + 2].replace("<path", '<path fill="#fff"', 1)
    pts = squircle_points(100, 3, 360)
    d3 = "M" + " L".join(f"{x:.2f} {y:.2f}" for x, y in pts) + " Z"
    k = 0.70 * 94 / 100
    o = (100 - 100 * k) / 2
    (ROOT / "assets/icon.svg").write_text(
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><path fill="#e0532d" d="{d3}"/>'
        f'<g transform="translate({o:.3f} {o:.3f}) scale({k:.4f})">{glyph}</g></svg>\n')


if __name__ == "__main__":
    main()
