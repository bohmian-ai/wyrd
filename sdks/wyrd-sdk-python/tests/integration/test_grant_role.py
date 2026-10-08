"""A tenant administrator grants a Service's principal a Role with ``wyrd auth grant-role``.

The Service's key acts with the Role at its next key exchange, so a client
built from the same key afterwards holds it.
"""

from __future__ import annotations

from collections.abc import Iterator

import pytest
from wyrd import WyrdError
from wyrd.bifrost import Bifrost
from wyrd.cards import CardRef, Cards
from wyrd.client import WyrdClient
from wyrd.testing import WyrdTestServer, cli

from .support import Count, client_of, register

pytestmark = pytest.mark.integration

TENANT_WIDE = "SELECT COUNT(*) AS n FROM vala.drift.observations"
"""A tenant-wide query over a built-in table, which ``wyrd_default`` does not permit and ``workload`` does."""


@pytest.fixture
def server(wyrd_server: WyrdTestServer) -> Iterator[WyrdTestServer]:
    """A server of this test's own, so no other test sees a Role granted here.

    It leaves the session endpoints in the environment untouched, so every
    call to it passes an explicit client.
    """
    with WyrdTestServer(mutate_env=False) as server:
        yield server


@pytest.fixture
def service(server: WyrdTestServer) -> CardRef:
    """The observed Service, registered with its Model through the SDK."""
    cards = Cards(client_of(server))
    register(cards, "cards/observe_a_run/observed-model.yaml")
    return register(cards, "cards/observe_a_run/observed-service.yaml")["observed-service"]


def service_key(server: WyrdTestServer, service: CardRef) -> str:
    """Issue ``service``'s own key on ``server``; it holds only the default Card Role."""
    return cli.issue_key(
        kind="Service",
        name=service.name,
        version="1.0.0",
        space="default",
        client=client_of(server),
    ).key


def test_default_service_key_needs_workload_for_tenant_wide_queries(
    server: WyrdTestServer, service: CardRef
) -> None:
    key = service_key(server, service)

    with pytest.raises(WyrdError) as refused:
        Bifrost(client=client_of(server, key)).sql(TENANT_WIDE, model=Count)
    assert refused.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"

    granted = cli.grant_role(
        kind="Service",
        name=service.name,
        version="1.0.0",
        space="default",
        role="workload",
        client=client_of(server),
    )
    assert granted.granted
    assert "workload" in granted.roles

    counted = Bifrost(client=client_of(server, key)).sql(TENANT_WIDE, model=Count)
    assert len(counted) == 1


def test_only_a_tenant_admin_can_grant_a_role(server: WyrdTestServer, service: CardRef) -> None:
    own: WyrdClient = client_of(server, service_key(server, service))

    with pytest.raises(WyrdError) as refused:
        cli.grant_role(
            kind="Service",
            name=service.name,
            version="1.0.0",
            space="default",
            role="workload",
            client=own,
        )
    assert refused.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"
