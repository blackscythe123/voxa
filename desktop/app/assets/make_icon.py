"""Voxa app icon: a sepia Fluent tile with a cream quill and a seismograph ink line.

Run: python make_icon.py  -> icon.png (1024), icon.ico (16..256), tray.png (32, no ink line)
Drawn at 4x and downsampled for clean edges. The quill is the old overlay's path (viewBox 16).
"""
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

HERE = Path(__file__).parent
SEPIA_TOP = (176, 98, 58)
SEPIA_BOTTOM = (122, 60, 32)
CREAM = (251, 244, 234)


def bezier(p0, p1, p2, p3, n=40):
    pts = []
    for i in range(n + 1):
        t = i / n
        a = (1 - t) ** 3
        b = 3 * (1 - t) ** 2 * t
        c = 3 * (1 - t) * t * t
        d = t ** 3
        pts.append((a * p0[0] + b * p1[0] + c * p2[0] + d * p3[0], a * p0[1] + b * p1[1] + c * p2[1] + d * p3[1]))
    return pts


def quill_points():
    # M 12.5 1.5 C 9 4 6.5 8 5.5 13 L 4.5 14.5 L 4 15 C 6 14.5 8 13 9.5 11 C 11.5 8.5 13 5 12.5 1.5 Z
    pts = bezier((12.5, 1.5), (9, 4), (6.5, 8), (5.5, 13))
    pts += [(4.5, 14.5), (4, 15)]
    pts += bezier((4, 15), (6, 14.5), (8, 13), (9.5, 11))[1:]
    pts += bezier((9.5, 11), (11.5, 8.5), (13, 5), (12.5, 1.5))[1:]
    return pts


def tile(size, ink=True):
    s = size * 4
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    # Gradient tile.
    grad = Image.new("RGBA", (s, s))
    gd = ImageDraw.Draw(grad)
    for y in range(s):
        t = y / (s - 1)
        c = tuple(int(SEPIA_TOP[i] * (1 - t) + SEPIA_BOTTOM[i] * t) for i in range(3))
        gd.line([(0, y), (s, y)], fill=c + (255,))
    mask = Image.new("L", (s, s), 0)
    margin = int(s * (0.04 if size <= 32 else 0.06))
    ImageDraw.Draw(mask).rounded_rectangle([margin, margin, s - margin, s - margin], radius=int(s * 0.22), fill=255)
    img.paste(grad, (0, 0), mask)

    d = ImageDraw.Draw(img)
    # Quill: viewBox 16 scaled so it spans the tile diagonally; nib low-left.
    scale = s / 16 * (0.70 if ink else 0.84)
    ox = s * (0.16 if ink else 0.06)
    oy = s * (0.07 if ink else 0.06)
    q = [(ox + x * scale, oy + y * scale) for x, y in quill_points()]
    d.polygon(q, fill=CREAM + (255,))
    nib = [(ox + x * scale, oy + y * scale) for x, y in [(4, 15), (2, 15.6), (3, 14)]]
    d.polygon(nib, fill=CREAM + (255,))
    # Spine: a thin sepia groove down the feather.
    spine = [(ox + x * scale, oy + y * scale) for x, y in bezier((12.2, 2.0), (9.5, 4.5), (7.0, 8.5), (5.5, 13.0))]
    d.line(spine, fill=SEPIA_BOTTOM + (200,), width=max(2, int(s * 0.012)), joint="curve")

    if ink:
        # Seismograph from the nib: flat, a burst of speech, flat.
        nx, ny = ox + 2 * scale, oy + 15.6 * scale
        x_end = s - margin - s * 0.10
        w = max(3, int(s * 0.026))
        pts = [(nx, ny)]
        burst = [0, -0.55, 0.9, -1.0, 0.75, -0.5, 0.3, 0]
        x0 = nx + s * 0.10
        step = (x_end - x0 - s * 0.10) / (len(burst) - 1)
        pts.append((x0, ny))
        for i, a in enumerate(burst):
            pts.append((x0 + i * step, ny + a * s * 0.075))
        pts.append((x_end, ny))
        d.line(pts, fill=CREAM + (235,), width=w, joint="curve")
        r = w * 0.9
        d.ellipse([x_end - r, ny - r, x_end + r, ny + r], fill=CREAM + (235,))

    return img.resize((size, size), Image.LANCZOS)


def main():
    big = tile(1024)
    big.save(HERE / "icon.png")
    sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256]
    frames = [tile(n, ink=n >= 32) for n in sizes]
    frames[-1].save(HERE / "icon.ico", sizes=[(n, n) for n in sizes], append_images=frames[:-1])
    tray = tile(32, ink=False)
    tray.save(HERE / "tray.png")
    (HERE / "tray.rgba").write_bytes(tray.tobytes())
    # Preview sheet for review.
    sheet = Image.new("RGBA", (16 + 20 + 24 + 32 + 48 + 64 + 128 + 256 + 9 * 16, 290), (240, 238, 234, 255))
    x = 16
    for n in [16, 20, 24, 32, 48, 64, 128, 256]:
        sheet.paste(tile(n, ink=n >= 32), (x, 280 - n - 8), tile(n, ink=n >= 32))
        x += n + 16
    sheet.save(HERE / "icon-preview.png")
    print("wrote icon.png, icon.ico, tray.png, icon-preview.png")


if __name__ == "__main__":
    main()
