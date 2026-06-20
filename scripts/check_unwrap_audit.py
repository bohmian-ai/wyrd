#!/usr/bin/env python3
"""Audit unwrap/expect calls outside Rust tests and examples.

Policy:
- `.unwrap()` is always blocked in production code.
- `.expect(<arg>)` is permitted iff `<arg>` is a non-empty Rust string
  literal (regular, byte, raw, or raw-byte). Dynamic messages,
  empty literals, and non-literal arguments are blocked.
"""

from __future__ import annotations

import re
import sys
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CALL_RE = re.compile(r"\.(unwrap|expect)\(")


@dataclass(frozen=True)
class Finding:
    """A production unwrap/expect call location."""

    path: Path
    line: int
    text: str


def is_ignored_path(path: Path) -> bool:
    """Return whether `path` is a Rust test/example path."""
    parts = path.relative_to(ROOT).parts
    return "tests" in parts or "examples" in parts


def read_string_literal(text: str, i: int) -> tuple[int, str] | None:
    """If `text` at offset `i` begins a Rust string literal, return
    `(end_index, body)`; otherwise return `None`.

    Recognized forms: regular `"..."`, byte `b"..."`, raw `r"..."` and
    `r#"..."#` (any hash count), raw-byte `br"..."` / `br#"..."#`.
    """
    n = len(text)
    if i >= n:
        return None
    j = i
    if text[j] == "b" and j + 1 < n and text[j + 1] in "r\"":
        j += 1
    if j < n and text[j] == "r" and j + 1 < n and text[j + 1] in "#\"":
        hashes = 0
        k = j + 1
        while k < n and text[k] == "#":
            hashes += 1
            k += 1
        if k >= n or text[k] != '"':
            return None
        terminator = '"' + "#" * hashes
        end = text.find(terminator, k + 1)
        if end == -1:
            return None
        return end + len(terminator), text[k + 1 : end]
    if j < n and text[j] == '"':
        k = j + 1
        body: list[str] = []
        while k < n:
            if text[k] == "\\" and k + 1 < n:
                body.append(text[k : k + 2])
                k += 2
                continue
            if text[k] == '"':
                return k + 1, "".join(body)
            body.append(text[k])
            k += 1
        return None
    return None


def expect_arg_is_named_literal(line: str, after_open_paren: int) -> bool:
    """Return whether the argument to `.expect(` starting at
    `after_open_paren` is a single non-empty Rust string literal.

    The match must be a plain literal with only optional surrounding
    whitespace inside the parentheses. Constructs like
    `.expect(&format!(...))`, `.expect(var)`, `.expect("")`,
    `.expect("msg".into())` are all rejected.
    """
    n = len(line)
    i = after_open_paren
    while i < n and line[i] in " \t":
        i += 1
    parsed = read_string_literal(line, i)
    if parsed is None:
        return False
    end, body = parsed
    if not body:
        return False
    j = end
    while j < n and line[j] in " \t":
        j += 1
    return j < n and line[j] == ")"


def strip_strings_and_comments(text: str) -> str:
    """Return `text` with Rust string literals, char literals, and comments
    replaced by spaces of equal length so byte offsets stay aligned.

    This lets a downstream brace-counter ignore `{`/`}` chars that appear
    inside string or comment content.
    """
    out = list(text)
    n = len(text)
    i = 0
    while i < n:
        ch = text[i]
        nxt = text[i + 1] if i + 1 < n else ""
        if ch == "/" and nxt == "/":
            j = text.find("\n", i)
            j = n if j == -1 else j
            for k in range(i, j):
                if out[k] != "\n":
                    out[k] = " "
            i = j
            continue
        if ch == "/" and nxt == "*":
            j = text.find("*/", i + 2)
            j = n if j == -1 else j + 2
            for k in range(i, j):
                if out[k] != "\n":
                    out[k] = " "
            i = j
            continue
        if ch in ("r", "b") or ch == '"':
            parsed = read_string_literal(text, i)
            if parsed is not None:
                end, _ = parsed
                for k in range(i, end):
                    if out[k] != "\n":
                        out[k] = " "
                i = end
                continue
        if ch == "'":
            j = i + 1
            if j < n and text[j] == "\\" and j + 1 < n:
                j += 2
            elif j < n:
                j += 1
            if j < n and text[j] == "'":
                j += 1
                for k in range(i, j):
                    if out[k] != "\n":
                        out[k] = " "
                i = j
                continue
        i += 1
    return "".join(out)


def cfg_test_ranges(text: str) -> list[tuple[int, int]]:
    """Return line ranges covered by inline `#[cfg(test)] mod ...` blocks."""
    sanitized = strip_strings_and_comments(text)
    ranges: list[tuple[int, int]] = []
    cursor = 0
    while True:
        cfg = sanitized.find("#[cfg(test)]", cursor)
        if cfg == -1:
            break
        mod_pos = sanitized.find("mod ", cfg)
        brace = sanitized.find("{", cfg)
        if mod_pos == -1 or brace == -1 or mod_pos > brace:
            cursor = cfg + len("#[cfg(test)]")
            continue

        depth = 0
        end = brace
        for index, char in enumerate(sanitized[brace:], start=brace):
            if char == "{":
                depth += 1
            elif char == "}":
                depth -= 1
                if depth == 0:
                    end = index + 1
                    break

        start_line = sanitized.count("\n", 0, cfg) + 1
        end_line = sanitized.count("\n", 0, end) + 1
        ranges.append((start_line, end_line))
        cursor = end
    return ranges


def in_ranges(line: int, ranges: list[tuple[int, int]]) -> bool:
    """Return whether `line` falls inside any ignored line range."""
    return any(start <= line <= end for start, end in ranges)


def scan_file(path: Path) -> list[Finding]:
    """Return production unwrap/expect findings from one Rust file.

    `.unwrap()` is always a finding. `.expect(<arg>)` is a finding unless
    `<arg>` is a non-empty Rust string literal.
    """
    text = path.read_text(encoding="utf-8")
    sanitized = strip_strings_and_comments(text)
    ignored_ranges = cfg_test_ranges(text)
    findings: list[Finding] = []
    raw_lines = text.splitlines()
    scan_lines = sanitized.splitlines()
    for line_number, (raw_line, scan_line) in enumerate(
        zip(raw_lines, scan_lines, strict=False), start=1
    ):
        if in_ranges(line_number, ignored_ranges):
            continue
        for match in CALL_RE.finditer(scan_line):
            kind = match.group(1)
            if kind == "unwrap":
                findings.append(Finding(path, line_number, raw_line.strip()))
            elif not expect_arg_is_named_literal(raw_line, match.end()):
                findings.append(Finding(path, line_number, raw_line.strip()))
    return findings


def main() -> int:
    """Run the unwrap audit."""
    findings: list[Finding] = []
    for path in sorted((ROOT / "crates").rglob("*.rs")):
        if is_ignored_path(path):
            continue
        findings.extend(scan_file(path))

    if findings:
        for finding in findings:
            location = finding.path.relative_to(ROOT)
            print(f"{location}:{finding.line}: {finding.text}")
        print("unwrap/expect audit failed")
        return 1

    print("unwrap/expect audit passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
