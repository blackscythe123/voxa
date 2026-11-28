"""Rule 9, licence. The owner chose Apache-2.0 with a NOTICE crediting Simiyon Vinscent Samuel L
(2026-11-28). The LICENSE file, NOTICE, README and Cargo.toml must all agree, so the repo never
again says GPL in one place and MIT in another.

  licence-consistent  LICENSE is the Apache License 2.0; NOTICE exists and names the author;
                      Cargo.toml's workspace licence is "Apache-2.0"; the README's License
                      section starts with Apache-2.0.
"""

from __future__ import annotations

import re

from .core import Ctx, Violation

AUTHOR = "Simiyon Vinscent Samuel L"
RULE = "licence-consistent"
WHY = "Rule: Voxa is Apache-2.0 with a NOTICE crediting the author (docs/guardrails.md#9-licence)"


def _read(ctx: Ctx, name: str) -> str | None:
    p = ctx.root / name
    return p.read_text(encoding="utf-8", errors="replace") if p.is_file() else None


def check(ctx: Ctx) -> list[Violation]:
    # Only trees that look like the repo (have a Cargo workspace) are checked.
    cargo = _read(ctx, "Cargo.toml")
    if cargo is None:
        return []
    out = []
    lic = _read(ctx, "LICENSE") or ""
    if "Apache License" not in lic[:400] or "Version 2.0" not in lic[:400]:
        out.append(Violation(RULE, "LICENSE", 1, f"LICENSE is not the Apache License 2.0. {WHY}"))
    notice = _read(ctx, "NOTICE")
    if notice is None or AUTHOR not in notice:
        out.append(Violation(RULE, "NOTICE", 1, f"NOTICE is missing or doesn't credit {AUTHOR}. {WHY}"))
    m = re.search(r'(?m)^license\s*=\s*"([^"]*)"', cargo)
    if not m or m.group(1) != "Apache-2.0":
        line = cargo.count("\n", 0, m.start()) + 1 if m else 1
        out.append(Violation(RULE, "Cargo.toml", line, f'Cargo.toml licence is {m.group(1) if m else "missing"!r}, expected "Apache-2.0". {WHY}'))
    readme = _read(ctx, "README.md") or ""
    sec = re.search(r"(?m)^## License\s*\n+(.*)", readme)
    if not sec or not sec.group(1).startswith("Apache-2.0"):
        line = readme.count("\n", 0, sec.start()) + 1 if sec else 1
        out.append(Violation(RULE, "README.md", line, f"README's License section doesn't start with Apache-2.0. {WHY}"))
    return out
