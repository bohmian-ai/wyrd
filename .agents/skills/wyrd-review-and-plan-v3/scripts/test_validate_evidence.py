"""Tests for compact terminal-review specialist evidence."""

from __future__ import annotations

import importlib.util
import sys
from pathlib import Path
from types import ModuleType


SCRIPT = (
    Path(__file__).parents[1]
    / "references"
    / "review-core"
    / "scripts"
    / "validate_evidence.py"
)


def load_validator() -> ModuleType:
    """Load the evidence validator without requiring package installation."""
    spec = importlib.util.spec_from_file_location("validate_evidence", SCRIPT)
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def test_validate_specialists_accepts_compact_clean_report(tmp_path: Path) -> None:
    """Identity stays in the packet while a clean report stays concise."""
    validator = load_validator()
    report = tmp_path / "baseline-correctness.md"
    report.write_text(
        """## Scope

- Inspected `src/engine.rs` and its direct caller against REQ-1.

## Adversarial probe

- Invariant attacked: retry must not duplicate a completed operation.
  Result: survived; the idempotency guard dominates the mutation path.

## Candidate findings

None.

## Clean rationale

The assigned behavior is covered by the guard and its caller.

## Static-analysis limits

None.
""",
        encoding="utf-8",
    )
    errors: list[str] = []
    candidate_sources = validator.validate_specialists(
        tmp_path,
        {
            "baseline-correctness": {
                "status": "completed",
                "domain": "correctness",
                "reviewer_id": "correctness-session",
                "report_path": "evidence/specialists/baseline-correctness.md",
            }
        },
        errors,
    )

    assert errors == []
    assert candidate_sources == {}
