"""Rule 4, never steal focus on the desktop. Dictation happens in another app; Voxa's strip, tray
and toasts must never take the keyboard away from it.

  focus-steal   SetForegroundWindow, SetActiveWindow, BringWindowToTop, SwitchToThisWindow,
                activateIgnoringOtherApps, makeKeyAndOrderFront, gpui activate_window / activate(true),
                and ShowWindow with SW_SHOW / SW_SHOWNORMAL / SW_RESTORE / SW_SHOWDEFAULT, anywhere under
                desktop/app/src except the functions in ALLOWED (each with its reason).
  focus-notify  macOS notification actions with the foreground option, except Open and Sign in.
"""

from __future__ import annotations

import re

from . import src as S
from .core import Ctx, Violation

GLOBS = ("desktop/app/src/**/*.rs",)

STEAL = re.compile(
    r"\b(SetForegroundWindow|SetActiveWindow|BringWindowToTop|SwitchToThisWindow|AllowSetForegroundWindow|SetFocus)\s*\("
    r"|\bactivateIgnoringOtherApps\b|\bmakeKeyAndOrderFront\b|\bactivateWithOptions\b"
    r"|\.activate_window\s*\(|\.activate\s*\(\s*true\s*\)"
    r"|\bShowWindow(?:Async)?\s*\([^;]*\bSW_(?:SHOW|SHOWNORMAL|RESTORE|SHOWDEFAULT|SHOWMAXIMIZED|NORMAL|MAXIMIZE)\b")

# (file, function, call): why taking focus is right there. Anything else fails, including a new
# focus call added to one of these functions.
ALLOWED = {
    ("desktop/app/src/win.rs", "no_activate_proc", "SetForegroundWindow"):
        "hands focus BACK to the app that had it, after GPUI activated the hidden strip on a display change",
    ("desktop/app/src/win.rs", "give_focus_back", "SetForegroundWindow"):
        "hands focus BACK to the app that had it, after GPUI activated a newly shown window",
    ("desktop/app/src/mac.rs", "give_focus_back", "activateWithOptions"):
        "hands focus BACK to the app that had it, after GPUI activated a newly shown window",
    ("desktop/app/src/win.rs", "switch_to", "ShowWindow"):
        "paste target: the user dictated from Voxa's own window; focus goes to the app the text is for",
    ("desktop/app/src/win.rs", "switch_to", "SetForegroundWindow"):
        "paste target: the user dictated from Voxa's own window; focus goes to the app the text is for",
    ("desktop/app/src/win.rs", "bring_to_front", "ShowWindow"): "the user asked for the window: tray Open Voxa or a second launch",
    ("desktop/app/src/win.rs", "bring_to_front", "SetForegroundWindow"): "the user asked for the window: tray Open Voxa or a second launch",
    ("desktop/app/src/mac.rs", "bring_to_front", "activateIgnoringOtherApps"): "the user asked for the window: tray Open Voxa or a second launch",
    ("desktop/app/src/mac.rs", "bring_to_front", "makeKeyAndOrderFront"): "the user asked for the window: tray Open Voxa or a second launch",
    ("desktop/app/src/main.rs", "run_window", "activate_window"): "the user asked for the window: another Open Voxa while it is open",
}


# Action labels whose button may open Voxa (UNNotificationActionOptionForeground).
FOREGROUND_OK = re.compile(r"(?i)^&?\s*(\"(open|open voxa|sign in)\"|signin|sign_in|open)\s*$")


def check(ctx: Ctx) -> list[Violation]:
    out = []
    for rel in S.files(ctx.root, *GLOBS):
        src = S.load(ctx.root, rel)
        m = S.masked(src)
        tests = S.test_ranges(src)
        fns = S.functions(src)
        for hit in STEAL.finditer(m):
            if S.in_ranges(hit.start(), tests) or _is_import(m, hit.start()):
                continue
            f = S.enclosing_fn(fns, hit.start())
            name = f.name if f else "?"
            call = re.match(r"\.?(\w+)", hit.group(0)).group(1)
            if (rel, name, call) in ALLOWED:
                continue
            line = src.line_of(hit.start())
            out.append(Violation("focus-steal", rel, line, f"{hit.group(0).strip()[:40]!r} in {name}(). Rule: Voxa never steals focus from "
                                 "the app being dictated into; use SW_SHOWNOACTIVATE / WS_EX_NOACTIVATE, or add (file, function) "
                                 "with a reason to ALLOWED in tools/lint/voxa_lint/focus.py (docs/guardrails.md#4-focus)",
                                 key=f"{name}: {src.line_text(line).strip()}"))
        # macOS: category(tag, label, foreground)
        for c, o, e in S.calls(src, r"(?<![\w.])category"):
            args = _split(src.code[o + 1:e])
            if len(args) == 3 and args[2].strip() == "true" and not FOREGROUND_OK.match(args[1].strip()):
                line = src.line_of(c.start())
                out.append(Violation("focus-notify", rel, line, f"notification action {args[1].strip()!r} opens Voxa (foreground). Rule: only "
                                     "Open and Sign in may bring Voxa forward; Copy and Send now run in the background (docs/guardrails.md#4-focus)",
                                     key=src.line_text(line).strip()))
    return out


def _is_import(m: str, at: int) -> bool:
    start = m.rfind("\n", 0, at) + 1
    stmt_start = max(m.rfind(";", 0, at), m.rfind("}", 0, at), 0)
    return bool(re.match(r"\s*(pub\s+)?use\b", m[stmt_start + 1:at])) or m[start:at].lstrip().startswith("use ")


def _split(args: str) -> list[str]:
    out, depth, cur = [], 0, ""
    for ch in args:
        if ch in "([{":
            depth += 1
        elif ch in ")]}":
            depth -= 1
        if ch == "," and depth == 0:
            out.append(cur)
            cur = ""
        else:
            cur += ch
    out.append(cur)
    return out
