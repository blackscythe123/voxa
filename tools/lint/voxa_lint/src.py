"""Small source readers for Rust and Kotlin: comments blanked, string literals found, functions and
test modules located. Not a parser: good enough for the guardrail lints, and every lint has a
fixture in tools/lint/fixtures proving it still catches what it is meant to catch."""

from __future__ import annotations

import os
import re
from dataclasses import dataclass, field
from pathlib import Path


@dataclass
class Str:
    value: str          # the literal's text (escapes left as written, template parts kept)
    line: int           # 1-based line of the opening quote
    start: int          # offset of the opening quote in the file
    end: int            # offset just past the closing quote


@dataclass
class Source:
    path: Path
    rel: str
    text: str
    code: str                       # comments replaced by spaces (newlines kept)
    strings: list[Str] = field(default_factory=list)

    def line_of(self, offset: int) -> int:
        return self.text.count("\n", 0, offset) + 1

    def line_text(self, line: int) -> str:
        lines = self.text.splitlines()
        return lines[line - 1] if 0 < line <= len(lines) else ""


_RUST_CHAR = re.compile(r"'(\\u\{[0-9a-fA-F]+\}|\\x[0-9a-fA-F]{2}|\\.|[^\\'\n])'")


def _blank(buf: list[str], a: int, b: int) -> None:
    for i in range(a, b):
        if buf[i] != "\n":
            buf[i] = " "


def lex(text: str, lang: str) -> tuple[str, list[Str]]:
    """Returns (code with comments blanked, string literals). `lang` is "rust" or "kotlin"."""
    buf = list(text)
    strings: list[Str] = []
    n = len(text)

    def line_at(i: int) -> int:
        return text.count("\n", 0, i) + 1

    def skip_string(i: int, depth: int = 0) -> int:
        """`i` at an opening quote; returns the index after the literal and records it."""
        start = i
        if lang == "rust":
            m = re.match(r'b?r(#*)"', text[i:])
            if m:
                hashes = m.group(1)
                close = '"' + hashes
                j = text.find(close, i + len(m.group(0)))
                j = n if j < 0 else j + len(close)
                strings.append(Str(text[i + len(m.group(0)):j - len(close)], line_at(start), start, j))
                return j
            if text[i] == "b":
                i += 1
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            strings.append(Str(text[i + 1:j], line_at(start), start, j + 1))
            return j + 1
        # Kotlin: "...", """...""", with ${ ... } templates that may hold strings themselves.
        triple = text.startswith('"""', i)
        q = '"""' if triple else '"'
        j = i + len(q)
        while j < n:
            if not triple and text[j] == "\\":
                j += 2
                continue
            if text.startswith("${", j):
                j = skip_code(j + 2, "}", depth + 1)
                continue
            if text.startswith(q, j):
                break
            if not triple and text[j] == "\n":
                break
            j += 1
        strings.append(Str(text[i + len(q):j], line_at(start), start, j + len(q)))
        return j + len(q)

    def skip_code(i: int, until: str | None, depth: int) -> int:
        braces = 0
        while i < n:
            c = text[i]
            if until and c == "{":
                braces += 1
            elif until and c == "}":
                if braces == 0:
                    return i + 1
                braces -= 1
            if text.startswith("//", i):
                j = text.find("\n", i)
                j = n if j < 0 else j
                _blank(buf, i, j)
                i = j
                continue
            if text.startswith("/*", i):
                level, j = 1, i + 2
                while j < n and level:
                    if text.startswith("/*", j):
                        level, j = level + 1, j + 2
                    elif text.startswith("*/", j):
                        level, j = level - 1, j + 2
                    else:
                        j += 1
                _blank(buf, i, j)
                i = j
                continue
            if lang == "rust" and c in "br" and re.match(r'(b?r#*"|b")', text[i:]) and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
                i = skip_string(i, depth)
                continue
            if c == '"':
                i = skip_string(i, depth)
                continue
            if c == "'":
                m = _RUST_CHAR.match(text, i) if lang == "rust" else re.compile(r"'(\\u[0-9a-fA-F]{4}|\\.|[^\\'\n])'").match(text, i)
                if m:
                    i = m.end()
                    continue
            i += 1
        return i

    skip_code(0, None, 0)
    return "".join(buf), strings


def load(root: Path, rel: str) -> Source:
    path = root / rel
    text = path.read_text(encoding="utf-8", errors="replace")
    lang = "kotlin" if rel.endswith((".kt", ".kts")) else "rust"
    code, strings = lex(text, lang)
    return Source(path, rel.replace("\\", "/"), text, code, strings)


PRUNE = {".git", "target", "build", "node_modules", ".gradle", ".idea", ".claude", "dist", "__pycache__"}
_walk_cache: dict[Path, list[str]] = {}


def _all_files(root: Path) -> list[str]:
    if root not in _walk_cache:
        out = []
        for d, dirs, names in os.walk(root):
            dirs[:] = [x for x in dirs if x not in PRUNE]
            rel_d = Path(d).relative_to(root).as_posix()
            out += [n if rel_d == "." else f"{rel_d}/{n}" for n in names]
        _walk_cache[root] = out
    return _walk_cache[root]


def _glob_re(g: str) -> re.Pattern:
    out, i = "", 0
    while i < len(g):
        if g.startswith("**/", i):
            out, i = out + "(?:.*/)?", i + 3
        elif g[i] == "*":
            out, i = out + "[^/]*", i + 1
        elif g[i] == "?":
            out, i = out + "[^/]", i + 1
        else:
            out, i = out + re.escape(g[i]), i + 1
    return re.compile(out + r"\Z")


def files(root: Path, *globs: str, exclude: tuple[str, ...] = ()) -> list[str]:
    """Repo-relative paths matching any glob (`**/` spans folders), skipping build output and
    anything matching an `exclude` regex."""
    pats = [_glob_re(g) for g in globs]
    return sorted(r for r in _all_files(root) if any(p.match(r) for p in pats) and not any(re.search(e, r) for e in exclude))


def match_brace(code: str, open_at: int) -> int:
    """Index of the bracket closing the one at `open_at` (code must have comments blanked; string
    contents may hold brackets, so callers pass `masked` code)."""
    pairs = {"{": "}", "(": ")", "[": "]"}
    o = code[open_at]
    c = pairs[o]
    depth = 0
    for i in range(open_at, len(code)):
        if code[i] == o:
            depth += 1
        elif code[i] == c:
            depth -= 1
            if depth == 0:
                return i
    return len(code) - 1


def masked(src: Source) -> str:
    """Code with comments blanked and string contents replaced by spaces (quotes kept)."""
    buf = list(src.code)
    for s in src.strings:
        for i in range(s.start + 1, max(s.start + 1, s.end - 1)):
            if buf[i] != "\n":
                buf[i] = " "
    return "".join(buf)


@dataclass
class Fn:
    name: str
    start: int   # offset of `fn` / `fun`
    body: int    # offset of `{`
    end: int     # offset of `}`


def functions(src: Source) -> list[Fn]:
    m = masked(src)
    kw = r"\bfn\s+([A-Za-z_]\w*)" if not src.rel.endswith(".kt") else r"\bfun\s+(?:<[^>]*>\s*)?(?:[\w.]+\.)?([A-Za-z_]\w*)"
    out = []
    for f in re.finditer(kw, m):
        # The body's brace: the first `{` (or a `;` / `=` expression body) after the signature.
        j = f.end()
        depth = 0
        while j < len(m):
            ch = m[j]
            if m.startswith("->", j) or m.startswith("=>", j):
                j += 2
                continue
            if ch in "([<":
                depth += 1
            elif ch in ")]>":
                depth -= 1
            elif depth <= 0 and ch == "{":
                out.append(Fn(f.group(1), f.start(), j, match_brace(m, j)))
                break
            elif depth <= 0 and ch in ";=":
                # Expression body (Kotlin `= ...`) or a declaration: the rest of the line.
                e = m.find("\n", j)
                out.append(Fn(f.group(1), f.start(), j, len(m) if e < 0 else e))
                break
            j += 1
    return out


def enclosing_fn(fns: list[Fn], offset: int) -> Fn | None:
    best = None
    for f in fns:
        if f.start <= offset <= f.end and (best is None or f.start > best.start):
            best = f
    return best


def test_ranges(src: Source) -> list[tuple[int, int]]:
    """Rust `#[cfg(test)]` items (usually `mod tests { ... }`) and `#[test]` functions."""
    m = masked(src)
    out = []
    for a in re.finditer(r"#\[cfg\(test\)\]|#\[test\]", m):
        j = m.find("{", a.end())
        semi = m.find(";", a.end())
        if j < 0 or (0 <= semi < j):
            continue
        out.append((a.start(), match_brace(m, j)))
    return out


def in_ranges(offset: int, ranges: list[tuple[int, int]]) -> bool:
    return any(a <= offset <= b for a, b in ranges)


def calls(src: Source, pattern: str) -> list[tuple[re.Match, int, int]]:
    """Every call whose callee matches `pattern` (a regex ending just before `(`): the match, and the
    offsets of its opening and closing parentheses."""
    m = masked(src)
    out = []
    for c in re.finditer(f"(?:{pattern})" + r"\s*\(", m):
        o = c.end() - 1
        out.append((c, o, match_brace(m, o)))
    return out


def decode(value: str) -> str:
    """A literal's text with Rust (u{...}) and Kotlin (uXXXX) unicode escapes turned into characters."""
    value = re.sub(r"\\u\{([0-9a-fA-F]+)\}", lambda m: chr(int(m.group(1), 16)), value)
    value = re.sub(r"\\u([0-9a-fA-F]{4})", lambda m: chr(int(m.group(1), 16)), value)
    # Kotlin writes characters outside the BMP as surrogate pairs ("😀"): join them.
    return value.encode("utf-16", "surrogatepass").decode("utf-16", "replace")
