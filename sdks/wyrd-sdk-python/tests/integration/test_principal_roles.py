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
    from wyrd.auth import (
        AccessSummary,
        CreateRoleBindingRequest,
        FamilyRestriction,
        Operation,
        PrincipalKind,
        PrincipalSummary,
    )

from .support import FIXTURES, Count, client_of, declare, hydrated, runtime_account

pytestmark = pytest.mark.integration

TENANT_WIDE = "SELECT COUNT(*) AS n FROM bifrost.drift.observations"
"""A tenant-wide query over a built-in table, which ``workload`` permits."""

READS: list[Operation] = [{"resource": "cards", "action": "read"}]
"""The one operation the custom roles in these tests carry."""

AUTHORED = FIXTURES / "cards/gateway_inference/ask-prompt.yaml"
"""A Card only an author can declare."""

DENIED = "WYRD_PERMISSION_403_DENIED_RBAC"


@pytest.fixture
def server(wyrd_server: WyrdTestServer) -> Iterator[WyrdTestServer]:
    """A server of this test's own, so no other test sees a role bound here."""
    with WyrdTestServer(mutate_env=False) as server:
        yield server


ACCOUNT = "observed-service-runtime"


def issue(server: WyrdTestServer) -> tuple[str, str]:
    """Create the unrestricted ``ACCOUNT`` holding only ``workload``; return its principal id and key."""
    created = Principals(client_of(server)).create_service_principal(
        {"name": ACCOUNT, "assignments": [{"role": "workload", "scope": "all"}]}
    )
    return created["principal_id"], created["credential"]


def bind_all(principal: str, kind: PrincipalKind, role: str) -> CreateRoleBindingRequest:
    """A request binding ``role`` tenant-wide to one account."""
    return {"subject": {"principal": {"id": principal, "kind": kind}}, "role": role, "scope": "all"}


def grants(access: AccessSummary) -> list[tuple[str, str]]:
    """Each grant's role and source kind, sorted: the summary promises no grant order."""
    return sorted((grant["role"], grant["source"]["kind"]) for grant in access["grants"])


def author(server: WyrdTestServer, key: str) -> None:
    """Register ``AUTHORED`` through a fresh client, so it carries the key's next token."""
    Cards(client_of(server, key)).declare_from_path(AUTHORED)


def test_bound_editor_reaches_the_next_request_until_unbound(server: WyrdTestServer) -> None:
    principal, key = issue(server)
    principals = Principals(client_of(server))

    (found,) = principals.list(kind="service", name=ACCOUNT)["principals"]
    assert found["principal_id"] == principal
    assert found["workload_restriction"] == {"mode": "unrestricted"}
    assert grants(principals.access(principal)) == [("workload", "direct")]
    assert len(Bifrost(client=client_of(server, key)).sql(TENANT_WIDE, model=Count)) == 1
    with pytest.raises(WyrdError) as refused:
        author(server, key)
    assert refused.value.code == DENIED

    granted = principals.bind(bind_all(principal, "service", "editor"))
    assert granted["scope"] == "all"
    assert principals.bind(bind_all(principal, "service", "editor"))["id"] == granted["id"]
    assert grants(principals.access(principal)) == [("editor", "direct"), ("workload", "direct")]
    author(server, key)

    principals.unbind(granted["id"])
    assert grants(principals.access(principal)) == [("workload", "direct")]
    with pytest.raises(WyrdError) as refused:
        author(server, key)
    assert refused.value.code == DENIED


def test_only_a_tenant_admin_assigns_roles(server: WyrdTestServer) -> None:
    principal, key = issue(server)
    (workload,) = Principals(client_of(server)).access(principal)["grants"]
    own = Principals(client_of(server, key))

    with pytest.raises(WyrdError) as granted:
        own.bind(bind_all(principal, "service", "editor"))
    with pytest.raises(WyrdError) as revoked:
        own.unbind(workload["binding_id"])

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

    granted = principals.bind(bind_all(user, "user", "viewer"))
    assert grants(principals.access(user)) == [("viewer", "direct"), ("viewer", "provider")]
    principals.unbind(granted["id"])
    assert grants(principals.access(user)) == [("viewer", "provider")]


def test_a_role_filter_pages_this_tenants_holders_with_their_counts(
    server: WyrdTestServer,
) -> None:
    principals = Principals(client_of(server))
    principals.put_role("ledger_reader", {"operations": READS})
    users = {server.bootstrap_user(["ledger_reader"], name) for name in ("ann", "bo")}
    service, _ = issue(server)
    principals.bind(bind_all(service, "service", "ledger_reader"))
    # Another tenant's holder of a same-named role is never counted or listed.
    other = server.seed_tenant("ledger-other")
    foreign = Principals(
        client_of(server, server.bootstrap_service_in_tenant(other, ["admin"], "other-admin"))
    )
    foreign.put_role("ledger_reader", {"operations": READS})
    foreign.create_service_principal(
        {"name": "stranger", "assignments": [{"role": "ledger_reader", "scope": "all"}]}
    )

    seen: list[PrincipalSummary] = []
    after = None
    while True:
        page = principals.list(role="ledger_reader", limit=1, after=after)
        assert page["counts"] == {"active": 3, "suspended": 0}
        seen += page["principals"]
        if page["next"] is None:
            break
        after = page["next"]

    by_id = {found["principal_id"]: found for found in seen}
    assert set(by_id) == {*users, service}
    assert by_id[service]["authentication"] is None
    assert by_id[service]["roles"] == [
        {"role": "ledger_reader", "scope": "all"},
        {"role": "workload", "scope": "all"},
    ]
    for user in users:
        assert by_id[user]["authentication"] == "sso"
        assert by_id[user]["display_name"] is None
        assert by_id[user]["roles"] == [{"role": "ledger_reader", "scope": "all"}]


def test_a_role_nobody_holds_lists_no_one(server: WyrdTestServer) -> None:
    nobody = Principals(client_of(server)).list(role="no_such_role")
    assert nobody["principals"] == []
    assert nobody["counts"] == {"active": 0, "suspended": 0}


def test_roles_carry_their_description_and_member_count(server: WyrdTestServer) -> None:
    principals = Principals(client_of(server))
    created = principals.put_role(
        "ledger_reader", {"operations": READS, "description": "Reads ledger Cards"}
    )
    assert created["description"] == "Reads ledger Cards"
    assert created["member_count"] == 0

    server.bootstrap_user(["ledger_reader"], "ann")
    roles = {role["name"]: role for role in principals.roles()["roles"]}
    assert roles["ledger_reader"]["member_count"] == 1
    assert roles["viewer"]["description"] == "Read Cards, executions, artifacts and tables"

    # The role is replaced whole: a request without a description clears it.
    cleared = principals.put_role("ledger_reader", {"operations": READS})
    assert cleared["description"] is None
    assert cleared["member_count"] == 1


def test_a_built_in_role_refuses_a_new_description(server: WyrdTestServer) -> None:
    principals = Principals(client_of(server))
    with pytest.raises(WyrdError) as refused:
        principals.put_role("viewer", {"operations": READS, "description": "Mine now"})
    assert refused.value.code == "WYRD_SPEC_400_VALIDATION"


def test_users_manage_personal_keys(server: WyrdTestServer) -> None:
    ceiling = ["personal_credentials:write", "cards:read", "personal_credentials:read"]
    user, key = server.bootstrap_user_key(["editor"], "keeper", ceiling)
    own = Principals(client_of(server, key))

    me = own.me()
    assert me["principal_id"] == user
    assert me["principal_kind"] == "user"
    assert me["username"] == "keeper"
    assert me["authentication"] == "password"
    assert me["display_name"] is None
    assert me["email"] is None
    assert me["permissions"] == sorted(ceiling)

    page = own.list_credentials_page(user, limit=10)
    assert "next_cursor" not in page
    [listed] = page["credentials"]
    assert listed["name"] == "keeper"
    assert listed["permission_ceiling"] == sorted(ceiling)
    assert listed["expires_at"] is not None
    with pytest.raises(WyrdError) as unpaged:
        own.list_credentials(user)
    assert unpaged.value.code == "WYRD_SPEC_400_VALIDATION"
    with pytest.raises(WyrdError) as issued:
        own.issue_credential(user)
    assert issued.value.code == "WYRD_SPEC_400_VALIDATION"

    expected = {*server.seed_user_keys(user, 105), listed["id"]}
    seen: list[str] = []
    cursor = None
    while True:
        page = own.list_credentials_page(user, limit=100, cursor=cursor)
        assert len(page["credentials"]) <= 100
        seen += [credential["id"] for credential in page["credentials"]]
        if "next_cursor" not in page:
            break
        cursor = page["next_cursor"]
    assert len(seen) == len(set(seen))
    assert set(seen) == expected

    admin = Principals(client_of(server))
    assert {
        credential["id"] for credential in admin.list_credentials(user)["credentials"]
    } == expected
    with pytest.raises(WyrdError) as service:
        admin.me()
    assert service.value.code == DENIED

    own.revoke_credential(user, listed["id"])
    with pytest.raises(WyrdError) as revoked:
        Principals(client_of(server, key)).me()
    assert revoked.value.code.startswith("WYRD_AUTH_401_")


FAMILY = "observed-service"
"""The Service family whose two checked-in releases share one runtime account."""


class Attributed(BaseModel):
    """One drift row's run, subject Card, and account pair."""

    run_id: str
    subject_card_uid: str
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
        "SELECT run_id, subject_card_uid, sub_principal_id, act_principal_id "
        f"FROM bifrost.drift.observations WHERE run_id IN ({placeholders}) ORDER BY run_id",
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
        Attributed(
            run_id=run, subject_card_uid=model, sub_principal_id=account, act_principal_id=account
        )
        for run in sorted(run_ids)
    ]


def test_a_family_has_one_runtime_account_until_released(server: WyrdTestServer) -> None:
    principals = Principals(client_of(server))
    family: FamilyRestriction = {"mode": "family", "kind": "Service", "name": FAMILY}
    first, _ = runtime_account(FAMILY, client=client_of(server))
    second = principals.create_service_principal(
        {"name": "second", "assignments": [{"role": "workload", "scope": "all"}]}
    )

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
        {"name": "delegating-subject", "assignments": [{"role": "workload", "scope": "all"}]}
    )
    subject = client_of(server, owner["credential"]).access_token()

    with pytest.raises(WyrdError) as rootless:
        client_of(server, key).on_behalf_of(subject)
    assert rootless.value.status == 400

    delegated = client_of(server, key, root).on_behalf_of(subject)
    [row] = attributed(server, [emit(hydrated(bundle, delegated))])
    assert (row.sub_principal_id, row.act_principal_id) == (owner["principal_id"], account)


def test_a_family_bound_viewer_reads_only_that_family(server: WyrdTestServer) -> None:
    cards = Cards(client_of(server))
    payments = declare(cards, "cards/repository_declaration/code-review.yaml")["code-review"]
    other = declare(cards, "cards/repository_declaration/unrelated.yaml")["unrelated-check"]
    created = Principals(client_of(server)).create_service_principal(
        {
            "name": "payments-reader",
            "assignments": [
                {
                    "role": "viewer",
                    "scope": {"card": {"family": {"kind": "Verifier", "name": "code-review"}}},
                }
            ],
        }
    )
    own = Cards(client_of(server, created["credential"]))

    assert [ref.name for ref in own.list(kind="Verifier").refs] == ["code-review"]
    assert own.get(payments).card_ref.name == "code-review"
    with pytest.raises(WyrdError) as hidden:
        own.get(other)
    assert hidden.value.code == "WYRD_REGISTRY_404_CARD_NOT_FOUND"


def test_a_release_grants_nothing_and_its_account_holds_only_its_grants(
    server: WyrdTestServer, tmp_path: Path
) -> None:
    root = next(iter(releases(server, tmp_path)))
    principals = Principals(client_of(server))
    assert principals.list(kind="service", name=f"{FAMILY}-runtime")["principals"] == []

    account, key = runtime_account(FAMILY, client=client_of(server))
    access = principals.access(account)
    assert grants(access) == [("workload", "direct")]
    (summary,) = principals.list(kind="service", name=f"{FAMILY}-runtime")["principals"]
    assert summary["workload_restriction"] == {"mode": "family", "kind": "Service", "name": FAMILY}

    with pytest.raises(WyrdError) as ungranted:
        Cards(client_of(server, key, root)).declare_from_path(AUTHORED)
    assert ungranted.value.code == DENIED
    for refused_root, code in (
        ("not-a-card-uid", "WYRD_SPEC_400_VALIDATION"),
        ("01900000-0000-7000-8000-000000000000", "WYRD_AUTH_401_WORKLOAD_UID_REFUSED"),
    ):
        with pytest.raises(WyrdError) as refused:
            client_of(server, key, refused_root).access_token()
        assert refused.value.code == code, refused_root
