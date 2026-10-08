"""Boundary tests for the wyrd.cards.CardRef pyclass."""

import pytest
from wyrd import WyrdError
from wyrd.cards import CardKind, CardRef


def test_constructs_card_ref_with_space() -> None:
    ref = CardRef(kind="Artifact", name="weights", version="1.0.0", space="default")

    assert ref.kind == CardKind.Artifact
    assert ref.name == "weights"
    assert ref.version == "1.0.0"
    assert ref.space == "default"
    assert ref.uid is None


def test_constructs_full_card_ref() -> None:
    ref = CardRef(
        kind=CardKind.Model,
        name="churn-classifier",
        version="1.4.2",
        space="prod",
        uid="018f90f5-8e1b-7c4a-a834-4d2d4df6e9c2",
    )

    assert ref.space == "prod"
    assert ref.uid == "018f90f5-8e1b-7c4a-a834-4d2d4df6e9c2"


def test_equality_by_value() -> None:
    a = CardRef(kind=CardKind.Data, name="dataset", version="1.0.0", space="default")
    b = CardRef(kind=CardKind.Data, name="dataset", version="1.0.0", space="default")
    c = CardRef(kind=CardKind.Data, name="dataset", version="1.1.0", space="default")

    assert a == b
    assert a != c


def test_repr_shows_identity_fields() -> None:
    ref = CardRef(kind="Artifact", name="weights", version="1.0.0", space="prod")

    assert (
        repr(ref)
        == "CardRef(kind='Artifact', name='weights', version='1.0.0', space='prod', uid=None)"
    )


def test_rejects_unknown_kind() -> None:
    with pytest.raises(WyrdError) as exc_info:
        CardRef(kind="NotAKind", name="card-x", version="1.0.0", space="default")

    assert exc_info.value.code == "WYRD_SPEC_400_VALIDATION"


def test_rejects_invalid_version() -> None:
    with pytest.raises(WyrdError) as exc_info:
        CardRef(kind=CardKind.Model, name="card-x", version="not-a-version", space="default")

    assert exc_info.value.code == "WYRD_SPEC_400_VALIDATION"


@pytest.mark.parametrize(
    "kind",
    [
        "Data",
        "Model",
        "Experiment",
        "Prompt",
        "Agent",
        "Workflow",
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
    ref = CardRef(kind=kind, name="card-x", version="1.0.0", space="default")

    assert ref.kind.name == kind


def test_rejects_missing_space() -> None:
    with pytest.raises(TypeError):
        CardRef(kind=CardKind.Model, name="card-x", version="1.0.0")  # ty: ignore[missing-argument]


def test_rejects_empty_space() -> None:
    with pytest.raises(WyrdError) as exc_info:
        CardRef(kind=CardKind.Model, name="card-x", version="1.0.0", space="")

    assert exc_info.value.code == "WYRD_SPEC_400_VALIDATION"


def test_rejects_invalid_space() -> None:
    with pytest.raises(WyrdError) as exc_info:
        CardRef(kind=CardKind.Model, name="card-x", version="1.0.0", space="A B")

    assert exc_info.value.code == "WYRD_SPEC_400_VALIDATION"
