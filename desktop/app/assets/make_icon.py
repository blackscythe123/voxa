"""Voxa app icon: the "Speaking nib" (docs/ui-mocks/icon-options.html, concept 2).

A fountain-pen nib whose slit is cut as a voice line: light sepia nib on a warm near-black tile
with a subtle rim, so the tile does not vanish on dark taskbars.

Run: python make_icon.py            -> icon.ico, icon.png (macOS 1024), tray.png/tray.rgba, icon-preview.png
     python make_icon.py --android  -> prints the Android vector path data (same geometry)

Geometry is the mockup's 60 x 60 glyph box. Frames of 32 px and up use the detailed mark (collar
line, ink hole, zigzag slit); 16, 20 and 24 px use the heavier small cut, hand-placed on the pixel
grid (2 px slit, whole-pixel nib edges) so it stays a crisp nib at taskbar size. Everything is
drawn supersampled and box-reduced.
"""
import math
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

HERE = Path(__file__).parent
TILE = (0x21, 0x1B, 0x17)
RIM = (0x3F, 0x34, 0x2D)
RIM_SMALL = (0x4A, 0x3E, 0x35)  # a touch lighter at 16-24 px, where the rim is one pixel
INK = (0xD9, 0x93, 0x66)

# Detailed mark, in the 60 x 60 glyph box.
NIB = dict(l=14, r=46, t=4, s=16, y=57)
HOLE = (30, 24, 4.4)
BAND = (10, 11, 50, 13.6)  # the collar line under the cap
SLIT = [(30, 28), (26.6, 35), (33.4, 41), (27.6, 48), (30, 58)]
SLIT_W = 2.6


# ── geometry ──────────────────────────────────────────────────────────────

def cubic(p0, p1, p2, p3, n=48):
    out = []
    for i in range(1, n + 1):
        t = i / n
        a, b, c, d = (1 - t) ** 3, 3 * (1 - t) ** 2 * t, 3 * (1 - t) * t * t, t ** 3
        out.append((a * p0[0] + b * p1[0] + c * p2[0] + d * p3[0], a * p0[1] + b * p1[1] + c * p2[1] + d * p3[1]))
    return out


def nib_ctrl(l, r, t, s, y):
    """The nib: flat top, straight sides to the shoulder s, then two curves meeting at the tip."""
    cx, w, h = (l + r) / 2, (r - l) / 2, y - s
    right = ((r, s + 14 / 41 * h), (cx + 0.5 * w, s + 26 / 41 * h), (cx, y))
    left = ((cx - 0.5 * w, s + 26 / 41 * h), (l, s + 14 / 41 * h), (l, s))
    return cx, right, left


def nib_points(l, r, t, s, y, n=48):
    cx, right, left = nib_ctrl(l, r, t, s, y)
    pts = [(l, t), (r, t), (r, s)]
    pts += cubic((r, s), *right, n=n)
    pts += cubic((cx, y), *left, n=n)
    return pts


def nib_path(l, r, t, s, y):
    cx, (a, b, c), (d, e, f) = nib_ctrl(l, r, t, s, y)
    f2 = lambda p: f"{p[0]:g},{p[1]:g}"
    return f"M{l:g},{t:g}H{r:g}V{s:g}C{f2(a)} {f2(b)} {f2(c)}C{f2(d)} {f2(e)} {f2(f)}Z"


def _unit(v):
    m = math.hypot(*v)
    return (v[0] / m, v[1] / m)


def _circle_exit(p, d, c, r):
    """Largest t where p + t d crosses the circle (the line starts inside it)."""
    fx, fy = p[0] - c[0], p[1] - c[1]
    b = 2 * (fx * d[0] + fy * d[1])
    cc = fx * fx + fy * fy - r * r
    return (-b + math.sqrt(b * b - 4 * cc)) / 2


def _line_x(p, d, q, e):
    den = d[0] * e[1] - d[1] * e[0]
    t = ((q[0] - p[0]) * e[1] - (q[1] - p[1]) * e[0]) / den
    return (p[0] + t * d[0], p[1] + t * d[1])


def keyhole(slit_w=SLIT_W):
    """Outline of (ink hole + zigzag slit stroke with round joins) as one closed shape.

    Returns a list of segments: ("L", pt) or ("A", pt, centre, radius, sweep, large). The slit is
    started inside the hole (same direction, so it looks identical) and run past the tip with a
    flat end; anything outside the nib is clipped by the nib itself.
    """
    pts = list(SLIT)
    d0 = _unit((pts[1][0] - pts[0][0], pts[1][1] - pts[0][1]))
    pts[0] = (pts[0][0] - 2 * d0[0], pts[0][1] - 2 * d0[1])
    dl = _unit((pts[-1][0] - pts[-2][0], pts[-1][1] - pts[-2][1]))
    pts[-1] = (pts[-1][0] + 3 * dl[0], pts[-1][1] + 3 * dl[1])
    h = slit_w / 2
    dirs = [_unit((pts[i + 1][0] - pts[i][0], pts[i + 1][1] - pts[i][1])) for i in range(len(pts) - 1)]

    def chain(side):
        segs = []
        for i, d in enumerate(dirs):
            n = (-d[1] * side, d[0] * side)
            a = (pts[i][0] + h * n[0], pts[i][1] + h * n[1])
            b = (pts[i + 1][0] + h * n[0], pts[i + 1][1] + h * n[1])
            if i == 0:
                t = _circle_exit(a, d, HOLE[:2], HOLE[2])
                a = (a[0] + t * d[0], a[1] + t * d[1])
                segs.append(("start", a))
            if i < len(dirs) - 1:
                e = dirs[i + 1]
                n2 = (-e[1] * side, e[0] * side)
                turn = d[0] * e[1] - d[1] * e[0]
                if turn * side < 0:  # this side is the outside of the bend: round join
                    b2 = (pts[i + 1][0] + h * n2[0], pts[i + 1][1] + h * n2[1])
                    segs.append(("L", b))
                    segs.append(("arc", b2, pts[i + 1]))
                else:  # inside of the bend: the offset lines meet
                    segs.append(("L", _line_x(a, d, (pts[i + 1][0] + h * n2[0], pts[i + 1][1] + h * n2[1]), e)))
            else:
                segs.append(("L", b))
        return segs

    A, B = chain(1), chain(-1)
    out = [("M", A[0][1])]
    for s in A[1:]:
        out.append(("L", s[1]) if s[0] == "L" else ("A", s[1], s[2], h))
    # Flat end across, then B walked from its end back to its start.
    pts_b = [("pt", B[0][1])] + [(s[0], s[1], s[2] if s[0] == "arc" else None) for s in B[1:]]
    out.append(("L", pts_b[-1][1]))
    for i in range(len(pts_b) - 1, 0, -1):
        kind = pts_b[i][0]
        target = pts_b[i - 1][1]
        if kind == "arc":
            out.append(("A", target, pts_b[i][2], h))
        else:
            out.append(("L", target))
    # Around the top of the hole, back to the start of A.
    out.append(("C", A[0][1], HOLE[:2], HOLE[2]))
    return out


def _arc_pts(p0, p1, c, r, long=False, n=12):
    a0 = math.atan2(p0[1] - c[1], p0[0] - c[0])
    a1 = math.atan2(p1[1] - c[1], p1[0] - c[0])
    da = a1 - a0
    while da > math.pi:
        da -= 2 * math.pi
    while da < -math.pi:
        da += 2 * math.pi
    if long:
        da = da - 2 * math.pi if da > 0 else da + 2 * math.pi
    return [(c[0] + r * math.cos(a0 + da * i / n), c[1] + r * math.sin(a0 + da * i / n)) for i in range(1, n + 1)], da


def keyhole_points(slit_w=SLIT_W):
    pts = []
    cur = None
    for s in keyhole(slit_w):
        if s[0] in ("M", "L"):
            cur = s[1]
            pts.append(cur)
        elif s[0] == "A":
            arc, _ = _arc_pts(cur, s[1], s[2], s[3], n=10)
            pts += arc
            cur = s[1]
        else:  # big arc over the top of the hole
            arc, _ = _arc_pts(cur, s[1], s[2], s[3], long=True, n=64)
            pts += arc
            cur = s[1]
    return pts


def keyhole_path():
    f = lambda v: f"{v:.3f}".rstrip("0").rstrip(".")
    out = []
    cur = None
    for s in keyhole():
        if s[0] == "M":
            out.append(f"M{f(s[1][0])},{f(s[1][1])}")
        elif s[0] == "L":
            out.append(f"L{f(s[1][0])},{f(s[1][1])}")
        else:
            long = s[0] == "C"
            _, da = _arc_pts(cur, s[1], s[2], s[3], long=long, n=2)
            sweep = 1 if da > 0 else 0  # y points down, so a positive angle is clockwise
            out.append(f"A{f(s[3])},{f(s[3])} 0 {1 if long else 0} {sweep} {f(s[1][0])},{f(s[1][1])}")
        cur = s[1]
    return "".join(out) + "Z"


# ── raster ────────────────────────────────────────────────────────────────

def glyph_mask(size, ss, k, ox, oy, slit_w=SLIT_W, snap=False):
    """Detailed nib mask at size*ss pixels; glyph unit k px, glyph origin (ox, oy) in px.
    snap: put the collar line on whole pixel rows (small frames)."""
    S = size * ss
    m = Image.new("L", (S, S), 0)
    d = ImageDraw.Draw(m)
    tr = lambda pts: [((ox + x * k) * ss, (oy + y * k) * ss) for x, y in pts]
    d.polygon(tr(nib_points(**NIB, n=96)), fill=255)
    x0, y0, x1, y1 = BAND
    if snap:
        py0 = round(oy + y0 * k)
        py1 = max(py0 + 1, round(oy + y1 * k))
        y0, y1 = (py0 - oy) / k, (py1 - oy) / k
    d.polygon(tr([(x0, y0), (x1, y0), (x1, y1), (x0, y1)]), fill=0)
    d.polygon(tr(keyhole_points(slit_w)), fill=0)
    return m


# The small cut, hand-placed per size on whole pixels (x: nib left/right, t: top, s: shoulder,
# y: tip, hole centre/radius, slit width). 16 px sits half a pixel left of centre so its 1 px slit
# lands on a pixel column instead of smearing across two.
SMALL = {
    16: dict(l=4, r=11, t=2, s=5, y=14.4, hy=6.9, hr=1.6, slit=1),
    20: dict(l=5, r=15, t=3, s=6, y=17.6, hy=8.6, hr=1.8, slit=2),
    24: dict(l=7, r=17, t=4, s=7, y=21, hy=10.3, hr=2.1, slit=2),
    "tray": dict(l=5, r=23, t=3, s=9, y=28.5, hy=11.8, hr=2.7, slit=2),
}


def small_mask(size, ss, key=None):
    """The heavy small cut: a bold nib, round ink hole, straight slit."""
    g = SMALL[key or size]
    S = size * ss
    cx = (g["l"] + g["r"]) / 2
    hy, hr, w = g["hy"], g["hr"], g["slit"]
    m = Image.new("L", (S, S), 0)
    d = ImageDraw.Draw(m)
    d.polygon([(x * ss, y * ss) for x, y in nib_points(g["l"], g["r"], g["t"], g["s"], g["y"], n=96)], fill=255)
    d.ellipse([(cx - hr) * ss, (hy - hr) * ss, (cx + hr) * ss - 1, (hy + hr) * ss - 1], fill=0)
    d.rectangle([(cx - w / 2) * ss, hy * ss, (cx + w / 2) * ss - 1, S], fill=0)
    return m, g


def rounded(size, ss, inset, radius):
    S = size * ss
    m = Image.new("L", (S, S), 0)
    ImageDraw.Draw(m).rounded_rectangle([inset * ss, inset * ss, S - inset * ss - 1, S - inset * ss - 1], radius=radius * ss, fill=255)
    return m


def paint(size, ss, layers):
    S = size * ss
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    for color, mask in layers:
        img.paste(Image.new("RGBA", (S, S), color + (255,)), (0, 0), mask)
    return img.reduce(ss)


def win_frame(size):
    """A Windows .ico frame: the small cut at 24 px and below, the detailed mark above."""
    if size <= 24:
        ss = 32  # PIL fills polygon edges inclusively: one sub-pixel of 32 is invisible
        radius = {16: 3.5, 20: 4.25, 24: 5}.get(size, size * 0.21)
        outer = rounded(size, ss, 0, radius)
        inner = rounded(size, ss, 1, radius - 1)
        glyph, _ = small_mask(size, ss)
        return paint(size, ss, [(RIM_SMALL, outer), (TILE, inner), (INK, glyph)])
    ss = 32 if size <= 64 else 8
    m = max(1, round(size * 0.04))       # tile inset, on whole pixels
    tile_px = size - 2 * m
    radius = tile_px * 22 / 92
    rim = max(1.0, size * 0.012)
    outer = rounded(size, ss, m, radius)
    S = size * ss
    inner = Image.new("L", (S, S), 0)
    ImageDraw.Draw(inner).rounded_rectangle([(m + rim) * ss, (m + rim) * ss, S - (m + rim) * ss - 1, S - (m + rim) * ss - 1], radius=(radius - rim) * ss, fill=255)
    k = tile_px / 92                      # mockup: the 60-unit glyph sits in a 92-unit tile
    snap = size <= 64
    if size <= 40:
        k *= 1.15                         # a bigger mark where the zigzag only has ~10 rows
    ox = oy = size / 2 - 30 * k
    if snap:
        # Whole-pixel nib: an even pixel width, so its sides, top and centre sit on the grid.
        k = 2 * max(1, round(16 * k)) / 32
        ox = size / 2 - 30 * k
        oy = round(size / 2 - 30 * k + 4 * k) - 4 * k
    # Keep the zigzag at least ~1.1 px wide at 32-48 px, where 2.6 units would be under a pixel.
    glyph = glyph_mask(size, ss, k, ox, oy, slit_w=max(SLIT_W, 1.1 / k), snap=snap)
    return paint(size, ss, [(RIM, outer), (TILE, inner), (INK, glyph)])


def mac_icon():
    """macOS: 1024 canvas, 824 tile (radius 185), soft shadow, rim, glyph at the mockup's 0.78."""
    size, ss = 1024, 2
    S = size * ss
    unit = 10.24                          # mockup viewBox 100 -> 1024 px
    shadow = Image.new("L", (S, S), 0)
    ImageDraw.Draw(shadow).rounded_rectangle([100 * ss, (100 + 2 * unit) * ss, 924 * ss, (924 + 2 * unit) * ss], radius=185 * ss, fill=int(255 * 0.28))
    shadow = shadow.filter(ImageFilter.GaussianBlur(2.2 * unit * ss))
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    img.paste(Image.new("RGBA", (S, S), (0, 0, 0, 255)), (0, 0), shadow)
    outer = Image.new("L", (S, S), 0)
    ImageDraw.Draw(outer).rounded_rectangle([100 * ss, 100 * ss, 924 * ss, 924 * ss], radius=185 * ss, fill=255)
    rim = 6
    inner = Image.new("L", (S, S), 0)
    ImageDraw.Draw(inner).rounded_rectangle([(100 + rim) * ss, (100 + rim) * ss, (924 - rim) * ss, (924 - rim) * ss], radius=(185 - rim) * ss, fill=255)
    k = 0.78 * unit
    o = 512 - 30 * k
    glyph = glyph_mask(size, ss, k, o, o)
    for color, mask in [(RIM, outer), (TILE, inner), (INK, glyph)]:
        img.paste(Image.new("RGBA", (S, S), color + (255,)), (0, 0), mask)
    return img.resize((size, size), Image.LANCZOS)


def tray_icon():
    """Tray (32 x 32 RGBA, shown at 16 px on 100% displays): the bare small-cut nib, ink on a dark
    plate one display pixel wider, so it reads on light and dark taskbars alike. Nudged left and up
    so the status dot main.rs paints at (25, 25) only touches the nib's lower flank."""
    size, ss = 32, 32
    glyph, g = small_mask(size, ss, "tray")
    l, r, t, s, y = g["l"], g["r"], g["t"], g["s"], g["y"]
    S = size * ss
    plate = Image.new("L", (S, S), 0)
    pd = 2
    ImageDraw.Draw(plate).polygon([(x * ss, yy * ss) for x, yy in nib_points(l - pd, r + pd, t - pd, s, y + pd * 1.5, n=96)], fill=255)
    plate = plate.filter(ImageFilter.GaussianBlur(ss * 0.4)).point(lambda v: 255 if v > 96 else int(v * 255 / 96))
    return paint(size, ss, [(TILE, plate), (INK, glyph)])


def with_dot(img, c):
    """The same dot main.rs tray_icon() paints, for the preview."""
    out = img.copy()
    for y in range(32):
        for x in range(32):
            d2 = (x - 25) ** 2 + (y - 25) ** 2
            if d2 <= 36:
                out.putpixel((x, y), c + (255,))
            elif d2 <= 64:
                out.putpixel((x, y), (255, 255, 255, 255))
    return out


def main():
    if "--android" in sys.argv:
        print("nib     ", nib_path(**NIB))
        print("band    ", "M{0:g},{1:g}H{2:g}V{3:g}H{0:g}Z".format(*BAND))
        print("keyhole ", keyhole_path())
        print("slit    ", "M" + "L".join(f"{x:g},{y:g}" for x, y in SLIT))
        return

    sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256]
    frames = {n: win_frame(n) for n in sizes}
    frames[256].save(HERE / "icon.ico", sizes=[(n, n) for n in sizes], append_images=[frames[n] for n in sizes[:-1]])
    mac_icon().save(HERE / "icon.png")
    tray = tray_icon()
    tray.save(HERE / "tray.png")
    (HERE / "tray.rgba").write_bytes(tray.tobytes())

    # Preview: the frames at real size on light and dark, then the tray with its dots.
    W = sum(sizes) + 16 * (len(sizes) + 1) + 3 * 48
    sheet = Image.new("RGBA", (W, 2 * 280), (243, 243, 243, 255))
    ImageDraw.Draw(sheet).rectangle([0, 280, W, 560], fill=(32, 32, 32, 255))
    for row in range(2):
        x = 16
        for n in sizes:
            sheet.alpha_composite(frames[n], (x, row * 280 + 264 - n))
            x += n + 16
        for dot in [None, (0xE5, 0x48, 0x3D), (0xC9, 0x8A, 0x16)]:
            t = with_dot(tray, dot) if dot else tray
            sheet.alpha_composite(t, (x, row * 280 + 232))
            x += 48
    sheet.save(HERE / "icon-preview.png")
    print("wrote icon.ico, icon.png, tray.png, tray.rgba, icon-preview.png")


if __name__ == "__main__":
    main()
