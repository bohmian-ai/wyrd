"""A queued execution's invocations and publications read back as stable, authorized pages.

A settled execution lists its one requirement attempt and no publications.
``verifier:read`` on the exact Verifier reads; ``cards:read`` alone or a grant
on another Verifier is refused, another tenant sees no such execution, and a
viewer reads but cannot cancel. Cancelling a settled execution keeps its
status, again and again.
"""

from __future__ import annotations

from collections.abc import Callable

import pytest
from wyrd import WyrdError, verify
from wyrd.cards import Cards
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer

from .support import client_of, declare

pytestmark = pytest.mark.integration

WAIT_SECONDS = 90.0
"""How long a test waits for a queued execution."""

DENIED = "WYRD_PERMISSION_403_DENIED_RBAC"

INVALID = "WYRD_SPEC_400_VALIDATION"


def verifier_read(cards: Cards, verifier: str) -> dict[str, object]:
    """The ``verifier:read`` grant on exactly the registered ``verifier``."""
    uid = declare(cards, f"cards/verify_in_real_time/{verifier}.yaml")[verifier].uid
    return {"resource": "verifier", "action": "read", "scope": {"verifier": str(uid)}}


def refused(call: Callable[[], object]) -> str:
    """The code of the ``WyrdError`` the zero-argument ``call`` raises."""
    with pytest.raises(WyrdError) as error:
        call()
    return error.value.code


def test_execution_pages_are_stable_and_tenant_scoped(
    assistant: WyrdState,
    wyrd_server: WyrdTestServer,
    cards: Cards,
    other_tenant: str,
    viewer_key: str,
) -> None:
    submitted = assistant.run("agent").observe.submit("answer-check", "yes")
    assert isinstance(submitted, verify.Execution)
    submitted.wait(WAIT_SECONDS)
    execution = verify.Execution.connect(submitted.id, client_of(wyrd_server))

    items, cursor = [], None
    while True:
        page = execution.invocations(limit=1, cursor=cursor)
        assert (page["execution_id"], page["status"]) == (submitted.id, "completed")
        items += page["items"]
        cursor = page["next_cursor"]
        if cursor is None:
            break
    assert [
        (item["producer"]["step_id"], item["producer"]["attempt"], item["state"], item["outcome"])
        for item in items
    ] == [("says-yes", 1, "requirement", "passed")]
    publications = execution.publications(limit=1)
    assert (publications["items"], publications["next_cursor"]) == ([], None)

    for limit in (0, 201):
        assert refused(lambda limit=limit: execution.invocations(limit=limit)) == INVALID
    assert refused(lambda: execution.publications(cursor="not-a-cursor")) == INVALID
    second: verify.Producer = {"step_id": "says-yes", "attempt": 2}
    assert (
        refused(lambda: execution.publication(second, "report"))
        == "WYRD_VERIFICATION_404_PUBLICATION_NOT_FOUND"
    )
    zero: verify.Producer = {"step_id": "says-yes", "attempt": 0}
    assert refused(lambda: execution.publication(zero, "report")) == INVALID

    def as_key(key: str) -> verify.Execution:
        return verify.Execution.connect(submitted.id, client_of(wyrd_server, key))

    cards_only = as_key(
        wyrd_server.scoped_api_key(
            "pages_cards", [{"resource": "cards", "action": "read", "scope": "all"}]
        )
    )
    assert refused(cards_only.invocations) == DENIED
    elsewhere = as_key(
        wyrd_server.scoped_api_key("pages_elsewhere", [verifier_read(cards, "answer-judged")])
    )
    assert refused(elsewhere.publications) == DENIED
    exact = as_key(
        wyrd_server.scoped_api_key("pages_exact", [verifier_read(cards, "answer-check")])
    )
    assert exact.invocations()["items"] == items
    foreign = wyrd_server.bootstrap_service_in_tenant(other_tenant, ["admin"], name="pages-foreign")
    assert refused(as_key(foreign).invocations) == "WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND"
    viewer = as_key(viewer_key)
    assert viewer.publications()["items"] == []
    assert refused(viewer.cancel) == DENIED

    for _ in range(2):
        assert execution.cancel() == {"execution_id": submitted.id, "status": "completed"}
