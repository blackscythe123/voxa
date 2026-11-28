"""Rule 11, licences. Voxa ships other people's code, fonts and models; their licences travel with it
(owner decision, 2026-11-28). tools/licences/generate.py builds THIRD_PARTY_NOTICES.txt and the
apps' "Open source licences" pages from the real dependency graph; this rule keeps them honest.

  licences-stale    THIRD_PARTY_NOTICES.txt, tools/licences/inventory.tsv and the three app copies
                    (desktop/app/assets/licences-*.json.gz, android/app/src/main/assets/licences.json.gz)
                    carry the dependency fingerprint of today's tree: Cargo.lock, the dependency
                    sections of the workspace's Cargo.toml files, the committed Android dependency
                    list (tools/licences/android-deps.txt, itself checked against libs.versions.toml
                    and the app's dependencies block), NOTICE, the bundled assets and the stored
                    licence texts. No network, no Android SDK: CI recomputes it from the checkout.
  licences-allowed  every shipped dependency's licence is compatible with shipping Voxa under
                    Apache-2.0 (ALLOWED below). A dual expression passes when one side does;
                    GPL, AGPL, LGPL-only, SSPL and anything unknown fail.
  licences-asset    every bundled font, sound, model or native library in the app trees is listed in
                    ASSETS (so the generator writes its licence).

The generator imports this module, so the fingerprint and the allow-list are defined once.
"""

from __future__ import annotations

import gzip
import hashlib
import json
import re
from pathlib import Path

from .core import Ctx, Violation

WHY = "Rule: shipped licences are listed and Apache-2.0 compatible (docs/guardrails.md#11-licences)"
REGEN = "run `python tools/licences/generate.py` (add `--gradle` when the Android dependencies changed)"

NOTICES = "THIRD_PARTY_NOTICES.txt"
INVENTORY = "tools/licences/inventory.tsv"
ANDROID_DEPS = "tools/licences/android-deps.txt"
TEXTS_DIR = "tools/licences/texts"
RESOURCES = {
    "windows": "desktop/app/assets/licences-windows.json.gz",
    "macos": "desktop/app/assets/licences-macos.json.gz",
    "android": "android/app/src/main/assets/licences.json.gz",
}
VERSIONS_TOML = "android/gradle/libs.versions.toml"
APP_GRADLE = "android/app/build.gradle.kts"

# SPDX identifiers Voxa may ship under Apache-2.0. MPL-2.0 is file-level copyleft: fine for
# unmodified use. OFL-1.1 is for fonts.
ALLOWED = {
    "MIT", "MIT-0", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Zlib", "Unicode-3.0", "Unicode-DFS-2016",
    "MPL-2.0", "CC0-1.0", "OFL-1.1", "BSL-1.0", "0BSD", "Unlicense",
}
# Exceptions that only add permissions.
ALLOWED_EXCEPTIONS = {"LLVM-exception"}

# Bundled files that are not Rust crates or Gradle libraries: path -> (name, licence, apps, texts).
# `texts` are repo paths of the licence and notice files shipped with the asset. Every font, sound,
# model and native library found by ASSET_GLOBS must be a key here.
ASSETS: dict[str, tuple[str, str, tuple[str, ...], tuple[str, ...]]] = {
    "desktop/app/assets/Fraunces.ttf": ("Fraunces font", "OFL-1.1", ("windows", "macos"), ("desktop/app/assets/OFL-Fraunces.txt",)),
    "android/app/src/main/res/font/manrope_bold.ttf": ("Manrope font", "OFL-1.1", ("android",), ("android/app/OFL-Manrope.txt",)),
    "android/app/src/main/res/font/manrope_medium.ttf": ("Manrope font", "OFL-1.1", ("android",), ("android/app/OFL-Manrope.txt",)),
    # Embedded by voxa-vad, which only the desktop links (Android builds voxa-ffi without `vad`).
    "core/voxa-vad/model/silero_vad.onnx": ("Silero VAD model", "MIT", ("windows", "macos"), (f"{TEXTS_DIR}/silero-vad-LICENSE.txt",)),
    # Downloaded at build time (desktop/windows/fetch-onnxruntime.ps1, desktop/macos/package.sh), shipped next to the app.
    "vendor/onnxruntime/onnxruntime.dll": ("ONNX Runtime", "MIT", ("windows", "macos"),
                                           (f"{TEXTS_DIR}/onnxruntime-LICENSE.txt", f"{TEXTS_DIR}/onnxruntime-ThirdPartyNotices.txt")),
}
ASSET_GLOBS = (
    "android/app/src/main/res/font/*", "android/app/src/main/res/raw/*", "android/app/src/main/assets/**/*",
    "desktop/app/assets/*.ttf", "desktop/app/assets/*.otf", "desktop/app/assets/*.wav", "desktop/app/assets/*.ogg",
    "desktop/app/assets/*.mp3", "core/*/model/*.onnx", "vendor/**/*.dll", "vendor/**/*.dylib", "vendor/**/*.so",
)
# Where the onnxruntime version is pinned (fetched at build time, so the version is part of the fingerprint).
ORT_PINS = ("desktop/windows/fetch-onnxruntime.ps1", "desktop/macos/package.sh")
ORT_VERSION = re.compile(r"microsoft\.ml\.onnxruntime/([0-9.]+)/")


# ── fingerprint ──────────────────────────────────────────────────────────────────────────────────

def _norm(b: bytes) -> bytes:
    # The same digest on a Windows (autocrlf) checkout and on Linux CI.
    return b.replace(b"\r\n", b"\n")


def _read(root: Path, rel: str) -> bytes:
    p = root / rel
    return _norm(p.read_bytes()) if p.is_file() else b"<missing>"


def cargo_lock_view(text: str) -> str:
    """Cargo.lock with the workspace members' own versions blanked: bumping Voxa's version is not
    a dependency change."""
    out = []
    for block in text.replace("\r\n", "\n").split("\n[[package]]\n"):
        if block.startswith("[[package]]\n"):
            block = block[len("[[package]]\n"):]
        if "\nsource = " not in "\n" + block:
            block = re.sub(r'(?m)^version = ".*"$', 'version = "<workspace>"', block)
        out.append(block)
    return "\n[[package]]\n".join(out)


def manifest_deps(text: str) -> str:
    """The dependency and feature sections of a Cargo.toml."""
    keep, out = False, []
    for line in text.replace("\r\n", "\n").split("\n"):
        s = line.strip()
        if s.startswith("["):
            keep = "dependencies" in s or s == "[features]" or s.startswith("[profile")
        if keep:
            out.append(line)
    return "\n".join(out)


def gradle_inputs(root: Path) -> str:
    """What the Android dependency list was resolved from: libs.versions.toml and the app's
    dependencies block. android-deps.txt records its digest."""
    gradle = (root / APP_GRADLE).read_text(encoding="utf-8", errors="replace") if (root / APP_GRADLE).is_file() else ""
    m = re.search(r"(?ms)^dependencies \{.*?^\}", gradle.replace("\r\n", "\n"))
    h = hashlib.sha256()
    h.update(_read(root, VERSIONS_TOML))
    h.update(b"\0" + (m.group(0) if m else "").encode())
    return h.hexdigest()


def members(root: Path) -> list[str]:
    cargo = (root / "Cargo.toml").read_text(encoding="utf-8", errors="replace")
    m = re.search(r"(?s)members\s*=\s*\[(.*?)\]", cargo)
    return sorted(re.findall(r'"([^"]+)"', m.group(1))) if m else []


def fingerprint(root: Path) -> str:
    h = hashlib.sha256()

    def part(name: str, data: bytes):
        h.update(name.encode() + b"\0" + hashlib.sha256(data).digest())

    lock = (root / "Cargo.lock").read_text(encoding="utf-8", errors="replace") if (root / "Cargo.lock").is_file() else ""
    part("Cargo.lock", cargo_lock_view(lock).encode())
    part("Cargo.toml", manifest_deps((root / "Cargo.toml").read_text(encoding="utf-8", errors="replace")).encode())
    for m in members(root):
        p = root / m / "Cargo.toml"
        part(f"{m}/Cargo.toml", manifest_deps(p.read_text(encoding="utf-8", errors="replace")).encode() if p.is_file() else b"<missing>")
    part(ANDROID_DEPS, _read(root, ANDROID_DEPS))
    part("gradle", gradle_inputs(root).encode())
    part("NOTICE", _read(root, "NOTICE"))
    part("LICENSE", _read(root, "LICENSE"))
    for rel, (name, lic, apps, texts) in sorted(ASSETS.items()):
        # vendor/ is not committed (downloaded at build time): its pin stands in for the file.
        data = b"<fetched>" if rel.startswith("vendor/") else _read(root, rel)
        part(rel, data + f"|{name}|{lic}|{','.join(apps)}".encode())
        for t in texts:
            part(t, _read(root, t))
    for pin in ORT_PINS:
        p = root / pin
        part(pin, ",".join(sorted(set(ORT_VERSION.findall(p.read_text(encoding="utf-8", errors="replace"))))).encode() if p.is_file() else b"")
    td = root / TEXTS_DIR
    if td.is_dir():
        for p in sorted(td.iterdir()):
            if p.is_file():
                part(f"{TEXTS_DIR}/{p.name}", _norm(p.read_bytes()))
    return h.hexdigest()


# ── SPDX expressions ─────────────────────────────────────────────────────────────────────────────

def _tokens(expr: str) -> list[str]:
    # Old crates write "MIT/Apache-2.0" for "MIT OR Apache-2.0".
    expr = re.sub(r"\s*/\s*", " OR ", expr.strip())
    return re.findall(r"\(|\)|[^\s()]+", expr)


def allowed(expr: str) -> bool:
    """True when the expression can be satisfied with ALLOWED licences only."""
    toks = _tokens(expr)
    if not toks:
        return False
    pos = 0

    def peek():
        return toks[pos] if pos < len(toks) else None

    def take():
        nonlocal pos
        pos += 1
        return toks[pos - 1]

    def primary() -> bool:
        t = take()
        if t == "(":
            v = or_expr()
            if take() != ")":
                raise ValueError("unbalanced")
            return v
        if t in ("AND", "OR", "WITH", ")") or t is None:
            raise ValueError(f"unexpected {t}")
        ok = t.rstrip("+") in ALLOWED
        if peek() == "WITH":
            take()
            ok = ok and take() in ALLOWED_EXCEPTIONS
        return ok

    def and_expr() -> bool:
        v = primary()
        while peek() == "AND":
            take()
            v = primary() and v
        return v

    def or_expr() -> bool:
        v = and_expr()
        while peek() == "OR":
            take()
            v = and_expr() or v
        return v

    try:
        v = or_expr()
        return v and pos == len(toks)
    except (ValueError, IndexError):
        return False


def leaves(expr: str) -> list[str]:
    return [t for t in _tokens(expr) if t not in ("(", ")", "AND", "OR", "WITH") and not t.endswith("-exception")]


# ── the check ────────────────────────────────────────────────────────────────────────────────────

def read_inventory(text: str) -> tuple[str | None, list[tuple[int, list[str]]]]:
    """(fingerprint, [(line number, [kind, name, version, licence, apps])])"""
    fp, rows = None, []
    for n, line in enumerate(text.replace("\r\n", "\n").split("\n"), 1):
        if line.startswith("# fingerprint "):
            fp = line.split()[2]
        elif line and not line.startswith("#"):
            rows.append((n, line.split("\t")))
    return fp, rows


def check(ctx: Ctx) -> list[Violation]:
    root = ctx.root
    # Only trees that look like the repo (a Cargo workspace with a lock file) are checked.
    if not (root / "Cargo.toml").is_file() or not (root / "Cargo.lock").is_file():
        return []
    out: list[Violation] = []
    want = fingerprint(root)

    def stale(rel: str, line: int, what: str):
        out.append(Violation("licences-stale", rel, line, f"{what}; {REGEN}. {WHY}", key=what))

    # The Android dependency list matches what it claims to be resolved from.
    deps = root / ANDROID_DEPS
    if not deps.is_file():
        stale(ANDROID_DEPS, 1, "the Android dependency list is missing")
    else:
        m = re.search(r"(?m)^# inputs ([0-9a-f]{64})", deps.read_text(encoding="utf-8", errors="replace"))
        if not m or m.group(1) != gradle_inputs(root):
            stale(ANDROID_DEPS, 1, "libs.versions.toml or the app's dependencies block changed since the Android dependency list was resolved")

    notices = root / NOTICES
    if not notices.is_file():
        stale(NOTICES, 1, f"{NOTICES} is missing")
    else:
        head = notices.read_text(encoding="utf-8", errors="replace")[:2000]
        m = re.search(r"Dependency fingerprint: ([0-9a-f]{64})", head)
        if not m or m.group(1) != want:
            stale(NOTICES, 1, f"{NOTICES} was generated from different dependencies")

    inv = root / INVENTORY
    rows: list[tuple[int, list[str]]] = []
    if not inv.is_file():
        stale(INVENTORY, 1, "the licence inventory is missing")
    else:
        fp, rows = read_inventory(inv.read_text(encoding="utf-8", errors="replace"))
        if fp != want:
            stale(INVENTORY, 1, "the licence inventory was generated from different dependencies")

    for app, rel in RESOURCES.items():
        p = root / rel
        try:
            data = json.loads(gzip.decompress(p.read_bytes()).decode("utf-8"))
            ok = data.get("fingerprint") == want and data.get("app") == app
        except (OSError, ValueError, EOFError):
            ok = False
        if not ok:
            stale(rel, 1, f"the {app} app's licence page data is missing or out of date")

    # Every shipped licence is on the allow-list.
    for line, row in rows:
        if len(row) < 4:
            continue
        kind, name, version, lic = row[:4]
        if not allowed(lic):
            out.append(Violation("licences-allowed", INVENTORY, line,
                                 f"{name} {version} ({kind}) is licensed {lic!r}, which is not on the allow-list for shipping "
                                 f"Voxa under Apache-2.0: replace the dependency, or (owner decision) add it to tools/lint/allowlist.py "
                                 f"with the reason. {WHY}", key=f"{name} {lic}"))

    # Every bundled font, sound, model and native library is listed.
    seen: set[str] = set()
    for g in ASSET_GLOBS:
        for p in root.glob(g):
            rel = p.relative_to(root).as_posix()
            if p.is_file() and rel not in ASSETS and rel not in RESOURCES.values() and rel not in seen:
                seen.add(rel)
                out.append(Violation("licences-asset", rel, 1, f"{rel} is bundled with the app but has no licence entry: add it to "
                                     f"ASSETS in tools/lint/voxa_lint/licences.py with its licence and text, then regenerate. {WHY}",
                                     key=rel))
    return out
