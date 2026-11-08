"""Verifier orchestration, end to end, as the ``orchestration-desk`` Service.

``specialist-review``'s planner selects specialists by its decisions: true
runs one, false skips it before its workspace binds, and a ``null``,
missing, or unreadable decision fails it. The consolidator reads every
settled collection's manifest whatever its status, and the quoter, requiring
the security account by name, runs only when that account was published.
``specialists-only`` judging only an unselected specialist is inconclusive.
A submitted review whose generalist is held is cancelled and stays
cancelled once the held reply returns.

The session's local model provider scripts each Agent by its prompt marker
and turn, as the Rust journey's upstream does.
"""

from __future__ import annotations

import json
from typing import Any

import pytest
from wyrd import WyrdError, verify
from wyrd.cards import Cards
from wyrd.client import WyrdClient
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer

from .support import (
    HOLD_ARRIVED,
    HOLD_RELEASED,
    Receiver,
    client_of,
    declare,
    pull,
    read_text,
    tool_results,
)
from .test_local_development import configure_gateway
from .test_verifier_building_blocks import grants, requirement, step

pytestmark = pytest.mark.integration

WAIT_SECONDS = 90.0
"""How long a test waits for a held reply or a queued execution."""

MARKERS = ("GENERAL:", "SECURITY:", "STYLE:", "CONSOLIDATE:", "QUOTE:")
"""The prompt markers of the Agents a ``specialist-review`` question may call."""

CASES: list[tuple[str, list[str], set[str], list[tuple[str, list[str]]], str | None]] = [
    (
        "NEITHER",
        ["succeeded", "skipped", "skipped", "skipped"],
        {"GENERAL:", "CONSOLIDATE:"},
        [("skipped", []), ("skipped", [])],
        None,
    ),
    (
        "SECURITY-ONLY",
        ["succeeded", "succeeded", "skipped", "succeeded"],
        {"GENERAL:", "SECURITY:", "CONSOLIDATE:", "QUOTE:"},
        [("succeeded", ["account"]), ("skipped", [])],
        None,
    ),
    (
        "BOTH",
        ["succeeded", "succeeded", "failed", "succeeded"],
        {"GENERAL:", "SECURITY:", "STYLE:", "CONSOLIDATE:", "QUOTE:"},
        [("succeeded", ["account"]), ("failed", ["draft"])],
        None,
    ),
    (
        "UNDECIDED",
        ["succeeded", "failed", "skipped", "skipped"],
        {"GENERAL:", "CONSOLIDATE:"},
        [("failed", []), ("skipped", [])],
        "kind (null, bool)",
    ),
    (
        "FORGOTTEN",
        ["succeeded", "failed", "skipped", "skipped"],
        {"GENERAL:", "CONSOLIDATE:"},
        [("failed", []), ("skipped", [])],
        "security",
    ),
    (
        "UNPLANNED",
        ["failed", "failed", "failed", "skipped"],
        {"GENERAL:", "CONSOLIDATE:"},
        [("failed", []), ("failed", [])],
        None,
    ),
]
"""Each question beside the status of ``plan``, ``security``, ``style``, and ``quote``, the
markers whose Agents it called, the ``security`` and ``style`` collections' manifest status and
entry names, and the text the failed ``security`` step's reason carries, when it is checked."""


@pytest.fixture(scope="module")
def desk(
    wyrd_server: WyrdTestServer, cards: Cards, tmp_path_factory: pytest.TempPathFactory
) -> WyrdState:
    """``orchestration-desk``, registered, downloaded, and loaded as a key granted every tool."""
    configure_gateway(WyrdClient())
    declare(cards, "cards/verifier_orchestration/orchestration-desk.yaml")
    bundle = pull(cards, "orchestration-desk", tmp_path_factory.mktemp("orchestration-desk"))
    key = wyrd_server.scoped_api_key("orchestration", grants("all"))
    return WyrdState.from_path(bundle, WyrdClient(credential=key))


def asked(receiver: Receiver, marker: str, question: str) -> list[list[dict[str, Any]]]:
    """The messages of every request carrying both ``marker`` and ``question``, in arrival order."""
    return [
        messages
        for delivery in receiver.to("/v1/chat/completions")
        if marker in (text := json.dumps(messages := delivery.body.get("messages", [])))
        and question in text
    ]


def test_conditions_and_collections_preserve_missing_evidence(
    desk: WyrdState, receiver: Receiver, wyrd_server: WyrdTestServer
) -> None:
    for question, statuses, called, collections, security_reason in CASES:
        result = desk.run().observe.verify("specialist-review", question)
        assert [step(result, id)["status"] for id in ("plan", "security", "style", "quote")] == (
            statuses
        ), question
        assert step(result, "general")["status"] == "succeeded", question
        assert requirement(result, "consolidated")["outcome"] == "passed", question
        assert {marker for marker in MARKERS if asked(receiver, marker, question)} == called, (
            question
        )
        if security_reason is not None:
            assert security_reason in step(result, "security").get("reason", ""), question
        read = tool_results(asked(receiver, "CONSOLIDATE:", question)[-1])
        for collection, (status, names) in zip(("security", "style"), collections, strict=True):
            manifest = json.loads(read_text(read[f"c-{collection}"]))
            entries = [entry["name"] for entry in manifest["entries"]]
            assert (manifest["status"], entries) == (status, names), (question, collection)

    unjudged = desk.run().observe.verify("specialists-only", "NEITHER-ONLY")
    assert (
        step(unjudged, "security")["status"],
        requirement(unjudged, "secured")["outcome"],
        unjudged.verdict,
    ) == ("skipped", "not_applicable", "inconclusive")

    held = desk.run().observe.submit("specialist-review", "CANCEL-HELD")
    assert isinstance(held, verify.Execution)
    try:
        assert HOLD_ARRIVED.wait(WAIT_SECONDS), "the held generalist was never called"
        assert held.cancel() == {"execution_id": held.id, "status": "cancelled"}
    finally:
        HOLD_RELEASED.set()
    admin = verify.Execution.connect(held.id, client_of(wyrd_server))
    with pytest.raises(WyrdError) as settled:
        admin.wait(WAIT_SECONDS)
    assert settled.value.details["original_code"] == "cancelled"
    state = admin.status()
    assert (state["status"], state.get("error", {}).get("code"), "result" in state) == (
        "cancelled",
        "cancelled",
        False,
    )
    assert not asked(receiver, "CONSOLIDATE:", "CANCEL-HELD"), "cancelled work never resumes"
