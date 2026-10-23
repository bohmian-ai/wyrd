"""A tenant administrator discovers a Service account and grants it Roles.

A new Service account holds direct ``workload``: it queries but cannot author until
``editor`` is granted, and each change reaches the key's next token. One stable
account runs every release of its Service family: each state runs as its own
exact root, a foreign root is refused, and a delegate keeps its own root.
"""

from __future__ import annotations

from collections.abc import Iterator
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from typing import TYPE_CHECKING

import httpx
import pytest
from pydantic import BaseModel
from wyrd import WyrdError, verify
from wyrd.auth import Principals
from wyrd.bifrost import Bifrost
from wyrd.cards import CardRef, Cards
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer

if TYPE_CHECKING:
    from wyrd.auth import FamilyRestriction

from .support import FIXTURES, Count, client_of, declare, hydrated, runtime_account

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


FAMILY = "observed-service"
"""The Service family whose two checked-in releases share one runtime account."""


class Attributed(BaseModel):
    """One drift row's run, subject Card, and account pair."""

    run_id: str
    card_uid: str
    sub_principal_id: str
    act_principal_id: str


def releases(server: WyrdTestServer, destination: Path) -> dict[str, Path]:
    """Register both ``FAMILY`` releases as the administrator and download each, by root UID."""
    cards = Cards(client_of(server))
    declare(cards, "cards/observe_a_run/observed-model.yaml")
    roots = [
        declare(cards, f"cards/observe_a_run/{path}")[FAMILY]
        for path in ("observed-service.yaml", "observed-service-2.yaml")
    ]
    return {
        str(root.uid): cards.pull(
            CardRef("Service", FAMILY, root.version), destination / root.version
        ).destination
        for root in roots
    }


def emit(state: WyrdState) -> str:
    """Submit one model observation in a new run of ``state`` and flush it; return the run id."""
    state.start_bifrost()
    try:
        run = state.run()
        submitted = run.for_card("model").observe.submit("observed-drift", {"latency": 12.5})
        assert isinstance(submitted, verify.Recorded)
        state.flush()
    finally:
        state.shutdown()
    return run.run_id


def attributed(server: WyrdTestServer, run_ids: list[str]) -> list[Attributed]:
    """The published drift rows of ``run_ids``, read as the administrator, ordered by run."""
    server.flush_bifrost()
    placeholders = ", ".join(f"${index}" for index in range(1, len(run_ids) + 1))
    return Bifrost(client=client_of(server)).sql(
        "SELECT run_id, card_uid, sub_principal_id, act_principal_id "
        f"FROM vala.drift.observations WHERE run_id IN ({placeholders}) ORDER BY run_id",
        run_ids,
        model=Attributed,
    )


def test_stable_accounts_preserve_execution_identity(
    server: WyrdTestServer, tmp_path: Path
) -> None:
    bundles = releases(server, tmp_path)
    account, key = runtime_account(FAMILY, client=client_of(server))
    states = [hydrated(bundle, client_of(server, key)) for bundle in bundles.values()]

    assert [str(state.root_ref.uid) for state in states] == list(bundles)
    with ThreadPoolExecutor(len(states)) as pool:
        run_ids = list(pool.map(emit, states))

    model = str(states[0].card_ref("model").uid)
    assert attributed(server, run_ids) == [
        Attributed(run_id=run, card_uid=model, sub_principal_id=account, act_principal_id=account)
        for run in sorted(run_ids)
    ]


def test_a_family_has_one_runtime_account_until_released(server: WyrdTestServer) -> None:
    principals = Principals(client_of(server))
    family: FamilyRestriction = {"mode": "family", "kind": "Service", "name": FAMILY}
    first, _ = runtime_account(FAMILY, client=client_of(server))
    second = principals.create_service_principal({"name": "second", "roles": ["workload"]})

    (found,) = principals.list(kind="service", name=f"{FAMILY}-runtime")["principals"]
    assert found["workload_restriction"] == family
    with pytest.raises(WyrdError) as conflict:
        principals.set_workload_restriction(second["principal_id"], family)
    assert conflict.value.code == "WYRD_AUTH_409_WORKLOAD_BINDING_CONFLICT"

    released = principals.set_workload_restriction(first, {"mode": "unrestricted"})
    assert released["workload_restriction"] == {"mode": "unrestricted"}
    claimed = principals.set_workload_restriction(second["principal_id"], family)
    assert claimed["workload_restriction"] == family

    principals.revoke_principal(
        second["principal_id"], {"principal_kind": "service", "reason": "rotated out"}
    )
    with pytest.raises(WyrdError) as revoked:
        Bifrost(client=client_of(server, second["credential"])).sql(TENANT_WIDE, model=Count)
    assert revoked.value.code == "WYRD_AUTH_401_API_KEY_INVALID"


def test_another_familys_account_cannot_run_as_the_root(
    server: WyrdTestServer, tmp_path: Path
) -> None:
    root = next(iter(releases(server, tmp_path)))
    _, foreign = runtime_account("another-service", client=client_of(server))

    with pytest.raises(WyrdError) as foreign_root:
        Bifrost(client=client_of(server, foreign, root)).sql(TENANT_WIDE, model=Count)
    assert foreign_root.value.code == "WYRD_AUTH_401_WORKLOAD_UID_REFUSED"
    selected = httpx.get(
        f"{server.base_url}/v1/cards/by-uid/Service/{root}",
        headers={
            "x-wyrd-access-token": f"Bearer {client_of(server, foreign).access_token()}",
            "x-wyrd-workload-uid": root,
        },
        timeout=30,
    )
    assert (selected.status_code, selected.json()["code"]) == (
        401,
        "WYRD_AUTH_401_WORKLOAD_UID_REFUSED",
    )


def test_a_delegate_acts_for_its_subject_from_its_own_root(
    server: WyrdTestServer, tmp_path: Path
) -> None:
    root, bundle = next(iter(releases(server, tmp_path).items()))
    account, key = runtime_account(FAMILY, client=client_of(server))
    owner = Principals(client_of(server)).create_service_principal(
        {"name": "delegating-subject", "roles": ["workload"]}
    )
    subject = client_of(server, owner["credential"]).access_token()

    with pytest.raises(WyrdError) as rootless:
        client_of(server, key).on_behalf_of(subject)
    assert rootless.value.status == 400

    delegated = client_of(server, key, root).on_behalf_of(subject)
    [row] = attributed(server, [emit(hydrated(bundle, delegated))])
    assert (row.sub_principal_id, row.act_principal_id) == (owner["principal_id"], account)
