#!/usr/bin/env python3
"""Audit unwrap/expect calls outside Rust tests and examples."""

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
        if ch == "r" and nxt in "#\"":
            hashes = 0
            j = i + 1
            while j < n and text[j] == "#":
                hashes += 1
                j += 1
            if j < n and text[j] == '"':
                terminator = '"' + "#" * hashes
                end = text.find(terminator, j + 1)
                end = n if end == -1 else end + len(terminator)
                for k in range(i, end):
                    if out[k] != "\n":
                        out[k] = " "
                i = end
                continue
        if ch == '"':
            j = i + 1
            while j < n:
                if text[j] == "\\" and j + 1 < n:
                    j += 2
                    continue
                if text[j] == '"':
                    j += 1
                    break
                j += 1
            for k in range(i, j):
                if out[k] != "\n":
                    out[k] = " "
            i = j
            continue
        if ch == "b" and nxt == '"':
            j = i + 2
            while j < n:
                if text[j] == "\\" and j + 1 < n:
                    j += 2
                    continue
                if text[j] == '"':
                    j += 1
                    break
                j += 1
            for k in range(i, j):
                if out[k] != "\n":
                    out[k] = " "
            i = j
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
    """Return production unwrap/expect calls from one Rust file."""
    text = path.read_text(encoding="utf-8")
    ignored_ranges = cfg_test_ranges(text)
    findings: list[Finding] = []
    for line_number, line in enumerate(text.splitlines(), start=1):
        if in_ranges(line_number, ignored_ranges):
            continue
        if CALL_RE.search(line):
            findings.append(Finding(path, line_number, line.strip()))
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
