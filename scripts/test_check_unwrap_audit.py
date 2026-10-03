#!/usr/bin/env python3
"""Fixtures for `check_unwrap_audit.py`'s cfg-test module allowlist.

Run with `uv run python scripts/test_check_unwrap_audit.py`.
"""

from __future__ import annotations

import tempfile
from pathlib import Path

import check_unwrap_audit

BODY = "fn production() -> u32 {\n    Some(1).unwrap()\n}\n"
ALLOWLISTED = "crates/wyrd-spec/src/query/tests.rs"
PRODUCTION = "crates/demo/src/tests.rs"


def audit_fixtures() -> list[str]:
    """Audit an allowlisted and a production `tests.rs` with identical bodies
    under a temporary root and return the flagged relative paths."""
    with tempfile.TemporaryDirectory() as root:
        check_unwrap_audit.ROOT = Path(root)
        for relative in (ALLOWLISTED, PRODUCTION):
            path = Path(root, relative)
            path.parent.mkdir(parents=True)
            path.write_text(BODY, encoding="utf-8")
        return [finding.path.relative_to(root).as_posix() for finding in check_unwrap_audit.audit()]


def test_allowlisted_cfg_test_module_is_skipped() -> None:
    """An allowlisted cfg-test module body is not scanned."""
    assert ALLOWLISTED not in audit_fixtures()


def test_production_tests_rs_is_scanned_and_rejected() -> None:
    """A production file named `tests.rs` is still scanned and rejected."""
    assert audit_fixtures() == [PRODUCTION]


if __name__ == "__main__":
    test_allowlisted_cfg_test_module_is_skipped()
    test_production_tests_rs_is_scanned_and_rejected()
    print("check_unwrap_audit fixtures passed")
