"""A Service graph registers from checked-in YAML, reads back typed, and hydrates (AC-047, AC-049, AC-051)."""

from __future__ import annotations

from pathlib import Path

import pytest
from wyrd import WyrdError, cli
from wyrd.cards import CardRef, Cards
from wyrd.state import WyrdState

from .conftest import FIXTURES, StandInModel, download, register

SUPPORT_MODEL = "cards/register_and_hydrate/support-model.yaml"
SUPPORT_DESK = "cards/register_and_hydrate/support-desk.yaml"


@pytest.fixture(scope="module")
def support_desk(cards: Cards) -> dict[str, CardRef]:
    """The ``support-desk`` graph, registered after its artifact-bearing Model; every Card by name."""
    return {**register(cards, SUPPORT_MODEL), **register(cards, SUPPORT_DESK)}


@pytest.mark.integration
def test_service_graph_registers_and_hydrates(
    support_desk: dict[str, CardRef], tmp_path: Path
) -> None:
    bundle = download("support-desk", tmp_path)

    state = WyrdState.from_path(bundle, interfaces={"model": StandInModel()})

    assert state.service.card_ref == support_desk["support-desk"]
    assert state.card_ref("model") == support_desk["support-model"]
    assert state.card_ref("agent") == support_desk["support-agent"]
    assert state.agent("agent").prompt is not None


@pytest.mark.integration
def test_registering_the_graph_again_is_idempotent(
    cards: Cards, support_desk: dict[str, CardRef]
) -> None:
    receipt = cards.register_from_path(str(FIXTURES / SUPPORT_DESK))

    assert receipt.root == support_desk["support-desk"]
    assert {outcome.outcome for outcome in receipt.outcomes} == {"idempotent_noop"}


@pytest.mark.integration
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

    # `get` returns a union of typed envelopes; checking `kind` narrows each
    # one to its kind-specific `spec`.
    assert data["kind"] == "Data"
    assert [column["name"] for column in data["spec"]["schema"]["columns"]] == ["latency", "tier"]
    assert model["kind"] == "Model"
    assert model["metadata"]["artifact_hash"]
    assert prompt["kind"] == "Prompt"
    assert prompt["spec"]["model"] == "gpt-4o"
    assert agent["kind"] == "Agent"
    assert agent["spec"]["run_config"]["max_iterations"] == 2
    assert verifier["kind"] == "Verifier"
    assert verifier["spec"]["implementation"]["kind"] == "eval"
    assert service["kind"] == "Service"
    assert service["spec"]["service_type"] == "agent"
    assert trigger["kind"] == "Trigger"
    assert trigger["spec"]["kind"] == "observations_ready"
    assert operator["kind"] == "Operator"
    assert operator["spec"]["kind"] == "http"


@pytest.mark.integration
def test_artifact_without_digest_registers(cards: Cards, support_desk: dict[str, CardRef]) -> None:
    """The fixture Model lists its artifact without a digest; the client computes it."""
    model = cards.get(support_desk["support-model"])

    assert model["metadata"]["artifact_hash"]


@pytest.mark.integration
def test_wrong_artifact_digest_is_refused(cards: Cards) -> None:
    with pytest.raises(WyrdError) as refused:
        cards.register_from_path(str(FIXTURES / "invalid/wrong-artifact-digest/support-model.yaml"))
    assert refused.value.code == "WYRD_REGISTRY_400_MANIFEST_HASH_MISMATCH"


@pytest.mark.integration
def test_retired_card_kind_is_refused(cards: Cards) -> None:
    with pytest.raises(WyrdError) as refused:
        cards.register_from_path(str(FIXTURES / "invalid/retired-drift-kind.yaml"))
    assert refused.value.code == "WYRD_LOADER_400_INVALID_ENVELOPE"


@pytest.mark.integration
def test_reader_cannot_register_cards(reader_key: str, monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("WYRD_API_KEY", reader_key)
    with pytest.raises(WyrdError) as refused:
        Cards().register_from_path(str(FIXTURES / SUPPORT_DESK))
    assert refused.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"


@pytest.mark.integration
def test_cli_apply_and_get_round_trip_the_graph(
    support_desk: dict[str, CardRef], tmp_path: Path
) -> None:
    applied = cli.apply(FIXTURES / SUPPORT_DESK)
    fetched = cli.get(output_dir=tmp_path, kind="Service", uid=applied["root"]["uid"])

    assert applied["root"]["uid"] == support_desk["support-desk"].uid
    assert fetched["root"]["uid"] == applied["root"]["uid"]
    assert WyrdState.from_path(tmp_path, interfaces={"model": StandInModel()}).aliases


@pytest.mark.integration
def test_refused_cli_command_raises_its_catalog_code() -> None:
    with pytest.raises(WyrdError) as refused:
        cli.apply(FIXTURES / "invalid/retired-drift-kind.yaml")
    assert refused.value.code == "WYRD_LOADER_400_INVALID_ENVELOPE"
