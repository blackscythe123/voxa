"""Rule 13, every icon is a real picture from Voxa's icon font (Segoe Fluent Icons).

The birthday row drew a Chinese character: its code point, E88C, is in the font, but the glyph
there is LanguageCht (the input language indicator), not a gift. A range check passed it.

  icon-glyph  every private use code point in desktop/app/src (outside tests) is an `I_` constant
              in home.rs followed by `// <Microsoft's glyph name>`, the pair is in home.rs's
              REVIEWED_ICONS table, and it is not one of the font's letter glyphs (TEXT_GLYPHS).
  icon-font   every such code point is in the icon font's cmap. Checked against
              C:/Windows/Fonts/SegoeIcons.ttf when it is installed (the pre-commit hook on Windows);
              elsewhere a note says the check was skipped.
"""

from __future__ import annotations

import re
import struct
from pathlib import Path

from . import src as S
from .core import Ctx, Violation

GLOBS = ("desktop/app/src/**/*.rs",)
HOME = "desktop/app/src/home.rs"
SYSTEM_FONT = Path("C:/Windows/Fonts/SegoeIcons.ttf")
# A fixture tree carries its own small font here; the real repo never does.
FIXTURE_FONT = "lint-fonts/SegoeIcons.ttf"

# The font's letters, not pictures: input language and input method indicators (Microsoft's
# names: LangJPN, JpnRomanji*, LanguageKor, LanguageCht, LanguageChs, Korean, ChineseChangjie,
# ChineseQuick, Japanese, ChineseBoPoMoFo, ChinesePinyin, PinyinIMELogo2).
TEXT_GLYPHS = {0xE7DE, 0xE87C, 0xE87D, 0xE87E, 0xE87F, 0xE88B, 0xE88C, 0xE88D, 0xE97D, 0xE981, 0xE984, 0xE985, 0xE989, 0xE98A, 0xF698}

ESCAPE = re.compile(r"\\u\{([0-9A-Fa-f]{1,6})\}")
CONST = re.compile(r"^const I_[A-Z0-9_]+: char = '\\u\{[0-9A-Fa-f]{4}\}'; // (\w+)\s*$")
REVIEWED_ROW = re.compile(r'\(0x([0-9A-Fa-f]{4}), "(\w+)"\)')


def _pua(c: int) -> bool:
    return 0xE000 <= c <= 0xF8FF


def _reviewed(text: str) -> dict[int, str]:
    i = text.find("const REVIEWED_ICONS")
    if i < 0:
        return {}
    j = text.find("];", i)
    return {int(h, 16): n for h, n in REVIEWED_ROW.findall(text[i:j])}


def code_points(ctx: Ctx) -> list[tuple[str, int, str, int]]:
    """(path, line number, line, code point) for every private use code point outside tests."""
    out = []
    for rel in S.files(ctx.root, *GLOBS):
        text = (ctx.root / rel).read_text(encoding="utf-8")
        body = text.split("#[cfg(test)]")[0]
        for n, line in enumerate(body.splitlines(), 1):
            for m in ESCAPE.finditer(line):
                c = int(m.group(1), 16)
                if _pua(c):
                    out.append((rel, n, line.strip(), c))
            for ch in line:
                if _pua(ord(ch)):
                    out.append((rel, n, line.strip(), ord(ch)))
    return out


def cmap(font: Path) -> set[int]:
    """The code points a TrueType/OpenType font maps (cmap formats 4 and 12). Standard library only."""
    b = font.read_bytes()
    num = struct.unpack_from(">H", b, 4)[0]
    off = None
    for k in range(num):
        tag, _, o, _ = struct.unpack_from(">4sIII", b, 12 + 16 * k)
        if tag == b"cmap":
            off = o
    if off is None:
        return set()
    out: set[int] = set()
    count = struct.unpack_from(">H", b, off + 2)[0]
    for k in range(count):
        _, _, sub = struct.unpack_from(">HHI", b, off + 4 + 8 * k)
        t = off + sub
        fmt = struct.unpack_from(">H", b, t)[0]
        if fmt == 4:
            segx2 = struct.unpack_from(">H", b, t + 6)[0]
            seg = segx2 // 2
            ends = struct.unpack_from(f">{seg}H", b, t + 14)
            starts = struct.unpack_from(f">{seg}H", b, t + 16 + segx2)
            deltas = struct.unpack_from(f">{seg}h", b, t + 16 + 2 * segx2)
            ro_at = t + 16 + 3 * segx2
            ros = struct.unpack_from(f">{seg}H", b, ro_at)
            for i in range(seg):
                for c in range(starts[i], ends[i] + 1):
                    if c == 0xFFFF:
                        continue
                    if ros[i] == 0:
                        g = (c + deltas[i]) & 0xFFFF
                    else:
                        at = ro_at + 2 * i + ros[i] + 2 * (c - starts[i])
                        g = struct.unpack_from(">H", b, at)[0]
                        g = (g + deltas[i]) & 0xFFFF if g else 0
                    if g:
                        out.add(c)
        elif fmt == 12:
            groups = struct.unpack_from(">I", b, t + 12)[0]
            for i in range(groups):
                a, z, _ = struct.unpack_from(">III", b, t + 16 + 12 * i)
                out.update(range(a, z + 1))
    return out


def check(ctx: Ctx) -> list[Violation]:
    out: list[Violation] = []
    home = ctx.root / HOME
    if not home.exists():
        return out
    reviewed = _reviewed(home.read_text(encoding="utf-8"))
    found = code_points(ctx)
    for rel, n, line, c in found:
        cp = f"U+{c:04X}"
        if rel != HOME or not line.startswith("const I_"):
            out.append(Violation("icon-glyph", rel, n, f"icon code point {cp} outside an I_ constant in home.rs", key=line))
            continue
        if c in TEXT_GLYPHS:
            out.append(Violation("icon-glyph", rel, n, f"{cp} is a letter glyph of the icon font, not a picture", key=line))
            continue
        m = CONST.match(line)
        if not m:
            out.append(Violation("icon-glyph", rel, n, f"{cp} needs Microsoft's glyph name in a // comment", key=line))
        elif reviewed.get(c) != m.group(1):
            out.append(Violation("icon-glyph", rel, n, f"{cp} named {m.group(1)}, REVIEWED_ICONS says {reviewed.get(c)!r}", key=line))
    font = ctx.root / FIXTURE_FONT
    if not font.exists():
        font = SYSTEM_FONT
    if not font.exists():
        ctx.notes.append("icon-font skipped: Segoe Fluent Icons is not installed here (it runs on Windows)")
        return out
    have = cmap(font)
    for rel, n, line, c in found:
        if c not in have:
            out.append(Violation("icon-font", rel, n, f"U+{c:04X} is not in {font.name}: it would draw a fallback glyph", key=line))
    return out
