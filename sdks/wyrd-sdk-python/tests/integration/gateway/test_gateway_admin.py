"""An administrator configures the gateway: provider credentials, deployments, and capture.

Credentials are written with in-process ``wyrd gateway credential`` commands,
because the Python ``Gateway`` deliberately reads them but never writes one.
"""

from __future__ import annotations

from collections.abc import Iterator

import pytest
from wyrd import WyrdError, cli
from wyrd.gateway import Gateway
from wyrd.testing import WyrdTestServer

BINDING = "test-provider-key"
"""The operator binding every ``WyrdTestServer`` declares."""

SOURCE = {"environment": {"binding": BINDING}}

DEPLOYMENT = {
    "name": "py-gpt",
    "model": {"provider": "openai", "model": "gpt-4o"},
    "adapter": "openai",
    "auth": {"bearer": {"credential": "py-openai"}},
    "capabilities": ["chat_completions"],
    "routing_weight": 1,
}


@pytest.fixture
def credential(wyrd_server: WyrdTestServer) -> Iterator[str]:
    """The ``py-openai`` environment credential, deleted with any deployment after the test."""
    cli.put_provider_credential({"name": "py-openai", "provider": "openai", "source": SOURCE})
    yield "py-openai"
    gateway = Gateway()
    if "py-gpt" in [deployment["name"] for deployment in gateway.deployments()]:
        gateway.delete_deployment("py-gpt")
    cli.delete_provider_credential("py-openai")


@pytest.mark.integration
def test_written_credential_reads_back_without_its_secret(credential: str) -> None:
    view = Gateway().credential(credential)

    assert (view["name"], view["provider"], view["state"]) == (credential, "openai", "active")
    assert view["source"] == SOURCE
    assert view["revoked_at"] is None
    assert credential in [listed["name"] for listed in Gateway().credentials()]


@pytest.mark.integration
def test_deployment_is_listed_after_it_is_put(credential: str) -> None:
    gateway = Gateway()

    assert gateway.put_deployment(DEPLOYMENT)["name"] == "py-gpt"
    assert "py-gpt" in [deployment["name"] for deployment in gateway.deployments()]


@pytest.mark.integration
def test_credential_in_use_cannot_be_deleted(credential: str) -> None:
    Gateway().put_deployment(DEPLOYMENT)

    with pytest.raises(WyrdError) as conflict:
        cli.delete_provider_credential(credential)
    assert conflict.value.code == "WYRD_GATEWAY_409_RESOURCE_CONFLICT"


@pytest.mark.integration
def test_deleted_credential_is_gone(wyrd_server: WyrdTestServer) -> None:
    cli.put_provider_credential({"name": "py-gone", "provider": "openai", "source": SOURCE})
    cli.delete_provider_credential("py-gone")
    cli.delete_provider_credential("py-gone")

    with pytest.raises(WyrdError) as missing:
        Gateway().credential("py-gone")
    assert missing.value.code == "WYRD_GATEWAY_404_RESOURCE_NOT_FOUND"


@pytest.mark.integration
def test_capture_policy_reads_back(wyrd_server: WyrdTestServer) -> None:
    gateway = Gateway()
    policy = gateway.put_capture_policy({"mode": "payload", "payload_fields": ["request"]})

    assert policy["mode"] == "payload"
    assert gateway.capture_policy() == policy


@pytest.mark.integration
def test_reader_cannot_administer_the_gateway(reader_key: str) -> None:
    with pytest.raises(WyrdError) as denied:
        Gateway(credential=reader_key).credentials()
    assert denied.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"
