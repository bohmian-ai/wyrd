"""Every server-facing handle opens with ``connect()`` or on first use.

The ambient client resolves from the environment or saved configuration once,
exchanges eagerly under ``connect`` and on the first operation of a
constructed handle, and is retained with its renewable token cache.
"""

from __future__ import annotations

import asyncio
import time
from collections.abc import Callable, Iterator
from pathlib import Path
from typing import Any

import pytest
from pydantic import BaseModel
from wyrd import WyrdError
from wyrd.auth import Principals
from wyrd.bifrost import AsyncBifrost, Bifrost, TableConfig
from wyrd.cards import Cards
from wyrd.client import WyrdClient
from wyrd.gateway import Gateway
from wyrd.operators import OperatorConnections
from wyrd.testing import WyrdTestServer

pytestmark = pytest.mark.integration

ACCESS_TTL_SECONDS = 35
"""Access-token lifetime: the client renews within 30 seconds of expiry, so a
token is reused for five seconds and then renewed."""

FRESH_SECONDS = ACCESS_TTL_SECONDS - 30
"""How long a freshly exchanged token is reused before renewal."""

UNREACHABLE = "http://127.0.0.1:1"
"""A loopback port nothing listens on."""

DENIED = "WYRD_PERMISSION_403_DENIED_RBAC"

HANDLES: list[tuple[Any, Callable[[Any], object]]] = [
    (Bifrost, lambda bifrost: bifrost.list_tables()),
    (Cards, lambda cards: cards.prompt.list(space="default")),
    (Gateway, lambda gateway: gateway.deployments()),
    (OperatorConnections, lambda connections: connections.list()),
    (Principals, lambda principals: principals.list()),
]
"""Every server-facing handle, each with one harmless real call."""


class Event(BaseModel):
    id: int
    label: str


class Decision(BaseModel):
    act_principal_id: str


@pytest.fixture(scope="module")
def server() -> Iterator[WyrdTestServer]:
    """A server with short-lived access tokens, exported to the environment."""
    with WyrdTestServer(access_ttl_seconds=ACCESS_TTL_SECONDS) as server:
        yield server


def saved_configuration_only(
    server: WyrdTestServer, monkeypatch: pytest.MonkeyPatch, home: Path
) -> None:
    """Replace the environment with saved configuration alone."""
    home.mkdir(mode=0o700)
    files = {
        "config.toml": f'[client]\nhttp_url = "{server.base_url}"\ngrpc_url = "{server.grpc_url}"\n',
        "credentials.toml": f'[default]\napi_key = "{server.api_key}"\n',
    }
    for name, contents in files.items():
        (home / name).write_text(contents)
        (home / name).chmod(0o600)
    monkeypatch.setenv("WYRD_CONFIG_HOME", str(home))
    for name in ("WYRD_SERVER_URL", "WYRD_GRPC_URL", "WYRD_API_KEY"):
        monkeypatch.delenv(name, raising=False)


def forged_clients(server: WyrdTestServer) -> tuple[WyrdClient, WyrdClient]:
    """An API key the server refuses to exchange, and a bearer it refuses on use."""
    tenant = server.api_key.removeprefix("wyrd_sk_").split("_")[0]
    issued = WyrdClient(credential=server.api_key).access_token()
    signed, signature = issued.rsplit(".", 1)
    return (
        WyrdClient(credential=f"wyrd_sk_{tenant}_forged_secret"),
        WyrdClient(credential=f"{signed}.{'A' * len(signature)}"),
    )


def test_ambient_connect_opens_every_handle(
    server: WyrdTestServer, monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    async def eager_async_operation() -> object:
        return await (await AsyncBifrost.connect()).list_tables()

    for ambient in (
        lambda: None,
        lambda: saved_configuration_only(server, monkeypatch, tmp_path / "saved"),
    ):
        ambient()
        for handle, call in HANDLES:
            call(handle.connect())
        asyncio.run(eager_async_operation())
        bound = Bifrost.connect(
            table=TableConfig(Event, "vala.datasets.ambient_options"),
            client_byte_limit_bytes=8 * 1024 * 1024,
        )
        assert bound.register() in ("created", "already_exists")


def test_lazy_constructors_open_every_handle_on_first_operation(
    server: WyrdTestServer, monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    async def first_async_operation() -> object:
        return await AsyncBifrost().list_tables()

    for ambient in (
        lambda: None,
        lambda: saved_configuration_only(server, monkeypatch, tmp_path / "saved"),
    ):
        ambient()
        for handle, call in HANDLES:
            call(handle())
        asyncio.run(first_async_operation())


def test_local_state_survives_the_first_connection(
    server: WyrdTestServer, monkeypatch: pytest.MonkeyPatch
) -> None:
    bifrost = Bifrost(table=TableConfig(Event, "vala.datasets.lazy_first"))
    bifrost.use_table(TableConfig(Event, "vala.datasets.lazy_bound"))
    cards = Cards()
    prompts = cards.prompt

    assert bifrost.register() in ("created", "already_exists")
    assert bifrost.table is not None and bifrost.table.fqn == "vala.datasets.lazy_bound"
    assert "vala.datasets.lazy_bound" in [
        f"{entry['namespace']}.{entry['name']}" for entry in bifrost.list_tables()
    ]
    prompts.list(space="default")
    monkeypatch.setenv("WYRD_SERVER_URL", UNREACHABLE)
    monkeypatch.delenv("WYRD_API_KEY")
    cards.prompt.list(space="default")


def test_connect_and_lazy_first_call_share_the_no_credentials_error(
    server: WyrdTestServer, monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    monkeypatch.delenv("WYRD_API_KEY")
    monkeypatch.setenv("HOME", str(tmp_path))
    monkeypatch.setenv("WYRD_CONFIG_HOME", str(tmp_path))

    for handle, call in HANDLES:
        with pytest.raises(WyrdError) as eager:
            handle.connect()
        with pytest.raises(WyrdError) as lazy:
            call(handle())
        assert eager.value.code == lazy.value.code == "WYRD_CLIENT_401_NO_CREDENTIALS"


def test_credential_paths_preserve_exchange_and_bearer_behavior(server: WyrdTestServer) -> None:
    forged, bearer = forged_clients(server)

    for handle, call in HANDLES:
        with pytest.raises(WyrdError) as eager:
            handle.connect(client=forged)
        with pytest.raises(WyrdError) as lazy:
            call(handle(client=forged))
        assert eager.value.code == lazy.value.code == "WYRD_SPEC_400_VALIDATION"
        with pytest.raises(WyrdError) as refused:
            call(handle.connect(client=bearer))
        assert refused.value.status == 401


def test_delegated_connect_uses_the_subject_principal(server: WyrdTestServer) -> None:
    subject = WyrdClient(
        credential=server.scoped_api_key("instantiation_subject", ["bifrost_query:read"])
    )
    actor = WyrdClient(
        credential=server.scoped_api_key(
            "instantiation_actor", ["bifrost_query:read", "bifrost_table:read"]
        )
    )
    delegated = actor.on_behalf_of(subject.access_token())
    principals = Principals.connect()
    subject_id, actor_id = (
        principals.list(name=name)["principals"][0]["principal_id"]
        for name in ("instantiation_subject", "instantiation_actor")
    )

    for bifrost in (Bifrost.connect(client=delegated), Bifrost(client=delegated)):
        with pytest.raises(WyrdError) as refused:
            bifrost.list_tables()
        assert refused.value.code == DENIED
        bifrost.sql("SELECT COUNT(*) AS n FROM vala.system.audit_log")
    Bifrost.connect(client=actor).list_tables()

    server.flush_bifrost()
    decisions = Bifrost.connect(client=actor).sql(
        "SELECT act_principal_id FROM vala.system.audit_log "
        "WHERE operation = 'bifrost.query.read_decision' AND sub_principal_id = $1",
        [subject_id],
        model=Decision,
    )
    actors = [row.act_principal_id for row in decisions]
    assert actors == [actor_id, actor_id]


def test_handles_reuse_the_client_cache_and_renew(
    server: WyrdTestServer, monkeypatch: pytest.MonkeyPatch
) -> None:
    client = WyrdClient(credential=server.api_key)
    explicit = [(handle.connect(client=client), call) for handle, call in HANDLES]
    lazy = [(handle(), call) for handle, call in HANDLES]
    exchanged = client.access_token()

    for opened, call in [*explicit, *lazy]:
        call(opened)
    assert client.access_token() == exchanged

    monkeypatch.setenv("WYRD_SERVER_URL", UNREACHABLE)
    monkeypatch.setenv("WYRD_GRPC_URL", UNREACHABLE)
    monkeypatch.delenv("WYRD_API_KEY")
    time.sleep(FRESH_SECONDS + 1)

    for opened, call in [*explicit, *lazy]:
        call(opened)
    assert client.access_token() != exchanged


def test_bifrost_lists_tables_and_describes_on_demand(server: WyrdTestServer) -> None:
    own = Bifrost.connect(table=TableConfig(Event, "vala.datasets.listed"))
    own.register()
    foreign_key = server.bootstrap_service_in_tenant(
        server.seed_tenant("listing-tenant"), ["admin"], "listing-admin"
    )
    foreign = Bifrost.connect(
        table=TableConfig(Event, "vala.datasets.foreign_listed"),
        client=WyrdClient(credential=foreign_key),
    )
    foreign.register()

    listed = [f"{entry['namespace']}.{entry['name']}" for entry in own.list_tables()]

    assert "vala.datasets.listed" in listed
    assert "vala.drift.observations" in listed
    assert "vala.datasets.foreign_listed" not in listed
    unreadable = WyrdClient(
        credential=server.scoped_api_key("no_table_read", ["bifrost_query:read"])
    )
    with pytest.raises(WyrdError) as refused:
        Bifrost.connect(client=unreadable).list_tables()
    assert refused.value.code == DENIED
    described = own.describe_table("vala.datasets", "listed")
    assert [field["name"] for field in described["user_fields"]] == ["id", "label"]
