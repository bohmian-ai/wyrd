"""A scheduled Drift binding that fails alerts its HTTP Operator through a named connection.

``latency-watch`` binds ``latency-shift`` on the ``daily`` Trigger; on failure
the ``alert-on-call`` Operator posts to the path ``/hooks/latency-shift`` on
the origin, and with the bearer token, of the ``on-call-hooks`` connection.
"""

import pytest
from wyrd import WyrdError
from wyrd.cards import Cards
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer

from .support import FIXTURES, ON_CALL_TOKEN, Receiver, binding_ids

pytestmark = pytest.mark.integration

DELIVERY_TIMEOUT_SECONDS = 90.0
"""How long the scheduler, Verifier, and Operator together may take to deliver."""


def test_failed_schedule_alerts_its_operator_on_the_connection_origin(
    latency_watch: WyrdState, wyrd_server: WyrdTestServer, cards: Cards, receiver: Receiver
) -> None:
    with latency_watch.run() as run:
        for row in range(120):
            run.observe.drift({"latency": 150.0 + row})
    latency_watch.flush()
    # Publish the drifted observations so the scheduled Verifier evaluates them.
    wyrd_server.flush_bifrost()
    [binding_id] = binding_ids(cards, latency_watch.root_ref)

    wyrd_server.make_binding_due(binding_id)

    alert = receiver.wait_for("/hooks/latency-shift", DELIVERY_TIMEOUT_SECONDS)
    assert alert.headers["authorization"] == f"Bearer {ON_CALL_TOKEN}"


def test_path_only_operator_without_connection_is_refused(cards: Cards) -> None:
    with pytest.raises(WyrdError) as raised:
        cards.register_from_path(FIXTURES / "invalid/operator-path-without-connection.yaml")
    assert raised.value.code == "WYRD_SPEC_400_INVALID_OPERATOR"
