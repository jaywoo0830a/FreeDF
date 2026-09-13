#!/usr/bin/env python3
"""FreeDF app icon generator (Pillow).

Renders the application logo for FreeDF — "Lightweight PDF Viewer & Ink" —
as a modern rounded square tile in the app's **Nord** palette:

  * Deep indigo → polar-night radial-gradient tile with a soft top-left glow
  * A softly shadowed "document page" floating on the tile (folded corner)
  * An elegant **calligraphic ink stroke** (varying width, fountain-pen tip)
    sweeping across the page  -> handwriting / ink

Every shape is drawn in a 1.0-unit space at 4x supersampling, then downscaled
with LANCZOS for crisp anti-aliased edges at every output size.

Output (written to crates/freedf/assets/icon/):
  * app_icon_1024.png  master render
  * app_icon.png       512 px (embedded at build time via include_bytes!)
  * app_icon_%.png     16/24/32/48/64/128/256 px (PNG sizes)
  * app_icon.ico       multi-size Windows icon
     (Windows 빌드 시 build.rs → win/app.rc 가 이 .ico 를 .exe 아이콘 리소스로
      임베드 — 바탕화면/탐색기/작업 관리자 아이콘)

Usage (격리된 파이썬 도커 컨테이너에서 실행 권장):
    bash scripts/generate_icons.sh
    # 로컬 실행:  python3 -m pip install Pillow && python3 scripts/design_icon.py
"""
import math
import os
import sys

from PIL import Image, ImageDraw

# --------------------------------------------------------------------------
# 펠레트 — 캡슐/토큰과 일치하는 Nord + 잉크(먹) 계열
# --------------------------------------------------------------------------
NORD2 = (0x43, 0x4C, 0x5E)  # lighter panel
NORD0 = (0x2E, 0x34, 0x40)  # darkest background
NORD6 = (0xEC, 0xEF, 0xF4)  # snow storm / strong text
NORD8 = (0x88, 0xC0, 0xD0)  # frost-blue accent

# 잉크 계열 — 흰 페이지 위 대비용 인디고
INK_DEEP = (0x25, 0x33, 0x4E)   # 스트로크 본체
INK_MID  = (0x2E, 0x3E, 0x5C)   # 글로우/그라디언트
INK_HL   = (0x3D, 0x54, 0x7E)   # 하이라이트

PAGE_BG   = (0xF8, 0xFA, 0xFC)  # 페이지 중심 (거의 순백)
PAGE_EDGE = (0xE6, 0xE9, 0xEF)  # 페이지 경계선
FOLD      = (0xC9, 0xD0, 0xDD)  # 접힌 모서리 음영
SHADOW    = (0x10, 0x14, 0x1A)  # 페이지 드롭 섀도우

SUPERSAMPLE = 4  # 안티앨리어싱용 초과샘플 배율

OUT_DIR = os.path.normpath(
    os.path.join(os.path.dirname(__file__), "..", "crates", "freedf", "assets", "icon")
)


# ── 색 보조 ───────────────────────────────────────────────────────────────────
def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))


def lerp_alpha(c, alpha):
    return (c[0], c[1], c[2], int(255 * alpha))


# ── 기하 보조 ─────────────────────────────────────────────────────────────────
class Bezier:
    """드 리카스텔죠(de Casteljau) 베지어 곡선."""
    def __init__(self, pts):
        self.pts = [(float(x), float(y)) for x, y in pts]

    def point(self, t):
        q = list(self.pts)
        while len(q) > 1:
            q = [
                (q[i][0] * (1 - t) + q[i + 1][0] * t,
                 q[i][1] * (1 - t) + q[i + 1][1] * t)
                for i in range(len(q) - 1)
            ]
        return q[0]

    def tangent(self, t):
        # 유한 차분으로 접선 근사 (변곡점에서도 안정)
        p0 = self.point(max(0.0, t - 1e-3))
        p1 = self.point(min(1.0, t + 1e-3))
        dx, dy = p1[0] - p0[0], p1[1] - p0[1]
        L = math.hypot(dx, dy) or 1.0
        return (dx / L, dy / L)


def rounded_rect_path(x, y, w, h, r):
    """모서리가 둥근 사각형 폴리곤 (왼-위부터 시계방향)."""
    r = min(r, w / 2.0, h / 2.0)
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

def calligraphic_stroke_polygon(bez, w0, w1, n=240):
    """중심 베지어의 폭이 w0→w1로 가늘어지는 칼리그래피 스트로크 폴리곤.

    곡선 양 측을 노멀 방향으로 w(t)/2 만큼 펼쳐 하나의 폐합 다각형으로 만든다
    → 뾰족한 펜 팁, 매끄러운 굵기 변화의 붓/펜 선.
    """
    left, right = [], []
    for i in range(n + 1):
        t = i / n
        cx, cy = bez.point(t)
        tx, ty = bez.tangent(t)
        nx, ny = -ty, tx  # 시계방향 노멀
        w = w0 + (w1 - w0) * t
        left.append((cx + nx * w, cy + ny * w))
        right.append((cx - nx * w, cy - ny * w))
    return left + list(reversed(right))


def rotate_pts(pts, cx, cy, angle):
    ca, sa = math.cos(angle), math.sin(angle)
    out = []
    for x, y in pts:
        dx, dy = x - cx, y - cy
        out.append((cx + dx * ca - dy * sa, cy + dx * sa + dy * ca))
    return out


# ── 렌더 ─────────────────────────────────────────────────────────────────────
def render_master(S):
    """해상도 S x S 로 아이콘 렌더. 내부는 SUPERSAMPLE로 그려 LANCZOS 다운샘플."""
    S_hi = S * SUPERSAMPLE
    img = Image.new("RGBA", (S_hi, S_hi), (0, 0, 0, 0))

    # -- 1. 배경: 인디고→폴라나잇 방사형 그라디언트 타일 + 서리 글로우 --------
    bg = Image.new("RGBA", (S_hi, S_hi), (0, 0, 0, 0))
    bgd = ImageDraw.Draw(bg)
    top = lerp(NORD2, NORD0, 0.10)
    bot = lerp(NORD0, (0x1C, 0x22, 0x2B), 0.85)
    for y in range(S_hi):
        bgd.line([(0, y), (S_hi, y)], fill=lerp(top, bot, y / S_hi))
    # 상단-왼쪽 서리(글로우) → 깊이감
    glow = Image.new("RGBA", (S_hi, S_hi), (0, 0, 0, 0))
    gd = ImageDraw.Draw(glow)
    gcx, gcy = 0.28 * S_hi, 0.28 * S_hi
    for i in range(90, 0, -6):
        r = i * S_hi * 0.022
        a = min(90, 12 + (90 - i) * 0.10)
        gd.ellipse((gcx - r, gcy - r, gcx + r, gcy + r), fill=lerp_alpha(NORD8, a))
    bg = Image.alpha_composite(bg, glow)
    # 라운드 타일 마스킹
    tile_mask = Image.new("L", (S_hi, S_hi), 0)
    ImageDraw.Draw(tile_mask).polygon(
        rounded_rect_path(0, 0, S_hi, S_hi, 0.20 * S_hi), fill=255
    )
    tile = Image.new("RGBA", (S_hi, S_hi), (0, 0, 0, 0))
    tile.paste(bg, (0, 0), tile_mask)
    img = Image.alpha_composite(img, tile)

    # -- 2. 문서 페이지 (부드러운 섀도우, 살짝 회전, 접힌 모서리) --------------
    pw, ph = 0.585 * S_hi, 0.66 * S_hi
    px0, py0 = 0.5 * (S_hi - pw), 0.5 * (S_hi - ph) - 0.01 * S_hi
    angle = math.radians(-6.0)
    cxc, cyc = S_hi / 2.0, py0 + ph / 2.0

    page_r = 0.028 * S_hi
    corners = [
        (px0 + page_r, py0),
        (px0 + pw - page_r, py0),
        (px0 + pw, py0 + page_r),
        (px0 + pw, py0 + ph - page_r),
        (px0 + pw - page_r, py0 + ph),
        (px0 + page_r, py0 + ph),
        (px0, py0 + ph - page_r),
        (px0, py0 + page_r),
    ]
    rot = rotate_pts(corners, cxc, cyc, angle)

    # 드롭 섀도우 — 아래/오른쪽으로 밀린 다층 투명
    shadow = Image.new("RGBA", (S_hi, S_hi), (0, 0, 0, 0))
    sd = ImageDraw.Draw(shadow)
    for i in range(7, 0, -1):
        off = i * 0.007 * S_hi
        poly = [(x + off * 0.7, y + off) for x, y in rot]
        sd.polygon(poly, fill=lerp_alpha(SHADOW, 10 + (7 - i) * 2))
    img = Image.alpha_composite(img, shadow)

    # 페이지 면
    page = Image.new("RGBA", (S_hi, S_hi), (0, 0, 0, 0))
    pd = ImageDraw.Draw(page)
    pd.polygon(rot, fill=PAGE_BG + (255,))
    pd.polygon(rot + [rot[0]], outline=PAGE_EDGE + (255,),
               width=max(1, int(S_hi * 0.0025)))

    # 접힌 모서리(오른쪽 위) — 삼각 음영 + 경계선
    fold = 0.155 * S_hi
    fold_tri = rotate_pts(
        [
            (px0 + pw - fold, py0 + fold),
            (px0 + pw - fold * 0.86, py0),
            (px0 + pw, py0 + fold * 0.55),
        ],
        cxc, cyc, angle,
    )
    fd = Image.new("RGBA", (S_hi, S_hi), (0, 0, 0, 0))
    ImageDraw.Draw(fd).polygon(fold_tri, fill=FOLD + (255,))
    ImageDraw.Draw(fd).polygon(fold_tri + [fold_tri[0]],
                               outline=lerp_alpha(FOLD, 180),
                               width=max(1, int(S_hi * 0.0015)))
    page = Image.alpha_composite(page, fd)

    # 빛 방향(왼-위) 미세 하이라이트
    hl_layer = Image.new("RGBA", (S_hi, S_hi), (0, 0, 0, 0))
    ImageDraw.Draw(hl_layer).polygon(
        rotate_pts(
            [(px0, py0), (px0 + pw * 0.36, py0), (px0, py0 + ph * 0.36)],
            cxc, cyc, angle,
        ),
        fill=(255, 255, 255, 24),
    )
    page = Image.alpha_composite(page, hl_layer)
    img = Image.alpha_composite(img, page)

    # -- 3. 칼리그래피 잉크 스트로크 (굵기 변화 + 펜 팁, 은은한 글로우) --------
    stroke = Image.new("RGBA", (S_hi, S_hi), (0, 0, 0, 0))
    path = [(0.265, 0.56), (0.40, 0.40), (0.585, 0.30), (0.745, 0.235)]
    bez = Bezier([(x * S_hi, y * S_hi) for x, y in path])
    w0, w1 = 0.030 * S_hi, 0.008 * S_hi  # 시작 굵음 → 끝(펜 팁) 가늘음

    glow_layer = Image.new("RGBA", (S_hi, S_hi), (0, 0, 0, 0))
    ImageDraw.Draw(glow_layer).polygon(
        calligraphic_stroke_polygon(bez, w0 * 2.4, w1 * 2.4),
        fill=lerp_alpha(INK_MID, 70),
    )
    stroke = Image.alpha_composite(stroke, glow_layer)

    # 본체 (인디고)
    ImageDraw.Draw(stroke).polygon(
        calligraphic_stroke_polygon(bez, w0, w1), fill=INK_DEEP + (255,)
    )
    # 상단 하이라이트 → 둥근 볼륨감
    ImageDraw.Draw(stroke).polygon(
        calligraphic_stroke_polygon(bez, w0 * 0.45, w1 * 0.45),
        fill=lerp_alpha(INK_HL, 110),
    )
    # 시작점 펜 눌림(도트)
    sx, sy = bez.point(0.0)
    tip_r = w0 * 0.62
    ImageDraw.Draw(stroke).ellipse(
        (sx - tip_r, sy - tip_r, sx + tip_r, sy + tip_r),
        fill=INK_DEEP + (255,),
    )
    img = Image.alpha_composite(img, stroke)

    # -- 4. 얇은 서리(프레임) 링 인세트 ----------------------------------------
    ring = Image.new("RGBA", (S_hi, S_hi), (0, 0, 0, 0))
    m = 0.045 * S_hi
    ImageDraw.Draw(ring).rounded_rectangle(
        (m, m, S_hi - m, S_hi - m),
        radius=0.16 * S_hi,
        outline=lerp_alpha(NORD8, 150),
        width=max(1, int(S_hi * 0.010)),
    )
    img = Image.alpha_composite(img, ring)

    return img.resize((S, S), Image.LANCZOS)


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
