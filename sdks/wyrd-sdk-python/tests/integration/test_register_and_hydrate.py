"""A Service graph registers from checked-in YAML, reads back typed, and hydrates (AC-047, AC-049, AC-051)."""

from __future__ import annotations

import json
from pathlib import Path

import pytest
from wyrd import WyrdError
from wyrd.cards import AgentCard, CardKind, CardRef, Cards
from wyrd.client import WyrdClient
from wyrd.prompt import PromptCard
from wyrd.state import CardEnvelope, WyrdState
from wyrd.testing import WyrdTestServer, cli

from .support import FIXTURES, download, hydrated, register

pytestmark = pytest.mark.integration

SUPPORT_MODEL = "cards/register_and_hydrate/support-model.yaml"
SUPPORT_DESK = "cards/register_and_hydrate/support-desk.yaml"


@pytest.fixture(scope="module")
def support_desk(cards: Cards) -> dict[str, CardRef]:
    """The ``support-desk`` graph, registered after its artifact-bearing Model; every Card by name."""
    return {**register(cards, SUPPORT_MODEL), **register(cards, SUPPORT_DESK)}


def test_service_graph_registers_and_hydrates(
    cards: Cards, support_desk: dict[str, CardRef], tmp_path: Path
) -> None:
    bundle = download(cards, "support-desk", tmp_path)

    state = hydrated(bundle)

    assert state.root_ref == support_desk["support-desk"]
    assert state.card_ref("model") == support_desk["support-model"]
    assert state.card_ref("agent") == support_desk["support-agent"]
    assert state.agent("agent").prompt().ref == support_desk["support-prompt"]


def test_latest_version_resolves_to_the_registered_card(
    cards: Cards, support_desk: dict[str, CardRef]
) -> None:
    assert (
        cards.resolve_latest("Service", "default", "support-desk") == support_desk["support-desk"]
    )
    with pytest.raises(WyrdError) as refused:
        cards.resolve_latest("Service", "default", "never-registered")
    assert refused.value.code == "WYRD_REGISTRY_404_CARD_NOT_FOUND"


def test_registering_the_graph_again_is_idempotent(
    cards: Cards, support_desk: dict[str, CardRef]
) -> None:
    again = cards.register_from_path(FIXTURES / SUPPORT_DESK)

    assert again.root == support_desk["support-desk"]
    status = cards.get(again.root).status
    # The desk's only binding has no schedule Trigger, so none is projected.
    assert status is not None and status.get("verification") is None


def test_cards_get_returns_every_kind_as_envelope(
    cards: Cards, support_desk: dict[str, CardRef]
) -> None:
    baseline = register(cards, "cards/latency_baseline/latency-baseline.yaml")["latency-baseline"]
    refs = [
        (CardKind.Data, baseline),
        (CardKind.Model, support_desk["support-model"]),
        (CardKind.Prompt, support_desk["support-prompt"]),
        (CardKind.Agent, support_desk["support-agent"]),
        (CardKind.Verifier, support_desk["no-refund-promise"]),
        (CardKind.Service, support_desk["support-desk"]),
        (CardKind.Trigger, support_desk["every-batch"]),
        (CardKind.Operator, support_desk["notify-support"]),
    ]

    envelopes = [cards.get(ref) for _, ref in refs]

    # Every kind reads back as the native envelope with its exact identity,
    # mapping-valued content, and no graph aliases (registry reads hydrate nothing).
    for (kind, ref), envelope in zip(refs, envelopes, strict=True):
        assert isinstance(envelope, CardEnvelope)
        assert envelope.card_ref == ref
        assert envelope.kind == kind
        assert envelope.aliases == ()
        assert envelope.metadata["name"] == ref.name
        assert isinstance(envelope.relationships, dict)
        assert envelope.status is None or isinstance(envelope.status, dict)
        dumped = envelope.model_dump()
        assert dumped["spec"] == envelope.spec
        assert dumped["metadata"] == envelope.metadata
        assert json.loads(envelope.model_dump_json()) == dumped

    data, model, prompt, agent, verifier, service, trigger, operator = envelopes
    assert [column["name"] for column in data.spec["schema"]["columns"]] == ["latency", "tier"]
    assert model.metadata["artifact_hash"]
    assert prompt.spec["model"] == "gpt-4o"
    assert agent.spec["run_config"]["max_iterations"] == 2
    assert verifier.spec["requirements"][0]["check"]["kind"] == "assert"
    assert service.spec["service_type"] == "agent"
    assert trigger.spec["kind"] == "schedule"
    assert operator.spec["kind"] == "http"


def test_artifact_without_digest_registers(cards: Cards, support_desk: dict[str, CardRef]) -> None:
    """The fixture Model lists its artifact without a digest; the client computes it."""
    model = cards.get(support_desk["support-model"])

    assert model.metadata["artifact_hash"]


def test_wrong_artifact_digest_is_refused(cards: Cards) -> None:
    with pytest.raises(WyrdError) as refused:
        cards.register_from_path(FIXTURES / "invalid/wrong-artifact-digest/support-model.yaml")
    assert refused.value.code == "WYRD_REGISTRY_400_MANIFEST_HASH_MISMATCH"


def test_retired_card_kind_is_refused(cards: Cards) -> None:
    with pytest.raises(WyrdError) as refused:
        cards.register_from_path(FIXTURES / "invalid/retired-drift-kind.yaml")
    assert refused.value.code == "WYRD_LOADER_400_INVALID_ENVELOPE"


def test_viewer_cannot_register_cards(viewer_key: str) -> None:
    reader = Cards(WyrdClient(credential=viewer_key))

    with pytest.raises(WyrdError) as refused:
        reader.register_from_path(FIXTURES / SUPPORT_DESK)
    assert refused.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"


def test_cli_apply_and_get_round_trip_the_graph(
    wyrd_server: WyrdTestServer, support_desk: dict[str, CardRef], tmp_path: Path
) -> None:
    client = WyrdClient(credential=wyrd_server.api_key)
    applied = cli.apply(FIXTURES / SUPPORT_DESK, client=client)

    summary = cli.get(
        output_dir=tmp_path,
        kind="Service",
        space="default",
        name="support-desk",
        version="1.0.0",
        client=client,
    )

    assert applied.root == support_desk["support-desk"]
    assert (summary.mode, summary.root) == ("complete", support_desk["support-desk"])
    assert hydrated(tmp_path).service.card_ref == support_desk["support-desk"]


def test_refused_cli_command_raises_its_catalog_code(tmp_path: Path) -> None:
    with pytest.raises(WyrdError) as refused:
        cli.get(
            output_dir=tmp_path,
            kind="Service",
            space="default",
            name="no-such-service",
            version="1.0.0",
        )
    assert refused.value.code == "WYRD_REGISTRY_404_CARD_NOT_FOUND"


def test_hydrated_agent_prompt_card_navigation(
    cards: Cards, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Inline, path and CardRef Agents navigate offline to their registered Prompt Cards."""
    register(cards, "cards/agent_prompt/desk-prompt.yaml")
    desk = register(cards, "cards/agent_prompt/prompt-desk.yaml")
    bundle = download(cards, "prompt-desk", tmp_path)
    monkeypatch.setenv("WYRD_SERVER_URL", "http://127.0.0.1:1")

    state = WyrdState.from_path(bundle)

    explicit = state["my_prompt"]
    assert isinstance(explicit, PromptCard)
    assert explicit.ref == state.card_ref("my_prompt") == desk["desk-prompt"]
    for alias, prompt in (
        ("inline_agent", "inline-agent-prompt"),
        ("path_agent", "desk-prompt"),
        ("ref_agent", "desk-prompt"),
    ):
        agent = state[alias]
        assert isinstance(agent, AgentCard)
        assert agent.prompt().ref == agent.prompt_ref.card_ref == desk[prompt]
        assert state.agent(alias).prompt().ref == desk[prompt]
    assert state.agent("path_agent").prompt().prompt.model == explicit.prompt.model == "gpt-4o"
    with pytest.raises(WyrdError) as unknown:
        state.get("missing")
    assert unknown.value.code == "WYRD_SDK_404_UNKNOWN_ALIAS"
    with pytest.raises(WyrdError) as unknown_item:
        state["missing"]
    assert unknown_item.value.code == "WYRD_SDK_404_UNKNOWN_ALIAS"
    with pytest.raises(WyrdError) as mismatch:
        state.agent("my_prompt")
    assert mismatch.value.code == "WYRD_SDK_400_CARD_KIND_MISMATCH"


PORTABLE_SPEC = {
    "provider": "anthropic",
    "model": "claude-sonnet-4-5",
    "system": "You answer for ${team}.",
    "messages": [
        {
            "role": "user",
            "content": [
                {"type": "text", "text": "${question}"},
                {"type": "image", "source": "${media:photo}"},
                {
                    "type": "image",
                    "source": {"type": "base64", "mime_type": "image/png", "data": "iVBORw0KGgo="},
                },
                {"type": "document", "source": "${media:__wyrd_media_0}"},
            ],
        }
    ],
    "settings": {"temperature": 0.2, "max_output_tokens": 256},
    "response_format": {
        "type": "object",
        "properties": {"answer": {"type": "string"}},
        "required": ["answer"],
    },
}
NATIVE_SPEC = {
    "provider": "openai",
    "model": "gpt-4o",
    "format": "chat_completion",
    "body": {
        "messages": [{"role": "user", "content": "Summarize {{ticket}}."}],
        "seed": 123,
        "future_knob": {"enabled": True},
    },
}


def test_prompt_forms_round_trip_as_authored(cards: Cards, tmp_path: Path) -> None:
    """Portable and native Prompts, standalone and inline, read and hydrate as authored with one hash."""
    registered = register(cards, "cards/register_and_hydrate/authored-prompts.yaml")
    state = WyrdState.from_path(download(cards, "authored-prompts", tmp_path))
    answer = PORTABLE_SPEC["response_format"]

    for name, prompt, authored, variables, schema in (
        ("portable-prompt", state.prompt("portable"), PORTABLE_SPEC, ["question", "team"], answer),
        ("native-prompt", state.prompt("native"), NATIVE_SPEC, ["ticket"], None),
        (
            "inline-prompt-agent-prompt",
            state.agent("agent").prompt(),
            PORTABLE_SPEC,
            ["question", "team"],
            answer,
        ),
        (
            "inline-native-prompt-agent-prompt",
            state.agent("native-agent").prompt(),
            NATIVE_SPEC,
            ["ticket"],
            None,
        ),
    ):
        read = cards.get(registered[name])
        envelope = state.card(next(a for a in state.aliases if state.card_ref(a) == read.card_ref))
        assert prompt.ref == read.card_ref == registered[name]
        assert read.spec == envelope.spec == prompt.metadata.to_spec() == authored
        assert read.metadata["spec_hash"] == envelope.metadata["spec_hash"]
        assert sorted(prompt.parameters) == sorted(prompt.prompt.variables) == variables
        assert prompt.prompt.response_schema == schema
        media = ["__wyrd_media_0", "photo"] if authored is PORTABLE_SPEC else []
        assert sorted(prompt.prompt.media_variables) == media
