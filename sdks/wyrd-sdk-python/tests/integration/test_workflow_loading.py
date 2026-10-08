"""Workflow files and registered Workflows load, resolve their Card refs, and run.

``fixtures/README.md`` says what each ``workflow_loading`` fixture proves. Its
Prompts send their Chat request to the built-in ``mock`` provider, which
answers with the rendered user message, so each output names the Prompt body
that ran and what was bound into it.
"""

from __future__ import annotations

from dataclasses import asdict
from pathlib import Path
from typing import Any

import pytest
from wyrd import WyrdError
from wyrd.agent import Workflow
from wyrd.cards import CardRef, Cards, RegisteredAgentCard, RegisteredCard, RegisteredWorkflowCard
from wyrd.client import WyrdClient
from wyrd.testing import WyrdTestServer, cli

from .support import FIXTURES, register

pytestmark = pytest.mark.integration

WORKFLOWS = FIXTURES / "cards/workflow_loading"

EXAMPLE = FIXTURES.parent / "examples/workflows/code-review/workflow.yaml"
"""The code-review example Workflow, which calls its models through the Wyrd gateway."""

CODE = {"code": "diff"}
"""The one input every Workflow here runs with."""

LOCAL_REVIEW = (
    "final review of diff | local security review of diff | local correctness review of diff"
)
REGISTERED_REVIEW = (
    "final review of diff"
    " | registered security review of diff"
    " | registered correctness review of diff"
)


PINNED_PROMPTS = {
    "security-reviewer": "security-review-prompt",
    "correctness-reviewer": "correctness-review-prompt",
    "final-reviewer": "final-review-prompt",
}
"""Each Agent the applied ``code-review`` Workflow runs, and the Prompt it is pinned to."""


def outbound(card: RegisteredCard) -> dict[str, Any]:
    """Server-derived outbound relationship targets of a Card, by name."""
    assert card.relationships is not None
    return {target.ref.name: asdict(target.ref) for target in card.relationships.outbound_refs}


def stored_agent(cards: Cards, ref: CardRef) -> RegisteredAgentCard:
    """The registered Agent ``ref`` names, read back typed."""
    card = cards.get(ref)
    assert isinstance(card, RegisteredAgentCard)
    return card


def prompt_ref(agent: RegisteredAgentCard) -> dict[str, Any]:
    """The Card reference an Agent's ``prompt`` is pinned to."""
    prompt = agent.spec.prompt
    # A registered Agent stores its Prompt as a reference, never inline text.
    assert not isinstance(prompt, str)
    return asdict(prompt)


@pytest.fixture(scope="module")
def team(cards: Cards) -> dict[str, CardRef]:
    """The registered ``security-reviewer`` and ``correctness-reviewer`` Agents and Prompts, by name."""
    return {
        **register(cards, "cards/workflow_loading/team/security.yaml"),
        **register(cards, "cards/workflow_loading/team/correctness.yaml"),
    }


@pytest.fixture(scope="module")
def applied(cards: Cards, team: dict[str, CardRef]) -> dict[str, CardRef]:
    """The applied ``code-review`` Workflow and the team, by name."""
    receipt = cli.apply(WORKFLOWS / "mixed/workflow.yaml")
    return {**team, **{outcome.card_ref.name: outcome.card_ref for outcome in receipt.outcomes}}


@pytest.fixture(scope="module")
def newer_security_reviewer(cards: Cards, applied: dict[str, CardRef]) -> CardRef:
    """A newer ``security-reviewer``, registered after the Workflow was applied; pinned runs never pick it up."""
    return register(cards, "cards/workflow_loading/team-v2/security.yaml")["security-reviewer"]


@pytest.fixture
def no_ambient_credential(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> None:
    """Remove every ambient credential, leaving only the server address."""
    for name in ("WYRD_API_KEY", "WYRD_ACCESS_TOKEN", "WYRD_WORKLOAD_TOKEN", "WYRD_TENANT"):
        monkeypatch.delenv(name, raising=False)
    monkeypatch.setenv("WYRD_CONFIG_HOME", str(tmp_path))


@pytest.mark.usefixtures("no_ambient_credential")
def test_local_workflow_runs_without_credentials() -> None:
    local = Workflow.from_path(WORKFLOWS / "shadowed/local-workflow.yaml")

    run = local.run(CODE)

    assert (run.status, run.outputs) == (
        "succeeded",
        {"security": "local security review of diff", "review": LOCAL_REVIEW},
    )


def test_text_input_needs_a_declared_input_named_input() -> None:
    local = Workflow.from_path(WORKFLOWS / "shadowed/local-workflow.yaml")

    with pytest.raises(WyrdError) as refused:
        local.run("diff")
    assert refused.value.code == "WYRD_WORKFLOW_422_RUN_REQUEST"


# YAML text has no directory, so its relative Agent targets never resolve.
def test_yaml_workflow_loads_without_resolving_file_targets() -> None:
    yaml = Workflow.from_yaml((WORKFLOWS / "shadowed/local-workflow.yaml").read_text())

    assert [yaml.space, yaml.name, yaml.version] == ["workflow-loading", "local-review", "1.0.0"]
    assert list(yaml.steps) == ["security", "correctness", "final_review"]
    with pytest.raises(WyrdError) as unresolved:
        yaml.run(CODE)
    assert unresolved.value.code == "WYRD_WORKFLOW_404_AGENT"
    with pytest.raises(WyrdError) as invalid:
        Workflow.from_yaml("kind: Nope")
    assert invalid.value.code == "WYRD_WORKFLOW_422_VALIDATION"


@pytest.mark.usefixtures("no_ambient_credential")
def test_gateway_workflow_without_credentials_is_refused_before_any_step() -> None:
    example = Workflow.from_path(EXAMPLE)

    assert list(example.steps) == ["security", "correctness", "final_review"]
    with pytest.raises(WyrdError) as refused:
        example.run(CODE)
    assert refused.value.code == "WYRD_WORKFLOW_503_BINDING_UNAVAILABLE"


@pytest.mark.usefixtures("team", "no_ambient_credential")
def test_registry_refs_resolve_through_the_registry(reader_key: str) -> None:
    reader = WyrdClient(credential=reader_key)

    run = Workflow.from_path(WORKFLOWS / "mixed/workflow.yaml", client=reader).run(CODE)

    assert (run.status, run.outputs) == ("succeeded", {"review": REGISTERED_REVIEW})


@pytest.mark.usefixtures("team")
def test_registry_refs_without_read_access_are_refused(wyrd_server: WyrdTestServer) -> None:
    no_roles = WyrdClient(credential=wyrd_server.scoped_api_key("workflow_no_roles", []))

    with pytest.raises(WyrdError) as refused:
        Workflow.from_path(WORKFLOWS / "mixed/workflow.yaml", client=no_roles)
    assert refused.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"


@pytest.mark.usefixtures("team")
def test_local_sibling_never_satisfies_a_registry_ref() -> None:
    run = Workflow.from_path(WORKFLOWS / "shadowed/workflow.yaml").run(CODE)

    assert (run.status, run.outputs) == (
        "succeeded",
        {
            "security": "local security review of diff",
            "registered_security": "registered security review of diff",
            "review": LOCAL_REVIEW,
        },
    )


def test_deleted_registry_card_is_refused(cards: Cards) -> None:
    retired = cards.register_from_path(WORKFLOWS / "retired/retired-prompt.yaml")
    cards.prompt.delete(retired.root)

    with pytest.raises(WyrdError) as refused:
        Workflow.from_path(WORKFLOWS / "retired/workflow.yaml")
    assert refused.value.code == "WYRD_REGISTRY_404_CARD_NOT_FOUND"


@pytest.mark.usefixtures("newer_security_reviewer")
def test_applied_workflow_stays_pinned_to_its_registered_cards(
    applied: dict[str, CardRef], reader_key: str
) -> None:
    reader = Cards(WyrdClient(credential=reader_key))
    agents = {name: applied[name].to_dict() for name in PINNED_PROMPTS}

    stored = reader.get(applied["code-review"])

    assert isinstance(stored, RegisteredWorkflowCard)
    assert [step["action"]["target"] for step in stored.spec["steps"]] == list(agents.values())
    assert outbound(stored) == agents
    prompts = {agent: applied[prompt].to_dict() for agent, prompt in PINNED_PROMPTS.items()}
    pinned = {agent: stored_agent(reader, applied[agent]) for agent in PINNED_PROMPTS}
    assert {agent: prompt_ref(card) for agent, card in pinned.items()} == prompts
    assert {agent: outbound(card) for agent, card in pinned.items()} == {
        agent: {prompt["name"]: prompt} for agent, prompt in prompts.items()
    }


@pytest.mark.usefixtures("newer_security_reviewer")
@pytest.mark.parametrize("selector", ["identity", "uid"])
def test_loaded_workflow_runs_its_pinned_cards(
    applied: dict[str, CardRef], reader_key: str, selector: str
) -> None:
    reader = Cards(WyrdClient(credential=reader_key))
    workflow = (
        reader.workflow.load(space="workflow-loading", name="code-review", version="1.0.0")
        if selector == "identity"
        else reader.workflow.load(uid=str(applied["code-review"].uid))
    )

    run = workflow.run(CODE)

    assert (run.status, run.outputs) == ("succeeded", {"review": REGISTERED_REVIEW})
    assert run.steps["final_review"]["text"] == REGISTERED_REVIEW


def test_loading_a_bad_selector_is_refused(applied: dict[str, CardRef], reader_key: str) -> None:
    """An Agent's uid names no Workflow."""
    reader = Cards(WyrdClient(credential=reader_key))

    with pytest.raises(WyrdError) as refused:
        reader.workflow.load(uid=str(applied["security-reviewer"].uid))
    assert refused.value.code == "WYRD_REGISTRY_404_CARD_NOT_FOUND"
