"""Rule 1, WCAG contrast. Colours are read from the source the apps compile, so a palette edit is
checked the moment it is made:

  desktop   every `const X: Palette = Palette { ... }` under desktop/app/src (light and dark), and
            colour literals drawn straight into elements (`.bg(rgb(..))` with `.text_color(rgb(..))`)
  android   LightVx / DarkVx in ui/Theme.kt
  keyboard  KbColors.voxa() (light and dark), the solid presets in KeyboardLook.kt and the mic swatches

Targets: AAA 7:1 for body and secondary text, AA 4.5:1 for other text, 3:1 for large text, icons and
component edges. The brand sepia is #7A3F22 in light mode everywhere.
"""

from __future__ import annotations

import colorsys
import re

from . import src as S
from .core import Ctx, Violation

BRAND_SEPIA = 0x7A3F22
AAA, AA, UI = 7.0, 4.5, 3.0


# ── WCAG maths ──────────────────────────────────────────────────────────────────────────────────

def _ch(v: int) -> float:
    x = v / 255.0
    return x / 12.92 if x <= 0.04045 else ((x + 0.055) / 1.055) ** 2.4


def lum(rgb: int) -> float:
    return 0.2126 * _ch(rgb >> 16 & 0xFF) + 0.7152 * _ch(rgb >> 8 & 0xFF) + 0.0722 * _ch(rgb & 0xFF)


def ratio(a: int, b: int) -> float:
    x, y = lum(a), lum(b)
    return (max(x, y) + 0.05) / (min(x, y) + 0.05)


def over(fg: int, alpha: float, bg: int) -> int:
    """`fg` at `alpha` (0..1) composited over opaque `bg`, as 0xRRGGBB."""
    out = 0
    for s in (16, 8, 0):
        f, b = fg >> s & 0xFF, bg >> s & 0xFF
        out |= int(round(f * alpha + b * (1 - alpha))) << s
    return out


def rgba_over(rgba: int, bg: int) -> int:
    """Desktop 0xRRGGBBAA over an opaque 0xRRGGBB."""
    return over(rgba >> 8, (rgba & 0xFF) / 255.0, bg)


def argb(v: int) -> tuple[int, float]:
    """Android 0xAARRGGBB to (0xRRGGBB, alpha)."""
    return v & 0xFFFFFF, (v >> 24 & 0xFF) / 255.0


def _hx(c: int) -> str:
    return f"#{c:06X}"


class Checker:
    def __init__(self, rel: str, line: int, where: str):
        self.rel, self.line, self.where = rel, line, where
        self.out: list[Violation] = []

    def need(self, what: str, fg: int, bg: int, target: float, tier: str) -> None:
        r = ratio(fg, bg)
        if r + 1e-9 < target:
            self.out.append(Violation(
                "contrast", self.rel, self.line,
                f"{self.where} {what}: {_hx(fg)} on {_hx(bg)} is {r:.2f}:1, needs {target:g}:1 ({tier}). "
                "Rule: WCAG AA everywhere, AAA for body and secondary text (docs/guardrails.md#1-contrast)",
                key=f"{self.where} {what}"))


# ── desktop ─────────────────────────────────────────────────────────────────────────────────────

DESKTOP_GLOBS = ("desktop/app/src/**/*.rs",)


def desktop_palettes(ctx: Ctx) -> list[tuple[str, int, str, dict[str, int]]]:
    """(file, line, name, fields) for every Palette constant."""
    out = []
    for rel in S.files(ctx.root, *DESKTOP_GLOBS):
        src = S.load(ctx.root, rel)
        for m in re.finditer(r"\bconst\s+(\w+)\s*:\s*Palette\s*=\s*Palette\s*\{", src.code):
            close = S.match_brace(src.code, m.end() - 1)
            body = src.code[m.end():close]
            fields: dict[str, int] = {}
            for f in re.finditer(r"(\w+)\s*:\s*(0x[0-9A-Fa-f_]+|true|false)", body):
                v = f.group(2)
                fields[f.group(1)] = (1 if v == "true" else 0) if v in ("true", "false") else int(v.replace("_", ""), 16)
            out.append((rel, src.line_of(m.start()), m.group(1), fields))
    return out


def check_desktop_palette(rel: str, line: int, name: str, t: dict[str, int]) -> list[Violation]:
    mode = "dark" if t.get("dark") else "light"
    c = Checker(rel, line, f"desktop {name} ({mode})")
    g = t.get
    missing = [k for k in ("text", "muted", "page", "card", "accent", "on_accent") if k not in t]
    if missing:
        return [Violation("contrast", rel, line, f"desktop palette {name}: could not read {', '.join(missing)}; "
                          "update tools/lint/voxa_lint/contrast.py if the Palette fields were renamed", key=f"{name} parse")]
    page, card = t["page"], t["card"]
    strip = rgba_over(t["strip_a"], page) if "strip_a" in t else None

    def pair(what, fg, bg, target, tier):
        if fg is not None and bg is not None:
            c.need(what, fg, bg, target, tier)

    # Body and secondary text: AAA.
    for what, fg, bg in [
        ("text/page", g("text"), page), ("text/nav", g("text"), g("nav")), ("text/card", g("text"), card),
        ("text/hover", g("text"), g("hover")), ("body/page", g("body"), page), ("body/card", g("body"), card),
        ("muted/page", g("muted"), page), ("muted/card", g("muted"), card),
        ("text/strip", g("text"), strip), ("strip muted/strip", g("strip_muted"), strip),
        ("text/error bar", g("text"), g("error_bar")), ("text/warn bar", g("text"), g("warn_bar")),
        ("caption text/caption", g("caption_text"), g("caption")),
    ]:
        pair(what, fg, bg, AAA, "AAA body or secondary text")
    # Other text: AA.
    for what, fg, bg in [
        ("accent/page", g("accent"), page), ("accent/card", g("accent"), card), ("accent/nav", g("accent"), g("nav")),
        ("on accent/accent", g("on_accent"), g("accent")), ("on accent/accent hover", g("on_accent"), g("accent_hover")),
        ("on danger/danger", g("on_danger"), g("danger")), ("on danger/danger hover", g("on_danger"), g("danger_hover")),
        ("good text/card", g("good_text"), card), ("good text/page", g("good_text"), page),
        ("warn text/card", g("warn_text"), card), ("warn text/page", g("warn_text"), page),
        ("error/card", g("error"), card), ("danger text/card", g("danger_text"), card),
        ("plan pill", g("plan_text"), g("plan_bg")), ("signed pill", g("signed_text"), g("signed_bg")),
        ("warn text/strip", g("warn_text"), strip), ("muted/accent wash", g("muted"), g("accent_wash")),
    ]:
        pair(what, fg, bg, AA, "AA text")
    # Icons and component edges: 3:1.
    for what, fg, bg in [
        ("good icon/card", g("good"), card), ("error glyph/error bar", g("error_glyph"), g("error_bar")),
        ("warn glyph/warn bar", g("warn_glyph"), g("warn_bar")), ("mark/error glyph", g("bar_mark"), g("error_glyph")),
        ("mark/warn glyph", g("bar_mark"), g("warn_glyph")), ("toggle knob off/card", g("toggle_knob_off"), card),
        ("toggle track off/card", rgba_over(t["toggle_line_a"], card) if "toggle_line_a" in t else None, card),
        ("accent icon/accent wash", g("accent"), g("accent_wash")), ("rec dot/strip", g("strip_rec"), strip),
        ("paused glyph/strip", g("strip_paused"), strip), ("good/strip", g("good"), strip), ("error/strip", g("error"), strip),
        ("accent/strip", g("accent"), strip), ("pen/strip", g("pen"), strip),
    ]:
        pair(what, fg, bg, UI, "UI component")
    if not t.get("dark") and t["accent"] != BRAND_SEPIA:
        c.out.append(Violation("contrast", rel, line, f"desktop {name} (light): accent is {_hx(t['accent'])}; the brand sepia "
                               f"is {_hx(BRAND_SEPIA)} in light mode (docs/guardrails.md#1-contrast)", key=f"{name} brand"))
    return c.out


def desktop_literals(ctx: Ctx) -> list[Violation]:
    """Colours written straight into an element: a `.text_color(rgb(..))` with the `.bg(rgb(..))` of
    the same element chain. A computed background (an app's brand colour) is checked against every
    colour literal in the function."""
    out = []
    for rel in S.files(ctx.root, *DESKTOP_GLOBS):
        src = S.load(ctx.root, rel)
        tests = S.test_ranges(src)
        consts = {m.group(1): int(m.group(2), 16) for m in re.finditer(r"const\s+(\w+)\s*:\s*u32\s*=\s*0x([0-9A-Fa-f]{6})\s*;", src.code)}
        fns = S.functions(src)
        for t in re.finditer(r"\.text_color\(\s*rgb\(\s*(0x[0-9A-Fa-f]{6}|[A-Z_][A-Z0-9_]*)\s*\)\s*\)", src.code):
            if S.in_ranges(t.start(), tests):
                continue
            fg = _resolve(t.group(1), consts)
            if fg is None:
                continue
            chain_start = max(src.code.rfind(";", 0, t.start()), src.code.rfind("{\n", 0, t.start()))
            chain = src.code[chain_start:t.start()]
            bgs = list(re.finditer(r"\.bg\(\s*rgb\(\s*(.+?)\)\s*\)", chain))
            if not bgs:
                continue
            expr = bgs[-1].group(1).strip()
            line = src.line_of(t.start())
            c = Checker(rel, line, f"desktop literal in {(S.enclosing_fn(fns, t.start()) or S.Fn('?', 0, 0, 0)).name}()")
            bg = _resolve(expr, consts)
            if bg is not None:
                c.need(f"text {_hx(fg)} on {expr}", fg, bg, AA, "AA text")
            else:
                f = S.enclosing_fn(fns, t.start())
                body = src.code[f.body:f.end] if f else chain
                for lit in sorted({int(x, 16) for x in re.findall(r"\b0x([0-9A-Fa-f]{6})\b", body)} - {fg}):
                    c.need(f"text {_hx(fg)} on computed background {_hx(lit)}", fg, lit, AA, "AA text")
            out += c.out
    return out


def _resolve(expr: str, consts: dict[str, int]) -> int | None:
    expr = expr.strip()
    if re.fullmatch(r"0x[0-9A-Fa-f]{6}", expr):
        return int(expr, 16)
    return consts.get(expr)


# ── Android app ─────────────────────────────────────────────────────────────────────────────────

THEME_KT = "android/app/src/main/java/com/voxa/android/ui/Theme.kt"
KB_KT = "android/app/src/main/java/com/voxa/android/ime/KbColors.kt"
LOOK_KT = "android/app/src/main/java/com/voxa/android/ime/KeyboardLook.kt"


def android_palettes(ctx: Ctx) -> dict[str, tuple[int, dict[str, int]]]:
    src = S.load(ctx.root, THEME_KT)
    out = {}
    for m in re.finditer(r"\bval\s+(\w+)\s*=\s*VxColors\(", src.code):
        close = S.match_brace(src.code, m.end() - 1)
        body = src.code[m.end():close]
        f = {k: int(v, 16) for k, v in re.findall(r"(\w+)\s*=\s*Color\(\s*0x([0-9A-Fa-f]{8})\s*\)", body)}
        f["dark"] = 1 if re.search(r"\bdark\s*=\s*true", body) else 0
        out[m.group(1)] = (src.line_of(m.start()), f)
    return out


def check_android_palette(name: str, line: int, f: dict[str, int]) -> list[Violation]:
    mode = "dark" if f["dark"] else "light"
    c = Checker(THEME_KT, line, f"Android {name} ({mode})")
    need = ("base", "surface", "card", "text", "muted", "accent", "onAccent", "goodText", "warnText", "dangerText", "good", "warn", "danger", "switchOff")
    missing = [k for k in need if k not in f]
    if missing:
        return [Violation("contrast", THEME_KT, line, f"Android {name}: could not read {', '.join(missing)}; update tools/lint/voxa_lint/contrast.py", key=f"{name} parse")]
    rgb = {k: argb(v)[0] for k, v in f.items() if k != "dark"}
    text = rgb["text"]
    grounds = [("base", rgb["base"]), ("card", rgb["card"]), ("surface", rgb["surface"])]
    for gname, bg in grounds:
        c.need(f"text/{gname}", text, bg, AAA, "AAA body text")
        c.need(f"text72 (body)/{gname}", over(text, 0.72, bg), bg, AAA, "AAA body text")
        c.need(f"muted/{gname}", rgb["muted"], bg, AAA, "AAA secondary text")
        for k in ("goodText", "warnText", "dangerText", "accent"):
            c.need(f"{k}/{gname}", rgb[k], bg, AA, "AA text")
    for gname, bg in grounds[:2]:
        for k in ("good", "warn", "danger"):
            c.need(f"{k} icon/{gname}", rgb[k], bg, UI, "UI component")
        c.need(f"text40 icon/{gname}", over(text, 0.40, bg), bg, UI, "UI component")
    c.need("onAccent/accent", rgb["onAccent"], rgb["accent"], AA, "AA text")
    c.need("switchOff/card", rgb["switchOff"], rgb["card"], UI, "UI component")
    if not f["dark"] and rgb["accent"] != BRAND_SEPIA:
        c.out.append(Violation("contrast", THEME_KT, line, f"Android {name}: accent is {_hx(rgb['accent'])}; the brand sepia is {_hx(BRAND_SEPIA)} in light mode", key=f"{name} brand"))
    return c.out


# ── keyboard ────────────────────────────────────────────────────────────────────────────────────

def _kt_consts(code: str) -> dict[str, int]:
    out = {}
    for m in re.finditer(r"\bconst\s+val\s+(\w+)\s*=\s*0x([0-9A-Fa-f]{8})", code):
        out[m.group(1)] = int(m.group(2), 16)
    return out


def _kt_color(tok: str, consts: dict[str, int]) -> int | None:
    tok = tok.strip()
    m = re.fullmatch(r"0x([0-9A-Fa-f]{8})(?:\.toInt\(\))?", tok)
    if m:
        return int(m.group(1), 16)
    return consts.get(tok)


def keyboard_voxa(ctx: Ctx) -> tuple[int, dict[bool, dict[str, int]]] | Violation:
    """The Voxa look's colours, light and dark, read from KbColors.voxa()."""
    src = S.load(ctx.root, KB_KT)
    code = src.code
    consts = _kt_consts(code)
    mf = re.search(r"\bfun\s+mutedFor\(dark: Boolean\)\s*=\s*if\s*\(dark\)\s*(\S+)\s+else\s+(\S+)", code)
    fn = next((f for f in S.functions(src) if f.name == "voxa"), None)
    if not fn or not mf:
        return Violation("contrast", KB_KT, 1, "keyboard: could not find KbColors.voxa() or mutedFor(); update tools/lint/voxa_lint/contrast.py", key="keyboard parse")
    body = code[fn.body:fn.end]
    pairs = {m.group(1): (m.group(2), m.group(3)) for m in re.finditer(r"\b(\w+)\s*=\s*(?:if\s*\(\w+\)\s*[\w.()]+\s+else\s+)?if\s*\(dark\)\s*([\w.()]+)\s+else\s+([\w.()]+)", body)}
    pairs["muted"] = (mf.group(1), mf.group(2))
    km = re.search(r"key\s*=\s*if\s*\(!look\.keyShapes\)\s*0\s*else\s*if\s*\(dark\)\s*([\w.()]+)\s+else\s+(\w+)", body)
    if km:
        pairs["key"] = (km.group(1), km.group(2))
    out: dict[bool, dict[str, int]] = {True: {}, False: {}}
    consts.setdefault("WHITE", 0xFFFFFFFF)
    for k, (d, l) in pairs.items():
        for dark, tok in ((True, d), (False, l)):
            v = _kt_color(tok, consts)
            if v is not None:
                out[dark][k] = v
    for dark in (True, False):
        acc = out[dark].get("accent")
        if acc is None:
            out[dark]["accent"] = consts.get("SEPIA_DARK" if dark else "SEPIA", 0)
    return src.line_of(fn.start), out


def check_keyboard(ctx: Ctx) -> list[Violation]:
    r = keyboard_voxa(ctx)
    if isinstance(r, Violation):
        return [r]
    line, looks = r
    out = []
    for dark, f in looks.items():
        c = Checker(KB_KT, line, f"keyboard Voxa look ({'dark' if dark else 'light'})")
        need = ("surface", "text", "muted", "accent", "onAccent", "goodTitle", "good", "recording")
        missing = [k for k in need if k not in f]
        if missing:
            out.append(Violation("contrast", KB_KT, line, f"keyboard: could not read {', '.join(missing)} from voxa(); update tools/lint/voxa_lint/contrast.py", key="keyboard parse"))
            continue
        rgb = {k: argb(v)[0] for k, v in f.items()}
        s = rgb["surface"]
        c.need("text/surface", rgb["text"], s, AAA, "AAA body text")
        c.need("muted/surface", rgb["muted"], s, AAA, "AAA secondary text")
        c.need("key text (72 %)/surface", over(rgb["text"], 0.72, s), s, AA, "AA text")
        if "key" in rgb:
            c.need("key text (72 %)/key", over(rgb["text"], 0.72, rgb["key"]), rgb["key"], AA, "AA text")
        c.need("good title/surface", rgb["goodTitle"], s, AA, "AA text")
        c.need("mic icon/mic", rgb["onAccent"], rgb["accent"], AA, "AA text")
        c.need("mic/surface", rgb["accent"], s, UI, "UI component")
        c.need("good check/surface", rgb["good"], s, UI, "UI component")
        c.need("recording dot/surface", rgb["recording"], s, UI, "UI component")
        if not dark and rgb["accent"] != BRAND_SEPIA:
            c.out.append(Violation("contrast", KB_KT, line, f"keyboard (light): the mic is {_hx(rgb['accent'])}; the brand sepia is {_hx(BRAND_SEPIA)} in light mode", key="keyboard brand"))
        out += c.out
    return out


LIGHT_FG, DARK_FG = 0xF2F2F0, 0x141414


def _solid(base: int) -> tuple[int, int]:
    """KbColors.solid(): keys 7 % lighter (darker on very light colours); the text that reads best."""
    r, g, b = (base >> 16 & 0xFF) / 255, (base >> 8 & 0xFF) / 255, (base & 0xFF) / 255
    h, l, s = colorsys.rgb_to_hls(r, g, b)
    l = l - 0.07 if l > 0.9 else min(1.0, l + 0.07)
    kr, kg, kb = colorsys.hls_to_rgb(h, l, s)
    key = int(round(kr * 255)) << 16 | int(round(kg * 255)) << 8 | int(round(kb * 255))
    fg = max((LIGHT_FG, DARK_FG), key=lambda x: min(ratio(x, base), ratio(x, key)))
    return key, fg


def check_presets(ctx: Ctx) -> list[Violation]:
    src = S.load(ctx.root, LOOK_KT)
    out = []
    presets = [(m.group(1), int(m.group(2), 16) & 0xFFFFFF, src.line_of(m.start())) for m in re.finditer(r'Preset\("([^"]+)",\s*0x([0-9A-Fa-f]{8})', src.code)]
    mics = [(m.group(1), int(m.group(2), 16) & 0xFFFFFF, src.line_of(m.start())) for m in re.finditer(r'MicSwatch\("([^"]+)",\s*0x([0-9A-Fa-f]{8})', src.code)]
    if not presets or not mics:
        return [Violation("contrast", LOOK_KT, 1, "keyboard: no Preset(...) or MicSwatch(...) found; update tools/lint/voxa_lint/contrast.py", key="presets parse")]
    for name, base, line in presets:
        key, fg = _solid(base)
        c = Checker(LOOK_KT, line, f"keyboard preset {name}")
        c.need("text/background", fg, base, AAA, "AAA body text")
        c.need("secondary text (80 %)/background", over(fg, 0.8, base), base, AA, "AA text")
        c.need("key text (80 %)/key", over(fg, 0.8, key), key, AA, "AA text")
        dark = fg == LIGHT_FG
        mic = 0xD9946D if dark else BRAND_SEPIA
        c.need("mic/background", mic, base, UI, "UI component")
        out += c.out
    for name, mic, line in mics:
        icon = max((0xFFFFFF, DARK_FG), key=lambda x: ratio(x, mic))
        c = Checker(LOOK_KT, line, f"keyboard mic colour {name}")
        c.need("icon/mic", icon, mic, AA, "AA text")
        out += c.out
    return out


# ── entry ───────────────────────────────────────────────────────────────────────────────────────

def check(ctx: Ctx) -> list[Violation]:
    out: list[Violation] = []
    if any(ctx.exists(g.split("/**")[0]) for g in DESKTOP_GLOBS):
        pals = desktop_palettes(ctx)
        modes = {bool(p[3].get("dark")) for p in pals}
        if pals and modes != {True, False}:
            out.append(Violation("contrast", pals[0][0], pals[0][1], "desktop: need both a light and a dark Palette (dark: true / false)", key="desktop modes"))
        for p in pals:
            out += check_desktop_palette(*p)
        out += desktop_literals(ctx)
    if ctx.exists(THEME_KT):
        pals = android_palettes(ctx)
        if len(pals) < 2:
            out.append(Violation("contrast", THEME_KT, 1, "Android: expected LightVx and DarkVx = VxColors(...)", key="android parse"))
        for name, (line, f) in pals.items():
            out += check_android_palette(name, line, f)
    if ctx.exists(KB_KT):
        out += check_keyboard(ctx)
    if ctx.exists(LOOK_KT):
        out += check_presets(ctx)
    return out
