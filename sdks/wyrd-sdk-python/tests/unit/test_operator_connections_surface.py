"""Offline boundary checks for the public ``wyrd.operators`` handle."""

from __future__ import annotations

import pytest
from wyrd import WyrdError
from wyrd.operators import OperatorConnections

UNREACHABLE = "http://127.0.0.1:9"
SECRET = "xoxb-never-echoed"


def test_malformed_arguments_are_refused_without_echoing_secrets() -> None:
    """Non-UUID IDs and off-contract requests name the argument, never its value."""
    connections = OperatorConnections(server_url=UNREACHABLE, credential="wyrd_unused")
    cases = [
        ("connection_id", lambda: connections.get("not-a-uuid")),
        ("connection_id", lambda: connections.disable("not-a-uuid")),
        ("request", lambda: connections.create({"provider": "slack", "bot_token": SECRET})),
    ]
    for field, call in cases:
        with pytest.raises(WyrdError) as caught:
            call()
        assert caught.value.code == "WYRD_SPEC_400_VALIDATION"
        assert caught.value.details is not None
        assert caught.value.details["field"] == field
        assert SECRET not in repr(caught.value)
