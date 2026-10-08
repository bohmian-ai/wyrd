"""A Service graph registers from checked-in YAML, reads back typed, and hydrates (AC-047, AC-049, AC-051)."""

from __future__ import annotations

from pathlib import Path

import pytest
from wyrd import WyrdError
from wyrd.cards import (
    CardRef,
    Cards,
    RegisteredAgentCard,
    RegisteredDataCard,
    RegisteredModelCard,
    RegisteredOperatorCard,
    RegisteredPromptCard,
    RegisteredServiceCard,
    RegisteredTriggerCard,
    RegisteredVerifierCard,
)
from wyrd.client import WyrdClient
from wyrd.testing import WyrdTestServer, cli

from .support import FIXTURES, binding_ids, download, hydrated, register

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
    assert state.agent("agent").prompt is not None


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
    bindings = binding_ids(cards, support_desk["support-desk"])

    again = cards.register_from_path(FIXTURES / SUPPORT_DESK)

    assert again.root == support_desk["support-desk"]
    assert binding_ids(cards, again.root) == bindings


def test_cards_get_returns_every_kind_typed(cards: Cards, support_desk: dict[str, CardRef]) -> None:
    baseline = register(cards, "cards/latency_baseline/latency-baseline.yaml")["latency-baseline"]

    data = cards.get(baseline)
    model = cards.get(support_desk["support-model"])
    prompt = cards.get(support_desk["support-prompt"])
    agent = cards.get(support_desk["support-agent"])
    verifier = cards.get(support_desk["no-refund-promise"])
    service = cards.get(support_desk["support-desk"])
    trigger = cards.get(support_desk["every-batch"])
    operator = cards.get(support_desk["notify-support"])

    # `get` returns a typed Card per kind; checking its class narrows it to
    # the kind-specific `spec`.
    assert isinstance(data, RegisteredDataCard)
    assert [column.name for column in data.spec.schema.columns] == ["latency", "tier"]
    assert isinstance(model, RegisteredModelCard)
    assert model.metadata.artifact_hash
    assert isinstance(prompt, RegisteredPromptCard)
    assert prompt.spec.model == "gpt-4o"
    assert isinstance(agent, RegisteredAgentCard)
    assert agent.spec.run_config is not None
    assert agent.spec.run_config.max_iterations == 2
    assert isinstance(verifier, RegisteredVerifierCard)
    assert verifier.spec.implementation.kind == "eval"
    assert isinstance(service, RegisteredServiceCard)
    assert service.spec.service_type == "agent"
    assert isinstance(trigger, RegisteredTriggerCard)
    assert trigger.spec.kind == "observations_ready"
    assert isinstance(operator, RegisteredOperatorCard)
    assert operator.spec.kind == "http"


def test_artifact_without_digest_registers(cards: Cards, support_desk: dict[str, CardRef]) -> None:
    """The fixture Model lists its artifact without a digest; the client computes it."""
    model = cards.get(support_desk["support-model"])

    assert model.metadata.artifact_hash


def test_wrong_artifact_digest_is_refused(cards: Cards) -> None:
    with pytest.raises(WyrdError) as refused:
        cards.register_from_path(FIXTURES / "invalid/wrong-artifact-digest/support-model.yaml")
    assert refused.value.code == "WYRD_REGISTRY_400_MANIFEST_HASH_MISMATCH"


def test_retired_card_kind_is_refused(cards: Cards) -> None:
    with pytest.raises(WyrdError) as refused:
        cards.register_from_path(FIXTURES / "invalid/retired-drift-kind.yaml")
    assert refused.value.code == "WYRD_LOADER_400_INVALID_ENVELOPE"


def test_reader_cannot_register_cards(reader_key: str) -> None:
    reader = Cards(WyrdClient(credential=reader_key))

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
