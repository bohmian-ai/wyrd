"""A Service's run emits observations for the Cards it composes; they read back by run id.

Each row is stamped with the Card and the run it belongs to.
"""

from __future__ import annotations

from pathlib import Path

import pytest
from pydantic import BaseModel
from wyrd import WyrdError
from wyrd.bifrost import Bifrost
from wyrd.client import WyrdClient
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer, cli

from .support import hydrated

pytestmark = pytest.mark.integration


class DriftRow(BaseModel):
    """One ``vala.drift.observations`` row: one per emitted feature."""

    series: str
    num_value: float | None
    str_value: str | None
    card_uid: str


def test_run_observations_read_back_by_run_id(
    observed_with_bifrost: WyrdState, wyrd_server: WyrdTestServer, bifrost: Bifrost
) -> None:
    run = observed_with_bifrost.run()

    run.for_card("model").observe.drift({"latency": 12.5, "tier": "gold"})
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
    assert rows == [
        DriftRow(
            series="latency",
            num_value=12.5,
            str_value="12.5",
            card_uid=model,
        ),
        DriftRow(
            series="tier",
            num_value=None,
            str_value="gold",
            card_uid=model,
        ),
    ]


def test_run_view_exposes_its_alias(observed: WyrdState) -> None:
    run = observed.run()
    model = run.for_card("model")
    agent_run = observed.run("agent")

    assert (run.alias, model.alias, agent_run.alias) == ("root", "model", "agent")
    assert model.run_id == run.run_id
    assert agent_run.run_id != run.run_id


def test_card_scoped_key_cannot_write_another_cards_observations(observed_bundle: Path) -> None:
    agent_key = cli.issue_key(kind="Agent", name="observed-agent", version="1.0.0", space="default")
    agent = hydrated(observed_bundle, WyrdClient(credential=agent_key.key))
    agent.start_bifrost()

    agent.run("model").observe.drift({"latency": 12.5})

    with pytest.raises(WyrdError) as refused:
        agent.flush()
    agent.shutdown()
    assert refused.value.code == "WYRD_VALA_403_BIFROST_CARD_SCOPE"


def test_unsealable_byte_budget_is_refused_at_connect(observed: WyrdState) -> None:
    with pytest.raises(WyrdError) as refused:
        observed.start_bifrost(client_byte_limit_bytes=1024)
    assert refused.value.code == "WYRD_CLIENT_400_CONFIG_INVALID"
