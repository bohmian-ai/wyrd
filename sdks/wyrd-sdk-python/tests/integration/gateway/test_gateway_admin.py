"""Integration journey for Python gateway administration against a real server.

The SDK reads and administers deployments and policies; every credential
mutation here goes over the public HTTP operation the CLI uses, because the
Python ``Gateway`` deliberately has no method for one.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

import pytest
from wyrd import WyrdError
from wyrd.gateway import Gateway

from .support import delete_credential, put_credential

if TYPE_CHECKING:
    from wyrd.testing import WyrdTestServer

# Operator binding every WyrdTestServer declares.
BINDING = "test-provider-key"


@pytest.mark.integration
def test_gateway_admin_journey(wyrd_server: WyrdTestServer) -> None:
    gateway = Gateway(server_url=wyrd_server.base_url, credential=wyrd_server.api_key)
    source = {"environment": {"binding": BINDING}}

    put = put_credential(
        wyrd_server, {"name": "py-openai", "provider": "openai", "source": source}
    ).json()
    assert put["name"] == "py-openai"
    assert put["state"] == "active"

    view = gateway.credential("py-openai")
    assert view["source"] == source
    assert view["revoked_at"] is None
    assert set(view) == {
        "name",
        "provider",
        "source",
        "state",
        "created_at",
        "updated_at",
        "rotated_at",
        "revoked_at",
    }

    deployment = {
        "name": "py-gpt",
        "model": {"provider": "openai", "model": "gpt-4o"},
        "adapter": "openai",
        "auth": {"bearer": {"credential": "py-openai"}},
        "capabilities": ["chat_completions"],
        "routing_weight": 1,
    }
    assert gateway.put_deployment(deployment)["name"] == "py-gpt"
    assert [d["name"] for d in gateway.deployments()] == ["py-gpt"]
    assert "py-openai" in [c["name"] for c in gateway.credentials()]

    assert delete_credential(wyrd_server, "py-openai").json()["code"] == (
        "WYRD_GATEWAY_409_RESOURCE_CONFLICT"
    )

    assert gateway.delete_deployment("py-gpt") is None
    assert delete_credential(wyrd_server, "py-openai").status_code == 204
    assert delete_credential(wyrd_server, "py-openai").status_code == 204
    with pytest.raises(WyrdError) as missing:
        gateway.credential("py-openai")
    assert missing.value.code == "WYRD_GATEWAY_404_RESOURCE_NOT_FOUND"

    policy = gateway.put_capture_policy({"mode": "payload", "payload_fields": ["request"]})
    assert policy["mode"] == "payload"
    assert gateway.capture_policy() == policy


@pytest.mark.integration
def test_gateway_admin_requires_gateway_permissions(wyrd_server: WyrdTestServer) -> None:
    key = wyrd_server.bootstrap_service(["reader"], name="py-gateway-reader")
    gateway = Gateway(server_url=wyrd_server.base_url, credential=key)

    assert (
        put_credential(
            wyrd_server,
            {
                "name": "py-denied",
                "provider": "openai",
                "source": {"environment": {"binding": BINDING}},
            },
            key=key,
        ).status_code
        == 403
    )

    with pytest.raises(WyrdError) as denied:
        gateway.credentials()
    assert denied.value.status == 403
