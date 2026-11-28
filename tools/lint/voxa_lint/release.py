"""Rule 7, release hygiene. An Android GitHub release never becomes the repo's "latest" release
(the desktop's .../releases/latest/download/Voxa-Setup.exe links must keep pointing at the desktop).

  release-latest  every `gh release create` / `gh release edit` for Android (an APK, or a tag or title
                  that says android) in the docs, scripts or workflows passes --latest=false. The
                  `update-feed` release (both signed feeds) passes --latest=false or --prerelease.
                  The tag-built releases (desktop.yml, android-release.yml) are created with both
                  --prerelease and --latest=false: the owner promotes them after the hardware gate.
  release-version-sync
                  one version everywhere: the workspace version (Cargo.toml) is the voxa.iss default
                  AppVersion and the Android versionName, Android's versionCode follows
                  major*10000 + minor*100 + patch, and desktop.yml passes /DAppVersion to ISCC and
                  checks the tag against the Cargo version before publishing. A mismatch would make
                  the updater reinstall forever or downgrade.
  release-icon-refresh
                  voxa.iss refreshes Windows' icon cache after an install or update: ChangesAssociations=yes
                  in [Setup] (SHChangeNotify SHCNE_ASSOCCHANGED) and a SHChangeNotify call on Voxa.exe and its
                  shortcuts at ssPostInstall. 1.0.0's pinned taskbar shortcut kept the old icon without it.
"""

from __future__ import annotations

import re

from . import src as S
from .core import Ctx, Violation

GLOBS = ("**/*.md", "**/*.sh", "**/*.ps1", "**/*.yml", "**/*.yaml", "**/*.py", "**/*.kts", "**/*.bat", "**/*.cmd")
EXCLUDE = (r"^tools/lint/", r"node_modules/", r"^\.claude/", r"^target/")
CMD = re.compile(r"gh\s+release\s+(create|edit)\b")
ANDROID = re.compile(r"(?i)android|\.apk\b")


def _command(text: str, start: int) -> tuple[str, int]:
    """The command starting at `start`, with shell line continuations (\\ or `) joined."""
    end = start
    while True:
        nl = text.find("\n", end)
        if nl < 0:
            return text[start:], len(text)
        if text[nl - 1:nl] in ("\\", "`"):
            end = nl + 1
            continue
        return text[start:nl], nl


def check(ctx: Ctx) -> list[Violation]:
    return check_latest(ctx) + check_version_sync(ctx) + check_icon_refresh(ctx)


def check_icon_refresh(ctx: Ctx) -> list[Violation]:
    iss = _read(ctx, "desktop/windows/voxa.iss")
    if iss is None:
        return []
    out = []
    # Comments out: a commented-out setting does not count.
    code = "\n".join(line.split(";")[0] if not line.lstrip().startswith("//") else "" for line in iss.splitlines())
    setup = re.search(r"^\[Setup\][^\[]*", code, re.M)
    if not (setup and re.search(r"^\s*ChangesAssociations\s*=\s*yes\s*$", setup.group(0), re.M | re.I)):
        out.append(Violation("release-icon-refresh", "desktop/windows/voxa.iss", 1, "voxa.iss lacks ChangesAssociations=yes in [Setup]. "
                             "Rule: an install or update refreshes the icon cache, or a pinned taskbar shortcut keeps the old icon "
                             "(docs/guardrails.md#7-release)", key="iss: ChangesAssociations"))
    if not (re.search(r"external\s+'SHChangeNotify@shell32\.dll", iss) and re.search(r"ssPostInstall\s+then\s+RefreshIcons", iss)
            and re.search(r"SHChangeNotify\(\$00002000", iss)):
        out.append(Violation("release-icon-refresh", "desktop/windows/voxa.iss", 1, "voxa.iss does not call SHChangeNotify(SHCNE_UPDATEITEM) "
                             "on Voxa.exe and its shortcuts at ssPostInstall (RefreshIcons). Rule: an install or update refreshes the "
                             "icon cache (docs/guardrails.md#7-release)", key="iss: RefreshIcons"))
    return out


def _read(ctx: Ctx, rel: str) -> str | None:
    return (ctx.root / rel).read_text(encoding="utf-8", errors="replace") if ctx.exists(rel) else None


def _line(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1


def check_version_sync(ctx: Ctx) -> list[Violation]:
    cargo = _read(ctx, "Cargo.toml")
    if cargo is None:
        return []
    ws = re.search(r"\[workspace\.package\][^\[]*?^version\s*=\s*\"([^\"]+)\"", cargo, re.M | re.S)
    if not ws:
        return []
    v = ws.group(1)
    out = []

    def bad(rel: str, line: int, msg: str, key: str):
        out.append(Violation("release-version-sync", rel, line, f"{msg}. Rule: one version everywhere, the workspace version {v} in "
                             "Cargo.toml is the source (docs/guardrails.md#7-release)", key=key))

    iss = _read(ctx, "desktop/windows/voxa.iss")
    if iss is not None:
        d = re.search(r'^\s*#define\s+AppVersion\s+"([^"]+)"', iss, re.M)
        if not d:
            bad("desktop/windows/voxa.iss", 1, "voxa.iss has no default #define AppVersion", "iss: missing")
        elif d.group(1) != v:
            bad("desktop/windows/voxa.iss", _line(iss, d.start()), f"voxa.iss defaults to AppVersion {d.group(1)}", "iss: differs")
        for h in re.finditer(r'^\s*AppVersion\s*=\s*(\d+\.\d+\.\d+)', iss, re.M):
            bad("desktop/windows/voxa.iss", _line(iss, h.start()), f"voxa.iss hard-codes AppVersion={h.group(1)}", "iss: hard-coded")
    yml = _read(ctx, ".github/workflows/desktop.yml")
    if yml is not None:
        if "/DAppVersion" not in yml:
            bad(".github/workflows/desktop.yml", 1, "desktop.yml builds the installer without /DAppVersion", "yml: no DAppVersion")
        if not re.search(r"GITHUB_REF_NAME[^\n]*(!=|==|-ne|-eq)[^\n]*\bv\$|\bv\$[^\n]*(!=|==)[^\n]*GITHUB_REF_NAME", yml):
            bad(".github/workflows/desktop.yml", 1, "desktop.yml publishes without checking the tag against the Cargo version", "yml: no tag check")
    gradle = _read(ctx, "android/app/build.gradle.kts")
    if gradle is not None:
        name = re.search(r'versionName\s*=\s*"([^"]+)"', gradle)
        code = re.search(r"versionCode\s*=\s*(\d+)", gradle)
        if name and name.group(1) != v:
            bad("android/app/build.gradle.kts", _line(gradle, name.start()), f"Android versionName is {name.group(1)}", "gradle: name")
        if name and code:
            parts = name.group(1).split(".")
            if len(parts) == 3 and all(p.isdigit() for p in parts):
                want = int(parts[0]) * 10000 + int(parts[1]) * 100 + int(parts[2])
                if int(code.group(1)) != want:
                    bad("android/app/build.gradle.kts", _line(gradle, code.start()), f"Android versionCode is {code.group(1)}, the scheme "
                        f"major*10000 + minor*100 + patch gives {want} for {name.group(1)}", "gradle: code")
    return out


def check_latest(ctx: Ctx) -> list[Violation]:
    out = []
    for rel in S.files(ctx.root, *GLOBS, exclude=EXCLUDE):
        text = (ctx.root / rel).read_text(encoding="utf-8", errors="replace")
        for m in CMD.finditer(text):
            cmd, _ = _command(text, m.start())
            if "update-feed" in cmd and not re.search(r"--latest(=|\s+)false\b|--prerelease\b", cmd):
                line = text.count("\n", 0, m.start()) + 1
                out.append(Violation("release-latest", rel, line, "the update-feed release without --latest=false or --prerelease. Rule: "
                                     "the feed release never becomes the latest release (docs/guardrails.md#7-release)",
                                     key=cmd.strip()[:120]))
                continue
            if rel.endswith("workflows/desktop.yml") and "create" in cmd and "GITHUB_REF_NAME" in cmd \
                    and not (re.search(r"--prerelease\b", cmd) and re.search(r"--latest(=|\s+)false\b", cmd)):
                line = text.count("\n", 0, m.start()) + 1
                out.append(Violation("release-latest", rel, line, "the tagged desktop release is created without --prerelease --latest=false. "
                                     "Rule: a new version stays a prerelease until the owner promotes it after the real-hardware gate, "
                                     "so the public download links never jump early (docs/release-runbook.md B10)", key=cmd.strip()[:120]))
                continue
            if rel.endswith("workflows/android-release.yml") and "create" in cmd \
                    and not (re.search(r"--prerelease\b", cmd) and re.search(r"--latest(=|\s+)false\b", cmd)):
                line = text.count("\n", 0, m.start()) + 1
                out.append(Violation("release-latest", rel, line, "the tagged Android release is created without --prerelease "
                                     "--latest=false. Rule: it stays a prerelease until the owner promotes it after the real-hardware "
                                     "gate, and Android is never the latest release (docs/release-runbook.md B10)", key=cmd.strip()[:120]))
                continue
            if ANDROID.search(cmd) and not re.search(r"--latest(=|\s+)false\b", cmd):
                line = text.count("\n", 0, m.start()) + 1
                out.append(Violation("release-latest", rel, line, "Android release without --latest=false. Rule: Android GitHub releases "
                                     "never become the latest release, the desktop download links depend on it (docs/guardrails.md#7-release)",
                                     key=cmd.strip()[:120]))
    return out
