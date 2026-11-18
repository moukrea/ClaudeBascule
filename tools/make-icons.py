#!/usr/bin/env python3
"""Build every icon from assets/ClaudeBascule.png (black glyph on transparency):
the glyph in white, centred in a squircle of the default accent (coral).

    python3 tools/make-icons.py

Writes app/icons/* (Tauri, Windows .ico, macOS .icns), app/ui/logo-mask.png and
app/ui/squircle.svg (the in-app mark, coloured by CSS), and assets/icon-*.png.
"""
import math
from pathlib import Path
from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
CORAL = (224, 83, 45, 255)  # #e0532d, the default accent (app.js ACCENTS.coral)
N = 5.0  # superellipse exponent: a squircle


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


def icon(size, margin_ratio=0.0, glyph_ratio=0.60):
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
    full = icon(1024, margin_ratio=0.03)
    full.resize((512, 512), Image.LANCZOS).save(icons / "icon.png")
    for s in (32, 128):
        full.resize((s, s), Image.LANCZOS).save(icons / f"{s}x{s}.png")
    full.resize((256, 256), Image.LANCZOS).save(icons / "128x128@2x.png")
    full.save(icons / "icon.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
    # macOS: the shape sits inside Apple's 824/1024 grid.
    icon(1024, margin_ratio=0.0977).save(icons / "icon.icns")
    # README and release assets.
    assets = ROOT / "assets"
    full.resize((256, 256), Image.LANCZOS).save(assets / "icon-256.png")
    # In-app mark: the glyph as a mask (CSS colours it), and the squircle shape.
    g = glyph_mask()
    side = max(g.width, g.height)
    sq = Image.new("RGBA", (side, side), (0, 0, 0, 0))
    white = Image.new("RGBA", g.size, (255, 255, 255, 255))
    white.putalpha(g)
    sq.alpha_composite(white, ((side - g.width) // 2, (side - g.height) // 2))
    sq.resize((256, 256), Image.LANCZOS).save(ROOT / "app/ui/logo-mask.png", optimize=True)
    pts = squircle_points(100, 0, 360)
    d = "M" + " L".join(f"{x:.2f} {y:.2f}" for x, y in pts) + " Z"
    (ROOT / "app/ui/squircle.svg").write_text(f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><path d="{d}"/></svg>\n')


if __name__ == "__main__":
    main()
