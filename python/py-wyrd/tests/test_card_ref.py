"""Boundary tests for the wyrd.cards.CardRef pyclass."""

import pytest

from wyrd._wyrd import WyrdError
from wyrd.cards import CardRef, Kind


def test_constructs_minimal_card_ref() -> None:
    ref = CardRef(kind="Artifact", name="weights", version="1.0.0")

    assert ref.kind == Kind.Artifact
    assert ref.name == "weights"
    assert ref.version == "1.0.0"
    assert ref.space is None
    assert ref.uid is None


def test_constructs_full_card_ref() -> None:
    ref = CardRef(
        kind=Kind.Model,
        name="churn-classifier",
        version="1.4.2",
        space="prod",
        uid="018f90f5-8e1b-7c4a-a834-4d2d4df6e9c2",
    )

    assert ref.space == "prod"
    assert ref.uid == "018f90f5-8e1b-7c4a-a834-4d2d4df6e9c2"


def test_equality_by_value() -> None:
    a = CardRef(kind=Kind.Data, name="dataset", version="1.0.0")
    b = CardRef(kind=Kind.Data, name="dataset", version="1.0.0")
    c = CardRef(kind=Kind.Data, name="dataset", version="1.1.0")

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
        CardRef(kind="NotAKind", name="x", version="1.0.0")

    assert exc_info.value.code == "WYRD_SPEC_400_VALIDATION"
    assert "unknown card kind" in str(exc_info.value).lower()


def test_rejects_invalid_version() -> None:
    with pytest.raises(WyrdError) as exc_info:
        CardRef(kind=Kind.Model, name="x", version="not-a-version")

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
    ],
)
def test_every_native_kind_is_accepted(kind: str) -> None:
    ref = CardRef(kind=kind, name="x", version="1.0.0")

    assert ref.kind.name == kind


@pytest.mark.parametrize(
    ("kind", "name"),
    [
        (Kind.Data, "Data"),
        (Kind.Model, "Model"),
        (Kind.Experiment, "Experiment"),
        (Kind.Prompt, "Prompt"),
        (Kind.Tool, "Tool"),
        (Kind.Agent, "Agent"),
        (Kind.Workflow, "Workflow"),
        (Kind.Eval, "Eval"),
        (Kind.Drift, "Drift"),
        (Kind.Service, "Service"),
        (Kind.Policy, "Policy"),
        (Kind.Mcp, "Mcp"),
        (Kind.Skill, "Skill"),
        (Kind.SubAgent, "SubAgent"),
        (Kind.Audit, "Audit"),
        (Kind.Artifact, "Artifact"),
        (Kind.Trigger, "Trigger"),
        (Kind.Operator, "Operator"),
    ],
)
def test_every_native_kind_enum_is_accepted(kind: Kind, name: str) -> None:
    ref = CardRef(kind=kind, name="x", version="1.0.0")

    assert ref.kind == kind
    assert ref.kind.name == name
