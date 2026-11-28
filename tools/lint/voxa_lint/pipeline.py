"""Rule 12, the release pipeline: the Voxa 1.0.0 release review's decisions (docs/release-runbook.md).

  release-placeholder-guard
                  no release build can ship placeholder update keys: core/voxa-update/build.rs uses
                  release_refused(), the Android build fails a release with them (releaseRefusal),
                  and desktop/windows/voxa.iss stops ISCC with #error unless /DAllowPlaceholderKeys.
  release-android-signing
                  Android 1.0.0 on is signed with ONE release key, never the debug key and never a
                  rotation lineage (owner decision 2026-11-28): build.gradle.kts has no apksigner
                  lineage step, refuses a debug-key release and checks the APK signer after
                  packaging; android/signing/forbidden-signers.txt lists the test-build signer
                  e648aa43...; android-feed.yml refuses any APK not signed by the pinned release
                  certificate. With --release, android/signing/release-cert.sha256 must hold it.
  release-feed-guard
                  the update feeds only move forward and can be paused: android-feed.yml runs from
                  its tag ref (refs/tags/$TAG), both feed jobs call `guard-forward`, every job that
                  writes k.pem removes it with a trap and checks the secret is set, and the
                  feed-pause.yml kill switch exists.
  release-readme-links
                  every README download link names a file the release actually uploads, for the
                  workspace version (Voxa-Setup-V.exe, Voxa-V-mac.dmg, Voxa-Android-V-arm64-v8a.apk,
                  Voxa-Android-V-universal.apk, the version-free desktop copies), under the right tag.
  release-runbook docs/release-runbook.md exists with the ordered steps and the REAL-HARDWARE
                  RELEASE GATE, and the README points to it.
"""

from __future__ import annotations

import re

from .core import Ctx, Violation

DOC = "(docs/guardrails.md#12-release-pipeline)"
TEST_BUILD_SIGNER = "e648aa43bbc2a5e431a0f46a6e6d20cfd9422ec6ebe723922e8c86986fad802d"


def _read(ctx: Ctx, rel: str) -> str | None:
    return (ctx.root / rel).read_text(encoding="utf-8", errors="replace") if ctx.exists(rel) else None


def _digests(text: str) -> list[str]:
    out = []
    for line in text.splitlines():
        v = line.split("#")[0].strip().lower()
        if re.fullmatch(r"[0-9a-f]{64}", v):
            out.append(v)
    return out


def check_placeholder_guard(ctx: Ctx) -> list[Violation]:
    out = []

    def bad(rel: str, msg: str, key: str):
        out.append(Violation("release-placeholder-guard", rel, 1, f"{msg}. Rule: a release build never ships placeholder update "
                             f"keys; a copy built that way could never update itself {DOC}", key=key))

    build = _read(ctx, "core/voxa-update/build.rs")
    if build is not None and "release_refused(" not in build:
        bad("core/voxa-update/build.rs", "build.rs does not refuse a release build with placeholder keys", "build.rs")
    gradle = _read(ctx, "android/app/build.gradle.kts")
    if gradle is not None and not ("placeholderUpdateKeys()" in gradle and "releaseRefusal(" in gradle):
        bad("android/app/build.gradle.kts", "the Android release build does not check the update keys", "gradle")
    iss = _read(ctx, "desktop/windows/voxa.iss")
    if iss is not None and not (re.search(r"#error[^\n]*placeholder", iss, re.I) and "AllowPlaceholderKeys" in iss and "keys.rs" in iss):
        bad("desktop/windows/voxa.iss", "ISCC builds an installer with placeholder update keys", "iss")
    return out


def check_android_signing(ctx: Ctx) -> list[Violation]:
    out = []

    def bad(rel: str, msg: str, key: str, line: int = 1):
        out.append(Violation("release-android-signing", rel, line, f"{msg}. Rule: Android releases are signed with the one "
                             f"permanent release key, never the debug key and with no rotation lineage {DOC}", key=key))

    gradle = _read(ctx, "android/app/build.gradle.kts")
    if gradle is not None:
        for m in re.finditer(r"--lineage|--next-signer|apksigner\"?,\s*\"rotate", gradle):
            bad("android/app/build.gradle.kts", f"the build signs with a key lineage ({m.group(0)})", "lineage", gradle.count("\n", 0, m.start()) + 1)
        for need, what in (("releaseKeyIsDebug()", "a debug-key release"), ("verifyReleaseSigner", "the signer check after packaging")):
            if need not in gradle:
                bad("android/app/build.gradle.kts", f"the build does not refuse {what}", need)
    forbidden = _read(ctx, "android/signing/forbidden-signers.txt")
    if gradle is not None and (forbidden is None or TEST_BUILD_SIGNER not in _digests(forbidden)):
        bad("android/signing/forbidden-signers.txt", "the test-build signer is not on the forbidden list", "forbidden")
    feed = _read(ctx, ".github/workflows/android-feed.yml")
    if feed is not None and not ("release-cert.sha256" in feed and "forbidden-signers.txt" in feed):
        bad(".github/workflows/android-feed.yml", "the feed publishes whatever certificate the APK carries", "feed signer")
    if ctx.release and gradle is not None:
        pinned = _digests(_read(ctx, "android/signing/release-cert.sha256") or "")
        if not pinned:
            bad("android/signing/release-cert.sha256", "the release certificate is not pinned yet", "unpinned")
        elif forbidden and set(pinned) & set(_digests(forbidden)):
            bad("android/signing/release-cert.sha256", "the pinned release certificate is a forbidden test-build signer", "pinned forbidden")
    return out


def check_feed_guard(ctx: Ctx) -> list[Violation]:
    out = []

    def bad(rel: str, msg: str, key: str):
        out.append(Violation("release-feed-guard", rel, 1, f"{msg}. Rule: a feed only moves forward, can be paused, and its "
                             f"signing key never outlives the step {DOC}", key=key))

    feed = _read(ctx, ".github/workflows/android-feed.yml")
    if feed is not None and 'GITHUB_REF" = "refs/tags/$TAG"' not in feed:
        bad(".github/workflows/android-feed.yml", "android-feed runs from any ref (the release environment admits tags only)", "ref")
    for rel in (".github/workflows/android-feed.yml", ".github/workflows/desktop.yml"):
        text = _read(ctx, rel)
        if text is not None and "openssl pkeyutl -sign" in text and "guard-forward" not in text:
            bad(rel, "the feed is published without checking that its version moves forward", "guard-forward")
    for rel in (".github/workflows/android-feed.yml", ".github/workflows/desktop.yml", ".github/workflows/feed-pause.yml"):
        text = _read(ctx, rel)
        if text is None or "k.pem" not in text:
            continue
        if "trap 'rm -f k.pem' EXIT" not in text:
            bad(rel, "k.pem can be left on the runner when a command fails", "trap")
        if '[ -n "$VOXA_UPDATE_KEY" ]' not in text:
            bad(rel, "signing does not check that VOXA_UPDATE_KEY is set", "empty secret")
    if (ctx.exists(".github/workflows/desktop.yml") or ctx.exists(".github/workflows/android-feed.yml")) and not ctx.exists(".github/workflows/feed-pause.yml"):
        bad(".github/workflows/feed-pause.yml", "there is no kill switch workflow", "kill switch")
    return out


LINK = re.compile(r"https://github\.com/blackscythe123/voxa/releases/(?:download/([^/\s)]+)|latest/download)/([^\s)]+)")


def check_readme_links(ctx: Ctx) -> list[Violation]:
    readme = _read(ctx, "README.md")
    cargo = _read(ctx, "Cargo.toml")
    if readme is None or cargo is None:
        return []
    ws = re.search(r"\[workspace\.package\][^\[]*?^version\s*=\s*\"([^\"]+)\"", cargo, re.M | re.S)
    if not ws:
        return []
    v = ws.group(1)
    want = {
        f"Voxa-Setup-{v}.exe": f"v{v}", f"Voxa-{v}-mac.dmg": f"v{v}",
        f"Voxa-Android-{v}-arm64-v8a.apk": f"android-v{v}", f"Voxa-Android-{v}-universal.apk": f"android-v{v}",
    }
    latest = {"Voxa-Setup.exe", "Voxa-mac.dmg"}
    out = []
    for m in LINK.finditer(readme):
        tag, name = m.group(1), m.group(2)
        ok = (tag is None and name in latest) or (tag is not None and want.get(name) == tag)
        if not ok:
            line = readme.count("\n", 0, m.start()) + 1
            out.append(Violation("release-readme-links", "README.md", line, f"README links {name} under {tag or 'latest'}, which the release "
                                 f"never uploads for {v} (android/release.ps1, desktop.yml). Rule: every download link works on day one {DOC}",
                                 key=name))
    return out


def check_runbook(ctx: Ctx) -> list[Violation]:
    if not ctx.exists("README.md") or not ctx.exists("Cargo.toml"):
        return []
    out = []
    book = _read(ctx, "docs/release-runbook.md")
    if book is None or "REAL-HARDWARE RELEASE GATE" not in book:
        out.append(Violation("release-runbook", "docs/release-runbook.md", 1, "the release runbook or its real-hardware release gate is "
                             f"missing. Rule: releases follow the ordered runbook and the gate before friends get a build {DOC}", key="runbook"))
    if "docs/release-runbook.md" not in (_read(ctx, "README.md") or ""):
        out.append(Violation("release-runbook", "README.md", 1, f"the README does not point to docs/release-runbook.md {DOC}", key="readme"))
    return out


def check(ctx: Ctx) -> list[Violation]:
    return check_placeholder_guard(ctx) + check_android_signing(ctx) + check_feed_guard(ctx) + check_readme_links(ctx) + check_runbook(ctx)
