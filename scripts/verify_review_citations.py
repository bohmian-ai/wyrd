#!/usr/bin/env python3
"""Drop review findings whose citations do not match the candidate commit.

Every reviewer report (`*review*.md`) in a review directory must contain exactly
one fenced ```citations block holding a JSON array (empty when the report has no
findings). Each entry is {"id", "path", "line", "snippet", "symbols"}. A finding
is OK only when, at the candidate commit, the file exists, the snippet's
non-blank lines appear verbatim (whitespace-trimmed) starting at `line`, and
every symbol occurs as a whole word in the file.

Prints `<id>\tOK` or `<id>\tREJECT\t<reason>` per finding and exits 0.
Exits 2 (review BLOCKED) on a missing/duplicate/malformed citations block.

ponytail: textual symbol match; switch to the extractor's declarations and
references if hallucinated symbols start passing.
"""

import json
import re
import subprocess
import sys
from pathlib import Path

BLOCK = re.compile(r"^```citations\s*\n(.*?)^```", re.S | re.M)


def read_blob(candidate: str, path: str) -> str | None:
    """Returns the file at `candidate:path`, or None when it does not exist."""
    result = subprocess.run(
        ["git", "show", f"{candidate}:{path}"], capture_output=True, text=True
    )
    return result.stdout if result.returncode == 0 else None


def check(entry: dict, source: str | None) -> str | None:
    """Returns a rejection reason for one citation, or None when it holds."""
    if source is None:
        return f"{entry['path']} does not exist at candidate"
    lines = [line.strip() for line in source.splitlines()]
    wanted = [line.strip() for line in entry["snippet"].splitlines() if line.strip()]
    start = entry["line"] - 1
    window = [line for line in lines[start:] if line][: len(wanted)]
    if not wanted or window != wanted:
        return f"snippet not found at {entry['path']}:{entry['line']}"
    for symbol in entry.get("symbols", []):
        if not re.search(rf"(?<!\w){re.escape(symbol)}(?!\w)", source):
            return f"symbol {symbol!r} not in {entry['path']}"
    return None


def citations(report: Path) -> list[dict]:
    """Parses the single citations block in a report.

    Raises ValueError when the block is missing, duplicated, or malformed.
    """
    blocks = BLOCK.findall(report.read_text())
    if len(blocks) != 1:
        raise ValueError(f"{report.name}: expected 1 citations block, found {len(blocks)}")
    entries = json.loads(blocks[0])
    for entry in entries:
        missing = {"id", "path", "line", "snippet"} - entry.keys()
        if missing:
            raise ValueError(f"{report.name}: citation missing {sorted(missing)}")
    return entries


def main(review_dir: str, candidate: str) -> int:
    """Checks every reviewer report in `review_dir` against `candidate`."""
    reports = sorted(Path(review_dir).glob("*review*.md"))
    if not reports:
        print(f"no reviewer reports in {review_dir}", file=sys.stderr)
        return 2
    try:
        entries = [entry for report in reports for entry in citations(report)]
    except (ValueError, json.JSONDecodeError) as error:
        print(error, file=sys.stderr)
        return 2
    for entry in entries:
        reason = check(entry, read_blob(candidate, entry["path"]))
        print(f"{entry['id']}\tOK" if reason is None else f"{entry['id']}\tREJECT\t{reason}")
    return 0


if __name__ == "__main__":
    if len(sys.argv) != 3:
        print("usage: verify_review_citations.py <review-dir> <candidate-commit>", file=sys.stderr)
        sys.exit(2)
    sys.exit(main(sys.argv[1], sys.argv[2]))
