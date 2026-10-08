"""The generated typed Cards that `cards.get` returns.

`cards.get` passes each registered envelope through the generated
``from_wire``, so these tests pin its kind-discriminated decoding into frozen
dataclasses read by attribute. The classes are generated from the `wyrd-spec`
Card schema, so drift fails `codegen:check` before it reaches here.
"""

from __future__ import annotations

import dataclasses

import pytest
from wyrd._card_types import from_wire
from wyrd.cards import (
    DriftBaselineState,
    RegisteredCard,
    RegisteredVerifierCard,
    TypedCardKind,
)

BUILDING_DRIFT_VERIFIER = {
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
    """Narrow a registered Card by its class and read its typed baseline state."""
    if not isinstance(card, RegisteredVerifierCard) or card.status is None:
        return None
    verification = card.status.verification
    if verification is None or verification.baseline is None:
        return None
    return verification.baseline.state


def test_drift_verifier_envelope_exposes_typed_baseline_state() -> None:
    card = from_wire(BUILDING_DRIFT_VERIFIER)

    assert isinstance(card, RegisteredVerifierCard)
    assert card.metadata.name == "churn-drift"
    assert baseline_state(card) == "building"


def test_tagged_enum_members_decode_to_their_own_variant() -> None:
    card = from_wire(BUILDING_DRIFT_VERIFIER)

    assert isinstance(card, RegisteredVerifierCard)
    strategy = card.spec.implementation.spec.profile.binning_strategy  # ty: ignore[unresolved-attribute]
    assert type(strategy).__name__ == "PsiBinningStrategyQuantile"


def test_typed_cards_are_frozen() -> None:
    card = from_wire(BUILDING_DRIFT_VERIFIER)

    with pytest.raises(dataclasses.FrozenInstanceError):
        card.kind = "Data"  # ty: ignore[invalid-assignment]


def test_deferred_kinds_keep_a_plain_spec() -> None:
    card = from_wire(
        {
            "apiVersion": "wyrd/v1",
            "kind": "Workflow",
            "metadata": {"name": "flow", "space": "unit", "version": "1.0.0"},
            "spec": {"entry": "agent"},
        }
    )

    assert type(card).__name__ == "RegisteredWorkflowCard"
    assert card.spec == {"entry": "agent"}


def test_deferred_kinds_are_not_typed_kinds() -> None:
    typed: TypedCardKind = "Verifier"
    deferred: TypedCardKind = "Workflow"  # ty: ignore[invalid-assignment]
    assert [typed, deferred] == ["Verifier", "Workflow"]
