#!/usr/bin/env python3
"""Make a LOCAL TEST update feed, signed with the TEST key exactly the way CI signs a real one
(OpenSSL 3: `openssl pkeyutl -sign -rawin`). Never use it with a real key.

    python tools/update/make_test_feed.py desktop --version 1.0.1 --base http://127.0.0.1:8765/feed \
        --asset windows-x64=path/to/Voxa-Setup-1.0.1.exe --out feed/desktop.json
    python tools/update/make_test_feed.py android --version 1.0.1 --version-code 10001 --base http://10.0.2.2:8765/feed \
        --asset android-x86_64=app-x86_64-debug.apk --signer <sha256 hex> --out feed/android.json

Each asset file is copied next to the feed under a versioned name, so serving the --out folder
(python -m http.server) serves the whole test feed. Standard library plus the openssl command.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import shutil
import subprocess
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
TEST_KEY = HERE.parent.parent / "core" / "voxa-update" / "tests" / "fixtures" / "test-k1.TEST-ONLY.pem"


def sign(payload: bytes, key: Path) -> bytes:
    with tempfile.TemporaryDirectory() as tmp:
        p = Path(tmp) / "payload.json"
        s = Path(tmp) / "payload.sig"
        p.write_bytes(payload)
        subprocess.run(["openssl", "pkeyutl", "-sign", "-rawin", "-inkey", str(key), "-in", str(p), "-out", str(s)], check=True)
        return s.read_bytes()


def envelope(payload: bytes, key: Path = TEST_KEY, key_id: int = 1) -> bytes:
    sig = sign(payload, key)
    return json.dumps({"key_id": key_id, "payload": base64.b64encode(payload).decode(), "sig": base64.b64encode(sig).decode()}).encode()


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("channel", choices=["desktop", "android"])
    ap.add_argument("--version", required=True)
    ap.add_argument("--version-code", type=int)
    ap.add_argument("--base", required=True, help="the URL the --out folder is served at")
    ap.add_argument("--asset", action="append", default=[], help="key=path")
    ap.add_argument("--signer", help="Android: SHA-256 of the APK signing certificate")
    ap.add_argument("--serial", type=int, default=int(time.time()))
    ap.add_argument("--paused", action="store_true")
    ap.add_argument("--out", required=True)
    a = ap.parse_args()
    out = Path(a.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    base = a.base.rstrip("/")
    assets = {}
    for spec in a.asset:
        key, path = spec.split("=", 1)
        src = Path(path)
        name = f"Voxa-{key}-{a.version}{src.suffix}"
        dst = out.parent / name
        if src.resolve() != dst.resolve():
            shutil.copyfile(src, dst)
        data = dst.read_bytes()
        assets[key] = {"url": f"{base}/{name}", "sha256": hashlib.sha256(data).hexdigest(), "size": len(data)}
    payload = {"schema": 1, "channel": a.channel, "serial": a.serial, "version": a.version, "notes_url": f"{base}/notes.html",
               "paused": a.paused, "assets": assets}
    if a.channel == "android":
        payload["version_code"] = a.version_code
        if a.signer:
            payload["signer_sha256"] = a.signer.lower()
    raw = json.dumps(payload, separators=(",", ":")).encode()
    out.write_bytes(envelope(raw))
    (out.parent / "notes.html").write_text("<p>Test release notes.</p>", encoding="utf-8")
    print(f"wrote {out} ({a.channel} {a.version}, {len(assets)} assets), signed with the TEST key")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
