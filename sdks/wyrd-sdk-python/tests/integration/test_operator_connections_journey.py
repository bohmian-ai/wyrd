"""Python Operator connection journey through the public ``wyrd`` package.

A tenant administrator creates, lists, reads, rotates, disables, and
re-enables a Slack connection, then reads PagerDuty and HTTP connections
whose provider config is flattened into each view. A writer without
``operators:*`` is refused, another tenant's administrator sees nothing, and
no response or error ever carries the secret.
"""

from __future__ import annotations

import pytest
from wyrd import WyrdError
from wyrd.operators import OperatorConnections
from wyrd.testing import WyrdTestServer

SECRETS = ("xoxb-python-sdk-secret", "xoxb-python-sdk-rotated")


def assert_redacted(value: object) -> None:
    """Fail when a journey secret appears in ``value``."""
    for secret in SECRETS:
        assert secret not in repr(value), f"secret leaked: {value!r}"


@pytest.mark.integration
def test_admin_manages_redacted_connections_with_permission_and_tenant_separation() -> None:
    """Admin CRUD is redacted; writers are refused and tenants are isolated."""
    with WyrdTestServer(mutate_env=False) as server:
        admin = OperatorConnections(
            server_url=server.base_url,
            credential=server.bootstrap_service(["admin"], name="py-oc-admin"),
        )
        created = admin.create(
            {
                "provider": "slack",
                "name": "py-slack",
                "workspace_id": "T0001",
                "bot_token": SECRETS[0],
            }
        )
        assert_redacted(created)
        assert created["status"] == "active"
        with pytest.raises(WyrdError) as conflict:
            admin.create(
                {
                    "provider": "slack",
                    "name": "py-slack",
                    "workspace_id": "T0001",
                    "bot_token": SECRETS[0],
                }
            )
        assert conflict.value.code == "WYRD_OPERATOR_409_CONNECTION_CONFLICT"
        assert_redacted(conflict.value)

        assert admin.list() == [created]
        connection_id = created["connection_id"]
        rotated = admin.update(connection_id, {"provider": "slack", "bot_token": SECRETS[1]})
        assert_redacted(rotated)
        assert rotated["connection_id"] == connection_id
        assert admin.disable(connection_id)["status"] == "disabled"
        assert admin.get(connection_id)["status"] == "disabled"
        enabled = admin.update(connection_id, {"provider": "slack", "status": "active"})
        assert enabled["status"] == "active"

        assert created["provider"] == "slack"
        assert created.get("workspace_id") == "T0001"
        assert "config" not in created

        pager = admin.create(
            {"provider": "pager_duty", "name": "py-pager", "integration_key": SECRETS[0]}
        )
        assert_redacted(pager)
        assert pager["provider"] == "pager_duty"
        assert "config" not in pager
        assert "integration_key" not in pager

        http = admin.create(
            {
                "provider": "http",
                "name": "py-http",
                "origin": "HTTPS://Hooks.Example.COM:443",
                "auth": {"scheme": "header", "name": "X-Api-Key", "value": SECRETS[0]},
            }
        )
        assert_redacted(http)
        assert http["provider"] == "http"
        assert http.get("origin") == "https://hooks.example.com"
        assert http.get("auth") == {"scheme": "header", "name": "X-Api-Key"}
        assert "config" not in http
        http_id = http["connection_id"]
        basic = admin.update(
            http_id,
            {
                "provider": "http",
                "auth": {"scheme": "basic", "username": "u", "password": SECRETS[1]},
            },
        )
        assert_redacted(basic)
        assert basic.get("auth") == {"scheme": "basic"}
        bearer = admin.update(
            http_id,
            {
                "provider": "http",
                "origin": "https://other.example.com",
                "auth": {"scheme": "bearer", "token": SECRETS[1]},
            },
        )
        assert bearer.get("origin") == "https://other.example.com"
        assert bearer.get("auth") == {"scheme": "bearer"}
        with pytest.raises(WyrdError) as mismatch:
            admin.update(http_id, {"provider": "slack", "bot_token": "x"})
        assert mismatch.value.code == "WYRD_OPERATOR_400_INVALID_CONNECTION"

        writer = OperatorConnections(
            server_url=server.base_url,
            credential=server.bootstrap_service(["writer"], name="py-oc-writer"),
        )
        with pytest.raises(WyrdError) as denied:
            writer.list()
        assert denied.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"

        other = server.seed_tenant("py-oc-other")
        foreign = OperatorConnections(
            server_url=server.base_url,
            credential=server.bootstrap_service_in_tenant(other, ["admin"], name="py-oc-foreign"),
        )
        assert foreign.list() == []
        with pytest.raises(WyrdError) as hidden:
            foreign.get(connection_id)
        assert hidden.value.code == "WYRD_OPERATOR_404_CONNECTION_NOT_FOUND"
