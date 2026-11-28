"""Rule 3, privacy. What the user said, their cookies and tokens never reach a log or a notification.

  privacy-log     a log call (desktop `log`/`os::log`, `println!`/`eprintln!`, `log` macros; the core's
                  `diag::log`; Android `Diag.log`, `Log.*`) that interpolates a transcript, cookie or token,
                  or Debug-formats a whole event, effect or history entry. Lengths are fine (`.len()`,
                  `.length`, `.chars().count()`), and so is anything passed through the desktop's
                  `redact` module (desktop/app/src/redact.rs).
  privacy-notify  notification code never takes or shows transcript text: the notice builders take
                  no `text`/`transcript` parameter, and Android's builder setters get no such value.
"""

from __future__ import annotations

import re

from . import src as S
from .core import Ctx, Violation

LOG_GLOBS = ("desktop/app/src/**/*.rs", "desktop/signin/src/**/*.rs", "core/*/src/**/*.rs", "android/app/src/main/**/*.kt")
LOG_CALLS = (r"(?:\b[\w:]*::)?\blog|\beprintln!|\bprintln!|\beprint!|\bprint!|\b(?:log::)?(?:info|warn|error|debug|trace)!"
             r"|\bDiag\.log|\bLog\.[diwve]")
# Not shipped: offline training tools and examples print what they like.
NOT_SHIPPED = (r"core/[^/]+/src/bin/",)

SENSITIVE = re.compile(
    r"\b(text|texts|transcript|transcripts|transcription|said|words_said|cookie|cookies|cookie_header|cookieHeader|"
    r"token|tokens|access_token|accessToken|bearer|password|clipboard|clip|pasted|final_text|finalText|raw_text|rawText)\b")
WHOLE = re.compile(r"\b(event|ev|effect|effects|fx|entry|entries|job_state|outcome)\b")
SAFE_AFTER = re.compile(r"^\s*(\.len\(\)|\.length\b|\.size\b|\.chars\(\)\.count\(\)|\.is_empty\(\)|\.isEmpty\(\)|\.isNotEmpty\(\)|"
                        r"\.isBlank\(\)|\.is_some\(\)|\.is_none\(\)|\.javaClass|::class|\.count\(\)|\.lines\(\)\.count\(\))")
# The desktop's redact module and the core's diag helpers log a transcript as its length.
REDACT = re.compile(r"\bredact::|\bredact\.|\bdiag::(?:text|event|effect|effects|state)\(")


def _exprs(call: str, kotlin: bool) -> list[str]:
    """The interpolated expressions of a log call's text."""
    out = []
    if kotlin:
        i = 0
        while i < len(call):
            if call.startswith("${", i):
                j = S.match_brace(call, i + 1)
                out.append(call[i + 2:j])
                i = j + 1
            elif call[i] == "$" and i + 1 < len(call) and (call[i + 1].isalpha() or call[i + 1] == "_"):
                m = re.match(r"\$([A-Za-z_]\w*)", call[i:])  # `$a.b` interpolates `a` only
                out.append(m.group(1))
                i += m.end()
            else:
                i += 1
        return out
    # Rust: inline `{name...}` placeholders, and positional arguments after the format string.
    out += [m.group(1) + (m.group(2) or "") for m in re.finditer(r"(?<!\{)\{([A-Za-z_][\w.]*)(:[^}]*)?\}", call)]
    m = re.search(r'"((?:\\.|[^"\\])*)"', call)
    if m:
        rest = call[m.end():]
        depth, cur = 0, ""
        for ch in rest:
            if ch in ")]}" and depth == 0:
                break  # the end of format!( ... )
            if ch in "([{":
                depth += 1
            elif ch in ")]}":
                depth -= 1
            if ch == "," and depth == 0:
                if cur.strip():
                    out.append(cur.strip())
                cur = ""
            else:
                cur += ch
        if cur.strip():
            out.append(cur.strip())
    return out


def _leaks(expr: str) -> str | None:
    expr = re.sub(r'"(?:\\.|[^"\\])*"', '""', expr)  # words inside string literals are not values
    if REDACT.search(expr):
        return None
    for m in SENSITIVE.finditer(expr):
        if not SAFE_AFTER.match(expr[m.end():]):
            return m.group(1)
    w = WHOLE.search(expr)
    if w and not SAFE_AFTER.match(expr[w.end():]) and re.fullmatch(rf"\s*{w.group(1)}\s*(:\??#?\?)?\s*", expr):
        return w.group(1) + " (whole value)"
    return None


def check_logs(ctx: Ctx) -> list[Violation]:
    out = []
    for rel in S.files(ctx.root, *LOG_GLOBS, exclude=NOT_SHIPPED):
        src = S.load(ctx.root, rel)
        kotlin = rel.endswith(".kt")
        tests = [] if kotlin else S.test_ranges(src)
        for c, o, e in S.calls(src, f"(?:{LOG_CALLS})"):
            if S.in_ranges(c.start(), tests):
                continue
            call = src.code[o + 1:e]
            # Rust: `log(&format!(...))` and friends: the format string is inside.
            for expr in _exprs(call, kotlin):
                what = _leaks(expr)
                if what:
                    line = src.line_of(c.start())
                    hint = " Use desktop/app/src/redact.rs (redact::text, redact::event, redact::effects)." if rel.startswith("desktop/") else " Log its length instead."
                    out.append(Violation("privacy-log", rel, line, f"log line includes {what!r} via `{expr.strip()[:50]}`. Rule: no transcript, "
                                         f"cookie or token values in logs (docs/guardrails.md#3-privacy).{hint}", key=src.line_text(line).strip()))
                    break
    return out


NOTIFY_FILES = (
    "desktop/app/src/notify*.rs",
    "core/voxa-core/src/notices.rs",
    "core/voxa-ffi/src/notices.rs",
    "android/app/src/main/java/com/voxa/android/notify/Notifier.kt",
)
PARAM_BANNED = re.compile(r"^(text|transcript|said|words|body_text|dictation_text|final_text|finalText)$")
ANDROID_SETTERS = r"\.(?:setContentTitle|setContentText|bigText|setTicker|setSubText|setContentInfo|addLine|setSummaryText)"


def check_notifications(ctx: Ctx) -> list[Violation]:
    out = []
    for rel in S.files(ctx.root, *NOTIFY_FILES):
        src = S.load(ctx.root, rel)
        m = S.masked(src)
        tests = [] if rel.endswith(".kt") else S.test_ranges(src)
        for f in S.functions(src):
            if S.in_ranges(f.start, tests):
                continue
            o = m.find("(", f.start)
            if o < 0 or o > f.body:
                continue
            params = m[o + 1:S.match_brace(m, o)]
            for p in re.finditer(r"(?:^|[,(])\s*(?:mut\s+)?([A-Za-z_]\w*)\s*:", params):
                if PARAM_BANNED.match(p.group(1)):
                    line = src.line_of(f.start)
                    out.append(Violation("privacy-notify", rel, line, f"notification function {f.name}() takes {p.group(1)!r}. Rule: notifications "
                                         "never contain transcript text; name the app or a count instead (docs/guardrails.md#3-privacy)",
                                         key=f"{f.name}({p.group(1)})"))
        if rel.endswith(".kt"):
            for c, o, e in S.calls(src, ANDROID_SETTERS):
                arg = src.text[o + 1:e]
                s = SENSITIVE.search(arg)
                if s and not SAFE_AFTER.match(arg[s.end():]):
                    line = src.line_of(c.start())
                    out.append(Violation("privacy-notify", rel, line, f"notification shows {s.group(1)!r}. Rule: notifications never contain "
                                         "transcript text (docs/guardrails.md#3-privacy)", key=src.line_text(line).strip()))
    return out


DESCRIBE_FILES = ("desktop/app/src/win.rs", "desktop/app/src/mac.rs")
TITLE_READ = re.compile(r"\bGetWindowTextW?\b|\bkCGWindowName\b|\bAXTitle\b|\btitle\b")


def check_describe(ctx: Ctx) -> list[Violation]:
    """`os::describe(window)` goes into voxa-desktop.log on every take: it names the program and
    the window class, never the window title (email subjects, chat and document names)."""
    out = []
    for rel in S.files(ctx.root, *DESCRIBE_FILES):
        src = S.load(ctx.root, rel)
        tests = S.test_ranges(src)
        for f in S.functions(src):
            if f.name != "describe" or S.in_ranges(f.start, tests):
                continue
            body = src.code[f.body:f.end]
            m = TITLE_READ.search(body)
            if m:
                line = src.line_of(f.body + m.start())
                out.append(Violation("privacy-log", rel, line, f"describe() reads the window title ({m.group(0)}). Rule: window "
                                     "descriptions in the log name the program and window class only, never the title "
                                     "(docs/guardrails.md#3-privacy)", key="describe: title"))
    return out


def check(ctx: Ctx) -> list[Violation]:
    return check_logs(ctx) + check_notifications(ctx) + check_describe(ctx)
