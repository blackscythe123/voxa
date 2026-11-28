"""Rule 7, release hygiene. An Android GitHub release never becomes the repo's "latest" release
(the desktop's .../releases/latest/download/Voxa-Setup.exe links must keep pointing at the desktop).

  release-latest  every `gh release create` / `gh release edit` for Android (an APK, or a tag or title
                  that says android) in the docs, scripts or workflows passes --latest=false.
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
    out = []
    for rel in S.files(ctx.root, *GLOBS, exclude=EXCLUDE):
        text = (ctx.root / rel).read_text(encoding="utf-8", errors="replace")
        for m in CMD.finditer(text):
            cmd, _ = _command(text, m.start())
            if ANDROID.search(cmd) and not re.search(r"--latest(=|\s+)false\b", cmd):
                line = text.count("\n", 0, m.start()) + 1
                out.append(Violation("release-latest", rel, line, "Android release without --latest=false. Rule: Android GitHub releases "
                                     "never become the latest release, the desktop download links depend on it (docs/guardrails.md#7-release)",
                                     key=cmd.strip()[:120]))
    return out
