"""Public-surface tests for the Python ``WyrdClient`` projection."""

from __future__ import annotations

from collections.abc import Callable

import pytest


def test_client_without_a_resolvable_credential_raises(monkeypatch: pytest.MonkeyPatch):
    from wyrd import WyrdClient, WyrdError

    for name in ("WYRD_ACCESS_TOKEN", "WYRD_WORKLOAD_TOKEN", "WYRD_TENANT", "WYRD_API_KEY"):
        monkeypatch.delenv(name, raising=False)
    monkeypatch.setenv("HOME", "/nonexistent-wyrd-home")

    with pytest.raises(WyrdError) as captured:
        WyrdClient()
    assert captured.value.code == "WYRD_CLIENT_401_NO_CREDENTIALS"


def test_on_behalf_of_rejects_an_unknown_audience():
    from wyrd import WyrdClient, WyrdError

    client = WyrdClient(server_url="http://127.0.0.1:9", credential="wyrd_test_actor")
    with pytest.raises(WyrdError) as captured:
        client.on_behalf_of("subject-token", audience="storage")  # ty: ignore[invalid-argument-type]
    assert captured.value.code == "WYRD_SPEC_400_VALIDATION"


def test_on_behalf_of_runs_the_exchange_in_rust():
    """An unreachable server surfaces the Rust transport error, not a Python one."""
    from wyrd import WyrdClient, WyrdError

    client = WyrdClient(server_url="http://127.0.0.1:9", credential="wyrd_test_actor")
    with pytest.raises(WyrdError) as captured:
        client.on_behalf_of("subject-token")
    assert captured.value.code != "WYRD_SPEC_400_VALIDATION"


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
    monkeypatch.setenv("WYRD_SERVER_URL", "http://wyrd.internal:8080")

    client = WyrdClient(credential="wyrd_test_actor")
    assert client.server_url == "http://wyrd.internal:8080"
    assert client.grpc_url == "http://wyrd.internal:50051"


def test_every_public_constructor_accepts_and_forwards_tenant():
    """``tenant`` reaches the shared Rust client from every public entry point.

    An explicit credential names its own tenant, so the forwarded selector is
    refused by the shared resolver; that refusal proves it arrived. ``py:typecheck``
    checks this module, so each call is also a static proof that the
    declarations accept ``tenant``. No call here reaches a server.
    """
    from wyrd import WyrdClient, WyrdError
    from wyrd.bifrost import AsyncBifrost, Bifrost, TableConfig
    from wyrd.cards import Cards
    from wyrd.gateway import Gateway
    from wyrd.operators import OperatorConnections
    from wyrd.state import WyrdState
    from wyrd.verification import Verification

    options = {"server_url": "http://127.0.0.1:9", "credential": "wyrd_test_actor"}

    def refused(build: Callable[[], object]) -> None:
        with pytest.raises(WyrdError) as captured:
            build()
        assert captured.value.code == "WYRD_CLIENT_400_CONFIG_INVALID"
        assert "already names its tenant" in str(captured.value)

    refused(lambda: WyrdClient(**options, tenant="acme"))
    refused(lambda: Cards(**options, tenant="acme"))
    refused(lambda: Verification(**options, tenant="acme"))
    refused(lambda: OperatorConnections(**options, tenant="acme"))
    refused(lambda: Gateway(**options, tenant="acme"))
    refused(
        lambda: TableConfig.describe(
            "ns.table", grpc_url="http://127.0.0.1:9", tenant="acme", **options
        )
    )

    client = WyrdClient(**options)
    for facade in (Bifrost, AsyncBifrost):
        with pytest.raises(WyrdError) as captured:
            facade(client=client, tenant="acme")
        assert captured.value.code == "WYRD_SPEC_400_VALIDATION"
        assert "tenant" in str(captured.value)

    def start(state: WyrdState) -> None:
        state.start_bifrost(tenant="acme")

    assert callable(start)
