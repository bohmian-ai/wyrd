"""A tenant administrator discovers a Service's principal and grants it Roles.

A new Service key holds direct ``workload``: it queries but cannot author until
``editor`` is granted, and each change reaches the key's next token.
"""

from __future__ import annotations

from collections.abc import Iterator

import pytest
from wyrd import WyrdError
from wyrd.bifrost import Bifrost
from wyrd.cards import CardRef, Cards
from wyrd.principals import Principals
from wyrd.testing import WyrdTestServer, cli

from .support import FIXTURES, Count, client_of, register

pytestmark = pytest.mark.integration

TENANT_WIDE = "SELECT COUNT(*) AS n FROM vala.drift.observations"
"""A tenant-wide query over a built-in table, which ``workload`` permits."""

AUTHORED = FIXTURES / "cards/gateway_inference/ask-prompt.yaml"
"""A Card only an author can register."""

DENIED = "WYRD_PERMISSION_403_DENIED_RBAC"


@pytest.fixture
def server(wyrd_server: WyrdTestServer) -> Iterator[WyrdTestServer]:
    """A server of this test's own, so no other test sees a Role granted here."""
    with WyrdTestServer(mutate_env=False) as server:
        yield server


@pytest.fixture
def service(server: WyrdTestServer) -> CardRef:
    """The observed Service, registered with its Model through the SDK."""
    cards = Cards(client_of(server))
    register(cards, "cards/observe_a_run/observed-model.yaml")
    return register(cards, "cards/observe_a_run/observed-service.yaml")["observed-service"]


def issue(server: WyrdTestServer, service: CardRef) -> tuple[str, str]:
    """Issue ``service``'s own key; return its principal id and key."""
    issued = cli.issue_key(
        kind="Service",
        name=service.name,
        version="1.0.0",
        space="default",
        client=client_of(server),
    )
    return issued.principal_id, issued.key


def author(server: WyrdTestServer, key: str) -> None:
    """Register ``AUTHORED`` through a fresh client, so it carries the key's next token."""
    Cards(client_of(server, key)).register_from_path(AUTHORED)


def test_granted_editor_reaches_the_next_token_until_revoked(
    server: WyrdTestServer, service: CardRef
) -> None:
    principal, key = issue(server, service)
    principals = Principals(client_of(server))

    assert principals.roles(principal)["roles"] == [{"role": "workload", "source": "direct"}]
    assert len(Bifrost(client=client_of(server, key)).sql(TENANT_WIDE, model=Count)) == 1
    with pytest.raises(WyrdError) as refused:
        author(server, key)
    assert refused.value.code == DENIED

    granted = principals.grant_role(principal, "editor")
    assert granted["changed"]
    assert granted["roles"] == [
        {"role": "editor", "source": "direct"},
        {"role": "workload", "source": "direct"},
    ]
    assert not principals.grant_role(principal, "editor")["changed"]
    author(server, key)

    revoked = principals.revoke_role(principal, "editor")
    assert revoked["changed"]
    with pytest.raises(WyrdError) as refused:
        author(server, key)
    assert refused.value.code == DENIED


def test_only_a_tenant_admin_assigns_roles(server: WyrdTestServer, service: CardRef) -> None:
    principal, key = issue(server, service)
    own = Principals(client_of(server, key))

    with pytest.raises(WyrdError) as granted:
        own.grant_role(principal, "editor")
    with pytest.raises(WyrdError) as revoked:
        own.revoke_role(principal, "workload")

    assert granted.value.code == DENIED
    assert revoked.value.code == DENIED


def test_direct_and_idp_user_assignments_coexist(server: WyrdTestServer) -> None:
    user = server.bootstrap_user(["viewer"], "pr-person")
    principals = Principals(client_of(server))

    page = principals.list(email="pr-person@test.wyrd")
    assert [(found["principal_id"], found["kind"]) for found in page["principals"]] == [
        (user, "user")
    ]
    assert page["next"] is None

    granted = principals.grant_role(user, "viewer")
    assert granted["changed"]
    assert granted["roles"] == [
        {"role": "viewer", "source": "direct"},
        {"role": "viewer", "source": "idp"},
    ]
    revoked = principals.revoke_role(user, "viewer")
    assert revoked["changed"]
    assert revoked["roles"] == [{"role": "viewer", "source": "idp"}]
