#!/usr/bin/env python3
"""Verify Wyrd test-critical Rust contracts are covered by Python tests."""

from __future__ import annotations

import re
import sys
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RUST_ROOTS = [ROOT / "crates", ROOT / "sdks" / "wyrd-sdk-python" / "src"]
PYTHON_TEST_ROOTS = [ROOT / "sdks" / "wyrd-sdk-python" / "tests"]

CONTRACT_RE = re.compile(
    r"#\[\s*wyrd_test_contract_macros::critical\s*\(\s*['\"]([^'\"]+)['\"]\s*\)\s*\]"
)
COVERAGE_RE = re.compile(
    r"pytest\.mark\.wyrd_covers\s*\(\s*['\"]([^'\"]+)['\"]\s*\)"
)


@dataclass(frozen=True)
class FoundId:
    """A contract or coverage marker with its source location."""

    id: str
    path: Path
    line: int


def iter_files(roots: list[Path], suffix: str) -> list[Path]:
    """Return matching files under existing roots."""
    files: list[Path] = []
    for root in roots:
        if root.exists():
            files.extend(sorted(root.rglob(f"*{suffix}")))
    return files


def collect(pattern: re.Pattern[str], files: list[Path]) -> list[FoundId]:
    """Collect ids matched by `pattern` from files."""
    found: list[FoundId] = []
    for path in files:
        text = path.read_text(encoding="utf-8")
        for match in pattern.finditer(text):
            line = text.count("\n", 0, match.start()) + 1
            found.append(FoundId(match.group(1), path, line))
    return found


def location(value: FoundId) -> str:
    """Format a source location relative to the repository root."""
    return f"{value.path.relative_to(ROOT)}:{value.line}"


def main() -> int:
    """Run the contract coverage check."""
    contracts = collect(CONTRACT_RE, iter_files(RUST_ROOTS, ".rs"))
    coverage = collect(COVERAGE_RE, iter_files(PYTHON_TEST_ROOTS, ".py"))

    contract_ids = {item.id for item in contracts}
    covered_ids = {item.id for item in coverage}
    missing = sorted(contract_ids - covered_ids)
    unknown = sorted(covered_ids - contract_ids)

    if missing or unknown:
        if missing:
            print("Missing Python test coverage for Wyrd critical contracts:")
            for contract_id in missing:
                sources = ", ".join(location(item) for item in contracts if item.id == contract_id)
                print(f"  - {contract_id} declared at {sources}")
        if unknown:
            print("Python tests reference unknown Wyrd critical contracts:")
            for contract_id in unknown:
                sources = ", ".join(location(item) for item in coverage if item.id == contract_id)
                print(f"  - {contract_id} referenced at {sources}")
        return 1

    print(f"Wyrd test contracts covered: {len(contract_ids)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
