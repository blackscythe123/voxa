"""Rule 6, test isolation: a test copy of the desktop app (VOXA_DATA set) never reads or writes the
real user's data, and test hooks only work in a test copy.

  isolation-env        a function under desktop/ that reaches the real per-user folders (the HOME /
                       APPDATA / LOCALAPPDATA variables, `dirs::*_dir()`, `app_data_root()`) must also
                       check VOXA_DATA, unless it is on ALLOWED. The old app's folder (electron_dir) is
                       only reached through app_data_root(), so it is covered too.
  isolation-test-hook  every read of a VOXA_TEST_* variable sits in a function that checks VOXA_DATA
                       (directly, or through test_hooks()).
  update-isolation     in desktop/app/src/update.rs, every function that builds the update client
                       (voxa_update::fetch::Client::new) checks for a test copy: VOXA_DATA or
                       test_hooks() in its own body, or in a function of the same file it calls. So
                       a test copy picks its test feed, and the test seams only open through
                       test_hooks(). (Spawning the installer is guarded by update-gate: the install
                       gate refuses any test copy.)
"""

from __future__ import annotations

import re

from . import src as S
from .core import Ctx, Violation

GLOBS = ("desktop/app/src/**/*.rs", "desktop/signin/src/**/*.rs")

REAL_DIRS = re.compile(
    r"env::var(?:_os)?\(\s*\"(HOME|APPDATA|LOCALAPPDATA|USERPROFILE|XDG_DATA_HOME|XDG_CONFIG_HOME)\"\s*\)"
    r"|\bdirs(?:_next)?::(?:home|data|data_local|config|cache|document|desktop)_dir\s*\("
    r"|\b(app_data_root)\s*\(")
TEST_HOOK = re.compile(r"env::var(?:_os)?\(\s*\"(VOXA_TEST_\w+)\"\s*\)")
# A VOXA_DATA check, or test_hooks() (VOXA_DATA plus a debug build or VOXA_TEST=1).
GUARD = re.compile(r"\"VOXA_DATA\"|(?<!fn )\btest_hooks\(\)")

# (file, function): why it may reach a real folder without checking VOXA_DATA.
ALLOWED = {
    ("desktop/app/src/main.rs", "app_data_root"): "only called from data_dir() and electron_dir(), which both return the VOXA_DATA folder first",
    ("desktop/app/src/mac.rs", "launch_agent"): "Open at login is the OS's per-user login item, not app data; tests never toggle it",
}


def check(ctx: Ctx) -> list[Violation]:
    return check_env(ctx) + check_update(ctx)


def check_env(ctx: Ctx) -> list[Violation]:
    out = []
    for rel in S.files(ctx.root, *GLOBS):
        src = S.load(ctx.root, rel)
        tests = S.test_ranges(src)
        fns = S.functions(src)
        for pattern, rule in ((REAL_DIRS, "isolation-env"), (TEST_HOOK, "isolation-test-hook")):
            for m in pattern.finditer(src.code):
                if S.in_ranges(m.start(), tests):
                    continue
                f = S.enclosing_fn(fns, m.start())
                if f and rule == "isolation-env" and m.group(2) == f.name:
                    continue  # the definition's own name, not a call
                if f and (rel, f.name) in ALLOWED and rule == "isolation-env":
                    continue
                body = src.code[f.start:f.end] if f else ""
                if GUARD.search(body):
                    continue
                line = src.line_of(m.start())
                what = m.group(0).strip()
                if rule == "isolation-env":
                    msg = (f"{what!r} in {f.name if f else '?'}() reaches the real user's folders without checking VOXA_DATA. Rule: a test copy "
                           "(VOXA_DATA set) never reads the old app or real data (docs/guardrails.md#6-isolation)")
                else:
                    msg = (f"{m.group(1)} read in {f.name if f else '?'}() without a VOXA_DATA check in the same function. Rule: VOXA_TEST_* "
                           "hooks only work with a separate VOXA_DATA folder (docs/guardrails.md#6-isolation)")
                out.append(Violation(rule, rel, line, msg, key=f"{f.name if f else '?'}: {what}"))
    return out


UPDATE_FILE = "desktop/app/src/update.rs"


def check_update(ctx: Ctx) -> list[Violation]:
    if not ctx.exists(UPDATE_FILE):
        return []
    src = S.load(ctx.root, UPDATE_FILE)
    tests = S.test_ranges(src)
    fns = S.functions(src)
    guarded = {f.name for f in fns if GUARD.search(src.code[f.start:f.end])}
    out = []
    for m in re.finditer(r"\bClient::new\s*\(", src.code):
        if S.in_ranges(m.start(), tests):
            continue
        f = S.enclosing_fn(fns, m.start())
        body = src.code[f.start:f.end] if f else ""
        called = set(re.findall(r"\b(\w+)\s*\(", body))
        if GUARD.search(body) or called & guarded:
            continue
        out.append(Violation("update-isolation", UPDATE_FILE, src.line_of(m.start()), f"{f.name if f else '?'}() builds the update client "
                             "without a test copy check. Rule: a test copy (VOXA_DATA set) uses its test feed through test_hooks(), "
                             "never the real updater state (docs/guardrails.md#6-isolation)", key=f"{f.name if f else '?'}: Client::new"))
    return out
