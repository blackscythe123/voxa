"""Shared types: a violation, the rule names, and the allow-list match."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class Violation:
    rule: str      # e.g. "contrast", "copy-dash" (docs/guardrails.md lists them all)
    path: str      # repo-relative, forward slashes
    line: int
    message: str
    key: str = ""  # what an allow-list entry matches against (defaults to the source line)

    def render(self) -> str:
        return f"{self.path}:{self.line}: [{self.rule}] {self.message}"


@dataclass(frozen=True)
class Allow:
    rule: str
    path: str
    match: str     # a substring of the violation's key
    reason: str

    def covers(self, v: Violation) -> bool:
        return v.rule == self.rule and v.path == self.path and self.match in (v.key or v.message)


class Ctx:
    """What a check gets: the tree to read (the repo, or a fixture tree) and options."""

    def __init__(self, root: Path, apk: Path | None = None, require_apk: bool = False):
        self.root = root
        self.apk = apk
        self.require_apk = require_apk
        self.notes: list[str] = []   # informational lines (APK size, skipped checks)

    def exists(self, rel: str) -> bool:
        return (self.root / rel).exists()
