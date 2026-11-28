"""Rule 5, Android stays tiny and lean (voice-only keyboard on the Rust core).

  android-size         the release arm64 APK is at most 6.0 MB. Checked when an APK exists (built
                       locally, or `--apk PATH`); `--require-apk` makes a missing APK a failure.
  android-deps         every dependency in android/app/build.gradle.kts is on ALLOWED_DEPS, every plugin
                       on ALLOWED_PLUGINS. A new library is an owner decision: add it here with the reason.
  android-workmanager  no WorkManager (androidx.work): background work uses JobScheduler.
  android-exact-alarm  no exact alarms: no setExact*/setAlarmClock calls and no SCHEDULE_EXACT_ALARM or
                       USE_EXACT_ALARM permission. The birthday wish uses an inexact window alarm
                       (owner decision, 2026-11-28; notify/BirthdayWish.kt).
"""

from __future__ import annotations

import re
from pathlib import Path

from . import src as S
from .core import Ctx, Violation

BUDGET_BYTES = 6_000_000  # 6.0 MB (decimal, as the size is reported in the verification docs)
APK_GLOB = "android/app/build/outputs/apk/release/*arm64*.apk"
GRADLE = "android/app/build.gradle.kts"

# (configuration, dependency) as written in build.gradle.kts.
ALLOWED_DEPS = {
    ("implementation", "libs.androidx.core.ktx"),
    ("implementation", "libs.androidx.activity.compose"),
    ("implementation", "platform(libs.androidx.compose.bom)"),
    ("implementation", "libs.androidx.ui"),
    ("implementation", "libs.androidx.ui.graphics"),
    ("implementation", "libs.androidx.material3"),
    ("implementation", "libs.androidx.core.splashscreen"),    # a few KB: the back-ported splash screen
    ("implementation", "libs.kotlinx.coroutines.android"),
    ("implementation", '"${libs.jna.get()}@aar"'),              # UniFFI's Kotlin bindings call Rust through JNA
    ("debugImplementation", "libs.androidx.ui.tooling"),       # debug builds only
    ("androidTestImplementation", "libs.androidx.test.runner"),
    ("androidTestImplementation", "libs.androidx.test.ext.junit"),
    ("androidTestImplementation", "libs.junit"),
}
ALLOWED_PLUGINS = {"libs.plugins.android.application", "libs.plugins.kotlin.android", "libs.plugins.kotlin.compose"}

WORKMANAGER = re.compile(r"androidx\.work\b|work-runtime|\bWorkManager\b|\bCoroutineWorker\b|\bListenableWorker\b|\b(OneTime|Periodic)WorkRequest")


EXACT_ALARM = re.compile(r"\.setExact\w*\s*\(|\.setAlarmClock\s*\(|\bSCHEDULE_EXACT_ALARM\b|\bUSE_EXACT_ALARM\b")


def check_exact_alarm(ctx: Ctx) -> list[Violation]:
    out = []
    for rel in S.files(ctx.root, "android/app/src/**/*.kt", "android/app/src/**/*.xml"):
        text = (ctx.root / rel).read_text(encoding="utf-8", errors="replace")
        code = S.lex(text, "kotlin")[0] if rel.endswith(".kt") else text
        for m in EXACT_ALARM.finditer(code):
            line = code.count("\n", 0, m.start()) + 1
            out.append(Violation("android-exact-alarm", rel, line, f"{m.group(0).strip()!r}: Rule: no exact alarms and no exact-alarm "
                                 "permission; use an inexact window alarm (AlarmManager.setWindow) or JobScheduler "
                                 "(docs/guardrails.md#5-android)", key=m.group(0).strip()))
    return out


def check_size(ctx: Ctx) -> list[Violation]:
    apks = [ctx.apk] if ctx.apk else sorted(ctx.root.glob(APK_GLOB))
    if not apks:
        msg = "android-size: no release arm64 APK found (build with ./gradlew assembleRelease, or pass --apk PATH)"
        if ctx.require_apk:
            return [Violation("android-size", APK_GLOB, 0, msg + "; --require-apk was given", key="missing apk")]
        ctx.notes.append(msg + "; size check skipped")
        return []
    out = []
    for apk in apks:
        apk = Path(apk)
        if not apk.exists():
            out.append(Violation("android-size", str(apk), 0, "APK not found", key="missing apk"))
            continue
        size = apk.stat().st_size
        ctx.notes.append(f"android-size: {apk.name} is {size / 1e6:.2f} MB ({size} bytes), budget {BUDGET_BYTES / 1e6:.1f} MB")
        if size > BUDGET_BYTES:
            out.append(Violation("android-size", apk.as_posix(), 0, f"release arm64 APK is {size / 1e6:.2f} MB, over the {BUDGET_BYTES / 1e6:.1f} MB "
                                 "budget. Rule: Android stays tiny and low-RAM (docs/guardrails.md#5-android)", key="apk size"))
    return out


def check_deps(ctx: Ctx) -> list[Violation]:
    if not ctx.exists(GRADLE):
        return []
    src = S.load(ctx.root, GRADLE)
    out = []
    for block, allowed, kind in (("dependencies", None, "dependency"), ("plugins", ALLOWED_PLUGINS, "plugin")):
        m = re.search(rf"^{block}\s*\{{", src.code, re.M)
        if not m:
            continue
        body_end = S.match_brace(src.code, m.end() - 1)
        for d in re.finditer(r"^\s*(\w+)\((.*)\)\s*$", src.code[m.end():body_end], re.M):
            conf, dep = d.group(1), d.group(2).strip()
            line = src.line_of(m.end() + d.start(1))
            ok = dep in allowed if allowed is not None else (conf, dep) in ALLOWED_DEPS
            if block == "plugins":
                ok = conf == "alias" and dep in ALLOWED_PLUGINS
            if not ok:
                out.append(Violation("android-deps", GRADLE, line, f"{kind} {conf}({dep}) is not on the allow-list. Rule: no new Android "
                                     "dependencies without the owner's OK; add it to ALLOWED_DEPS in tools/lint/voxa_lint/android.py with the "
                                     "reason and its size cost (docs/guardrails.md#5-android)", key=f"{conf}({dep})"))
    return out


def check_workmanager(ctx: Ctx) -> list[Violation]:
    out = []
    for rel in S.files(ctx.root, "android/**/*.kts", "android/**/*.toml", "android/app/src/**/*.kt", "android/app/src/**/*.xml"):
        text = (ctx.root / rel).read_text(encoding="utf-8", errors="replace")
        code = S.lex(text, "kotlin")[0] if rel.endswith((".kt", ".kts")) else text
        for m in WORKMANAGER.finditer(code):
            line = code.count("\n", 0, m.start()) + 1
            out.append(Violation("android-workmanager", rel, line, f"{m.group(0)!r}: Rule: no WorkManager, background work uses "
                                 "JobScheduler (see notify/VoxaJobs.kt; docs/guardrails.md#5-android)", key=m.group(0)))
    return out


def check(ctx: Ctx) -> list[Violation]:
    return check_size(ctx) + check_deps(ctx) + check_workmanager(ctx) + check_exact_alarm(ctx)
