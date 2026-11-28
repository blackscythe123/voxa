"""Rule 2, UI copy. Every string literal the apps can show (desktop, the shared core, the Android app
and keyboard, strings.xml) except log lines, assertions, tests, font names and diagnostics:

  copy-emoji     no emojis (Voxa draws its own icons); the Android emoji panel is exempt (user input)
  copy-dash      no dashes as punctuation: " - ", en dash, em dash
  copy-case      sentence case: "Open settings", not "Open Settings" (product and page names allowed)
  copy-occasion  culture-neutral: no festival, religious or national occasion names
"""

from __future__ import annotations

import re
from pathlib import Path

from . import src as S
from .core import Ctx, Violation

UI_GLOBS = (
    "desktop/app/src/**/*.rs",
    "desktop/signin/src/**/*.rs",
    "core/*/src/**/*.rs",
    "android/app/src/main/**/*.kt",
)
# Not shown to the user: diagnostics, offline tools, build scripts.
NOT_UI = (r"/diag/", r"core/[^/]+/src/bin/", r"/build\.rs$", r"core/voxa-ffi/src/diag\.rs$")
# The emoji panel is the user's own input (feedback_no_emojis: exempt).
EMOJI_EXEMPT = re.compile(r"(?i)emoji")
STRINGS_XML = "android/app/src/main/res/values/strings.xml"

# Calls whose string arguments are never shown: logs, assertions, env names, fonts, keys.
NOT_COPY_CALLS = (
    r"(?:\b[\w:]*::)?\blog|\beprintln!|\bprintln!|\bprint!|\beprint!|\bdbg!|\bassert\w*!|\bdebug_assert\w*!|\bpanic!|\bunreachable!|\btodo!"
    r"|\b(?:info|warn|error|debug|trace)!|\.expect|\bvar_os|\bvar|\bfont_family|\bDiag\.\w+|\bLog\.[a-z]|\bdlog|\bwith_context|\bcontext"
    r"|\bgetString|\bgetInt|\bgetBoolean|\bgetLong|\bgetFloat|\bputString|\bputInt|\bputBoolean|\bputLong|\bputFloat|\bremove|\bcontains"
)

# Emoji: everything in the pictograph planes, the BMP characters that render as emoji by default,
# and the emoji variation selector / joiner that turn other characters into emoji.
_EMOJI_BMP = (
    "⌚⌛⏩-⏳⏸-⏺◽◾☔☕♈-♓♿⚓⚡⚪⚫"
    "⚽⚾⛄⛅⛎⛔⛪⛲⛳⛵⛺⛽✅✊✋✨❌❎"
    "❓-❕❗❤➕-➗➰➿⬛⬜⭐⭕️‍"
)
EMOJI = re.compile(f"[\U0001F000-\U0001FAFF{_EMOJI_BMP}]")
DASH = re.compile(r"(?<=\S) - (?=\S)|–|—")

# Festival, religious and national occasions: Voxa's copy is for everyone, everywhere.
OCCASIONS = re.compile(
    r"(?i)\b(diwali|deepavali|pongal|onam|navratri|durga puja|ganesh chaturthi|holi|lohri|baisakhi|vaisakhi|bihu|ugadi|"
    r"makar sankranti|sankranti|raksha bandhan|rakhi|janmashtami|dussehra|eid|ramadan|ramzan|bakrid|muharram|christmas|xmas|"
    r"easter|good friday|halloween|thanksgiving|hanukkah|passover|rosh hashanah|yom kippur|kwanzaa|vesak|lunar new year|"
    r"chinese new year|valentine'?s?|republic day|independence day|national day|guru nanak jayanti|gandhi jayanti)\b")

# Words that keep their capital in sentence case: product, platform, key and page names.
PROPER = {
    "Voxa", "ChatGPT", "OpenAI", "Claude", "Code", "Windows", "Mac", "macOS", "Android", "iPhone", "Google", "Docs", "Sheets",
    "Meet", "Gmail", "Microsoft", "Apple", "Ctrl", "Alt", "Shift", "Cmd", "Option", "Space", "Fn", "Enter", "Esc", "Tab",
    "Home", "History", "Insights", "Words", "Settings", "Accessibility", "Explorer", "File", "VS", "Next", "I", "OK",
    "Wi-Fi", "Bluetooth", "Support", "Application", "AI", "UI", "USB", "PC", "OS",
    # Days, months and New Year are capitalised in English sentence case too (the greetings use them).
    "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday", "New", "Year",
    "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December",
}


def _template_free(v: str) -> str:
    """Kotlin `${...}` and Rust `{...}` placeholders blanked: arithmetic inside them is not copy."""
    v = re.sub(r"\$\{[^}]*\}", "x", v)
    return re.sub(r"\{[^}]*\}", "x", v)


def _skip_spans(src: S.Source) -> list[tuple[int, int]]:
    spans = [(o, c) for _, o, c in S.calls(src, f"(?:{NOT_COPY_CALLS})")]
    return spans + S.test_ranges(src)


def is_title_case(v: str) -> bool:
    """A title or label (2 to 8 words, every word capitalised) with a capital that is not a name."""
    for sentence in re.split(r"[.!?:]\s+", v.strip()):
        words = sentence.rstrip(".:!?").split()
        if not 2 <= len(words) <= 8 or not all(re.match(r"[A-Z]", w) for w in words):
            continue
        rest = [w.strip("()\"'") for w in words[1:]]
        if any(w not in PROPER and re.fullmatch(r"[A-Z][a-z']+", w) for w in rest):
            return True
    return False


def check_text(rel: str, line: int, value: str, emoji_ok: bool, out: list[Violation]) -> None:
    v = S.decode(value)
    if not emoji_ok:
        e = EMOJI.search(v)
        if e:
            out.append(Violation("copy-emoji", rel, line, f"emoji {e.group(0)!r} (U+{ord(e.group(0)):04X}) in UI copy {v[:60]!r}. "
                                 "Rule: no emojis in the in-app UI, use Voxa's icons (docs/guardrails.md#2-copy)", key=v))
    d = DASH.search(_template_free(v))
    if d:
        out.append(Violation("copy-dash", rel, line, f"dash used as punctuation in UI copy {v[:70]!r}. Rule: no ' - ', en or em "
                             "dashes; use a colon, comma or full stop (docs/guardrails.md#2-copy)", key=v))
    o = OCCASIONS.search(v)
    if o:
        out.append(Violation("copy-occasion", rel, line, f"occasion name {o.group(0)!r} in UI copy {v[:60]!r}. Rule: culture-neutral "
                             "copy, no festival, religious or national occasions (docs/guardrails.md#2-copy)", key=v))
    if is_title_case(v):
        out.append(Violation("copy-case", rel, line, f"Title Case {v!r}. Rule: sentence case for titles and buttons; add real "
                             "product or page names to PROPER in tools/lint/voxa_lint/copy.py (docs/guardrails.md#2-copy)", key=v))


def check(ctx: Ctx) -> list[Violation]:
    out: list[Violation] = []
    for rel in S.files(ctx.root, *UI_GLOBS, exclude=NOT_UI):
        src = S.load(ctx.root, rel)
        skip = _skip_spans(src)
        emoji_ok = bool(EMOJI_EXEMPT.search(Path(rel).name))
        for s in src.strings:
            if S.in_ranges(s.start, skip):
                continue
            check_text(rel, s.line, s.value, emoji_ok, out)
    if ctx.exists(STRINGS_XML):
        text = (ctx.root / STRINGS_XML).read_text(encoding="utf-8")
        for m in re.finditer(r"<string[^>]*>(.*?)</string>", text, re.S):
            check_text(STRINGS_XML, text.count("\n", 0, m.start()) + 1, m.group(1), False, out)
    return out
