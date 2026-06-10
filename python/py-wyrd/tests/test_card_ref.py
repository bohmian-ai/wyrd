"""Boundary tests for the wyrd.cards.CardRef pyclass."""

import pytest
from wyrd._wyrd import WyrdError
from wyrd.cards import CardKind, CardRef


def test_constructs_minimal_card_ref() -> None:
    ref = CardRef(kind="Artifact", name="weights", version="1.0.0")

    assert ref.kind == CardKind.Artifact
    assert ref.name == "weights"
    assert ref.version == "1.0.0"
    assert ref.space is None
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
    a = CardRef(kind=CardKind.Data, name="dataset", version="1.0.0")
    b = CardRef(kind=CardKind.Data, name="dataset", version="1.0.0")
    c = CardRef(kind=CardKind.Data, name="dataset", version="1.1.0")

    assert a == b
    assert a != c


def test_repr_contains_identity_fields() -> None:
    ref = CardRef(kind="Artifact", name="weights", version="1.0.0", space="prod")
    text = repr(ref)

    assert "CardRef(" in text
    assert "kind='Artifact'" in text
    assert "name='weights'" in text
    assert "version='1.0.0'" in text
    assert "space='prod'" in text


def test_rejects_unknown_kind() -> None:
    with pytest.raises(WyrdError) as exc_info:
        CardRef(kind="NotAKind", name="card-x", version="1.0.0")

    assert exc_info.value.code == "WYRD_SPEC_400_VALIDATION"
    assert "unknown card kind" in str(exc_info.value).lower()


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
        "Tool",
        "Agent",
        "Workflow",
        "Eval",
        "Drift",
        "Service",
        "Policy",
        "Mcp",
        "Skill",
        "SubAgent",
        "Audit",
        "Artifact",
        "Trigger",
        "Operator",
        "External",
    ],
)
def test_every_native_kind_is_accepted(kind: str) -> None:
    ref = CardRef(kind=kind, name="card-x", version="1.0.0")

    assert ref.kind.name == kind


@pytest.mark.parametrize(
    ("kind", "name"),
    [
        (CardKind.Data, "Data"),
        (CardKind.Model, "Model"),
        (CardKind.Experiment, "Experiment"),
        (CardKind.Prompt, "Prompt"),
        (CardKind.Tool, "Tool"),
        (CardKind.Agent, "Agent"),
        (CardKind.Workflow, "Workflow"),
        (CardKind.Eval, "Eval"),
        (CardKind.Drift, "Drift"),
        (CardKind.Service, "Service"),
        (CardKind.Policy, "Policy"),
        (CardKind.Mcp, "Mcp"),
        (CardKind.Skill, "Skill"),
        (CardKind.SubAgent, "SubAgent"),
        (CardKind.Audit, "Audit"),
        (CardKind.Artifact, "Artifact"),
        (CardKind.Trigger, "Trigger"),
        (CardKind.Operator, "Operator"),
        (CardKind.External, "External"),
    ],
)
def test_every_native_kind_enum_is_accepted(kind: CardKind, name: str) -> None:
    ref = CardRef(kind=kind, name="card-x", version="1.0.0")

    assert ref.kind == kind
    assert ref.kind.name == name
