"""Public-surface tests for the Python ``WyrdClient`` projection."""

from __future__ import annotations

import pytest


@pytest.mark.usefixtures("no_credentials")
def test_client_without_a_resolvable_credential_raises():
    from wyrd import WyrdClient, WyrdError

    with pytest.raises(WyrdError) as captured:
        WyrdClient()
    assert captured.value.code == "WYRD_CLIENT_401_NO_CREDENTIALS"


def test_on_behalf_of_rejects_an_unknown_audience():
    from wyrd import WyrdClient, WyrdError

    client = WyrdClient(server_url="http://127.0.0.1:9", credential="wyrd_test_actor")
    with pytest.raises(WyrdError) as captured:
        client.on_behalf_of("subject-token", audience="storage")  # ty: ignore[invalid-argument-type]
    assert captured.value.code == "WYRD_SPEC_400_VALIDATION"


def test_on_behalf_of_an_unreachable_server_raises_transport_down():
    from wyrd import WyrdClient, WyrdError

    client = WyrdClient(server_url="http://127.0.0.1:9", credential="wyrd_test_actor")
    with pytest.raises(WyrdError) as captured:
        client.on_behalf_of("subject-token")
    assert captured.value.code == "WYRD_CLIENT_503_TRANSPORT_DOWN"


def test_grpc_url_derives_from_server_url_unless_overridden(monkeypatch: pytest.MonkeyPatch):
    from wyrd import WyrdClient

    monkeypatch.delenv("WYRD_GRPC_URL", raising=False)
    monkeypatch.delenv("WYRD_SERVER_URL", raising=False)

    derived = WyrdClient(server_url="https://wyrd.example.com/", credential="wyrd_test_actor")
    assert derived.server_url == "https://wyrd.example.com"
    assert derived.grpc_url == "https://wyrd.example.com:50051"

    overridden = WyrdClient(
        server_url="https://wyrd.example.com",
        credential="wyrd_test_actor",
        grpc_url="https://grpc.example.com:443",
    )
    assert overridden.grpc_url == "https://grpc.example.com:443"


def test_wyrd_server_url_alone_sets_both_endpoints(monkeypatch: pytest.MonkeyPatch):
    from wyrd import WyrdClient

    monkeypatch.delenv("WYRD_GRPC_URL", raising=False)
    monkeypatch.setenv("WYRD_SERVER_URL", "https://wyrd.internal:8080")

    client = WyrdClient(credential="wyrd_test_actor")
    assert client.server_url == "https://wyrd.internal:8080"
    assert client.grpc_url == "https://wyrd.internal:50051"


def test_wyrd_tenant_is_refused_beside_a_self_naming_credential(monkeypatch):
    """``WYRD_TENANT`` is the only tenant selector, and both client doors read it.

    Constructors take no ``tenant``: an explicit key already names its tenant,
    so a configured selector beside it is refused by the shared resolver. The
    refusal from ``WyrdClient`` and ``Cards`` proves the selector reaches it.
    No call here reaches a server.
    """
    from wyrd import WyrdClient, WyrdError
    from wyrd.cards import Cards

    monkeypatch.setenv("WYRD_TENANT", "acme")
    for build in (WyrdClient, Cards):
        with pytest.raises(WyrdError) as captured:
            build(server_url="http://127.0.0.1:9", credential="wyrd_test_actor")
        assert captured.value.code == "WYRD_CLIENT_400_CONFIG_INVALID"
