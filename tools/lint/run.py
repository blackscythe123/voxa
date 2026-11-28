#!/usr/bin/env python3
"""Voxa's guardrails: the owner's product decisions as checks that fail the build.

    python tools/lint/run.py                 every rule on the repo (exit 1 on any violation)
    python tools/lint/run.py --self-test     prove each rule still catches its fixture violations
    python tools/lint/run.py --only copy     one group: contrast copy privacy focus android isolation release robust licence
    python tools/lint/run.py --apk PATH      also check this APK's size (else a built one is found, or skipped)
    python tools/lint/run.py --require-apk   fail when there is no APK to measure
    python tools/lint/run.py --keys          show each violation's allow-list key
    python tools/lint/run.py --release       a tagged release: placeholder update keys fail too

docs/guardrails.md explains every rule. Exceptions live in tools/lint/allowlist.py, with reasons.
Standard library only (Python 3.10+).
"""

from __future__ import annotations

import argparse
import sys
import tempfile
from collections import Counter
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

from voxa_lint import android, contrast, copy_rules, focus, isolation, licence, privacy, release, robust, update  # noqa: E402
from voxa_lint.core import Ctx, Violation  # noqa: E402

GROUPS = {
    "contrast": contrast.check,
    "copy": copy_rules.check,
    "privacy": privacy.check,
    "focus": focus.check,
    "android": android.check,
    "isolation": isolation.check,
    "release": release.check,
    "robust": robust.check,
    "licence": licence.check,
    "update": update.check,
}


def run(ctx: Ctx, only: list[str] | None = None) -> list[Violation]:
    out: list[Violation] = []
    for name, check in GROUPS.items():
        if only and name not in only:
            continue
        out += check(ctx)
    return out


def main_lint(args) -> int:
    from allowlist import ALLOW

    root = HERE.parent.parent
    ctx = Ctx(root, apk=Path(args.apk) if args.apk else None, require_apk=args.require_apk, release=args.release)
    found = run(ctx, args.only)
    used = set()
    failing = []
    for v in found:
        hit = next((i for i, a in enumerate(ALLOW) if a.covers(v)), None)
        if hit is None:
            failing.append(v)
        else:
            used.add(hit)
    for n in ctx.notes:
        print(f"note: {n}")
    allowed = len(found) - len(failing)
    if allowed:
        print(f"note: {allowed} known exception(s) on the allow-list (tools/lint/allowlist.py)")
    for i, a in enumerate(ALLOW):
        if i not in used and (not args.only or any(a.rule.startswith(g) for g in args.only)):
            print(f"warning: allow-list entry no longer matches anything, delete it: [{a.rule}] {a.path} {a.match!r}")
    for v in failing:
        print(v.render())
        if args.keys:
            print(f"    key: {v.key}")
    if failing:
        print(f"\n{len(failing)} guardrail violation(s). See docs/guardrails.md; fix the code, or (rarely, with a reason) add "
              "an entry to tools/lint/allowlist.py.")
        return 1
    print(f"guardrails: all {len(GROUPS) if not args.only else len(args.only)} rule groups pass")
    return 0


def main_self_test() -> int:
    """Each fixture tree mirrors the repo's paths; expected.txt lists `count rule path` for every
    violation the checks must find there, so a check that stops catching (or starts over-catching)
    fails here."""
    fixtures = HERE / "fixtures" / "bad"
    expected: Counter = Counter()
    for line in (HERE / "fixtures" / "expected.txt").read_text(encoding="utf-8").splitlines():
        line = line.split("#")[0].strip()
        if line:
            n, rule, path = line.split()
            expected[(rule, path)] = int(n)
    ok = True
    with tempfile.TemporaryDirectory() as tmp:
        # The size budget: a sparse file just over 6.0 MB stands in for the APK.
        apk = Path(tmp) / "app-arm64-v8a-release.apk"
        with open(apk, "wb") as f:
            f.seek(android.BUDGET_BYTES)
            f.write(b"\0")
        # release=True: the fixture's placeholder update key must be caught as a release would.
        found = run(Ctx(fixtures, apk=apk, release=True))
    got = Counter((v.rule, v.path if v.rule != "android-size" else "APK") for v in found)
    for key in sorted(set(expected) | set(got)):
        if expected[key] != got[key]:
            ok = False
            print(f"FAIL {key[0]} {key[1]}: expected {expected[key]}, found {got[key]}")
            for v in found:
                if (v.rule, v.path) == key:
                    print(f"    {v.render()}")
    rules = {r for r, _ in expected}
    every = {"contrast", "copy-emoji", "copy-dash", "copy-case", "copy-occasion", "privacy-log", "privacy-notify", "focus-steal",
             "focus-notify", "android-size", "android-deps", "android-workmanager", "android-exact-alarm", "android-signing", "isolation-env", "isolation-test-hook",
             "release-latest", "release-version-sync", "robust-case-slice", "robust-unwrap", "robust-atomic-write", "robust-rename", "licence-consistent",
             "update-hosts", "update-key-present", "update-secret", "update-gate", "update-isolation", "android-release-key",
             "android-onboarding-permissions"}
    for r in sorted(every - rules):
        ok = False
        print(f"FAIL rule {r} has no fixture proving it catches a violation")
    # A clean APK passes, and no APK is only a note unless --require-apk.
    with tempfile.TemporaryDirectory() as tmp:
        small = Path(tmp) / "small.apk"
        small.write_bytes(b"\0" * 1000)
        if android.check_size(Ctx(fixtures, apk=small)):
            ok = False
            print("FAIL android-size flagged a 1 KB APK")
        if not android.check_size(Ctx(Path(tmp), require_apk=True)):
            ok = False
            print("FAIL android-size --require-apk passed with no APK")
    # The WCAG maths against known values.
    for fg, bg, want in ((0x000000, 0xFFFFFF, 21.0), (0x7A3F22, 0xFFFFFF, 8.2), (0x777777, 0xFFFFFF, 4.48)):
        r = contrast.ratio(fg, bg)
        if abs(r - want) > 0.05:
            ok = False
            print(f"FAIL contrast ratio #{fg:06X} on #{bg:06X} = {r:.2f}, expected {want}")
    print(f"self-test: {'pass' if ok else 'FAIL'} ({sum(got.values())} fixture violations across {len(rules)} rules)")
    return 0 if ok else 1


def main() -> int:
    # Messages quote the offending text, emojis included; never crash on a cp1252 console.
    for stream in (sys.stdout, sys.stderr):
        if hasattr(stream, "reconfigure"):
            stream.reconfigure(errors="backslashreplace")
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--self-test", action="store_true")
    p.add_argument("--only", nargs="+", choices=sorted(GROUPS))
    p.add_argument("--apk")
    p.add_argument("--require-apk", action="store_true")
    p.add_argument("--keys", action="store_true")
    p.add_argument("--release", action="store_true")
    args = p.parse_args()
    if args.self_test:
        return main_self_test()
    return main_lint(args)


if __name__ == "__main__":
    sys.exit(main())
