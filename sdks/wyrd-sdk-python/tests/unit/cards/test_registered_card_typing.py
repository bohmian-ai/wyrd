"""Type fixture pinning the generated `cards.get` envelope types.

`ty` checks this module in the `py:typecheck` lane with unused suppressions as
errors, so every valid envelope below must type-check and every
``ty: ignore`` line must be a real rejection. The types are generated from the
`wyrd-spec` Card schema, so drift fails `codegen:check` before it reaches here.
They exist only for type checkers: the native extension owns ``wyrd.cards`` at
runtime, so callers import them under ``TYPE_CHECKING``.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from wyrd.cards import (
        DriftBaselineState,
        RegisteredCard,
        RegisteredVerifierCard,
        TypedCardKind,
    )

BUILDING_DRIFT_VERIFIER: RegisteredVerifierCard = {
    "apiVersion": "wyrd/v1",
    "kind": "Verifier",
    "metadata": {"name": "churn-drift", "space": "retention", "version": "1.0.0"},
    "spec": {
        "implementation": {
            "kind": "drift",
            "spec": {
                "method": "Psi",
                "signal": {
                    "kind": "Distribution",
                    "baseline_ref": {"kind": "Data", "name": "training", "version": "1.0.0"},
                    "features": ["feature"],
                },
                "condition": {"kind": "Statistical"},
                "profile": {
                    "kind": "Psi",
                    "binning_strategy": {"kind": "Quantile", "n_bins": 10},
                    "threshold": {"kind": "Fixed", "value": 0.25},
                },
            },
        }
    },
    "status": {
        "phase": "active",
        "verification": {
            "baseline": {
                "state": "building",
                "data": {"kind": "Data", "name": "training", "version": "1.0.0"},
            }
        },
    },
}


def baseline_state(card: RegisteredCard) -> DriftBaselineState | None:
    """Narrow a registered envelope by `kind` and read its typed baseline state."""
    if card["kind"] != "Verifier":
        return None
    verification = (card.get("status") or {}).get("verification") or {}
    baseline = verification.get("baseline")
    return None if baseline is None else baseline["state"]


def test_drift_verifier_envelope_exposes_typed_baseline_state() -> None:
    assert baseline_state(BUILDING_DRIFT_VERIFIER) == "building"


def test_deferred_kinds_are_not_typed_kinds() -> None:
    typed: TypedCardKind = "Verifier"
    deferred: TypedCardKind = "Workflow"  # ty: ignore[invalid-assignment]
    assert [typed, deferred] == ["Verifier", "Workflow"]


def test_unknown_spec_field_is_rejected_by_the_type() -> None:
    card: RegisteredVerifierCard = {
        "apiVersion": "wyrd/v1",
        "kind": "Verifier",
        "metadata": {"name": "n"},
        "spec": {"implementation_kind": "drift"},  # ty: ignore[invalid-key, missing-typed-dict-key]
    }
    assert card["kind"] == "Verifier"
