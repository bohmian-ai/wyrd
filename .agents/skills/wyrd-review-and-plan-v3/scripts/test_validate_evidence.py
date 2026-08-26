"""Tests for compact terminal-review specialist evidence."""

from __future__ import annotations

import importlib.util
import json
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
    report_path = "evidence/specialists/baseline-correctness.md"
    attestation_path = "evidence/specialists/baseline-correctness.attestation.json"
    (tmp_path / "baseline-correctness.attestation.json").write_text(
        json.dumps(
            {
                "schema_version": 1,
                "assignment_id": "baseline-correctness",
                "domain": "correctness",
                "reviewer_id": "correctness-session",
                "target_sha": "abcdef1",
                "prompt_sha256": "a" * 64,
                "report_path": report_path,
                "report_sha256": validator.hashlib.sha256(
                    report.read_bytes()
                ).hexdigest(),
                "status": "completed",
            }
        ),
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
                "target_sha": "abcdef1",
                "prompt_digest": "sha256:" + "a" * 64,
                "report_path": report_path,
                "attestation_path": attestation_path,
            }
        },
        errors,
    )

    assert errors == []
    assert candidate_sources == {}


def test_validate_specialists_rejects_attestation_identity_mismatch(
    tmp_path: Path,
) -> None:
    """A persisted report cannot claim another reviewer's assignment."""
    validator = load_validator()
    report = tmp_path / "baseline-correctness.md"
    report.write_text(
        """## Scope

Inspected the assigned scope.

## Adversarial probe

The strongest counterexample survived.

## Candidate findings

None.

## Clean rationale

The invariant is preserved.

## Static-analysis limits

None.
""",
        encoding="utf-8",
    )
    report_path = "evidence/specialists/baseline-correctness.md"
    (tmp_path / "baseline-correctness.attestation.json").write_text(
        json.dumps(
            {
                "schema_version": 1,
                "assignment_id": "baseline-correctness",
                "domain": "correctness",
                "reviewer_id": "different-session",
                "target_sha": "abcdef1",
                "prompt_sha256": "a" * 64,
                "report_path": report_path,
                "report_sha256": validator.hashlib.sha256(
                    report.read_bytes()
                ).hexdigest(),
                "status": "completed",
            }
        ),
        encoding="utf-8",
    )
    errors: list[str] = []

    validator.validate_specialists(
        tmp_path,
        {
            "baseline-correctness": {
                "status": "completed",
                "domain": "correctness",
                "reviewer_id": "correctness-session",
                "target_sha": "abcdef1",
                "prompt_digest": "sha256:" + "a" * 64,
                "report_path": report_path,
                "attestation_path": (
                    "evidence/specialists/baseline-correctness.attestation.json"
                ),
            }
        },
        errors,
    )

    assert "baseline-correctness attestation has invalid reviewer_id" in errors


def test_validate_coverage_requires_capability_not_provider_role() -> None:
    """Coverage binds review capability without naming a provider-specific role."""
    validator = load_validator()
    roster = []
    for domain, capability in validator.BASELINE_CAPABILITIES.items():
        prompt_path = validator.PROMPT_BY_DOMAIN[domain]
        prompt_file = SCRIPT.parents[1] / prompt_path
        roster.append(
            {
                "assignment_id": f"baseline-{domain}",
                "domain": domain,
                "requirement": "baseline",
                "trigger_ids": [],
                "required_capability": capability,
                "reviewer_id": f"{domain}-session",
                "prompt_path": prompt_path,
                "prompt_digest": "sha256:"
                + validator.hashlib.sha256(prompt_file.read_bytes()).hexdigest(),
                "target_sha": "abcdef1",
                "report_path": f"evidence/specialists/baseline-{domain}.md",
                "attestation_path": (
                    f"evidence/specialists/baseline-{domain}.attestation.json"
                ),
                "status": "completed",
            }
        )
    coverage = {
        "schema_version": 3,
        "mode": "full",
        "risk": "low",
        "roster": roster,
        "triggers": [],
        "independent_validation": {
            "status": "not_required",
            "reviewer": "",
            "reason": "low-risk fixture",
        },
        "adversarial_review": {
            "mode": "full",
            "status": "completed",
            "summary": "all domains challenged",
        },
        "changed_files": [],
        "requirements": [],
        "high_risk_boundaries": [],
        "user_facing_capabilities": [],
        "policy_gates": [],
    }
    errors: list[str] = []

    validator.validate_coverage(coverage, errors, "CLEAN")

    assert errors == []


def test_validate_coverage_rejects_reused_triggered_reviewer() -> None:
    """Triggered and baseline assignments must use distinct review contexts."""
    validator = load_validator()
    roster = []
    for domain, capability in validator.BASELINE_CAPABILITIES.items():
        prompt_path = validator.PROMPT_BY_DOMAIN[domain]
        prompt_file = SCRIPT.parents[1] / prompt_path
        roster.append(
            {
                "assignment_id": f"baseline-{domain}",
                "domain": domain,
                "requirement": "baseline",
                "trigger_ids": [],
                "required_capability": capability,
                "reviewer_id": f"{domain}-session",
                "prompt_path": prompt_path,
                "prompt_digest": "sha256:"
                + validator.hashlib.sha256(prompt_file.read_bytes()).hexdigest(),
                "target_sha": "abcdef1",
                "report_path": f"evidence/specialists/baseline-{domain}.md",
                "attestation_path": (
                    f"evidence/specialists/baseline-{domain}.attestation.json"
                ),
                "status": "completed",
            }
        )
    prompt_path = validator.PROMPT_BY_DOMAIN["persistence-storage"]
    prompt_file = SCRIPT.parents[1] / prompt_path
    roster.append(
        {
            "assignment_id": "triggered-persistence",
            "domain": "persistence-storage",
            "requirement": "triggered",
            "trigger_ids": [],
            "required_capability": "persistence-specialist",
            "reviewer_id": "correctness-session",
            "prompt_path": prompt_path,
            "prompt_digest": "sha256:"
            + validator.hashlib.sha256(prompt_file.read_bytes()).hexdigest(),
            "target_sha": "abcdef1",
            "report_path": "evidence/specialists/triggered-persistence.md",
            "attestation_path": (
                "evidence/specialists/triggered-persistence.attestation.json"
            ),
            "status": "completed",
        }
    )
    coverage = {
        "schema_version": 3,
        "mode": "full",
        "risk": "low",
        "roster": roster,
        "triggers": [],
        "independent_validation": {
            "status": "not_required",
            "reviewer": "",
            "reason": "low-risk fixture",
        },
        "adversarial_review": {
            "mode": "full",
            "status": "completed",
            "summary": "all domains challenged",
        },
        "changed_files": [],
        "requirements": [],
        "high_risk_boundaries": [],
        "user_facing_capabilities": [],
        "policy_gates": [],
    }
    errors: list[str] = []

    validator.validate_coverage(coverage, errors, "CLEAN")

    assert "completed assignments share reviewer: correctness-session" in errors
