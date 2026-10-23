"""A tenant administrator discovers a Service account and grants it Roles.

A new Service account holds direct ``workload``: it queries but cannot author until
``editor`` is granted, and each change reaches the key's next token.
"""

from __future__ import annotations

from collections.abc import Iterator

import pytest
from wyrd import WyrdError
from wyrd.auth import Principals
from wyrd.bifrost import Bifrost
from wyrd.cards import Cards
from wyrd.testing import WyrdTestServer

from .support import FIXTURES, Count, client_of

pytestmark = pytest.mark.integration

TENANT_WIDE = "SELECT COUNT(*) AS n FROM vala.drift.observations"
"""A tenant-wide query over a built-in table, which ``workload`` permits."""

AUTHORED = FIXTURES / "cards/gateway_inference/ask-prompt.yaml"
"""A Card only an author can declare."""

DENIED = "WYRD_PERMISSION_403_DENIED_RBAC"


@pytest.fixture
def server(wyrd_server: WyrdTestServer) -> Iterator[WyrdTestServer]:
    """A server of this test's own, so no other test sees a Role granted here."""
    with WyrdTestServer(mutate_env=False) as server:
        yield server


ACCOUNT = "observed-service-runtime"


def issue(server: WyrdTestServer) -> tuple[str, str]:
    """Create the unrestricted ``ACCOUNT`` holding only ``workload``; return its principal id and key."""
    created = Principals(client_of(server)).create_service_principal(
        {"name": ACCOUNT, "roles": ["workload"]}
    )
    return created["principal_id"], created["credential"]


def author(server: WyrdTestServer, key: str) -> None:
    """Register ``AUTHORED`` through a fresh client, so it carries the key's next token."""
    Cards(client_of(server, key)).declare_from_path(AUTHORED)


def test_granted_editor_reaches_the_next_token_until_revoked(server: WyrdTestServer) -> None:
    principal, key = issue(server)
    principals = Principals(client_of(server))

    (found,) = principals.list(kind="service", name=ACCOUNT)["principals"]
    assert found["principal_id"] == principal
    assert found["workload_restriction"] == {"mode": "unrestricted"}
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


def test_only_a_tenant_admin_assigns_roles(server: WyrdTestServer) -> None:
    principal, key = issue(server)
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
    assert page["principals"][0]["workload_restriction"] is None
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
