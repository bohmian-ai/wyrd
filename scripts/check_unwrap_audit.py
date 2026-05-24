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

        depth = 0
        end = brace
        for index, char in enumerate(text[brace:], start=brace):
            if char == "{":
                depth += 1
            elif char == "}":
                depth -= 1
                if depth == 0:
                    end = index + 1
                    break

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
