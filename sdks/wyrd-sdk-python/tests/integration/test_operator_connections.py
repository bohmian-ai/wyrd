"""A tenant administrator manages Operator connections whose secrets are never served back."""

import pytest
from wyrd import WyrdError
from wyrd.client import WyrdClient
from wyrd.operators import OperatorConnections
from wyrd.testing import WyrdTestServer

pytestmark = pytest.mark.integration

SECRETS = ("xoxb-python-sdk-secret", "xoxb-python-sdk-rotated")
"""Every secret the stories write; none may appear in a view or an error."""


def assert_redacted(value: object) -> None:
    """Fail when a story secret appears in ``value``."""
    for secret in SECRETS:
        assert secret not in repr(value), f"secret leaked: {value!r}"


@pytest.fixture(scope="module")
def connections(wyrd_server: WyrdTestServer) -> OperatorConnections:
    """The session administrator's connection handle, resolved from the environment."""
    return OperatorConnections()


@pytest.fixture(scope="module")
def slack(connections: OperatorConnections) -> str:
    """The id of the ``ops-slack`` connection."""
    created = connections.create(
        {"provider": "slack", "name": "ops-slack", "workspace_id": "T0001", "bot_token": SECRETS[0]}
    )
    return created["connection_id"]


def test_admin_manages_redacted_connections(connections: OperatorConnections, slack: str) -> None:
    rotated = connections.update(slack, {"provider": "slack", "bot_token": SECRETS[1]})
    disabled = connections.disable(slack)
    enabled = connections.update(slack, {"provider": "slack", "status": "active"})
    http = connections.create(
        {
            "provider": "http",
            "name": "ops-http",
            "origin": "HTTPS://Hooks.Example.COM:443",
            "auth": {"scheme": "header", "name": "X-Api-Key", "value": SECRETS[0]},
        }
    )
    assert_redacted([rotated, disabled, enabled, http, connections.list()])
    assert (disabled["status"], enabled["status"]) == ("disabled", "active")
    assert http.get("origin") == "https://hooks.example.com"
    assert http.get("auth") == {"scheme": "header", "name": "X-Api-Key"}


def test_writer_is_refused(wyrd_server: WyrdTestServer) -> None:
    writer = OperatorConnections(
        WyrdClient(credential=wyrd_server.scoped_api_key("card_writer", ["cards:read", "cards:write"]))
    )
    with pytest.raises(WyrdError) as raised:
        writer.list()
    assert raised.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"


def test_reader_cannot_disable_a_connection(
    wyrd_server: WyrdTestServer, connections: OperatorConnections, slack: str
) -> None:
    reader = OperatorConnections(
        WyrdClient(credential=wyrd_server.scoped_api_key("operator_reader", ["operators:read"]))
    )

    assert reader.get(slack) == connections.get(slack)
    with pytest.raises(WyrdError) as raised:
        reader.disable(slack)
    assert raised.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"


def test_other_tenant_sees_nothing(
    wyrd_server: WyrdTestServer, other_tenant: str, slack: str
) -> None:
    foreign = OperatorConnections(
        WyrdClient(
            credential=wyrd_server.bootstrap_service_in_tenant(
                other_tenant, ["admin"], name="other-admin"
            )
        )
    )
    assert foreign.list() == []
    with pytest.raises(WyrdError) as raised:
        foreign.get(slack)
    assert raised.value.code == "WYRD_OPERATOR_404_CONNECTION_NOT_FOUND"
