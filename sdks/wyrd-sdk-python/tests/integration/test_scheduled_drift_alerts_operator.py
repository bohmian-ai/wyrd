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

from .conftest import FIXTURES, ON_CALL_TOKEN, Receiver

DELIVERY_TIMEOUT_SECONDS = 90.0
"""How long the scheduler, Verifier, and Operator together may take to deliver."""


@pytest.mark.integration
def test_failed_schedule_alerts_its_operator_on_the_connection_origin(
    latency_watch: WyrdState, wyrd_server: WyrdTestServer, cards: Cards, receiver: Receiver
) -> None:
    with latency_watch.run() as run:
        for row in range(120):
            run.observe.drift({"latency": 150.0 + row})
    latency_watch.flush()
    wyrd_server.flush_bifrost()
    service = cards.get(latency_watch.card_ref("root"))
    # Status and its verification block stay empty until the server derives them.
    status = service["status"]
    assert status is not None
    verification = status["verification"]
    assert verification is not None
    [binding_id] = verification["binding_ids"]

    wyrd_server.make_binding_due(binding_id)

    alert = receiver.wait_for("/hooks/latency-shift", DELIVERY_TIMEOUT_SECONDS)
    assert alert.headers["authorization"] == f"Bearer {ON_CALL_TOKEN}"


@pytest.mark.integration
def test_path_only_operator_without_connection_is_refused(cards: Cards) -> None:
    with pytest.raises(WyrdError) as raised:
        cards.register_from_path(str(FIXTURES / "invalid/operator-path-without-connection.yaml"))
    assert raised.value.code == "WYRD_SPEC_400_INVALID_OPERATOR"
