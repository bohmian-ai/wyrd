"""A Service's run submits samples for the Cards it composes; they read back by run id.

Each row is stamped with the Card and the run it belongs to.
"""

from __future__ import annotations

from pathlib import Path

import pytest
from pydantic import BaseModel
from wyrd import WyrdError, verify
from wyrd.bifrost import Bifrost
from wyrd.cards import Cards
from wyrd.client import WyrdClient
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer

from .support import (
    BASELINE_TIMEOUT_SECONDS,
    binding_ids,
    declare,
    hydrated,
    pull,
    scheduled_result,
    service_key,
    stale_bundle,
)

pytestmark = pytest.mark.integration


class DriftRow(BaseModel):
    """One ``vala.drift.observations`` row: one per submitted feature."""

    series: str
    num_value: float | None
    str_value: str | None
    card_uid: str


def test_run_observations_read_back_by_run_id(
    observed_with_bifrost: WyrdState, wyrd_server: WyrdTestServer, bifrost: Bifrost
) -> None:
    run = observed_with_bifrost.run()

    submitted = run.for_card("model").observe.submit("observed-drift", {"latency": 12.5})
    assert isinstance(submitted, verify.Recorded)
    observed_with_bifrost.flush()
    # Publish the flushed observations so the query reads them.
    wyrd_server.flush_bifrost()

    rows = bifrost.sql(
        "SELECT series, num_value, str_value, card_uid "
        "FROM vala.drift.observations WHERE run_id = $1 ORDER BY series",
        [run.run_id],
        model=DriftRow,
    )
    model = str(observed_with_bifrost.card_ref("model").uid)
    assert rows == [DriftRow(series="latency", num_value=12.5, str_value="12.5", card_uid=model)]


def test_custom_drift_judges_the_submitted_window_on_schedule(
    observed_with_bifrost: WyrdState, wyrd_server: WyrdTestServer, cards: Cards
) -> None:
    observe = observed_with_bifrost.run("model").observe
    assert isinstance(observe.submit("observed-drift", {"latency": 30.0}), verify.Recorded)
    observed_with_bifrost.flush()
    [binding_id] = binding_ids(cards, observed_with_bifrost.root_ref)

    result = scheduled_result(wyrd_server, binding_id, BASELINE_TIMEOUT_SECONDS)

    assert result.verdict == "failed"
    assert result.subject == observed_with_bifrost.card_ref("model")
    [requirement] = result.requirements
    features = requirement["detail"]["drift"]["features"].values()
    assert any((feature["score"] or 0.0) > 5.0 for feature in features), requirement


def test_run_view_exposes_its_alias(observed: WyrdState) -> None:
    run = observed.run()
    model = run.for_card("model")
    agent_run = observed.run("agent")

    assert (run.alias, model.alias, agent_run.alias) == ("root", "model", "agent")
    assert model.run_id == run.run_id
    assert agent_run.run_id != run.run_id


def test_stale_bundle_cannot_write_outside_its_service_graph(
    observed_bundle: Path, cards: Cards, tmp_path: Path
) -> None:
    stale = hydrated(stale_bundle(observed_bundle, cards, tmp_path), WyrdClient())
    stale.start_bifrost()

    stale.run("model").observe.submit("observed-drift", {"latency": 12.5})

    with pytest.raises(WyrdError) as refused:
        stale.flush()
    stale.shutdown()
    assert refused.value.code == "WYRD_VALA_403_BIFROST_CARD_SCOPE"


def test_unsealable_byte_budget_is_refused_at_connect(observed: WyrdState) -> None:
    with pytest.raises(WyrdError) as refused:
        observed.start_bifrost(client_byte_limit_bytes=1024)
    assert refused.value.code == "WYRD_CLIENT_400_CONFIG_INVALID"


def test_configured_service_context_reaches_both_verification_subjects(
    wyrd_server: WyrdTestServer, cards: Cards, tmp_path: Path
) -> None:
    declare(cards, "cards/observe_a_run/configured-service.yaml")
    bundle = pull(cards, "configured-service", tmp_path)
    key = service_key("configured-service")
    for environment in ("prod", "staging"):
        state = WyrdState.from_path(bundle, WyrdClient(credential=key))
        state.configure(deployment_environment=environment)
        run = state.run()
        agent = run.for_card("agent")

        result = agent.observe.verify("agent-check", "yes")
        assert result.subject == state.card_ref("agent")
        assert result.deployment_environment == environment
        execution = agent.observe.submit("agent-check", "yes")
        assert isinstance(execution, verify.Execution)
        queued = execution.wait(60.0)
        assert queued.subject == state.card_ref("agent")
        assert queued.deployment_environment == environment

        if environment == "prod":
            root = run.observe.verify("service-check", "yes")
            assert root.subject == state.root_ref
            assert root.deployment_environment == "prod"
        else:
            with pytest.raises(WyrdError) as refused:
                run.observe.verify("service-check", "yes")
            assert refused.value.code == "WYRD_SPEC_400_VALIDATION"
            assert refused.value.details["field"] == "deployment_environment"
        with pytest.raises(WyrdError) as frozen:
            state.configure()
        assert frozen.value.code == "WYRD_SDK_409_STATE_CONFIG_FROZEN"
