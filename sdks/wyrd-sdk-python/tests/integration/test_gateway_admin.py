"""An administrator configures the gateway: provider credentials, deployments, and capture.

Credentials are written with the in-process ``wyrd gateway credential``
commands, because ``Gateway`` deliberately reads them but never writes one.
"""

from __future__ import annotations

import pytest
from wyrd import WyrdError
from wyrd.client import WyrdClient
from wyrd.gateway import Gateway, ProviderDeployment
from wyrd.testing import WyrdTestServer, cli

pytestmark = pytest.mark.integration


def environment_credential(name: str) -> dict[str, object]:
    """A provider credential named ``name`` that the server resolves from its ``test-provider-key`` binding."""
    return {
        "name": name,
        "provider": "openai",
        "source": {"environment": {"binding": "test-provider-key"}},
    }


def deployment(name: str, credential: str) -> ProviderDeployment:
    """An ``openai`` deployment named ``name`` that authenticates with ``credential``."""
    return {
        "name": name,
        "model": {"provider": "openai", "model": "gpt-4o"},
        "adapter": "openai",
        "auth": {"bearer": {"credential": credential}},
        "capabilities": ["chat_completions"],
        "routing_weight": 1,
    }


@pytest.fixture(scope="module")
def gateway(wyrd_server: WyrdTestServer) -> Gateway:
    """The deployment administrator's Gateway handle, resolved from the environment."""
    return Gateway()


def test_cli_written_credential_reads_back_through_the_gateway(gateway: Gateway) -> None:
    written = cli.put_provider_credential(environment_credential("read-back-key"))

    assert (written["name"], written["source"], written["state"]) == (
        "read-back-key",
        {"environment": {"binding": "test-provider-key"}},
        "active",
    )
    assert gateway.credential("read-back-key") == written
    assert "read-back-key" in [credential["name"] for credential in gateway.credentials()]


def test_deployment_round_trips(gateway: Gateway) -> None:
    cli.put_provider_credential(environment_credential("round-trip-key"))
    primary = deployment("round-trip", "round-trip-key")

    assert gateway.put_deployment(primary) == primary
    assert gateway.deployment("round-trip") == primary
    assert primary in gateway.deployments()

    gateway.delete_deployment("round-trip")
    with pytest.raises(WyrdError) as deleted:
        gateway.deployment("round-trip")
    assert deleted.value.code == "WYRD_GATEWAY_404_RESOURCE_NOT_FOUND"


def test_deployment_without_capabilities_is_refused(gateway: Gateway) -> None:
    cli.put_provider_credential(environment_credential("no-capability-key"))

    with pytest.raises(WyrdError) as refused:
        gateway.put_deployment(
            {**deployment("no-capability", "no-capability-key"), "capabilities": []}
        )
    assert refused.value.code == "WYRD_GATEWAY_400_INVALID_CONFIGURATION"


def test_credential_in_use_cannot_be_deleted(gateway: Gateway) -> None:
    cli.put_provider_credential(environment_credential("in-use-key"))
    gateway.put_deployment(deployment("in-use", "in-use-key"))

    with pytest.raises(WyrdError) as conflict:
        cli.delete_provider_credential("in-use-key")
    assert conflict.value.code == "WYRD_GATEWAY_409_RESOURCE_CONFLICT"


def test_deleting_a_credential_twice_succeeds(gateway: Gateway) -> None:
    cli.put_provider_credential(environment_credential("deleted-key"))

    cli.delete_provider_credential("deleted-key")
    cli.delete_provider_credential("deleted-key")

    with pytest.raises(WyrdError) as missing:
        gateway.credential("deleted-key")
    assert missing.value.code == "WYRD_GATEWAY_404_RESOURCE_NOT_FOUND"


def test_capture_policy_round_trips(gateway: Gateway) -> None:
    assert gateway.capture_policy()["mode"] == "disabled"

    capture = gateway.put_capture_policy({"mode": "payload", "payload_fields": ["request"]})

    assert (capture["mode"], capture["payload_fields"]) == ("payload", ["request"])
    assert gateway.capture_policy() == capture


def test_gateway_reader_cannot_delete_a_deployment(
    wyrd_server: WyrdTestServer, gateway: Gateway
) -> None:
    cli.put_provider_credential(environment_credential("reader-key"))
    gateway.put_deployment(deployment("read-only", "reader-key"))
    reader = Gateway(
        WyrdClient(credential=wyrd_server.scoped_api_key("gateway_reader", ["gateway:read"]))
    )

    assert reader.deployment("read-only")["name"] == "read-only"
    with pytest.raises(WyrdError) as denied:
        reader.delete_deployment("read-only")
    assert denied.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"
