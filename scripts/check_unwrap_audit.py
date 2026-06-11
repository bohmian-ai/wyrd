#!/usr/bin/env python3
"""Audit unwrap/expect calls outside Rust tests and examples."""

from __future__ import annotations

import re
import sys
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CALL_RE = re.compile(r"\.(unwrap|expect)\(")

# Documented Wyrd invariant markers. A `.expect(MARKER)` call whose message is
# one of these strings is treated as a named invariant — the marker IS the
# documentation. Adding to this set should be deliberate: each entry is a
# convention the codebase has standardized on, not a free-form escape hatch.
INVARIANT_MARKERS: frozenset[str] = frozenset(
    {
        # PyO3 bridge wrappers in skald-prompt/src/wire_py.rs: the wrapper
        # struct is only constructible when the optional inner field is Some,
        # so a downstream `.expect("guarded")` is statically unreachable.
        "guarded",
    }
)

INVARIANT_EXPECT_RE = re.compile(
    r"""\.expect\(\s*"(?P<marker>[^"\\]*)"\s*\)"""
)


def line_is_invariant_only(line: str) -> bool:
    """Return whether every `.unwrap(`/`.expect(` on `line` is a known marker.

    A line passes only when no `.unwrap(` appears AND every `.expect(...)` it
    contains carries a message from `INVARIANT_MARKERS`.
    """
    if ".unwrap(" in line:
        return False
    matches = list(INVARIANT_EXPECT_RE.finditer(line))
    if not matches:
        return False
    if any(match.group("marker") not in INVARIANT_MARKERS for match in matches):
        return False
    stripped = INVARIANT_EXPECT_RE.sub("", line)
    return ".expect(" not in stripped


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


def _match_block_end(text: str, brace: int) -> int:
    """Return position one past the `}` that closes the block opened at `brace`.

    Skips braces inside `"..."`, `r#"..."#`, `'...'`, line comments, and block
    comments so JSON snippets in assertions / panic format strings can't drag
    the boundary off the end of the file.
    """
    depth = 0
    index = brace
    length = len(text)
    while index < length:
        char = text[index]
        if char == "/" and index + 1 < length and text[index + 1] == "/":
            newline = text.find("\n", index + 2)
            index = length if newline == -1 else newline + 1
            continue
        if char == "/" and index + 1 < length and text[index + 1] == "*":
            close = text.find("*/", index + 2)
            index = length if close == -1 else close + 2
            continue
        if char == "r" and index + 1 < length and text[index + 1] in ("#", '"'):
            cursor = index + 1
            hashes = 0
            while cursor < length and text[cursor] == "#":
                hashes += 1
                cursor += 1
            if cursor < length and text[cursor] == '"':
                terminator = '"' + ("#" * hashes)
                close = text.find(terminator, cursor + 1)
                index = length if close == -1 else close + len(terminator)
                continue
        if char == '"':
            cursor = index + 1
            while cursor < length:
                if text[cursor] == "\\" and cursor + 1 < length:
                    cursor += 2
                    continue
                if text[cursor] == '"':
                    cursor += 1
                    break
                cursor += 1
            index = cursor
            continue
        if char == "'":
            cursor = index + 1
            while cursor < length and cursor - index < 4:
                if text[cursor] == "\\" and cursor + 1 < length:
                    cursor += 2
                    continue
                if text[cursor] == "'":
                    cursor += 1
                    break
                cursor += 1
            index = cursor
            continue
        if char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return index + 1
        index += 1
    return brace


def cfg_test_ranges(text: str) -> list[tuple[int, int]]:
    """Return line ranges covered by inline `#[cfg(test)] mod ...` blocks."""
    ranges: list[tuple[int, int]] = []
    cursor = 0
    while True:
        cfg = text.find("#[cfg(test)]", cursor)
        if cfg == -1:
            break
        mod_pos = text.find("mod ", cfg)
        brace = text.find("{", cfg)
        if mod_pos == -1 or brace == -1 or mod_pos > brace:
            cursor = cfg + len("#[cfg(test)]")
            continue

        end = _match_block_end(text, brace)
        start_line = text.count("\n", 0, cfg) + 1
        end_line = text.count("\n", 0, end) + 1
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
        if not CALL_RE.search(line):
            continue
        if line_is_invariant_only(line):
            continue
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
