#!/usr/bin/env python3
"""FreeDF app icon generator (Pillow).

Renders the application logo for FreeDF — "Lightweight PDF Viewer & Ink" —
as a square tile in the app's **Nord** palette:

  * Dark polar-night gradient rounded tile (#2E3440 -> #434C5E)
  * A light "document page" with a dog-ear (folded) corner  -> PDF
  * A frost-blue pen stroke sweeping across the page        -> handwriting / ink

Output (written to crates/freedf/assets/icon/):
  * app_icon_1024.png  master render
  * app_icon.png       512 px (embedded at build time via include_bytes!)
  * app_icon_%.png     16/24/32/48/64/128/256 px (PNG sizes)
  * app_icon.ico       multi-size Windows icon
     (Windows 빌드 시 build.rs → win/app.rc 가 이 .ico 를 .exe 아이콘 리소스로
      임베드 — 바탕화면/탐색기/작업 관리자 아이콘)

Usage:
    python3 scripts/design_icon.py
"""

import math
import os
import sys

from PIL import Image, ImageDraw

# --------------------------------------------------------------------------
# Nord palette (matches crates/freedf/src/theme/tokens.rs)
# --------------------------------------------------------------------------
NORD2 = (0x43, 0x4C, 0x5E)  # lighter panel
NORD0 = (0x2E, 0x34, 0x40)  # darkest background
NORD6 = (0xEC, 0xEF, 0xF4)  # strong text / snow storm
NORD8 = (0x88, 0xC0, 0xD0)  # frost-blue accent (link)

OUT_DIR = os.path.normpath(
    os.path.join(os.path.dirname(__file__), "..", "crates", "freedf", "assets", "icon")
)


def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))


def lerp_alpha(c, alpha):
    return (c[0], c[1], c[2], int(255 * alpha))

# --------------------------------------------------------------------------
# Geometry helpers (draw in a 1.0 unit square, scaled by S)
# --------------------------------------------------------------------------
class Bezier:
    def __init__(self, pts):
        self.pts = [(float(x), float(y)) for x, y in pts]

    def point(self, t):
        p = self.pts
        n = len(p) - 1
        # de Casteljau
        q = list(p)
        while len(q) > 1:
            q = [
                (q[i][0] * (1 - t) + q[i + 1][0] * t, q[i][1] * (1 - t) + q[i + 1][1] * t)
                for i in range(len(q) - 1)
            ]
        return q[0]


def draw_thick_curve(draw, pts, width, color, steps=64):
    """Draw a smooth curve as overlapping circles -> round joints + caps."""
    c = Bezier(pts)
    for i in range(steps + 1):
        x, y = c.point(i / steps)
        r = width / 2.0
        draw.ellipse((x - r, y - r, x + r, y + r), fill=color)


def rounded_rect_path(x, y, w, h, r):
    """Rectangle with all corners rounded; returns polygon points."""
    pts = []
    for cx, cy, a0, a1 in [
        (x + r, y + r, 180, 270),
        (x + w - r, y + r, 270, 360),
        (x + w - r, y + h - r, 0, 90),
        (x + r, y + h - r, 90, 180),
    ]:
        for i in range(0, 91):
            a = math.radians(a0 + (a1 - a0) * i / 90.0)
            pts.append((cx + r * math.cos(a), cy + r * math.sin(a)))
    return pts



def render_master(S):
    """Render the icon at resolution S x S."""
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    # -- 1. Background: rounded tile with vertical-ish gradient -----------
    tile_r = 0.20 * S
    tile_poly = rounded_rect_path(0, 0, S, S, tile_r)
    tile_mask = Image.new("L", (S, S), 0)
    ImageDraw.Draw(tile_mask).polygon(tile_poly, fill=255)
    band = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    bd = ImageDraw.Draw(band)
    top = lerp(NORD2, NORD0, 0.05)
    bottom = lerp(NORD0, NORD2, 0.90)
    for y in range(S):
        bd.line([(0, y), (S, y)], fill=lerp(top, bottom, y / S))
    # subtle brightening toward the upper-left
    overlay = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    ovd = ImageDraw.Draw(overlay)
    cx, cy = 0.32 * S, 0.30 * S
    for i in range(40, 0, -12):
        r = i * S * 1.6
        ovd.ellipse((cx - r, cy - r, cx + r, cy + r), fill=(255, 255, 255, 10))
    band = Image.alpha_composite(band, overlay)
    bg = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    bg.paste(band, (0, 0), tile_mask)
    img = Image.alpha_composite(img, bg)

    # -- 2. Document page (dog-ear, slightly rotated) ----------------------
    pw, ph = 0.60 * S, 0.68 * S
    px0, py0 = 0.5 * (S - pw), 0.5 * (S - ph) - 0.02 * S
    angle = math.radians(-7.0)
    ca, sa = math.cos(angle), math.sin(angle)
    cx0, cy0 = S / 2.0, py0 + ph / 2.0

    def rot(x, y):
        dx, dy = x - cx0, y - cy0
        return (cx0 + dx * ca - dy * sa, cy0 + dx * sa + dy * ca)

    page_r = 0.045 * S
    page_pts = [
        (px0 + page_r, py0),
        (px0 + pw - page_r, py0),
        (px0 + pw, py0 + page_r),
        (px0 + pw, py0 + ph - page_r),
        (px0 + pw - page_r, py0 + ph),
        (px0 + page_r, py0 + ph),
        (px0, py0 + ph - page_r),
        (px0, py0 + page_r),
    ]
    rotated = [rot(x, y) for x, y in page_pts]
    # soft drop shadow under the page
    sp = ImageDraw.Draw(img)
    for i in range(6, 0, -1):
        sh = [(x, y + i * 0.006 * S) for x, y in rotated]
        sp.polygon(sh, fill=(0x1F, 0x24, 0x2E, 26 - i * 2))

    page_layer = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    pd = ImageDraw.Draw(page_layer)
    pd.polygon(rotated, fill=NORD6 + (255,))

    # dog-ear (folded top-right corner) -> shaded triangle over the page
    shd = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    fold = 0.18 * S
    ImageDraw.Draw(shd).polygon(
        [rot(px0 + pw - fold, py0 + fold), rot(px0 + pw - fold * 0.9, py0), rot(px0 + pw, py0 + fold * 0.6)],
        fill=(0xCF, 0xD5, 0xE0, 255),
    )
    page_layer = Image.alpha_composite(page_layer, shd)
    pd = ImageDraw.Draw(page_layer)
    pd.polygon(rotated + [rotated[0]], outline=(0xB8, 0xC0, 0xCC, 255), width=max(2, int(S * 0.004)))
    img = Image.alpha_composite(img, page_layer)

    # -- 3. Ink stroke (frost blue) sweeping across the page ---------------
    stroke = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    path = [(0.30, 0.56), (0.42, 0.40), (0.58, 0.30), (0.70, 0.20)]
    pts = [(x * S, y * S) for x, y in path]
    width = 0.052 * S
    glow = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    draw_thick_curve(ImageDraw.Draw(glow), pts, width * 2.2, lerp_alpha(NORD8, 90))
    stroke = Image.alpha_composite(stroke, glow)
    draw_thick_curve(ImageDraw.Draw(stroke), pts, width, NORD8 + (255,))
    sx, sy = pts[0]
    r_tip = width * 0.34
    ImageDraw.Draw(stroke).ellipse(
        (sx - r_tip, sy - r_tip, sx + r_tip, sy + r_tip), fill=lerp_alpha(NORD8, 235)
    )
    img = Image.alpha_composite(img, stroke)

    # -- 4. Thin frost-blue frame ring inset ------------------------------
    m = 0.045 * S
    ImageDraw.Draw(img).rounded_rectangle(
        (m, m, S - m, S - m),
        radius=0.16 * S,
        outline=lerp_alpha(NORD8, 190),
        width=max(2, int(S * 0.018)),
    )

    return img



def main():
    os.makedirs(OUT_DIR, exist_ok=True)

    master = render_master(1024)
    master.save(os.path.join(OUT_DIR, "app_icon_1024.png"))

    # Embedded PNG used at build time (good HiDPI taskbar size).
    render_master(512).save(os.path.join(OUT_DIR, "app_icon.png"))

    for s in [256, 128, 64, 48, 32, 24, 16]:
        render_master(s).save(os.path.join(OUT_DIR, f"app_icon_{s}.png"))

    # Multi-size Windows .ico
    icon_sizes = [256, 128, 64, 48, 32, 24, 16]
    images = [render_master(s).resize((s, s), Image.LANCZOS) for s in icon_sizes]
    images[0].save(
        os.path.join(OUT_DIR, "app_icon.ico"),
        format="ICO",
        sizes=[(s, s) for s in icon_sizes],
    )

    print("Generated icons in:", OUT_DIR)
    for name in sorted(os.listdir(OUT_DIR)):
        print("  -", name)


if __name__ == "__main__":
    sys.exit(main())
