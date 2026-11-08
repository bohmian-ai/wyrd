"""Verifier orchestration, end to end, as the ``orchestration-desk`` Service.

``specialist-review``'s planner selects specialists by its decisions: true
runs one, false skips it before its workspace binds, and a ``null``,
missing, or unreadable decision fails it. The consolidator reads every
settled collection's manifest whatever its status, and the quoter, requiring
the security account by name, runs only when that account was published.
``specialists-only`` judging only an unselected specialist is inconclusive.
A submitted review whose generalist is held is cancelled and stays
cancelled once the held reply returns. ``speculative-review``'s comparison
admits, discards, or blocks its extra review, and ``document-pipeline``
fans an uploaded document's sections out, repeats each, and refines a summary.

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
    FIXTURES,
    HOLD_ARRIVED,
    HOLD_RELEASED,
    Receiver,
    client_of,
    declare,
    prompt_of,
    pull,
    read_text,
    tool_results,
)
from .test_hosted_agent_tools import publications
from .test_local_development import configure_gateway
from .test_verifier_building_blocks import grants, requirement, step

pytestmark = pytest.mark.integration

WAIT_SECONDS = 90.0
"""How long a test waits for a held reply or a queued execution."""

MARKERS = ("GENERAL:", "SECURITY:", "STYLE:", "CONSOLIDATE:", "QUOTE:")
"""The prompt markers of the Agents a ``specialist-review`` question may call."""

CASES: list[
    tuple[str, list[str], set[str], list[tuple[str, list[str]]], tuple[str, str] | None]
] = [
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
        ("security", "kind (null, bool)"),
    ),
    (
        "FORGOTTEN",
        ["succeeded", "failed", "skipped", "skipped"],
        {"GENERAL:", "CONSOLIDATE:"},
        [("failed", []), ("skipped", [])],
        ("security", "security"),
    ),
    (
        "SILENT",
        ["succeeded", "succeeded", "skipped", "failed"],
        {"GENERAL:", "SECURITY:", "CONSOLIDATE:"},
        [("succeeded", []), ("skipped", [])],
        ("quote", "published no `account`"),
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
entry names, and the failed step whose reason is checked beside the text it carries."""


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
    for question, statuses, called, collections, reason in CASES:
        result = desk.run().observe.verify("specialist-review", question)
        assert [step(result, id)["status"] for id in ("plan", "security", "style", "quote")] == (
            statuses
        ), question
        assert step(result, "general")["status"] == "succeeded", question
        assert requirement(result, "consolidated")["outcome"] == "passed", question
        assert {marker for marker in MARKERS if asked(receiver, marker, question)} == called, (
            question
        )
        if reason is not None:
            failed, carried = reason
            assert carried in step(result, failed).get("reason", ""), question
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


SPECULATED: list[tuple[str, bool, str, str, list[str], list[str]]] = [
    ("SPEC-ADMIT-LATE-DECISION", True, "succeeded", "admitted", ["account"], ["admitted"]),
    ("SPEC-ADMIT-EARLY-DECISION", True, "succeeded", "admitted", ["account"], ["admitted"]),
    ("SPEC-DISCARD-LATE-DECISION", True, "skipped", "discarded", [], ["discarded"]),
    ("SPEC-DISCARD-EARLY-DECISION", True, "skipped", "discarded", [], []),
    ("SPEC-BLOCK-LATE-DECISION", True, "failed", "blocked", [], ["blocked"]),
    ("SPEC-KNOWN", False, "skipped", "discarded", [], []),
]
"""Each ``speculative-review`` question beside whether the extra reviewer was called, the
``extra`` step's status, the speculation and entry names of the consolidator's ``extra``
manifest, and the speculation the execution's listing retains for the extra account."""


def extra_speculation(execution: verify.Execution) -> list[str]:
    """The speculation of every publication ``execution``'s ``extra`` step retained."""
    listed = publications(execution)
    return [each["speculation"] for each in listed if each["producer"]["step_id"] == "extra"]


def test_speculative_disposition_controls_consumer_admission(
    desk: WyrdState, receiver: Receiver, wyrd_server: WyrdTestServer
) -> None:
    admin = client_of(wyrd_server)
    reader = client_of(
        wyrd_server,
        wyrd_server.scoped_api_key(
            "speculation_reader", [{"resource": "verifier", "action": "read", "scope": "all"}]
        ),
    )
    for question, called, status, speculation, names, retained in SPECULATED:
        result = desk.run().observe.verify("speculative-review", question)
        assert requirement(result, "consolidated")["outcome"] == "passed", question
        assert step(result, "extra")["status"] == status, question
        if called:
            with receiver.delivered:
                assert receiver.delivered.wait_for(
                    lambda question=question: asked(receiver, "EXTRA:", question), 90
                ), f"{question}: a cancelled extra review counts once abandoned"
        else:
            assert not asked(receiver, "EXTRA:", question), question
        read = tool_results(asked(receiver, "CONSOLIDATE:", question)[-1])
        manifest = json.loads(read_text(read["c-extra"]))
        entries = [entry["name"] for entry in manifest["entries"]]
        assert (manifest["speculation"], entries) == (speculation, names), question

        execution = verify.Execution.connect(result.execution_id, admin)
        assert extra_speculation(execution) == retained, question
        if retained:
            inspected = verify.Execution.connect(result.execution_id, reader).publication(
                {"step_id": "extra", "attempt": 1}, "account"
            )
            assert inspected.get("value") == {"extra": True}, question
            assert extra_speculation(execution) == retained, (question, "inspection keeps it")


def manifest_entries(read: dict[str, Any]) -> list[tuple[int | None, int | None, str]]:
    """The sorted ``(item_index, iteration_index, name)`` entries of a manifest ``fs.read`` result."""
    entries = json.loads(read_text(read))["entries"]
    return sorted(
        (entry.get("item_index"), entry.get("iteration_index"), entry["name"]) for entry in entries
    )


def test_document_pipeline_uses_array_repeat_and_publications(
    desk: WyrdState, receiver: Receiver, wyrd_server: WyrdTestServer
) -> None:
    document = verify.File(FIXTURES / "cards/verifier_orchestration/samples/field-report.json")
    result = desk.run().observe.verify("document-pipeline", document)

    sections = step(result, "sections")
    assert (sections["status"], sections.get("output")) == ("succeeded", [{"done": True}] * 3)
    reviewed = {
        prompt_of(messages).split("SECTION: ", 1)[1].splitlines()[0]
        for messages in asked(receiver, "SECTION:", "")
    }
    assert reviewed == {"intro at=0", "claims at=1", "claims at=2"}, (
        "the duplicate keeps its own index and the empty appendix calls no reviewer"
    )
    polish = step(result, "polish")
    assert (polish["status"], polish.get("output")) == (
        "succeeded",
        [
            {"score": 1, "text": "intro v1"},
            {"score": 1, "text": "claims v1"},
            {"score": 1, "text": "claims v1"},
        ],
    ), "each section repeats independently and the final values keep input order"
    rounds: dict[str, list[str]] = {}
    for messages in asked(receiver, "POLISH:", ""):
        item, round = prompt_of(messages).split(" item=", 1)[1].split(" round=", 1)
        rounds.setdefault(item, []).append(round.split()[0])
    assert rounds == {item: ["0", "1"] for item in ("0", "1", "2")}, (
        "every item runs its own rounds in order until its condition holds"
    )
    appendix = step(result, "appendix")
    assert (appendix["status"], appendix.get("output")) == ("succeeded", [])
    assert step(result, "escalate")["status"] == "failed"
    summarize = step(result, "summarize")
    assert (summarize["status"], summarize.get("output")) == (
        "succeeded",
        {"score": 1, "text": "summary v1"},
    )
    summaries = asked(receiver, "SUMMARIZE:", "")
    assert [
        int(prompt_of(messages).split("index=", 1)[1].split()[0]) for messages in summaries
    ] == [0, 0, 0, 1, 1, 1], "one iteration at a time, stopping within the bound of 3"
    assert "previous=null" in prompt_of(summaries[0])
    read = tool_results(summaries[1])
    assert manifest_entries(read["m-accounts"]) == [(item, None, "account") for item in range(3)]
    escalation = json.loads(read_text(read["m-escalation"]))
    assert (escalation["status"], manifest_entries(read["m-escalation"])) == (
        "failed",
        [(None, None, "note")],
    ), "the failed escalation keeps its partial note"
    execution = verify.Execution.connect(result.execution_id, client_of(wyrd_server))
    accounts = sorted(
        (
            (
                each["producer"].get("item_index"),
                execution.publication(each["producer"], each["name"]).get("value"),
            )
            for each in publications(execution)
            if each["producer"]["step_id"] == "sections"
        ),
        key=lambda account: account[0] or 0,
    )
    assert accounts == [
        (0, {"section": "intro", "at": 0}),
        (1, {"section": "claims", "at": 1}),
        (2, {"section": "claims", "at": 2}),
    ]

    assert {judged["id"]: judged["outcome"] for judged in result.requirements} == {
        "appendix-reviewed": "failed",
        "cited": "passed",
        "escalation-done": "inconclusive",
        "summarized": "passed",
    }
    assert requirement(result, "cited").get("citations") == [
        {"slot": "document", "locator": {"json_pointer": "/title"}}
    ]
    assert result.verdict == "failed"
