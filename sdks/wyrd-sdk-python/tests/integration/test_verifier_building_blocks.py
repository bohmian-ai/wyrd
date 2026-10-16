"""Verifier building blocks, end to end, as the ``policy-desk`` Service.

``policy-review`` runs every action kind (``extract``, ``agent``, ``code``)
and every Run check kind (``assert``, ``trace``, ``agent``, ``code``) over an
answer, its refund policy PDF, and the trace that produced it;
``document-formats`` extracts each of the nine extractable formats into its
locked output. Drift checks run on schedules in
``test_scheduled_drift_alerts_operator``, ``test_observe_a_run``, and
``test_verify_in_real_time``.

The session's local model provider stands in for the gateway's upstream: the
refund reader first calls ``fs.read`` on its workspace copy of the policy and
then answers a 100-dollar limit, and the policy judge passes every answer
with a citation unless the answer names a scripted case.
"""

from collections.abc import Iterator
from pathlib import Path
from typing import Any

import pytest
from opentelemetry import trace
from wyrd import WyrdError, verify
from wyrd.cards import Cards
from wyrd.client import WyrdClient
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer

from .support import FIXTURES, Receiver, download, register, service_key, timeline
from .test_local_development import configure_gateway

pytestmark = pytest.mark.integration

WAIT_SECONDS = 90.0
"""How long a test waits for a queued execution."""

POLICY = FIXTURES.parent / "examples/support-desk/service/refund-policy.pdf"
"""The policy every review reads: the support desk's checked-in refund policy."""

CONFORMING = "Refunds follow our policy."
"""An answer naming no scripted case, which names the policy."""

DOCUMENTS = {
    "sample.pdf": {"format": "pdf", "pages": [{"number": 1, "text": "Refunds"}]},
    "sample.pptx": {
        "format": "pptx",
        "slides": [
            {
                "number": 1,
                "title": None,
                "paragraphs": ["a"],
                "tables": [{"rows": [["x", ""]]}],
                "notes": "",
            }
        ],
    },
    "sample.docx": {
        "format": "docx",
        "blocks": [
            {"kind": "paragraph", "text": "Refund policy", "style": "Heading1"},
            {"kind": "table", "rows": [["Window", "Days"], ["Standard", "30"]]},
        ],
    },
    "sample.xlsx": {
        "format": "xlsx",
        "sheets": [
            {
                "name": "Refunds",
                "cells": [
                    {"address": "A1", "value": "Days", "formula": None, "error": None},
                    {"address": "B2", "value": 30, "formula": None, "error": None},
                    {"address": "C3", "value": None, "formula": "=1/0", "error": "#DIV/0!"},
                    {"address": "D4", "value": True, "formula": None, "error": None},
                ],
            }
        ],
    },
    "sample.csv": {"format": "csv", "rows": [["a", "b"], ["1", "2"]]},
    "sample.json": {"format": "json", "value": {"nested": [1, None]}},
    "sample.txt": {"format": "text", "text": ""},
    "sample.md": {"format": "markdown", "text": "# Title"},
    "sample.html": {"format": "html", "text": "<p>x</p>"},
}
"""Each extractable sample beside its exact locked extract output."""


@pytest.fixture(scope="module")
def policy_desk_bundle(
    wyrd_server: WyrdTestServer, cards: Cards, tmp_path_factory: pytest.TempPathFactory
) -> Path:
    """The ``policy-desk`` Service, registered and downloaded, with ``openai`` deployed."""
    configure_gateway(WyrdClient())
    register(cards, "cards/verifier_building_blocks/policy-desk.yaml")
    return download(cards, "policy-desk", tmp_path_factory.mktemp("policy-desk"))


@pytest.fixture(scope="module")
def policy_desk_key(policy_desk_bundle: Path) -> str:
    """The ``policy-desk`` Service's own key."""
    return service_key("policy-desk")


@pytest.fixture
def desk(policy_desk_bundle: Path, policy_desk_key: str) -> Iterator[WyrdState]:
    """The hydrated ``policy-desk`` Service acting as its own key, Bifrost started."""
    state = WyrdState.from_path(policy_desk_bundle, WyrdClient(credential=policy_desk_key))
    state.start_bifrost()
    yield state
    state.shutdown()


def review(
    desk: WyrdState, text: str, refund: int, trace_id: str | None = None
) -> verify.VerificationResult:
    """Verify an answer offering ``refund`` dollars with ``policy-review`` now."""
    inputs: dict[str, Any] = {
        "answer": {"text": text, "refund": refund},
        "policy": verify.File(POLICY),
    }
    if trace_id is not None:
        inputs["trace"] = trace_id
    return desk.run().observe.verify("policy-review", **inputs)


def requirement(result: verify.VerificationResult, id: str) -> dict[str, Any]:
    """The requirement ``id`` of ``result``."""
    [found] = [each for each in result.requirements if each["id"] == id]
    return found


def step(result: verify.VerificationResult, id: str) -> dict[str, Any]:
    """The step ``id`` of ``result``."""
    [found] = [each for each in result.steps if each["id"] == id]
    return found


def record_trace(desk: WyrdState, server: WyrdTestServer, name: str) -> str:
    """Export one span named ``name`` as the Service and return its stored trace ID."""
    desk.start_telemetry()
    with trace.get_tracer("journey.policy").start_as_current_span(name) as span:
        pass
    desk.flush()
    trace_id = format(span.get_span_context().trace_id, "032x")
    assert timeline(server, trace_id), "the trace is stored"
    return trace_id


def test_policy_review_runs_every_action_and_check_kind(
    desk: WyrdState, wyrd_server: WyrdTestServer, receiver: Receiver, fresh_tracer_provider: None
) -> None:
    trace_id = record_trace(desk, wyrd_server, "lookup-policy")
    asked = len(receiver.to("/v1/chat/completions"))

    result = review(desk, CONFORMING, 50, trace_id)

    assert result.verdict == "passed", result.requirements
    assert result.subject == desk.root_ref
    text = step(result, "policy-text")
    assert (text["status"], text["output"]["format"]) == ("succeeded", "pdf")
    assert text["provenance"]["kind"] == "extract"
    rules = step(result, "refund-rules")
    assert (rules["status"], rules["output"]) == ("succeeded", {"max_refund": 100})
    assert [call["tool"] for call in rules["tool_calls"]] == ["fs.read"]
    facts = step(result, "answer-facts")
    assert (facts["status"], facts["output"]) == ("succeeded", {"within_limit": True})
    assert facts["provenance"]["kind"] == "code"
    for id in (
        "policy-has-text",
        "limit-found",
        "refund-within-limit",
        "policy-looked-up",
        "follows-policy",
        "names-the-policy",
    ):
        assert requirement(result, id)["outcome"] == "passed", id
    cited = [{"slot": "answer", "locator": {"json_pointer": "/text"}}]
    assert requirement(result, "follows-policy")["citations"] == cited
    assert requirement(result, "names-the-policy")["citations"] == cited
    [judge] = [
        each.body
        for each in receiver.to("/v1/chat/completions")[asked:]
        if "Claim:" in str(each.body)
    ]
    assert "tools" not in judge, "the judge is offered no tools"


def test_refund_over_the_limit_fails_the_assertion(desk: WyrdState) -> None:
    result = review(desk, CONFORMING, 500)

    assert result.verdict == "failed"
    over = requirement(result, "refund-within-limit")
    assert (over["outcome"], over["actual"]) == ("failed", False)


def test_judge_verdict_fails_the_agent_check(desk: WyrdState) -> None:
    result = review(desk, "REFUSE-POLICY: no refunds, policy or not.", 50)

    judged = requirement(result, "follows-policy")
    assert judged["outcome"] == "failed"
    assert len(judged["citations"]) == 1, judged
    assert result.verdict == "failed"


def test_uncited_judge_verdict_cannot_pass(desk: WyrdState) -> None:
    result = review(desk, "UNCITED: refunds follow our policy.", 50)

    assert requirement(result, "follows-policy")["outcome"] != "passed"
    assert result.verdict != "passed"


def test_raising_code_check_cannot_pass(desk: WyrdState) -> None:
    result = review(desk, "raise", 50)

    assert requirement(result, "names-the-policy")["outcome"] != "passed"
    assert result.verdict != "passed"


def test_forbidden_code_access_cannot_pass_dependent_work(desk: WyrdState) -> None:
    result = review(desk, "read the host", 50)

    assert step(result, "answer-facts")["status"] == "failed"
    assert requirement(result, "refund-within-limit")["outcome"] != "passed"
    assert result.verdict != "passed"


def test_trace_check_fails_a_guess_and_needs_its_trace(
    desk: WyrdState, wyrd_server: WyrdTestServer, fresh_tracer_provider: None
) -> None:
    guessed = record_trace(desk, wyrd_server, "guess-policy")

    failed = review(desk, CONFORMING, 50, guessed)
    untraced = review(desk, CONFORMING, 50)

    assert requirement(failed, "policy-looked-up")["outcome"] == "failed"
    assert requirement(untraced, "policy-looked-up")["outcome"] == "inconclusive"
    assert untraced.verdict != "passed"


def test_missing_policy_is_inconclusive(desk: WyrdState) -> None:
    result = desk.run().observe.verify("policy-review", answer={"text": CONFORMING, "refund": 50})

    assert result.verdict == "inconclusive"


def test_malformed_policy_cannot_pass(desk: WyrdState) -> None:
    result = desk.run().observe.verify(
        "policy-review",
        answer={"text": CONFORMING, "refund": 50},
        policy=verify.File(b"not a pdf", format="pdf"),
    )

    assert step(result, "policy-text")["status"] == "failed"
    assert requirement(result, "policy-has-text")["outcome"] != "passed"
    assert result.verdict != "passed"


def test_oversized_policy_is_refused_locally(desk: WyrdState, receiver: Receiver) -> None:
    asked = len(receiver.to("/v1/chat/completions"))

    with pytest.raises(WyrdError) as refused:
        desk.run().observe.verify(
            "policy-review",
            answer={"text": CONFORMING, "refund": 50},
            policy=verify.File(b"%" * 65_537, format="pdf"),
        )

    assert refused.value.code == "WYRD_VERIFICATION_413_INPUT_TOO_LARGE"
    assert len(receiver.to("/v1/chat/completions")) == asked, "nothing ran"


def test_submitted_review_is_judged_in_the_background(
    desk: WyrdState, policy_desk_key: str
) -> None:
    execution = desk.run().observe.submit(
        "policy-review", answer={"text": CONFORMING, "refund": 500}, policy=verify.File(POLICY)
    )
    assert isinstance(execution, verify.Execution)

    result = verify.Execution.connect(execution.id, WyrdClient(credential=policy_desk_key)).wait(
        WAIT_SECONDS
    )

    assert result.verdict == "failed"
    assert requirement(result, "refund-within-limit")["outcome"] == "failed"


def test_every_format_extracts_its_locked_output(desk: WyrdState) -> None:
    evidence = FIXTURES / "cards/verifier_building_blocks/evidence"
    for name, expected in DOCUMENTS.items():
        result = desk.run().observe.verify("document-formats", verify.File(evidence / name))

        assert step(result, "read")["output"] == expected, name
        assert requirement(result, "extracted")["outcome"] == "passed", name
