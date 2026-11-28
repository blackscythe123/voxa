"""Rule 9, self update: the owner's update decisions (docs/updates.md, docs/update-keys.md).

  update-hosts       the updater's code (core/voxa-update/src, desktop/app/src/update.rs,
                     notify/Updater.kt) never names a plain http URL, never turns certificate checks
                     off (danger_accept_invalid*), and every URL literal is under
                     https://github.com/blackscythe123/voxa/ or one of GitHub's release asset hosts.
  update-key-present core/voxa-update/src/keys.rs holds at least 2 keys of 64 hex characters with
                     unique ids. All-zero placeholders are a note in everyday runs and a violation
                     with --release (a tagged release must ship the real public keys).
  update-secret      no private key in the tree: no PEM private key block, no Ed25519 PKCS#8 prefix
                     (the base64 of 302e020100300506032b6570) and no keystore or .p12 file, outside the TEST fixtures.
  update-gate        the only desktop function that starts the installer is update.rs
                     launch_installer(), and it calls can_install_now (or can_install_on_request)
                     and verify_file before it spawns. On Android, every PackageInstaller commit in
                     Updater.kt comes after the live dictation check in the same function.
  android-release-key buildTypes.release never signs with signingConfigs.getByName("debug") directly;
                     the debug fallback lives in a helper gated on the release key being absent.
  android-onboarding-permissions
                     SetupScreen.kt only routes to the microphone permission, the keyboard settings
                     and ACTION_MANAGE_UNKNOWN_APP_SOURCES; never POST_NOTIFICATIONS.
"""

from __future__ import annotations

import re

from . import src as S
from .core import Ctx, Violation

HOST_FILES = ("core/voxa-update/src/**/*.rs", "desktop/app/src/update.rs", "android/app/src/main/java/com/voxa/android/notify/Updater.kt")
ALLOWED_URL = re.compile(r"^https://(github\.com/blackscythe123/voxa/|release-assets\.githubusercontent\.com/|objects\.githubusercontent\.com/)")
DOC = "(docs/guardrails.md#10-update)"


def _code_ranges(src: S.Source) -> list[tuple[int, int]]:
    return [] if src.rel.endswith(".kt") else S.test_ranges(src)


def check_hosts(ctx: Ctx) -> list[Violation]:
    out = []
    for rel in S.files(ctx.root, *HOST_FILES):
        if "/src/bin/" in rel:
            continue
        src = S.load(ctx.root, rel)
        tests = _code_ranges(src)
        for s in src.strings:
            if S.in_ranges(s.start, tests):
                continue
            v = s.value
            if "http://" in v:
                out.append(Violation("update-hosts", rel, s.line, f"plain http URL {v[:60]!r}. Rule: every update request is HTTPS "
                                     f"on github.com or its release asset hosts {DOC}", key=v[:80]))
            elif v.startswith("https://") and not ALLOWED_URL.match(v):
                out.append(Violation("update-hosts", rel, s.line, f"URL {v[:60]!r} is not under the repo or GitHub's release asset hosts. "
                                     f"Rule: the only network peers for updates are github.com and its release hosts {DOC}", key=v[:80]))
        for m in re.finditer(r"danger_accept_invalid\w*", src.code):
            if S.in_ranges(m.start(), tests):
                continue
            out.append(Violation("update-hosts", rel, src.line_of(m.start()), f"{m.group(0)} turns certificate checks off. Rule: update "
                                 f"traffic is verified HTTPS {DOC}", key=m.group(0)))
    return out


def check_keys(ctx: Ctx) -> list[Violation]:
    rel = "core/voxa-update/src/keys.rs"
    if not ctx.exists(rel):
        return []
    src = S.load(ctx.root, rel)
    out = []
    # The literals themselves (code has strings intact; comments blanked).
    keys = [(m.group(1), m.group(2), src.line_of(m.start())) for m in re.finditer(r"pub const (K\d+)_HEX\s*:\s*&str\s*=\s*\"([^\"]*)\"", src.code)]
    ids = re.findall(r"\((\d+)\s*,\s*hex32\(", src.code)
    if len(keys) < 2:
        out.append(Violation("update-key-present", rel, 1, f"keys.rs has {len(keys)} update key(s). Rule: K1 (active) and K2 (offline "
                             f"backup) are both compiled in {DOC}", key="count"))
    if len(ids) != len(set(ids)):
        out.append(Violation("update-key-present", rel, 1, f"key ids repeat ({', '.join(ids)}). Rule: every key id is unique {DOC}", key="ids"))
    for name, value, line in keys:
        if not re.fullmatch(r"[0-9a-fA-F]{64}", value):
            out.append(Violation("update-key-present", rel, line, f"{name} is not 64 hex characters. Rule: an Ed25519 public key is 32 "
                                 f"bytes {DOC}", key=f"{name} shape"))
        elif set(value) == {"0"}:
            if ctx.release:
                out.append(Violation("update-key-present", rel, line, f"{name} is still the all-zero placeholder. Rule: a release ships the "
                                     f"real public keys (docs/update-keys.md) {DOC}", key=f"{name} placeholder"))
            else:
                ctx.notes.append(f"update-key-present: {name} is a placeholder (fine for everyday builds; --release and tagged release builds fail)")
    return out


SECRET_TEXT = re.compile(r"-----BEGIN (?:ENCRYPTED |RSA |EC |OPENSSH |DSA )?PRIVATE KEY-----|" + "MC4CAQAw" + "BQYDK2Vw")
SECRET_FILES = re.compile(r"\.(jks|keystore|p12|pfx)$", re.I)
SECRET_ALLOWED = (r"^core/voxa-update/tests/fixtures/", r"^tools/lint/fixtures/")


def check_secrets(ctx: Ctx) -> list[Violation]:
    out = []
    for rel in S.files(ctx.root, "**/*"):
        if any(re.search(a, rel) for a in SECRET_ALLOWED):
            continue
        if SECRET_FILES.search(rel):
            out.append(Violation("update-secret", rel, 1, f"{rel} looks like a keystore or certificate bundle. Rule: signing keys never "
                                 f"live in the repo (docs/update-keys.md) {DOC}", key="file"))
            continue
        p = ctx.root / rel
        try:
            if p.stat().st_size > 2_000_000:
                continue
            text = p.read_bytes().decode("utf-8", errors="ignore")
        except OSError:
            continue
        for m in SECRET_TEXT.finditer(text):
            out.append(Violation("update-secret", rel, text.count("\n", 0, m.start()) + 1, "a private key is committed. Rule: no private key "
                                 f"outside the labelled TEST fixtures; the owner keeps real keys offline and in the release environment {DOC}",
                                 key=m.group(0)[:40]))
    return out


INSTALLER_SPAWN = re.compile(r"VERYSILENT|installer_args\s*\(")
LIVE_CHECK = re.compile(r"\bdictationLive\s*\(")


def check_gate(ctx: Ctx) -> list[Violation]:
    out = []
    for rel in S.files(ctx.root, "desktop/app/src/**/*.rs"):
        src = S.load(ctx.root, rel)
        tests = S.test_ranges(src)
        m = S.masked(src)
        for f in S.functions(src):
            if S.in_ranges(f.start, tests):
                continue
            body = src.code[f.body:f.end]
            spawn = re.search(r"\.spawn\s*\(", m[f.body:f.end])
            if not spawn or not INSTALLER_SPAWN.search(body):
                continue
            line = src.line_of(f.body + spawn.start())
            if (rel, f.name) != ("desktop/app/src/update.rs", "launch_installer"):
                out.append(Violation("update-gate", rel, line, f"{f.name}() starts the installer. Rule: only update.rs launch_installer() "
                                     f"does, behind the install gate {DOC}", key=f"{f.name}: spawn"))
                continue
            before = body[:spawn.start()]
            for need, what in ((r"\bcan_install_(?:now|on_request)\s*\(", "can_install_now"), (r"\bverify_file\s*\(", "verify_file")):
                if not re.search(need, before):
                    out.append(Violation("update-gate", rel, line, f"launch_installer() spawns without calling {what} first. Rule: never "
                                         f"install during a take, and re-verify the staged file right before launch {DOC}",
                                         key=f"{f.name}: {what}"))
    for rel in S.files(ctx.root, "android/app/src/main/java/com/voxa/android/notify/Updater.kt"):
        src = S.load(ctx.root, rel)
        m = S.masked(src)
        for f in S.functions(src):
            for c in re.finditer(r"\.commit\s*\(", m[f.body:f.end]):
                if not LIVE_CHECK.search(m[f.body:f.body + c.start()]):
                    line = src.line_of(f.body + c.start())
                    out.append(Violation("update-gate", rel, line, f"{f.name}() commits a PackageInstaller session without checking "
                                         f"dictationLive() first. Rule: never install while a dictation is live {DOC}", key=f"{f.name}: commit"))
    return out


def check_android_signing(ctx: Ctx) -> list[Violation]:
    rel = "android/app/build.gradle.kts"
    if not ctx.exists(rel):
        return []
    src = S.load(ctx.root, rel)
    m = S.masked(src)
    out = []
    for b in re.finditer(r"\bbuildTypes\s*\{", m):
        end = S.match_brace(m, b.end() - 1)
        for r in re.finditer(r"\b(?:release|getByName\(\s*\"release\"\s*\))\s*\{", m[b.end():end]):
            ro = b.end() + r.end() - 1
            re_end = S.match_brace(m, ro)
            block = src.code[ro:re_end]
            for d in re.finditer(r"signingConfigs\.getByName\(\s*\"debug\"\s*\)", block):
                out.append(Violation("android-release-key", rel, src.line_of(ro + d.start()), "the release build type signs with the debug key "
                                     "directly. Rule: release APKs use the one permanent release key; the debug fallback lives in "
                                     f"a helper that only a dev release (-Pvoxa.devRelease=true) reaches {DOC}",
                                     key="release: debug signing"))
    return out


ALLOWED_ROUTES = {"ACTION_MANAGE_UNKNOWN_APP_SOURCES", "ACTION_INPUT_METHOD_SETTINGS", "ACTION_APPLICATION_DETAILS_SETTINGS"}


def check_onboarding(ctx: Ctx) -> list[Violation]:
    out = []
    for rel in S.files(ctx.root, "android/app/src/main/java/com/voxa/android/ui/SetupScreen.kt"):
        src = S.load(ctx.root, rel)
        for m in re.finditer(r"POST_NOTIFICATIONS", src.code):
            out.append(Violation("android-onboarding-permissions", rel, src.line_of(m.start()), "onboarding asks for notifications. Rule: "
                                 "never during onboarding; the one onboarding exception is install unknown apps (docs/notifications.md) "
                                 f"{DOC}", key="POST_NOTIFICATIONS"))
        for m in re.finditer(r"Manifest\.permission\.(\w+)", src.code):
            if m.group(1) not in ("RECORD_AUDIO", "POST_NOTIFICATIONS"):
                out.append(Violation("android-onboarding-permissions", rel, src.line_of(m.start()), f"onboarding asks for {m.group(1)}. "
                                     f"Rule: onboarding only asks for the microphone {DOC}", key=m.group(1)))
        for m in re.finditer(r"Settings\.(ACTION_\w+)", src.code):
            if m.group(1) not in ALLOWED_ROUTES:
                out.append(Violation("android-onboarding-permissions", rel, src.line_of(m.start()), f"onboarding opens {m.group(1)}. Rule: "
                                     "onboarding only routes to the microphone, the keyboard settings and install unknown apps "
                                     f"{DOC}", key=m.group(1)))
    return out


def check(ctx: Ctx) -> list[Violation]:
    return check_hosts(ctx) + check_keys(ctx) + check_secrets(ctx) + check_gate(ctx) + check_android_signing(ctx) + check_onboarding(ctx)
