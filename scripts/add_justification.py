#!/usr/bin/env python3
"""Insert a `// justification: <reason>` comment above a specific
`#[allow(clippy::LINT)]` attribute line in a Rust source file.

Usage:
    python scripts/add_justification.py FILE LINT REASON [LINT REASON ...]

For every occurrence of `#[allow(clippy::LINT)]`, `#![allow(clippy::LINT)]`,
or `#[allow(clippy::LINT, ...)]` in FILE, inserts a comment line matching
the attribute's leading whitespace immediately above it, unless the
attribute already has a `// justification:` comment on the prior
non-blank line.

Idempotent — running twice does not duplicate the justification.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path


JUSTIFICATION_RE = re.compile(r"//\s*justification\s*:\s*\S")


def has_justification_above(lines: list[str], idx: int) -> bool:
    i = idx - 1
    while i >= 0:
        stripped = lines[i].strip()
        if not stripped:
            return False
        if JUSTIFICATION_RE.search(stripped):
            return True
        return False
    return False


def process(path: Path, pairs: list[tuple[str, str]]) -> int:
    text = path.read_text(encoding="utf-8")
    lines = text.splitlines(keepends=False)
    out: list[str] = []
    inserted = 0
    i = 0
    for line in lines:
        matched_reason: str | None = None
        for lint, reason in pairs:
            attr_re = re.compile(
                rf"^(\s*)#!?\[[^\]]*\ballow\s*\([^)]*\bclippy::{re.escape(lint)}\b"
            )
            m = attr_re.match(line)
            if m:
                if not has_justification_above(out, len(out)):
                    indent = m.group(1)
                    out.append(f"{indent}// justification: {reason}")
                    inserted += 1
                matched_reason = reason
                break
        out.append(line)
        i += 1

    if inserted:
        path.write_text("\n".join(out) + ("\n" if text.endswith("\n") else ""), encoding="utf-8")
    return inserted


def main(argv: list[str]) -> int:
    if len(argv) < 4 or (len(argv) - 2) % 2 != 0:
        print(__doc__, file=sys.stderr)
        return 2
    path = Path(argv[1])
    pairs = [(argv[i], argv[i + 1]) for i in range(2, len(argv), 2)]
    inserted = process(path, pairs)
    print(f"{path}: inserted {inserted} justification(s)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
