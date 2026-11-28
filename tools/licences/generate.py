#!/usr/bin/env python3
"""Generate Voxa's third-party notices from the real dependency graph.

    python tools/licences/generate.py            regenerate from Cargo.lock and tools/licences/android-deps.txt
    python tools/licences/generate.py --gradle   first re-resolve the Android release dependencies with Gradle
                                                 (needs the Android SDK; JAVA_HOME as in docs/guardrails.md)

Writes (all committed, never edit by hand):
    THIRD_PARTY_NOTICES.txt                         every app, every library, every licence and NOTICE text
    tools/licences/inventory.tsv                    kind, name, version, licence, apps (read by the lint)
    tools/licences/android-deps.txt                 with --gradle: the release runtime classpath
    desktop/app/assets/licences-{windows,macos}.json.gz   the desktop's Open source licences page
    android/app/src/main/assets/licences.json.gzip       the Android app's Open source licences page

Rust: the crates linked into each artefact (`cargo tree -e normal,no-proc-macro`, per target, for
voxa-desktop + voxa-signin on Windows and macOS, voxa-ffi on Android), their licence from
`cargo metadata`, their LICENSE/COPYING/NOTICE files from the cargo registry (every NOTICE verbatim,
Apache-2.0 section 4(d)). Android: the release runtime classpath, licences from the POMs in the
Gradle cache. Assets: tools/lint/voxa_lint/licences.py ASSETS. Identical texts are stored once.
`python tools/lint/run.py --only licences` checks the result is current. Standard library only.
"""

from __future__ import annotations

import argparse
import gzip
import json
import os
import re
import struct
import subprocess
import sys
import xml.etree.ElementTree as ET
from collections import Counter, defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools" / "lint"))
from voxa_lint import licences as L  # noqa: E402

APPS = ("windows", "macos", "android")
APP_NAMES = {"windows": "Voxa for Windows", "macos": "Voxa for macOS", "android": "Voxa for Android"}
RUST = {
    "windows": (("voxa-desktop", "voxa-signin"), ("x86_64-pc-windows-msvc",)),
    "macos": (("voxa-desktop", "voxa-signin"), ("aarch64-apple-darwin", "x86_64-apple-darwin")),
    "android": (("voxa-ffi",), ("aarch64-linux-android", "armv7-linux-androideabi", "x86_64-linux-android")),
}
LICENCE_FILE = re.compile(r"(?i)^(licen[cs]e|copying|notice|copyright|unlicense|apache|mit)([-_.].*)?$")
# Licence texts for crates whose package has no licence file of its own.
STANDARD = {"MPL-2.0": "tools/licences/texts/MPL-2.0.txt", "Apache-2.0": "LICENSE", "MIT": "tools/licences/texts/MIT.txt",
            "CC0-1.0": "tools/licences/texts/CC0-1.0.txt"}
# Libraries Voxa takes under one side of a choice.
CHOSEN = {"net.java.dev.jna:jna": ("Apache-2.0", "Voxa uses JNA under the Apache License 2.0 (JNA is dual licensed, LGPL 2.1 or later or Apache 2.0).")}
POM_LICENCES = (
    (re.compile(r"apache.*2\.0|apache-2\.0|apache license, version 2|asl 2"), "Apache-2.0"),
    (re.compile(r"lgpl.*2\.1|lesser general public license.*2\.1"), "LGPL-2.1-or-later"),
    (re.compile(r"^mit\b|mit license|opensource\.org/licenses/mit"), "MIT"),
    (re.compile(r"bsd.*3|new bsd|bsd-3-clause"), "BSD-3-Clause"),
    (re.compile(r"bsd.*2|simplified bsd|bsd-2-clause"), "BSD-2-Clause"),
    (re.compile(r"eclipse public license.*2|epl-2\.0"), "EPL-2.0"),
)
GRADLE_HOME = Path(os.environ.get("GRADLE_USER_HOME", Path.home() / ".gradle")) / "caches" / "modules-2" / "files-2.1"


def run(cmd: list[str], cwd: Path = ROOT, env: dict | None = None) -> str:
    r = subprocess.run(cmd, cwd=cwd, env=env, capture_output=True, text=True, encoding="utf-8")
    if r.returncode != 0:
        sys.exit(f"generate: {' '.join(cmd)} failed:\n{r.stderr}")
    return r.stdout


def clean(text: str) -> str:
    text = text.replace("\r\n", "\n").replace("\r", "\n").replace("\ufeff", "")
    return "\n".join(l.rstrip() for l in text.split("\n")).strip("\n") + "\n"


class Texts:
    """Licence and notice texts, each stored once."""

    def __init__(self):
        self.bodies: list[str] = []
        self.index: dict[str, int] = {}

    def add(self, body: str) -> int:
        body = clean(body)
        if body not in self.index:
            self.index[body] = len(self.bodies)
            self.bodies.append(body)
        return self.index[body]


def rust_libraries(texts: Texts) -> dict[tuple[str, str, str], dict]:
    meta = json.loads(run(["cargo", "metadata", "--format-version", "1", "--locked", "--offline"]))
    pkgs = {(p["name"], p["version"]): p for p in meta["packages"]}
    libs: dict[tuple[str, str, str], dict] = {}
    for app, (roots, targets) in RUST.items():
        for t in targets:
            cmd = ["cargo", "tree", "-q", "--locked", "--offline", "-e", "normal,no-proc-macro", "--target", t, "--prefix", "none", "-f", "{p}"]
            for r in roots:
                cmd += ["-p", r]
            for line in run(cmd).splitlines():
                parts = line.replace("(*)", "").split()
                if len(parts) < 2:
                    continue
                p = pkgs[(parts[0], parts[1].lstrip("v"))]
                if p["source"] is None:
                    continue  # Voxa's own crates
                key = ("Rust crate", p["name"], p["version"])
                if key not in libs:
                    libs[key] = {"licence": p["license"] or "", "texts": crate_texts(p, texts), "source": p.get("repository") or "",
                                 "apps": set(), "note": ""}
                libs[key]["apps"].add(app)
    # The Rust standard library is linked into every Rust artefact.
    ver = re.search(r"rustc (\S+)", run(["rustc", "--version"]))
    libs[("Rust crate", "Rust standard library", ver.group(1) if ver else "")] = {
        "licence": "MIT OR Apache-2.0", "texts": [texts.add(read("tools/licences/texts/rust-std-LICENSE.txt")), texts.add(read("LICENSE"))],
        "source": "https://github.com/rust-lang/rust", "apps": set(APPS), "note": ""}
    return libs


def crate_texts(p: dict, texts: Texts) -> list[int]:
    d = Path(p["manifest_path"]).parent
    files = sorted(f for f in d.iterdir() if f.is_file() and LICENCE_FILE.match(f.name))
    if p.get("license_file"):
        lf = (d / p["license_file"]).resolve()
        if lf.is_file() and lf not in [f.resolve() for f in files]:
            files.append(lf)
    # A LICENSES/ folder (REUSE layout).
    for sub in ("LICENSES", "licenses", "license"):
        if (d / sub).is_dir():
            files += sorted(f for f in (d / sub).iterdir() if f.is_file())
    ids = [texts.add(f.read_text(encoding="utf-8", errors="replace")) for f in files]
    if not ids:
        expr = p["license"] or ""
        leaves = L.leaves(expr)
        # A choice ("MIT OR Zlib") needs the text of one side only; anything else needs every text.
        if " OR " in re.sub(r"\s*/\s*", " OR ", expr) and "AND" not in expr and any(x in STANDARD for x in leaves):
            leaves = [x for x in leaves if x in STANDARD]
        for leaf in leaves:
            if leaf not in STANDARD:
                sys.exit(f"generate: {p['name']} {p['version']} ships no licence file and there is no stored {leaf} text; "
                         f"add one to tools/licences/texts and STANDARD")
            body = read(STANDARD[leaf])
            if leaf == "MIT":
                # MIT needs the copyright line: the crate's authors, as its Cargo.toml names them.
                who = ", ".join(re.sub(r"\s*<[^>]*>", "", a) for a in p.get("authors") or []) or f"the {p['name']} authors"
                body = (f"MIT License\n\nCopyright (c) {who}\n\n{body}\n"
                        f"(The {p['name']} package has no licence file; the copyright line is from its Cargo.toml.)\n")
            ids.append(texts.add(body))
    return ids


def read(rel: str) -> str:
    return (ROOT / rel).read_text(encoding="utf-8", errors="replace")


# ── Android ──────────────────────────────────────────────────────────────────────────────────────

def resolve_gradle():
    java = os.environ.get("JAVA_HOME") or "C:/Program Files/Android/Android Studio/jbr"
    env = dict(os.environ, JAVA_HOME=java, PATH=str(Path.home() / ".cargo" / "bin") + os.pathsep + os.environ.get("PATH", ""))
    gradlew = str(ROOT / "android" / ("gradlew.bat" if os.name == "nt" else "gradlew"))
    out = run([gradlew, "-q", ":app:dependencies", "--configuration", "releaseRuntimeClasspath", "-PskipRust"], cwd=ROOT / "android", env=env)
    coords = set()
    for line in out.splitlines():
        if line.rstrip().endswith(("(c)", "(n)")):
            continue
        m = re.search(r"--- ([\w.\-]+):([\w.\-]+):([\w.\-]+)(?: -> ([\w.\-]+))?", line)
        if m:
            g, a, v, r = m.groups()
            if not a.endswith("-bom"):
                coords.add(f"{g}:{a}:{r or v}")
    head = ("# The Android app's release runtime classpath (./gradlew :app:dependencies --configuration releaseRuntimeClasspath),\n"
            "# written by tools/licences/generate.py --gradle. `inputs` is the digest of libs.versions.toml and the app's\n"
            "# dependencies block it was resolved from; the lint fails when they change without a re-resolve.\n"
            f"# inputs {L.gradle_inputs(ROOT)}\n")
    (ROOT / L.ANDROID_DEPS).write_text(head + "".join(c + "\n" for c in sorted(coords)), encoding="utf-8", newline="\n")
    print(f"generate: {len(coords)} Android libraries resolved")


POM_CACHE = Path(os.environ.get("TEMP", "/tmp")) / "voxa-licences-poms"
REPOS = ("https://dl.google.com/android/maven2", "https://repo1.maven.org/maven2")


def pom_path(g: str, a: str, v: str) -> Path | None:
    d = GRADLE_HOME / g / a / v
    if d.is_dir():
        for h in d.iterdir():
            p = h / f"{a}-{v}.pom"
            if p.is_file():
                return p
    # Gradle read the module metadata instead and never cached the POM: fetch it (Google Maven, then Maven Central).
    p = POM_CACHE / f"{g}-{a}-{v}.pom"
    if p.is_file():
        return p
    import urllib.request
    for repo in REPOS:
        try:
            with urllib.request.urlopen(f"{repo}/{g.replace('.', '/')}/{a}/{v}/{a}-{v}.pom", timeout=30) as r:
                POM_CACHE.mkdir(parents=True, exist_ok=True)
                p.write_bytes(r.read())
                return p
        except OSError:
            continue
    return None


def pom_licences(g: str, a: str, v: str, depth: int = 0) -> tuple[list[str], str, str]:
    """(SPDX ids, project name, url) from the POM, following parents."""
    p = pom_path(g, a, v)
    if p is None:
        sys.exit(f"generate: no POM for {g}:{a}:{v} in the Gradle cache; run with --gradle on a machine that built the app")
    root = ET.parse(p).getroot()
    ns = {"m": root.tag[1:].split("}")[0]} if root.tag.startswith("{") else {"m": ""}
    q = (lambda s: s.replace("m:", "")) if not ns["m"] else (lambda s: s)
    name = (root.findtext(q("m:name"), default="", namespaces=ns) or "").strip()
    url = (root.findtext(q("m:url"), default="", namespaces=ns) or root.findtext(q("m:scm/m:url"), default="", namespaces=ns) or "").strip()
    ids = []
    for lic in root.findall(q("m:licenses/m:license"), ns):
        label = " ".join(((lic.findtext(q("m:name"), default="", namespaces=ns) or "") + " " +
                          (lic.findtext(q("m:url"), default="", namespaces=ns) or "")).lower().split())
        spdx = next((s for rx, s in POM_LICENCES if rx.search(label)), f"Unknown({label.strip()})")
        if spdx not in ids:
            ids.append(spdx)
    if not ids and depth < 4:
        par = root.find(q("m:parent"), ns)
        if par is not None:
            pg, pa, pv = (par.findtext(q(f"m:{k}"), default="", namespaces=ns) for k in ("groupId", "artifactId", "version"))
            ids, _, purl = pom_licences(pg, pa, pv, depth + 1)
            url = url or purl
    return ids, name, url


def android_libraries(texts: Texts) -> dict[tuple[str, str, str], dict]:
    libs = {}
    for line in read(L.ANDROID_DEPS).splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        g, a, v = line.split(":")
        ids, _, url = pom_licences(g, a, v)
        note = ""
        if f"{g}:{a}" in CHOSEN:
            chosen, note = CHOSEN[f"{g}:{a}"]
            expr = " OR ".join([chosen] + [i for i in ids if i != chosen])
            leaves = [chosen]
        else:
            expr = " AND ".join(ids) if ids else "Unknown"
            leaves = ids
        tids = [texts.add(read(STANDARD[x])) for x in leaves if x in STANDARD]
        libs[("Android library", f"{g}:{a}", v)] = {"licence": expr, "texts": tids, "source": url, "apps": {"android"}, "note": note}
    return libs


# ── assets ───────────────────────────────────────────────────────────────────────────────────────

def font_version(path: Path) -> str:
    """The version string (name ID 5) from a TrueType font, e.g. "Version 1.000"."""
    try:
        b = path.read_bytes()
        n = struct.unpack(">H", b[4:6])[0]
        for i in range(n):
            tag, _, off, _ = struct.unpack(">4sIII", b[12 + 16 * i: 28 + 16 * i])
            if tag == b"name":
                _, count, so = struct.unpack(">HHH", b[off: off + 6])
                for j in range(count):
                    pid, eid, _, nid, ln, o = struct.unpack(">HHHHHH", b[off + 6 + 12 * j: off + 18 + 12 * j])
                    if nid == 5:
                        raw = b[off + so + o: off + so + o + ln]
                        s = raw.decode("utf-16-be") if pid in (0, 3) else raw.decode("latin-1")
                        m = re.search(r"\d+(\.\d+)+", s)
                        return m.group(0) if m else s
    except (OSError, struct.error, UnicodeDecodeError):
        pass
    return ""


def asset_libraries(texts: Texts, linked: set[str]) -> dict[tuple[str, str, str], dict]:
    libs = {}
    ort = sorted({v for pin in L.ORT_PINS for v in L.ORT_VERSION.findall(read(pin))})
    for rel, (name, lic, apps, files) in sorted(L.ASSETS.items()):
        if rel.startswith("core/voxa-vad/") and "voxa-vad" not in linked:
            continue
        if rel.endswith((".ttf", ".otf")):
            kind, version = "Font", font_version(ROOT / rel)
        elif "onnxruntime" in rel:
            kind, version = "Native library", ", ".join(ort)
        else:
            kind, version = "Model", "5" if "silero" in rel else ""
        key = (kind, name, version)
        lib = libs.setdefault(key, {"licence": lic, "texts": [texts.add(read(f)) for f in files], "source": "", "apps": set(), "note": ""})
        lib["apps"].update(apps)
    return libs


# ── output ───────────────────────────────────────────────────────────────────────────────────────

def linked_workspace_crates() -> set[str]:
    out = set()
    for app, (roots, targets) in RUST.items():
        if app == "android":
            continue
        for t in targets:
            cmd = ["cargo", "tree", "-q", "--locked", "--offline", "-e", "normal", "--target", t, "--prefix", "none", "-f", "{p}"]
            for r in roots:
                cmd += ["-p", r]
            out |= {l.split()[0] for l in run(cmd).splitlines() if l.strip()}
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--gradle", action="store_true", help="re-resolve the Android dependencies with Gradle first")
    args = ap.parse_args()
    if args.gradle:
        resolve_gradle()
    m = re.search(r"(?m)^# inputs ([0-9a-f]{64})", read(L.ANDROID_DEPS)) if (ROOT / L.ANDROID_DEPS).is_file() else None
    if not m or m.group(1) != L.gradle_inputs(ROOT):
        sys.exit("generate: the Android dependencies changed since tools/licences/android-deps.txt was resolved; run with --gradle")

    texts = Texts()
    libs = rust_libraries(texts)
    libs.update(android_libraries(texts))
    libs.update(asset_libraries(texts, linked_workspace_crates()))
    fp = L.fingerprint(ROOT)
    notice = clean(read("NOTICE"))
    order = sorted(libs, key=lambda k: (k[1].lower(), k[2], k[0]))

    # The apps' pages: only what each app ships, texts renumbered.
    for app in APPS:
        mine = [k for k in order if app in libs[k]["apps"]]
        used: dict[int, int] = {}
        for k in mine:
            for t in libs[k]["texts"]:
                used.setdefault(t, len(used))
        doc = {
            "app": app, "fingerprint": fp, "notice": notice,
            "libraries": [{"name": k[1], "version": k[2], "kind": k[0], "licence": libs[k]["licence"], "source": libs[k]["source"],
                           "note": libs[k]["note"], "texts": [used[t] for t in libs[k]["texts"]]} for k in mine],
            "texts": [texts.bodies[t] for t, _ in sorted(used.items(), key=lambda x: x[1])],
        }
        raw = json.dumps(doc, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
        dst = ROOT / L.RESOURCES[app]
        dst.parent.mkdir(parents=True, exist_ok=True)
        dst.write_bytes(gzip.compress(raw, compresslevel=9, mtime=0))
        print(f"generate: {L.RESOURCES[app]}: {len(mine)} libraries, {len(used)} texts, {len(raw)} bytes, {dst.stat().st_size} gzipped")

    # The inventory the lint reads.
    inv = ["# Every library, font, model and native library Voxa ships: kind, name, version, licence (SPDX), apps.",
           "# Written by tools/licences/generate.py; never edit by hand. tools/lint checks each licence and the fingerprint.",
           f"# fingerprint {fp}"]
    inv += ["\t".join((k[0], k[1], k[2], libs[k]["licence"], ",".join(a for a in APPS if a in libs[k]["apps"]))) for k in order]
    (ROOT / L.INVENTORY).write_text("\n".join(inv) + "\n", encoding="utf-8", newline="\n")

    # THIRD_PARTY_NOTICES.txt
    bar = "=" * 78
    users: dict[int, list[str]] = defaultdict(list)
    for k in order:
        for t in libs[k]["texts"]:
            users[t].append(f"{k[1]} {k[2]}".strip())
    tnum = {t: i + 1 for i, t in enumerate(sorted(users, key=lambda t: min(order.index(k) for k in order if t in libs[k]["texts"])))}
    out = [
        "Third-party notices for Voxa",
        "Generated by tools/licences/generate.py from Cargo.lock, the Android release classpath and the bundled assets.",
        "Do not edit by hand. Each app shows its own part of this under Settings, Open source licences.",
        f"Dependency fingerprint: {fp}",
        "", bar, "Voxa", bar, "", notice.rstrip(), "",
        bar, "Summary", bar, "",
    ]
    for app in APPS:
        c = Counter(libs[k]["licence"] for k in order if app in libs[k]["apps"])
        out.append(f"{APP_NAMES[app]}: {sum(c.values())} components")
        out += [f"    {n:4d}  {lic}" for lic, n in sorted(c.items(), key=lambda x: (-x[1], x[0]))]
        out.append("")
    out += [bar, "Components", bar, ""]
    for k in order:
        lib = libs[k]
        apps = ", ".join(a for a in APPS if a in lib["apps"])
        out.append(f"{k[1]} {k[2]}".strip())
        out.append(f"    {k[0]}; licence: {lib['licence']}; in: {apps}")
        if lib["source"]:
            out.append(f"    source: {lib['source']}")
        if lib["note"]:
            out.append(f"    {lib['note']}")
        out.append("    texts: " + ", ".join(f"T{tnum[t]}" for t in lib["texts"]))
        out.append("")
    out += [bar, "Licence and notice texts", bar, ""]
    for t, n in sorted(tnum.items(), key=lambda x: x[1]):
        u = users[t]
        out.append(f"----- T{n}, used by {len(u)}: {', '.join(u[:12])}{', ...' if len(u) > 12 else ''}")
        out.append("")
        out.append(texts.bodies[t].rstrip())
        out.append("")
    (ROOT / L.NOTICES).write_text("\n".join(out), encoding="utf-8", newline="\n")
    print(f"generate: {L.NOTICES}: {len(order)} components, {len(tnum)} distinct texts")
    bad = [f"{k[1]} {k[2]}: {libs[k]['licence']}" for k in order if not L.allowed(libs[k]["licence"])]
    if bad:
        print("generate: licences not on the allow-list (the lint will fail unless allow-listed):\n  " + "\n  ".join(bad))
    return 0


if __name__ == "__main__":
    sys.exit(main())
