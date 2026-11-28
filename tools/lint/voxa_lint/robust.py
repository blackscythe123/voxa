"""Rule 8, robustness rules from the bug hunt.

  robust-case-slice    no byte-index slicing of user text after a case change: `to_lowercase()` /
                       `to_uppercase()` can change byte lengths ("İ" is 2 bytes, "i̇" is 3), so an index
                       found in the lowered copy can split a character in the original. Best effort: in
                       one function, a `let x = y.to_lowercase()` and then `x[..]` or `y[..]` slicing
                       (Kotlin: `.lowercase()` then `.substring(`). ASCII case changes are fine.
  robust-unwrap        no `.unwrap()` / `.expect(..)` in core/ library code outside tests (network, file
                       and user input must not panic the keyboard or the app). Lock poisoning is handled
                       with `unwrap_or_else(|e| e.into_inner())`. Exceptions go on ALLOWED_UNWRAP.
  robust-atomic-write  history, settings, session and account files are written through the atomic
                       writers (voxa_store::write_atomic, desktop store.rs, voxa_chatgpt::
                       write_unique_then_rename), never with a bare std::fs::write / File::create on the
                       final path; and each writer's temporary file has a unique name (process id plus a
                       counter or time), so two writers never share one temp file.
  robust-strip-sync    no synchronous SetWindowPos / ShowWindow / ShowWindowAsync / MoveWindow under
                       desktop/app/src outside ALLOWED_WINDOW_CALLS: the strip is moved, shown and hidden by
                       posting to its own message queue (1.0.0 "RefCell already borrowed" at start).
  robust-toast-thread  WinRT toast calls in notify_win.rs run through off_ui (a worker thread), never on
                       the UI thread, where COM's modal loop re-entered a borrowed GPUI.
"""

from __future__ import annotations

import re

from . import src as S
from .core import Ctx, Violation

CASE_GLOBS = ("desktop/app/src/**/*.rs", "core/*/src/**/*.rs", "android/app/src/main/**/*.kt")
CORE_LIB = ("core/*/src/**/*.rs",)
CORE_NOT_LIB = (r"core/[^/]+/src/bin/",)  # offline tools (the nudge trainer) may panic on bad input

# (file, function, the unwrapped expression's start): why it cannot fail on real input.
ALLOWED_UNWRAP: dict[tuple[str, str, str], str] = {}

STORE_FILES = re.compile(r"\"(history|settings|session|account|nudge|notify|words|state)(\.json)?\"|history\.json|settings\.json|session\.json|account\.json")
# The helpers that do the temp-file-and-rename themselves (each writes a unique temp, then renames).
ATOMIC_WRITERS = {
    ("core/voxa-store/src/lib.rs", "write_atomic"),
    ("desktop/app/src/store.rs", "write_atomic"),
    ("desktop/app/src/link.rs", "write_atomic"),
    ("core/voxa-chatgpt/src/lib.rs", "write_unique_then_rename"),
    ("core/voxa-nudge/src/engine.rs", "save"),
    ("desktop/signin/src/main.rs", "save_session"),
}
# (file, function): a direct write of a store-like file that is fine, with the reason.
ALLOWED_WRITES = {
    ("desktop/app/src/store.rs", "write_backup"): "a best-effort .bak copy next to the file; the real file is written atomically",
}


def check_case_slice(ctx: Ctx) -> list[Violation]:
    out = []
    for rel in S.files(ctx.root, *CASE_GLOBS):
        src = S.load(ctx.root, rel)
        kotlin = rel.endswith(".kt")
        tests = [] if kotlin else S.test_ranges(src)
        m = S.masked(src)
        for f in S.functions(src):
            if S.in_ranges(f.start, tests):
                continue
            body = m[f.body:f.end]
            low = r"\.(?:lowercase|uppercase)\(\)" if kotlin else r"\.to_(?:lowercase|uppercase)\(\)"
            names = set()
            for a in re.finditer(rf"\b(?:let|val|var)\s+(?:mut\s+)?(\w+)\s*(?::\s*[\w<>]+\s*)?=\s*([\w.]+?)(?:\.trim\(\))?{low}", body):
                names.add(a.group(1))
                names.add(a.group(2).split(".")[0])
            if not names:
                continue
            alt = "|".join(re.escape(n) for n in sorted(names))
            sl = rf"\b({alt})\.substring\(" if kotlin else rf"\b({alt})\s*\[[^\]]*\.\.[^\]]*\]"
            for s in re.finditer(sl, body):
                line = src.line_of(f.body + s.start())
                out.append(Violation("robust-case-slice", rel, line, f"{f.name}() slices {s.group(1)!r} by byte index after a case change. "
                                     "Rule: case changes can change byte lengths; search the original text or map indices "
                                     "(docs/guardrails.md#8-robustness)", key=f"{f.name}: {s.group(0)}"))
    return out


def check_unwrap(ctx: Ctx) -> list[Violation]:
    out = []
    for rel in S.files(ctx.root, *CORE_LIB, exclude=CORE_NOT_LIB):
        src = S.load(ctx.root, rel)
        tests = S.test_ranges(src)
        m = S.masked(src)
        fns = S.functions(src)
        for u in re.finditer(r"\.(unwrap|expect)\s*\(", m):
            if S.in_ranges(u.start(), tests):
                continue
            f = S.enclosing_fn(fns, u.start())
            name = f.name if f else "?"
            line = src.line_of(u.start())
            stmt = src.line_text(line).strip()
            if any(k[0] == rel and k[1] == name and k[2] in stmt for k in ALLOWED_UNWRAP):
                continue
            out.append(Violation("robust-unwrap", rel, line, f".{u.group(1)}() in {name}(). Rule: core library code never panics on "
                                 "network, file or user input; return the error or fall back (docs/guardrails.md#8-robustness)",
                                 key=f"{name}: {stmt}"))
    return out


def check_atomic_writes(ctx: Ctx) -> list[Violation]:
    out = []
    for rel in S.files(ctx.root, "desktop/**/*.rs", "core/*/src/**/*.rs", exclude=CORE_NOT_LIB):
        src = S.load(ctx.root, rel)
        tests = S.test_ranges(src)
        fns = S.functions(src)
        for c, o, e in S.calls(src, r"\b(?:std::)?fs::write|\bFile::create"):
            if S.in_ranges(c.start(), tests):
                continue
            f = S.enclosing_fn(fns, c.start())
            name = f.name if f else "?"
            if (rel, name) in ATOMIC_WRITERS or (rel, name) in ALLOWED_WRITES:
                continue
            target = src.code[o + 1:e]
            body = src.code[f.body:f.end] if f else target
            # The path written: the first argument, or the variable it came from in this function.
            first = target.split(",")[0].strip().lstrip("&")
            origin = first
            v = re.match(r"(\w+)$", first)
            if v:
                d = re.search(rf"\blet\s+(?:mut\s+)?{v.group(1)}\s*=\s*([^;]+);", body)
                origin = d.group(1) if d else first
            if STORE_FILES.search(first) or STORE_FILES.search(origin) or re.search(r"\bpath\(\)", first):
                line = src.line_of(c.start())
                out.append(Violation("robust-atomic-write", rel, line, f"{name}() writes a store file directly ({first[:50]}). Rule: history, "
                                     "settings and session files go through the atomic unique-temp writer (voxa_store::write_atomic or "
                                     "store.rs), so a crash or a second process never leaves half a file (docs/guardrails.md#8-robustness)",
                                     key=f"{name}: {src.line_text(line).strip()}"))
    return out


UNIQUE = re.compile(r"process::id\(\)|nanos|SEQ|fetch_add|uuid|rand")


def check_unique_temp(ctx: Ctx) -> list[Violation]:
    """Every write-a-temp-then-rename function: the temp's name must be unique."""
    out = []
    for rel in S.files(ctx.root, "desktop/**/*.rs", "core/*/src/**/*.rs", exclude=CORE_NOT_LIB):
        src = S.load(ctx.root, rel)
        tests = S.test_ranges(src)
        for f in S.functions(src):
            if S.in_ranges(f.start, tests):
                continue
            name = f.name
            body = src.code[f.body:f.end]
            t = re.search(r"\blet\s+tmp\s*=\s*([^;]+);", body)
            if t and re.search(r"fs::rename\(|replace_file\(", body) and not UNIQUE.search(t.group(1)):
                line = src.line_of(f.body + t.start())
                out.append(Violation("robust-atomic-write", rel, line, f"{name}() writes through a fixed temporary name ({t.group(1).strip()[:50]}). "
                                     "Rule: the temp file is unique per write (process id plus a counter or time, as in "
                                     "voxa_chatgpt::write_unique_then_rename), so two writers never clobber one temp file "
                                     "(docs/guardrails.md#8-robustness)", key=f"{name}: fixed temp"))
    return out


RENAME_HELPER = "core/voxa-core/src/fsx.rs"


def check_rename(ctx: Ctx) -> list[Violation]:
    """No bare fs::rename outside the shared helper: on Windows a rename over a file that an
    antivirus scan or another save holds for a moment fails, and one try loses the save."""
    out = []
    for rel in S.files(ctx.root, "desktop/**/*.rs", "core/*/src/**/*.rs", exclude=CORE_NOT_LIB):
        if rel == RENAME_HELPER:
            continue
        src = S.load(ctx.root, rel)
        tests = S.test_ranges(src)
        for m in re.finditer(r"\bfs::rename\(", src.code):
            if S.in_ranges(m.start(), tests):
                continue
            out.append(Violation("robust-rename", rel, src.line_of(m.start()), "A bare fs::rename. Rule: replace files with "
                                 "voxa_core::fsx::replace_file, which retries while Windows reports the target busy "
                                 "(antivirus, another save), so a save is never silently lost (docs/guardrails.md#8-robustness)"))
    return out


def check(ctx: Ctx) -> list[Violation]:
    return (check_case_slice(ctx) + check_unwrap(ctx) + check_atomic_writes(ctx) + check_unique_temp(ctx) + check_rename(ctx)
        + check_strip_sync(ctx) + check_toast_thread(ctx))


# Synchronous window calls on the desktop: allowed only where nothing in GPUI is borrowed.
WINDOW_CALL = re.compile(r"\b(SetWindowPos|ShowWindow|ShowWindowAsync|MoveWindow|SetWindowPlacement)\s*\(")
ALLOWED_WINDOW_CALLS = {
    ("desktop/app/src/win.rs", "place_now", "SetWindowPos"): "the strip's posted message, run from the message loop",
    ("desktop/app/src/win.rs", "strip_message", "ShowWindowAsync"): "the strip's posted message, run from the message loop",
    ("desktop/app/src/win.rs", "no_activate_proc", "ShowWindow"): "hides the strip GPUI showed on a display change, from the message loop",
    ("desktop/app/src/win.rs", "make_overlay", "SetWindowPos"): "styles a window GPUI is still creating, before its callbacks are wired",
    ("desktop/app/src/win.rs", "switch_to", "ShowWindow"): "restores another app's window (the paste target), never a Voxa window",
    ("desktop/app/src/win.rs", "bring_to_front", "ShowWindow"): "the main window in its own process, never the strip",
}


def check_strip_sync(ctx: Ctx) -> list[Violation]:
    """robust-strip-sync: the strip is moved, shown and hidden only by posting to its own message
    queue (win.rs post_strip). A synchronous SetWindowPos / ShowWindow from inside a GPUI update
    re-enters GPUI's window procedure: "RefCell already borrowed" at the 1.0.0 start."""
    out = []
    for rel in S.files(ctx.root, "desktop/app/src/**/*.rs"):
        src = S.load(ctx.root, rel)
        m = S.masked(src)
        tests = S.test_ranges(src)
        fns = S.functions(src)
        for hit in WINDOW_CALL.finditer(m):
            if S.in_ranges(hit.start(), tests):
                continue
            f = S.enclosing_fn(fns, hit.start())
            name = f.name if f else "?"
            if (rel, name, hit.group(1)) in ALLOWED_WINDOW_CALLS:
                continue
            line = src.line_of(hit.start())
            out.append(Violation("robust-strip-sync", rel, line, f"{hit.group(1)} in {name}(). Rule: move, show or hide the strip "
                                 "through os::strip_visible / place_strip / move_strip, which post to the strip's message queue; "
                                 "a synchronous window call inside GPUI logs \"RefCell already borrowed\" and drops the move "
                                 "(docs/guardrails.md#8-robustness)", key=f"{name}: {src.line_text(line).strip()}"))
    return out


TOAST_FILE = "desktop/app/src/notify_win.rs"


def check_toast_thread(ctx: Ctx) -> list[Violation]:
    """robust-toast-thread: WinRT toast calls run through off_ui (a worker thread), never on the
    UI thread, where COM's modal loop dispatched window messages into a borrowed GPUI."""
    out = []
    for rel in S.files(ctx.root, TOAST_FILE):
        src = S.load(ctx.root, rel)
        m = S.masked(src)
        tests = S.test_ranges(src)
        fns = S.functions(src)
        hits = [(h, "ToastNotificationManager") for h in re.finditer(r"\bToastNotificationManager::", m)]
        hits += [(h, "post") for h in re.finditer(r"(?<!fn )\bpost\s*\(", m)]
        for hit, what in hits:
            if S.in_ranges(hit.start(), tests):
                continue
            f = S.enclosing_fn(fns, hit.start())
            if what == "ToastNotificationManager" and f and f.name == "post":
                continue
            line_start = m.rfind("\n", 0, hit.start()) + 1
            if "off_ui(" in m[line_start:hit.start()]:
                continue
            line = src.line_of(hit.start())
            out.append(Violation("robust-toast-thread", rel, line, f"{what} called on the UI thread in "
                                 f"{f.name if f else '?'}(). Rule: toast calls go through off_ui(|| ...); on the UI thread "
                                 "COM's modal loop re-enters GPUI (\"RefCell already borrowed\") (docs/guardrails.md#8-robustness)",
                                 key=src.line_text(line).strip()))
    return out
