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
    if "tests" in parts or "examples" in parts:
        return True
    if "pg_tests" in parts:
        return True
    if path.name.startswith("pg_") and path.suffix == ".rs":
        return True
    return False


def has_justification_above(lines: list[str], attr_index: int) -> bool:
    """Return whether a `// justification:` comment sits in the two
    non-blank lines immediately above `lines[attr_index]`.

    A blank line separates the justification from an unrelated comment; we
    stop scanning at the first blank line above the attribute.
    """
    seen = 0
    i = attr_index - 1
    while i >= 0 and seen < 2:
        stripped = lines[i].strip()
        if not stripped:
            return False
        if JUSTIFICATION_RE.search(stripped):
            return True
        seen += 1
        i -= 1
    return False


def scan_file(path: Path) -> list[Finding]:
    """Return unjustified `#[allow(clippy::...)]` findings from one file."""
    text = path.read_text(encoding="utf-8")
    lines = text.splitlines()
    findings: list[Finding] = []
    for line_number, raw_line in enumerate(lines, start=1):
        if not ATTR_RE.search(raw_line):
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
