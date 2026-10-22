"""Boundary tests for the wyrd.cards.CardRef pyclass."""

import pytest
from wyrd import WyrdError
from wyrd.cards import CardKind, CardRef


def test_constructs_card_ref_without_uid() -> None:
    ref = CardRef(kind="Artifact", name="weights", version="1.0.0")

    assert ref.kind == CardKind.Artifact
    assert ref.name == "weights"
    assert ref.version == "1.0.0"
    assert ref.uid is None


def test_constructs_full_card_ref() -> None:
    ref = CardRef(
        kind=CardKind.Model,
        name="churn-classifier",
        version="1.4.2",
        uid="018f90f5-8e1b-7c4a-a834-4d2d4df6e9c2",
    )

    assert ref.uid == "018f90f5-8e1b-7c4a-a834-4d2d4df6e9c2"


def test_equality_by_value() -> None:
    a = CardRef(kind=CardKind.Data, name="dataset", version="1.0.0")
    b = CardRef(kind=CardKind.Data, name="dataset", version="1.0.0")
    c = CardRef(kind=CardKind.Data, name="dataset", version="1.1.0")

    assert a == b
    assert a != c


def test_repr_shows_identity_fields() -> None:
    ref = CardRef(kind="Artifact", name="weights", version="1.0.0")

    assert repr(ref) == "CardRef(kind='Artifact', name='weights', version='1.0.0', uid=None)"


def test_rejects_unknown_kind() -> None:
    with pytest.raises(WyrdError) as exc_info:
        CardRef(kind="NotAKind", name="card-x", version="1.0.0")

    assert exc_info.value.code == "WYRD_SPEC_400_VALIDATION"


def test_rejects_invalid_version() -> None:
    with pytest.raises(WyrdError) as exc_info:
        CardRef(kind=CardKind.Model, name="card-x", version="not-a-version")

    assert exc_info.value.code == "WYRD_SPEC_400_VALIDATION"


@pytest.mark.parametrize(
    "kind",
    [
        "Data",
        "Model",
        "Experiment",
        "Prompt",
        "Agent",
        "Verifier",
        "Service",
        "Policy",
        "Mcp",
        "Audit",
        "Artifact",
        "Trigger",
        "Operator",
        "Source",
        "External",
    ],
)
def test_every_native_kind_is_accepted(kind: str) -> None:
    ref = CardRef(kind=kind, name="card-x", version="1.0.0")

    assert ref.kind.name == kind


def test_str_is_the_canonical_flat_text() -> None:
    ref = CardRef(kind="Model", name="churn", version="1.4.2")
    pinned = CardRef(
        kind="Model", name="churn", version="1.4.2", uid="018f90f5-8e1b-7c4a-a834-4d2d4df6e9c2"
    )

    assert str(ref) == "Model/churn@1.4.2"
    assert str(pinned) == "Model/churn@1.4.2#018f90f5-8e1b-7c4a-a834-4d2d4df6e9c2"


def test_rejects_a_space_argument() -> None:
    with pytest.raises(TypeError, match="space"):
        CardRef(kind=CardKind.Model, name="card-x", version="1.0.0", space="default")  # ty: ignore[unknown-argument]


def test_has_no_space_attribute() -> None:
    ref = CardRef(kind=CardKind.Model, name="card-x", version="1.0.0")

    assert not hasattr(ref, "space")
    assert "space" not in ref.to_dict()
