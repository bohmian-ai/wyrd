#!/usr/bin/env python3
"""Audit `#[allow(clippy::...)]` attributes outside tests and examples.

Policy:
- Any `#[allow(clippy::...)]`, `#![allow(clippy::...)]`, or
  `#[cfg_attr(..., allow(clippy::...))]` in production code (`crates/**/*.rs`
  outside `tests/` and `examples/`) is a finding unless the attribute is
  preceded, on one of the two immediately-prior non-blank lines, by a
  `// justification: <one-line reason>` comment.
- The justification exists so a reader knows why the lint was wrong for
  this specific site. Mirrors the `// SAFETY:` convention for `unsafe`.
- The escape hatch is a last resort. The preferred fix for every common
  clippy lint is documented in `architecture/agent-rules.md`.

Findings print as `path:line: <text>` and the script exits non-zero.
"""

from __future__ import annotations

import re
import sys
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

# Matches:
#   #[allow(clippy::xxx)]
#   #![allow(clippy::xxx, clippy::yyy)]
#   #[cfg_attr(test, allow(clippy::xxx))]
# The `clippy::` token distinguishes clippy lints from rustc lints.
ATTR_RE = re.compile(r"#!?\[[^\]]*\ballow\s*\([^)]*\bclippy::")

JUSTIFICATION_RE = re.compile(r"//\s*justification\s*:\s*\S")


@dataclass(frozen=True)
class Finding:
    """One unjustified clippy-allow location."""

    path: Path
    line: int
    text: str


def is_ignored_path(path: Path) -> bool:
    """Return whether `path` is a Rust test/example path (unaudited).

    Ignored:
    - Anything under a `tests/` or `examples/` directory.
    - Files under a `pg_tests/` directory or named `pg_*.rs` per the repo
      convention that `pg_*` files are Postgres/Docker test modules
      (see `architecture/agent-rules.md`).
    """
    parts = path.relative_to(ROOT).parts
    if "tests" in parts or "examples" in parts or "benches" in parts:
        return True
    if "pg_tests" in parts:
        return True
    if path.name.startswith("pg_") and path.suffix == ".rs":
        return True
    return False


def has_justification_above(lines: list[str], attr_index: int) -> bool:
    """Return whether a `// justification:` comment sits in the block of
    contiguous `//` comment lines immediately above `lines[attr_index]`.

    Walks upward through non-blank `//` lines; the `justification:` marker
    may sit anywhere in the block (top for a wrapped explanation, bottom
    for a one-liner). A blank line, a non-comment line, or a doc comment
    (`///`, `//!`) ends the block.
    """
    i = attr_index - 1
    while i >= 0:
        stripped = lines[i].strip()
        if not stripped:
            return False
        if not stripped.startswith("//"):
            return False
        if stripped.startswith("///") or stripped.startswith("//!"):
            return False
        if JUSTIFICATION_RE.search(stripped):
            return True
        i -= 1
    return False


CFG_TEST_RE = re.compile(r"^\s*#\[\s*cfg\s*\([^)]*\btest\b")
MOD_DECL_RE = re.compile(r"^\s*(?:pub(?:\s*\([^)]*\))?\s+)?mod\s+\w+\s*\{")


def find_cfg_test_line_ranges(lines: list[str]) -> list[tuple[int, int]]:
    """Return 1-indexed inclusive `(start, end)` line ranges for every
    inline `#[cfg(test)] mod ... { ... }` block in the file.

    The heuristic:
    1. A line matching `CFG_TEST_RE` (handles `#[cfg(test)]` and
       `#[cfg(all(test, ...))]`).
    2. The next non-blank line starts a `mod NAME {`.
    3. Track `{`/`}` balance from that opening brace forward; when depth
       returns to zero, the mod block ends.

    Braces inside string literals or block comments are not stripped —
    same limitation as the attribute regex (documented on the test).
    """
    ranges: list[tuple[int, int]] = []
    n = len(lines)
    i = 0
    while i < n:
        if not CFG_TEST_RE.match(lines[i]):
            i += 1
            continue
        j = i + 1
        while j < n and not lines[j].strip():
            j += 1
        if j >= n or not MOD_DECL_RE.match(lines[j]):
            i += 1
            continue
        start = i + 1
        depth = 0
        started = False
        k = j
        while k < n:
            for ch in lines[k]:
                if ch == "{":
                    depth += 1
                    started = True
                elif ch == "}":
                    depth -= 1
            if started and depth == 0:
                ranges.append((start, k + 1))
                i = k + 1
                break
            k += 1
        else:
            ranges.append((start, n))
            break
    return ranges


def _line_in_ranges(line: int, ranges: list[tuple[int, int]]) -> bool:
    for start, end in ranges:
        if start <= line <= end:
            return True
    return False


def _attribute_has_cfg_test_sibling(lines: list[str], attr_index: int) -> bool:
    """Return True if the `#[allow(...)]` at `lines[attr_index]` is part of
    an attribute stack (contiguous `#[...]` lines with no blank/code between)
    that also contains a `#[cfg(test)]` (or `#[cfg(all(test, ...))]`).

    Handles both orderings:
        #[cfg(test)]
        #[allow(clippy::...)]
        fn t() { }
    and
        #[allow(clippy::...)]
        #[cfg(test)]
        fn t() { }
    """
    i = attr_index - 1
    while i >= 0:
        stripped = lines[i].strip()
        if not stripped:
            break
        if not stripped.startswith("#["):
            break
        if CFG_TEST_RE.match(lines[i]):
            return True
        i -= 1
    i = attr_index + 1
    while i < len(lines):
        stripped = lines[i].strip()
        if not stripped:
            break
        if not stripped.startswith("#["):
            break
        if CFG_TEST_RE.match(lines[i]):
            return True
        i += 1
    return False


def scan_file(path: Path) -> list[Finding]:
    """Return unjustified `#[allow(clippy::...)]` findings from one file.

    Skips attributes inside inline `#[cfg(test)] mod ... { ... }` blocks or
    attached to a `#[cfg(test)]`-gated item — test-only code is not audited
    (mirrors the tests/ and examples/ exclusions in `is_ignored_path`).
    """
    text = path.read_text(encoding="utf-8")
    lines = text.splitlines()
    cfg_test_ranges = find_cfg_test_line_ranges(lines)
    findings: list[Finding] = []
    for line_number, raw_line in enumerate(lines, start=1):
        if not ATTR_RE.search(raw_line):
            continue
        if _line_in_ranges(line_number, cfg_test_ranges):
            continue
        if _attribute_has_cfg_test_sibling(lines, line_number - 1):
            continue
        if has_justification_above(lines, line_number - 1):
            continue
        findings.append(Finding(path, line_number, raw_line.strip()))
    return findings


def main() -> int:
    """Run the clippy-allow audit."""
    findings: list[Finding] = []
    for path in sorted((ROOT / "crates").rglob("*.rs")):
        if is_ignored_path(path):
            continue
        findings.extend(scan_file(path))

    if findings:
        for finding in findings:
            location = finding.path.relative_to(ROOT)
            print(f"{location}:{finding.line}: {finding.text}")
        print()
        print(
            "clippy-allow audit failed: each production `#[allow(clippy::...)]` must be"
        )
        print(
            "preceded on the previous non-blank line by `// justification: <reason>`."
        )
        print(
            "See architecture/agent-rules.md for the preferred fix for each common lint."
        )
        return 1

    print("clippy-allow audit passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
