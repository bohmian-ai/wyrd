"""Offline boundary checks for the public ``wyrd.operators`` handle."""

from __future__ import annotations

from collections.abc import Callable

import pytest
from wyrd import WyrdError
from wyrd.client import WyrdClient
from wyrd.operators import OperatorConnections

UNREACHABLE = "http://127.0.0.1:9"
SECRET = "xoxb-never-echoed"


@pytest.fixture
def connections() -> OperatorConnections:
    """A credential-complete handle whose server is never reached."""
    return OperatorConnections(WyrdClient(server_url=UNREACHABLE, credential="wyrd_unused"))


@pytest.mark.parametrize(
    ("field", "call"),
    [
        pytest.param("connection_id", lambda c: c.get("not-a-uuid"), id="get"),
        pytest.param("connection_id", lambda c: c.disable("not-a-uuid"), id="disable"),
        pytest.param(
            "request", lambda c: c.create({"provider": "slack", "bot_token": SECRET}), id="create"
        ),
    ],
)
def test_malformed_arguments_are_refused_without_echoing_secrets(
    connections: OperatorConnections, field: str, call: Callable[[OperatorConnections], object]
) -> None:
    """Non-UUID IDs and off-contract requests name the argument, never its value."""
    with pytest.raises(WyrdError) as caught:
        call(connections)

    assert caught.value.code == "WYRD_SPEC_400_VALIDATION"
    assert caught.value.details["field"] == field
    assert SECRET not in repr(caught.value)
