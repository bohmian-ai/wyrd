"""Verifier building blocks, end to end, as the ``policy-desk`` Service.

``policy-review`` runs every action kind (``extract``, ``agent``, ``code``)
and every Run check kind (``assert``, ``trace``, ``agent``, ``code``) over an
answer, its refund policy PDF, and the trace that produced it;
``document-formats`` extracts each of the nine extractable formats into its
locked output. Drift checks run on schedules in
``test_scheduled_drift_alerts_operator``, ``test_observe_a_run``, and
``test_verify_in_real_time``.

The session's local model provider stands in for the gateway's upstream: the
refund reader first calls ``fs.extract`` on its workspace copy of the policy and
then answers a 100-dollar limit, the policy judge passes every answer with a
citation unless the answer names a scripted case, and the ``tool-review``
investigator calls the Card, Bifrost, and code tools at once and then
finishes. Scripted answer texts also fail the reader's provider, schema, and
deadline and the code step and check; scoped and delegated principals
exercise every tool grant.

Uploaded Artifacts are published as exact files, optionally extracted before
registration, and mounted by selection: ``handbook-desk``'s two judges read
the prepared handbook with ``fs.read`` and cite the line holding a fresh
marker, and the file probe's scripted Agent plays real ``fs.*`` calls over
selected mounts so each journey asserts exactly what the tools returned.
"""

from __future__ import annotations

import hashlib
import json
import secrets
import time
from collections.abc import Callable, Iterator, Sequence
from pathlib import Path
from typing import TYPE_CHECKING, Any, cast

import httpx
import pytest
from opentelemetry import trace
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import SimpleSpanProcessor
from pydantic import BaseModel, RootModel
from wyrd import WyrdError, verify
from wyrd.auth import Principals
from wyrd.bifrost import Bifrost
from wyrd.cards import CardKind, CardRef, Cards, VersionBump
from wyrd.client import WyrdClient
from wyrd.otel import span_exporter
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer, cli

from .support import (
    FIXTURES,
    Receiver,
    client_of,
    declare,
    pull,
    rooted,
    service_key,
    timeline,
)
from .test_local_development import configure_gateway

if TYPE_CHECKING:
    from wyrd.verify import RequirementResult, StepResult

pytestmark = pytest.mark.integration

WAIT_SECONDS = 90.0
"""How long a test waits for a queued execution."""

POLICY = FIXTURES.parent / "examples/support-desk/service/refund-policy.pdf"
"""The policy every review reads: the support desk's checked-in refund policy."""

CONFORMING = "Refunds follow our policy."
"""An answer naming no scripted case, which names the policy."""

ANSWER_TEXT = [{"slot": "answer", "locator": {"json_pointer": "/text"}}]
"""The citation of the answer's text, which every cited Verdict of the story names."""

TOOLS = ("cards.get", "bifrost.query", "code.run")
"""The three built-ins the investigator calls, in call order."""

DENIED = "WYRD_PERMISSION_403_DENIED_RBAC"
"""The stable code of a permission denial, carried by every denied tool call's error."""

VERIFIES = ["verifier:run", "gateway:invoke"]
"""The permissions every ``tool-review`` principal holds."""

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
    declare(cards, "cards/verifier_building_blocks/policy-desk.yaml")
    return pull(cards, "policy-desk", tmp_path_factory.mktemp("policy-desk"))


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


@pytest.fixture
def isolated_desk(receiver: Receiver, tmp_path: Path) -> Iterator[WyrdState]:
    """A ``desk`` on its own server.

    A failed provider call takes the tenant's deployment out of selection for
    the gateway cooldown, so a test that fails the provider must not share the
    session tenant with the tests after it.
    """
    with WyrdTestServer(verification_runtime=True, provider_base_url=receiver.url):
        cards = Cards()
        configure_gateway(WyrdClient())
        declare(cards, "cards/verifier_building_blocks/policy-desk.yaml")
        bundle = pull(cards, "policy-desk", tmp_path)
        state = WyrdState.from_path(bundle, WyrdClient(credential=service_key("policy-desk")))
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


def requirement(result: verify.VerificationResult, id: str) -> RequirementResult:
    """The requirement ``id`` of ``result``."""
    [found] = [each for each in result.requirements if each["id"] == id]
    return found


def step(result: verify.VerificationResult, id: str) -> StepResult:
    """The step ``id`` of ``result``."""
    [found] = [each for each in result.steps if each["id"] == id]
    return found


def record_trace(
    desk: WyrdState, server: WyrdTestServer, outer: str, inner: str, policy_id: str
) -> str:
    """Export span ``outer``, carrying ``policy_id``, with child ``inner``, and return the trace ID.

    Stored spans order by start time, so ``outer`` is the trace document's
    ``spans[0]`` and ``inner`` its ``spans[1]``.
    """
    desk.start_telemetry()
    tracer = trace.get_tracer("journey.policy")
    with tracer.start_as_current_span(outer, attributes={"policy_id": policy_id}) as span:
        with tracer.start_as_current_span(inner):
            pass
    desk.flush()
    trace_id = format(span.get_span_context().trace_id, "032x")
    assert len(timeline(server, trace_id)) == 2, "both spans are stored"
    return trace_id


def record_whole_trace(desk: WyrdState, server: WyrdTestServer, trace_id: str | None = None) -> str:
    """Export the desk's ``retrieve`` span with an untagged writer's ``score`` span inside it, and return the trace ID.

    Given ``trace_id``, both spans join that trace.
    """
    desk.start_telemetry()
    writer = TracerProvider()
    writer.add_span_processor(SimpleSpanProcessor(span_exporter(client_of(server))))
    parent = None
    if trace_id is not None:
        remote = trace.SpanContext(
            int(trace_id, 16),
            int(secrets.token_hex(8), 16),
            is_remote=True,
            trace_flags=trace.TraceFlags(trace.TraceFlags.SAMPLED),
        )
        parent = trace.set_span_in_context(trace.NonRecordingSpan(remote))
    with trace.get_tracer("journey.desk").start_as_current_span("retrieve", context=parent) as span:
        with writer.get_tracer("journey.scorer").start_as_current_span("score"):
            pass
    writer.shutdown()
    desk.flush()
    traced = format(span.get_span_context().trace_id, "032x")
    assert len(timeline(server, traced)) == 2, "both spans are stored"
    return traced


SCORED_TEXT = "Refunds follow our policy."
"""The ``scored-review`` answer naming no scripted case: four words."""

SCORED_DATA = {
    "helpful": {"score": 0.9, "category": "refund"},
    "well-formed": {"score": 0.5, "category": "format"},
    "tally-counted": {"words": 4},
}
"""The custom data each scored ``scored-review`` requirement returns for ``SCORED_TEXT``."""


class Score(BaseModel):
    """A score with its category."""

    score: float
    category: str


class Tally(BaseModel):
    """A word count."""

    words: int


class Scored(RootModel[Score | Tally]):
    """Either scored shape ``scored-review`` returns."""


class Numbers(RootModel[dict[str, float]]):
    """Numbers only, which the scores' text categories do not fit."""


class RetainedOutput(BaseModel):
    """One retained result's output record as JSON text."""

    output: str


def scored_review(desk: WyrdState, text: str = SCORED_TEXT) -> verify.VerificationResult:
    """Verify ``text`` with ``scored-review`` now."""
    return desk.run().observe.verify("scored-review", answer={"text": text})


def submit_scored(desk: WyrdState, text: str = SCORED_TEXT) -> verify.Execution:
    """Submit ``text`` to ``scored-review`` and return its queued execution."""
    execution = desk.run().observe.submit("scored-review", answer={"text": text})
    assert isinstance(execution, verify.Execution)
    return execution


def assert_scored(requirements: list[RequirementResult]) -> None:
    """Assert the plain judges pass without data and the scored checks keep their exact data."""
    by_id = {each["id"]: each for each in requirements}
    for id in ("polite", "on-topic"):
        assert by_id[id]["outcome"] == "passed", by_id[id]
        assert "data" not in by_id[id], id
    assert by_id["helpful"]["outcome"] == "passed"
    assert by_id["well-formed"]["outcome"] == "failed"
    assert by_id["tally-counted"]["outcome"] == "passed"
    assert by_id["tally-counted"]["step"] == "tally"
    for id, data in SCORED_DATA.items():
        assert by_id[id]["data"] == data, id
        assert by_id[id]["citations"] == ANSWER_TEXT, id


def scored_calls(receiver: Receiver) -> int:
    """How many scored-judge calls the local provider has answered."""
    return sum("SCORE:" in json.dumps(each.body) for each in receiver.to("/v1/chat/completions"))


class Attribution(BaseModel):
    """One retained result's subject and actor."""

    sub_principal_id: str
    act_principal_id: str


def grants(tool_scope: str | dict[str, object]) -> list[str | dict[str, object]]:
    """Verify, invoke the gateway, read Cards and Bifrost, and call the tools ``tool_scope`` covers."""
    return [
        {"resource": "verifier", "action": "run", "scope": "all"},
        {"resource": "gateway", "action": "invoke", "scope": "all"},
        {"resource": "tool", "action": "invoke", "scope": tool_scope},
        {"resource": "cards", "action": "read", "scope": "all"},
        {"resource": "bifrost_query", "action": "read", "scope": "all"},
    ]


def principal_id(name: str) -> str:
    """The id of the principal named ``name``."""
    return Principals.connect().list(name=name)["principals"][0]["principal_id"]


def investigate(bundle: Path, client: WyrdClient, question: str) -> verify.VerificationResult:
    """Ask the investigator ``question`` with ``tool-review`` now, as ``client``."""
    return WyrdState.from_path(bundle, client).run().observe.verify("tool-review", question)


def assert_tools(
    result: verify.VerificationResult,
    receiver: Receiver,
    question: str,
    allowed: Sequence[str],
) -> None:
    """Assert each of ``TOOLS`` was called once, in order, and only ``allowed`` returned output.

    Every other call was denied with ``DENIED`` and returned nothing; the
    model's tool turn shows the policy desk's Card only when its read was
    allowed.
    """
    investigate = step(result, "investigate")
    assert investigate["status"] == "succeeded", question
    calls = investigate.get("tool_calls", [])
    assert [call["tool"] for call in calls] == list(TOOLS), question
    for call, tool in zip(calls, TOOLS, strict=True):
        if tool in allowed:
            assert "error" not in call, (question, call)
            assert "output_sha256" in call, (question, tool)
            assert call["returned_bytes"] > 0, (question, tool)
        else:
            assert DENIED in call.get("error", ""), (question, call)
            assert (call.get("output_sha256"), call["returned_bytes"]) == (None, 0), (
                question,
                tool,
            )
    turn = next(
        body
        for body in (json.dumps(each.body) for each in receiver.to("/v1/chat/completions"))
        if question in body and '"role": "tool"' in body
    )
    assert ("document-formats" in turn) == (TOOLS[0] in allowed), (
        f"{question}: the Card reaches the model only when its read is allowed"
    )


def test_policy_review_runs_every_action_and_check_kind(
    fresh_telemetry_providers: None,
    desk: WyrdState,
    wyrd_server: WyrdTestServer,
    receiver: Receiver,
) -> None:
    trace_id = record_trace(desk, wyrd_server, "lookup-policy", "draft-answer", "refunds")
    asked = len(receiver.to("/v1/chat/completions"))

    result = review(desk, CONFORMING, 50, trace_id)

    assert result.verdict == "passed", result.requirements
    assert result.subject == desk.root_ref
    text = step(result, "policy-text")
    output = cast("dict[str, Any]", text.get("output"))
    assert (text["status"], output["format"]) == ("succeeded", "pdf")
    assert text["provenance"]["kind"] == "extract"
    rules = step(result, "refund-rules")
    assert (rules["status"], rules["output"]) == ("succeeded", {"max_refund": 100})
    assert [call["tool"] for call in rules["tool_calls"]] == ["fs.extract"]
    facts = step(result, "answer-facts")
    assert (facts["status"], facts["output"]) == ("succeeded", {"within_limit": True})
    assert facts["provenance"]["kind"] == "code"
    for id in (
        "policy-has-text",
        "limit-found",
        "refund-within-limit",
        "policy-looked-up",
        "answer-drafted-after-lookup",
        "lookup-read-refunds",
        "follows-policy",
        "names-the-policy",
    ):
        assert requirement(result, id)["outcome"] == "passed", id
    assert requirement(result, "follows-policy")["citations"] == ANSWER_TEXT
    assert requirement(result, "names-the-policy")["citations"] == ANSWER_TEXT
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
    fresh_telemetry_providers: None, desk: WyrdState, wyrd_server: WyrdTestServer
) -> None:
    guessed = record_trace(desk, wyrd_server, "guess-policy", "draft-answer", "refunds")

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

    resumed = verify.Execution.connect(
        execution.id,
        WyrdClient(credential=policy_desk_key, workload_uid=str(desk.root_ref.uid)),
    )
    result = resumed.wait(WAIT_SECONDS)
    status = resumed.status()

    assert status["status"] == "completed"
    assert status["result"]["execution_id"] == execution.id
    assert "error" not in status
    assert result.verdict == "failed"
    assert requirement(result, "refund-within-limit")["outcome"] == "failed"


def test_trace_evidence_keeps_whole_trace_and_waits_when_submitted(
    fresh_telemetry_providers: None, desk: WyrdState, wyrd_server: WyrdTestServer
) -> None:
    whole = record_whole_trace(desk, wyrd_server)
    assert desk.run().observe.verify("whole-trace", trace=whole).verdict == "passed"

    [unlanded] = desk.run().observe.verify("whole-trace", trace=secrets.token_hex(16)).requirements
    assert unlanded["outcome"] == "inconclusive"
    assert "not yet landed" in unlanded["reason"]

    late = secrets.token_hex(16)
    execution = desk.run().observe.submit("whole-trace", trace=late)
    assert isinstance(execution, verify.Execution)
    deadline = time.monotonic() + WAIT_SECONDS
    while True:
        try:
            state = execution.status()
        except WyrdError as error:
            assert error.code == "WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND", error
        else:
            if state["status"] == "retrying":
                break
            assert state["status"] in ("pending", "running"), state
        assert time.monotonic() < deadline, "the absent trace never retried"
        time.sleep(0.1)
    assert state["error"]["code"] == "evidence_unavailable"
    record_whole_trace(desk, wyrd_server, late)

    result = execution.wait(WAIT_SECONDS)
    assert result.verdict == "passed"
    assert execution.status()["result"]["execution_id"] == execution.id


def test_every_format_extracts_its_locked_output(desk: WyrdState) -> None:
    evidence = FIXTURES / "cards/verifier_building_blocks/samples"
    for name, expected in DOCUMENTS.items():
        result = desk.run().observe.verify("document-formats", verify.File(evidence / name))

        assert step(result, "read")["output"] == expected, name
        assert requirement(result, "extracted")["outcome"] == "passed", name


def test_file_inputs_use_shared_uploads(desk: WyrdState, wyrd_server: WyrdTestServer) -> None:
    path = FIXTURES / "cards/verifier_building_blocks/samples/sample.csv"
    in_memory = b"a,b\n3,4"
    for file, rows in [
        (verify.File(path), [["a", "b"], ["1", "2"]]),
        (verify.File(in_memory, format="csv"), [["a", "b"], ["3", "4"]]),
    ]:
        result = desk.run().observe.verify("document-formats", file)
        assert step(result, "read")["output"] == {"format": "csv", "rows": rows}

    for content in [path.read_bytes(), in_memory]:
        planned = httpx.post(
            f"{wyrd_server.base_url}/v1/verification/evidence",
            headers={"x-wyrd-access-token": f"Bearer {wyrd_server.access_token()}"},
            json={
                "files": [
                    {"sha256": hashlib.sha256(content).hexdigest(), "size_bytes": len(content)}
                ]
            },
        )
        assert planned.status_code == 200, planned.text
        assert planned.json()["uploads"] == []


def test_stored_evidence_readback_uses_canonical_records(desk: WyrdState) -> None:
    content = b"a,b\n5,6"
    execution = desk.run().observe.submit("document-formats", verify.File(content, format="csv"))
    assert isinstance(execution, verify.Execution)

    result = execution.wait(WAIT_SECONDS)
    status = execution.status()

    assert status["result"]["execution_id"] == execution.id
    assert status["result"]["outcome"]["verdict"] == result.verdict
    assert list(status["result"]["outcome"]["evidence"].values()) == [
        {
            "kind": "file",
            "sha256": hashlib.sha256(content).hexdigest(),
            "size_bytes": len(content),
            "format": "csv",
        }
    ]


def test_failed_reader_leaves_its_dependents_inconclusive(isolated_desk: WyrdState) -> None:
    # PROVIDER-DOWN runs last: its failure starts the deployment's cooldown.
    for marker in ("OFF-SCHEMA", "SLOW-READER", "PROVIDER-DOWN"):
        result = review(isolated_desk, f"{marker}: refunds follow our policy.", 50)

        rules = step(result, "refund-rules")
        assert rules["status"] == "failed", (marker, rules)
        assert rules.get("reason"), f"{marker}: the failure says why"
        assert step(result, "answer-facts")["status"] == "skipped", marker
        for id in ("limit-found", "refund-within-limit", "follows-policy"):
            assert requirement(result, id)["outcome"] == "inconclusive", (marker, id)
        assert result.verdict == "inconclusive", marker


def test_failed_code_step_leaves_its_assertion_inconclusive(desk: WyrdState) -> None:
    for text in ("fail the facts", "spin the facts"):
        result = review(desk, text, 50)

        facts = step(result, "answer-facts")
        assert facts["status"] == "failed", (text, facts)
        assert facts.get("reason"), f"{text}: the failure says why"
        assert requirement(result, "refund-within-limit")["outcome"] == "inconclusive", text
        assert result.verdict != "passed", text


def test_trace_check_judges_span_order_and_attributes(
    fresh_telemetry_providers: None, desk: WyrdState, wyrd_server: WyrdTestServer
) -> None:
    drafted_first = record_trace(desk, wyrd_server, "draft-answer", "lookup-policy", "refunds")
    shipping = record_trace(desk, wyrd_server, "lookup-policy", "draft-answer", "shipping")

    for trace_id, expected in (
        (drafted_first, ["failed", "failed", "passed"]),
        (shipping, ["passed", "passed", "failed"]),
    ):
        result = review(desk, CONFORMING, 50, trace_id)

        judged = [
            requirement(result, id)["outcome"]
            for id in ("policy-looked-up", "answer-drafted-after-lookup", "lookup-read-refunds")
        ]
        assert judged == expected, trace_id
        assert result.verdict == "failed"


def test_invalid_judge_verdicts_are_inconclusive(desk: WyrdState) -> None:
    for marker in ("JUDGE-MALFORMED", "JUDGE-FOREIGN-CITE"):
        result = review(desk, f"{marker}: refunds follow our policy.", 50)

        judged = requirement(result, "follows-policy")
        assert judged["outcome"] == "inconclusive", (marker, judged)
        assert judged["reason"], f"{marker}: it says why"
        assert result.verdict == "inconclusive", marker
    malformed = requirement(review(desk, "JUDGE-MALFORMED", 50), "follows-policy")
    assert "WYRD_AGENT_422_STRUCTURED_INVALID" in malformed["reason"], malformed
    assert "not a verdict" not in malformed["reason"], malformed


def test_judge_repairs_an_invalid_verdict(desk: WyrdState, receiver: Receiver) -> None:
    result = review(desk, "JUDGE-REPAIRED: refunds follow our policy.", 50)

    judged = requirement(result, "follows-policy")
    assert (judged["outcome"], judged["citations"]) == ("passed", ANSWER_TEXT), judged
    asked = [
        text
        for text in map(json.dumps, (each.body for each in receiver.to("/v1/chat/completions")))
        if "JUDGE-REPAIRED" in text and "Rules:" in text
    ]
    assert len(asked) == 2 and "was not accepted" in asked[1], asked


def test_code_check_fails_with_citations_and_is_inconclusive_otherwise(desk: WyrdState) -> None:
    failed = review(desk, "Refunds are fine.", 50)
    named = requirement(failed, "names-the-policy")
    assert (named["outcome"], named.get("citations")) == ("failed", ANSWER_TEXT)
    assert failed.verdict == "failed"

    for text in ("CODE-MALFORMED", "CODE-FOREIGN-CITE", "CODE-FORBIDDEN", "CODE-SPIN"):
        result = review(desk, text, 50)

        named = requirement(result, "names-the-policy")
        assert named["outcome"] == "inconclusive", (text, named)
        assert named["reason"], f"{text}: it says why"
        assert result.verdict != "passed", text


def test_hosted_tools_need_the_tool_and_domain_grants(
    policy_desk_bundle: Path, wyrd_server: WyrdTestServer, receiver: Receiver
) -> None:
    for name, extra, allowed in (
        ("all_tools", ["tool:invoke", "cards:read", "bifrost_query:read"], TOOLS),
        ("no_tool_grant", ["cards:read", "bifrost_query:read"], ()),
        ("no_domain_grant", ["tool:invoke", "cards:read"], (TOOLS[0], TOOLS[2])),
    ):
        key = wyrd_server.scoped_api_key(name, [*VERIFIES, *extra])
        result = investigate(policy_desk_bundle, WyrdClient(credential=key), name)
        assert_tools(result, receiver, name, allowed)
    key = wyrd_server.scoped_api_key("card_tool_only", grants({"tool": TOOLS[0]}))
    result = investigate(policy_desk_bundle, WyrdClient(credential=key), "card_tool_only")
    assert_tools(result, receiver, "card_tool_only", (TOOLS[0],))


def test_delegated_tools_keep_the_narrower_authority(
    policy_desk_bundle: Path, wyrd_server: WyrdTestServer, receiver: Receiver
) -> None:
    subject = WyrdClient(credential=wyrd_server.scoped_api_key("tool_subject", grants("all")))
    actor = rooted(
        wyrd_server.scoped_api_key("tool_delegate", grants({"tool": TOOLS[0]})), "policy-desk"
    )
    delegated = actor.on_behalf_of(subject.access_token(), audience="wyrd")

    result = investigate(policy_desk_bundle, delegated, "delegated")

    assert_tools(result, receiver, "delegated", (TOOLS[0],))
    wyrd_server.flush_bifrost()
    attributed = Bifrost(client=client_of(wyrd_server)).sql(
        "SELECT sub_principal_id, act_principal_id "
        "FROM bifrost.verification.results WHERE execution_id = $1",
        [result.execution_id],
        model=Attribution,
    )
    assert attributed == [
        Attribution(
            sub_principal_id=principal_id("tool_subject"),
            act_principal_id=principal_id("tool_delegate"),
        )
    ]


def test_custom_verdict_data_survives_readback(
    desk: WyrdState, wyrd_server: WyrdTestServer
) -> None:
    assert_scored(scored_review(desk).requirements)
    execution = submit_scored(desk)
    waited = execution.wait(WAIT_SECONDS)
    assert_scored(waited.requirements)
    status = execution.status()
    assert status["result"]["outcome"]["requirements"] == waited.requirements

    assert retained(wyrd_server, waited.execution_id) == status["result"]["outcome"]["requirements"]

    for marker, data in EXACT_DATA.items():
        text = f"{marker} refunds"
        assert requirement(scored_review(desk, text), "well-formed")["data"] == data, marker
        execution = submit_scored(desk, text)
        waited = execution.wait(WAIT_SECONDS)
        assert requirement(waited, "well-formed")["data"] == data, marker
        status = execution.status()
        assert status["result"]["outcome"]["requirements"] == waited.requirements, marker
        assert retained(wyrd_server, waited.execution_id) == waited.requirements, marker


def retained(wyrd_server: WyrdTestServer, execution_id: str) -> object:
    """The retained requirements of ``execution_id``, read back from Bifrost after publishing."""
    wyrd_server.flush_bifrost()
    [row] = Bifrost(client=client_of(wyrd_server)).sql(
        "SELECT to_json(output) AS output FROM bifrost.verification.execution_outputs WHERE execution_id = $1",
        [execution_id],
        model=RetainedOutput,
    )
    return json.loads(row.output)["requirements"]


def deep(containers: int) -> object:
    """A ``tree`` that, inside its data object, nests ``containers`` containers."""
    tree: object = "s3cr3t"
    for _ in range(containers - 1):
        tree = [tree]
    return tree


EXACT_DATA = {
    "EXACT-BYTES": {"score": 0.5, "category": "é" * 32_000 + '\n"\u0001' + "x" * 1_499},
    "EXACT-WIDE": {"score": 0.5, "category": "format", "tree": [0] * 32_747},
    "DEEP-16": {"score": 0.5, "category": "format", "tree": deep(16)},
}
"""The ``well-formed`` code check's data at its inclusive bounds, by answer marker."""


def test_invalid_custom_verdict_data_is_inconclusive(desk: WyrdState) -> None:
    for marker in ("BAD-DATA", "HUGE-DATA"):
        result = scored_review(desk, f"{marker} refunds")
        for id in ("helpful", "well-formed"):
            refused = requirement(result, id)
            assert refused["outcome"] == "inconclusive", (marker, refused)
            assert "data" not in refused, (marker, id)
            assert "high" not in refused["reason"] and "xxxx" not in refused["reason"]
        assert requirement(result, "tally-counted")["data"] == {"words": 2}, marker
        assert result.verdict == "inconclusive"

    for marker in (
        "MISSING-DATA",
        "NULL-DATA",
        "EXTRA-DATA",
        "EXTRA-FIELD",
        "OVER-BYTES",
        "OVER-WIDE",
        "DEEP-17",
    ):
        result = scored_review(desk, f"{marker} refunds")
        refused = requirement(result, "well-formed")
        assert refused["outcome"] == "inconclusive", (marker, refused)
        assert "data" not in refused, marker
        assert "s3cr3t" not in refused["reason"] and "xxxx" not in refused["reason"], marker
        assert requirement(result, "helpful")["data"] == SCORED_DATA["helpful"], marker
        assert requirement(result, "tally-counted")["data"] == {"words": 2}, marker

    result = scored_review(desk, "ENVELOPE-DATA refunds")
    refused = requirement(result, "plainly-checked")
    assert refused["outcome"] == "inconclusive", refused
    assert "data" not in refused
    assert "s3cr3t" not in refused["reason"]
    assert requirement(result, "well-formed")["data"] == {"score": 0.5, "category": "format"}


def test_custom_verdict_data_is_typed_on_return(desk: WyrdState) -> None:
    expected = {
        "helpful": Score(score=0.9, category="refund"),
        "well-formed": Score(score=0.5, category="format"),
        "tally-counted": Tally(words=4),
    }

    def assert_typed(requirements: list[RequirementResult[Scored]]) -> None:
        by_id = {each["id"]: each for each in requirements}
        assert by_id["polite"].get("data") is None
        assert {id: by_id[id]["data"].root for id in expected} == expected

    direct = desk.run().observe.verify_typed("scored-review", Scored, answer={"text": SCORED_TEXT})
    assert_typed(direct.requirements)
    assert isinstance(requirement(scored_review(desk), "helpful")["data"], dict)
    execution = submit_scored(desk)
    assert_typed(execution.wait_typed(WAIT_SECONDS, Scored).requirements)
    assert_typed(execution.status_typed(Scored)["result"]["outcome"]["requirements"])

    with pytest.raises(WyrdError) as slot:
        desk.run().observe.verify_typed("scored-review", Scored, data_type={"text": "x"})
    assert slot.value.code == "WYRD_VERIFICATION_400_INPUT_INVALID"


def test_typed_verdict_data_mismatch_keeps_original_result(
    desk: WyrdState, receiver: Receiver
) -> None:
    def assert_mismatch(raised: pytest.ExceptionInfo[WyrdError]) -> None:
        assert raised.value.code == "WYRD_SDK_422_VERIFICATION_DATA_INVALID"
        assert raised.value.details == {"requirement": "helpful", "path": ""}
        assert "refund" not in raised.value.message

    with pytest.raises(WyrdError) as direct:
        desk.run().observe.verify_typed("scored-review", Numbers, answer={"text": SCORED_TEXT})
    assert_mismatch(direct)
    execution = submit_scored(desk)
    ordinary = execution.wait(WAIT_SECONDS)
    judged = scored_calls(receiver)
    with pytest.raises(WyrdError) as waited:
        execution.wait_typed(WAIT_SECONDS, Numbers)
    assert_mismatch(waited)
    with pytest.raises(WyrdError) as status:
        execution.status_typed(Numbers)
    assert_mismatch(status)

    assert execution.status()["result"]["outcome"]["requirements"] == ordinary.requirements
    assert_scored(ordinary.requirements)
    assert scored_calls(receiver) == judged, "nothing is judged again"


CORRUPT_PDF = b"%PDF-1.7\nnot a document"
"""Bytes a ``.pdf`` name claims but the extractor refuses."""


def tree(root: Path, files: dict[str, str | bytes]) -> Path:
    """Write ``files`` under ``root`` by relative path and return ``root``."""
    for relative, content in files.items():
        target = root / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        if isinstance(content, str):
            target.write_text(content)
        else:
            target.write_bytes(content)
    return root


def inventory(cards: Cards, ref: CardRef) -> dict[str, int]:
    """Every file ``ref`` holds, by relative path, with its size."""
    return {
        str(each["relative_path"]): cast("int", each["size_bytes"])
        for each in cards.list_artifacts(ref)
    }


def snapshot(root: Path) -> dict[str, bytes]:
    """Every file under ``root`` with its bytes."""
    return {
        str(path.relative_to(root)): path.read_bytes() for path in root.rglob("*") if path.is_file()
    }


INVALID = "WYRD_LOADER_400_INVALID_ENVELOPE"
"""The stable code of an upload the loader refuses before declaring anything."""

VALIDATION = "WYRD_SPEC_400_VALIDATION"
"""The stable code of a refusal the caller repairs by the rule in its details."""

HANDBOOK = {"type": "str", "value": "handbook"}
"""An authored ``spec.metadata`` value, in its tagged non-secret form."""


def pinned_upload(name: str) -> dict[str, str]:
    """The metadata uploading ``name`` at version 1.0.0."""
    return {"name": name, "version": "1.0.0"}


def refusal(call: Callable[[], object]) -> tuple[str, object]:
    """The code and details of the ``WyrdError`` ``call`` raises."""
    with pytest.raises(WyrdError) as refused:
        call()
    return refused.value.code, refused.value.details


def refused_upload(call: Callable[[], object]) -> tuple[str, str]:
    """The code of the ``WyrdError`` an upload ``call`` raises, with its first diagnostic's code or its rule."""
    code, details = refusal(call)
    found = cast("dict[str, Any]", details)
    return code, found["diagnostics"][0]["code"] if "diagnostics" in found else found["rule"]


def test_artifact_upload_preserves_files_and_versions(cards: Cards, tmp_path: Path) -> None:
    single = tree(tmp_path / "single", {"notes.txt": "one\n", "ignored.txt": "sibling\n"})

    ref = cards.upload_artifact(single / "notes.txt", pinned_upload("upload-notes"))

    assert (ref.kind, ref.name, ref.version) == (CardKind.Artifact, "upload-notes", "1.0.0")
    assert ref.uid is not None
    assert inventory(cards, ref) == {"notes.txt": 4}
    card = cards.get(ref)
    assert card.status is not None and card.status["phase"] == "active", card.status
    assert card.spec["artifact_kind"] == "file"

    files: dict[str, str | bytes] = {
        "card.json": '{"not": "an envelope"}',
        "nested/deep/data.bin": bytes(range(256)),
        "policy.pdf": CORRUPT_PDF,
    }
    collection = tree(tmp_path / "collection", files)
    authored = {
        **pinned_upload("upload-collection"),
        "labels": {"team": "support"},
        "annotations": {"acme.com/owner": "refund-desk"},
    }
    spec = {"artifact_kind": "policies", "metadata": {"source": HANDBOOK}}
    ref = cards.upload_artifact(collection, authored, spec=spec)
    assert inventory(cards, ref) == {path: len(content) for path, content in files.items()}
    held = cards.get(ref)
    assert (held.spec["artifact_kind"], held.spec["metadata"]) == ("policies", spec["metadata"])
    assert held.metadata["labels"] == {"team": "support"}, held.metadata
    assert held.metadata["annotations"]["acme.com/owner"] == "refund-desk", held.metadata
    pulled = cards.pull(ref, tmp_path / "pulled").destination
    [data] = list(pulled.rglob("data.bin"))
    assert snapshot(data.parents[2]) == {
        path: (content.encode() if isinstance(content, str) else content)
        for path, content in files.items()
    }

    again = cards.upload_artifact(collection, authored, spec=spec)
    assert again == ref, "an unchanged upload of a pinned version is the same Card"
    (collection / "policy.pdf").write_bytes(CORRUPT_PDF + b"!")
    assert refusal(lambda: cards.upload_artifact(collection, authored, spec=spec))[0] == (
        "WYRD_REGISTRY_409_SPEC_DRIFT"
    )
    bumped = cards.upload_artifact(
        collection, {"name": "upload-collection"}, version_bump=VersionBump.Patch, spec=spec
    )
    assert bumped.version == "1.0.1"
    assert inventory(cards, bumped)["policy.pdf"] == len(CORRUPT_PDF) + 1

    (tmp_path / "link.txt").symlink_to(single / "notes.txt")
    (tmp_path / "empty").mkdir()
    refused = {
        name: refused_upload(
            lambda source=source, kind=kind, name=name: cards.upload_artifact(
                source, pinned_upload(name), spec={"artifact_kind": kind}
            )
        )
        for name, source, kind in (
            ("blank-kind-upload", single / "notes.txt", " "),
            ("linked-upload", tmp_path / "link.txt", "file"),
            ("missing-upload", tmp_path / "missing.txt", "file"),
            ("empty-upload", tmp_path / "empty", "file"),
        )
    }
    assert refused == {
        "blank-kind-upload": ("WYRD_SPEC_400_VALIDATION", "artifact_kind"),
        "linked-upload": (VALIDATION, "artifact_source_regular"),
        "missing-upload": (INVALID, "WYRD_LOADER_400_IO"),
        "empty-upload": (VALIDATION, "artifact_source_empty"),
    }
    for name in refused:
        assert not cards.list(kind="Artifact", name=name).refs, name

    offline = Cards(WyrdClient(server_url="http://127.0.0.1:9", credential="unused"))
    for metadata, field in (
        ({"name": "Not A Name", "version": "1.0.0"}, "metadata.name"),
        ({"name": "upload-bump", "bump": "patch"}, "metadata.bump"),
        ({"name": "upload-extra", "colour": "red"}, "metadata"),
    ):
        code, details = refusal(lambda metadata=metadata: offline.upload_artifact(single, metadata))
        assert (code, cast("dict[str, object]", details)["field"]) == (
            "WYRD_SPEC_400_VALIDATION",
            field,
        ), "a caller mistake is refused before any request"


def test_artifact_upload_extracts_before_registration(cards: Cards, tmp_path: Path) -> None:
    files: dict[str, str | bytes] = {
        "handbook.md": "# Handbook\nEscalate to the duty lead.\n",
        "nested/rates.csv": "tier,days\nstandard,30\n",
        "logo.bin": b"\x00\x01opaque",
    }
    source = tree(tmp_path / "source", files)
    before = snapshot(source)
    spec = {"artifact_kind": "file", "metadata": {"source": HANDBOOK}}

    ref = cards.upload_artifact(
        source, pinned_upload("extracted-handbook"), spec=spec, extract=True
    )

    assert snapshot(source) == before, "the source tree is never written"
    held = inventory(cards, ref)
    assert set(held) == {
        "handbook.md",
        "nested/rates.csv",
        "logo.bin",
        "extracted/handbook.md.json",
        "extracted/nested/rates.csv.json",
    }
    metadata = cards.get(ref).spec["metadata"]
    assert metadata["source"] == HANDBOOK, "authored spec metadata survives extraction"
    report = metadata["extraction"]
    assert "extracted/handbook.md.json" in json.dumps(report), report
    assert "logo.bin" in json.dumps(report), "the unsupported file is reported skipped"
    pulled = cards.pull(ref, tmp_path / "pulled").destination
    [markdown] = list(pulled.rglob("handbook.md.json"))
    assert json.loads(markdown.read_text()) == {
        "format": "markdown",
        "text": "# Handbook\nEscalate to the duty lead.\n",
    }
    plain = cards.upload_artifact(source, pinned_upload("raw-handbook"))
    assert not any(path.startswith("extracted/") for path in inventory(cards, plain))

    authored_report = {
        "artifact_kind": "file",
        "metadata": {"extraction": {"type": "object", "value": {}}},
    }
    (source / "policy.pdf").write_bytes(CORRUPT_PDF)
    corrupt = refused_upload(
        lambda: cards.upload_artifact(source, pinned_upload("corrupt-handbook"), extract=True)
    )
    (source / "policy.pdf").unlink()
    reported = refused_upload(
        lambda: cards.upload_artifact(
            source, pinned_upload("reported-handbook"), spec=authored_report, extract=True
        )
    )
    (source / "extracted").mkdir()
    (source / "extracted/handbook.md.json").write_text("{}")
    colliding = refused_upload(
        lambda: cards.upload_artifact(source, pinned_upload("colliding-handbook"), extract=True)
    )
    (source / "extracted/handbook.md.json").unlink()
    (source / "extracted").rmdir()
    assert (corrupt, reported, colliding) == (
        (VALIDATION, "extraction_refused"),
        (VALIDATION, "extraction_report"),
        (VALIDATION, "extraction_output_collision"),
    )
    for name in ("corrupt-handbook", "reported-handbook", "colliding-handbook"):
        assert not cards.list(kind="Artifact", name=name).refs, name

    uploaded = cli.artifact_upload(source, name="cli-handbook", version="1.0.0", extract=True)
    assert uploaded.version == "1.0.0"
    listed = cli.artifact_files(name="cli-handbook", version="1.0.0")
    assert {each["relative_path"] for each in listed["artifacts"]} == set(held)
    raw = refusal(lambda: cli.artifact_upload(source, name="cli-handbook", version="1.0.0"))
    assert raw[0] == "WYRD_REGISTRY_409_SPEC_DRIFT", "the raw collection is not the extracted 1.0.0"


def test_uploaded_artifacts_are_reused_by_service_judges(
    cards: Cards, wyrd_server: WyrdTestServer, receiver: Receiver, tmp_path: Path
) -> None:
    marker = f"MARKER-{secrets.token_hex(8)}"
    source = tree(
        tmp_path / "handbook",
        {
            "handbook.md": f"# Staff handbook\nOutages escalate with code {marker}.\n",
            "rota.txt": "never mounted\n",
        },
    )
    handbook = cards.upload_artifact(source, pinned_upload("staff-handbook"), extract=True)
    assert "extracted/handbook.md.json" in inventory(cards, handbook)
    configure_gateway(WyrdClient())
    declare(cards, "cards/uploaded_artifacts/handbook-desk.yaml")
    bundle = pull(cards, "handbook-desk", tmp_path / "desk")
    key = wyrd_server.scoped_api_key("handbook_judge", grants({"tool": "fs.read"}))
    state = WyrdState.from_path(bundle, WyrdClient(credential=key))
    asked = len(receiver.to("/v1/chat/completions"))
    try:
        results = [
            state.run().observe.verify(verifier, question="How do outages escalate?")
            for verifier in ("handbook-review", "handbook-audit")
        ]
    finally:
        state.shutdown()

    for result in results:
        assert result.verdict == "passed", result.requirements
        judged = requirement(result, "handbook-answers")
        assert marker in judged["reason"]
        [citation] = judged.get("citations", [])
        assert citation["slot"].startswith("attachment_"), citation
        assert "lines" in citation["locator"], citation
    judges = [
        each.body
        for each in receiver.to("/v1/chat/completions")[asked:]
        if "HANDBOOK JUDGE" in json.dumps(each.body)
    ]
    assert len(judges) == 4, "each judge reads once and then answers"
    for body in judges:
        assert [tool["function"]["name"] for tool in body["tools"]] == ["fs_read"]
        assert marker not in json.dumps(body["messages"][0]), "only fs.read shows the marker"
    assert not any("rota" in json.dumps(body) for body in judges), "unselected files stay out"


class Probe:
    """A checked-in Service whose ``<service>-check`` Verifier's Agent plays a scripted list of file tool calls.

    The local model makes one scripted call per turn and answers every tool
    result, so each journey reads exactly what the real tools returned.
    """

    def __init__(
        self,
        cards: Cards,
        server: WyrdTestServer,
        home: Path,
        service: str,
        granted: str | dict[str, object] = "all",
    ) -> None:
        """Register ``cards/artifact_workspaces/<service>.yaml`` and run it as a key granted ``granted`` tools."""
        configure_gateway(WyrdClient())
        declare(cards, f"cards/artifact_workspaces/{service}.yaml")
        bundle = pull(cards, service, home)
        key = server.scoped_api_key(service.replace("-", "_"), grants(granted))
        self.verifier = f"{service}-check"
        self.state = WyrdState.from_path(bundle, WyrdClient(credential=key))

    def play(self, *script: tuple[str, dict[str, object]]) -> tuple[list[Any], StepResult]:
        """Run ``script`` and return each call's tool result beside the step record."""
        calls = [{"tool": tool, "arguments": arguments} for tool, arguments in script]
        result = self.state.run().observe.verify(self.verifier, script=calls)
        probe = step(result, "probe")
        assert probe["status"] == "succeeded", probe
        output = cast("dict[str, Any]", probe.get("output"))
        return output["results"], probe


def lines(read: dict[str, Any]) -> list[tuple[int, str]]:
    """Each unit of an ``fs.read`` result as its line number and text."""
    return [(unit["locator"]["lines"]["start"], unit["text"]) for unit in read["units"]]


def hits(found: dict[str, Any]) -> list[tuple[str, int]]:
    """Each ``fs.search`` match as its path and line number."""
    return [(hit["path"], hit["locator"]["lines"]["start"]) for hit in found["matches"]]


def tools_called(record: StepResult) -> list[str]:
    """The tools ``record``'s Agent called, in call order."""
    return [call["tool"] for call in record.get("tool_calls", [])]


def recorded_codes(record: StepResult) -> list[str]:
    """The error codes ``record`` keeps for its failed tool calls, in call order."""
    return [call["error"].split(":")[0] for call in record.get("tool_calls", []) if "error" in call]


def test_artifact_workspace_selects_files_and_directories(
    cards: Cards, wyrd_server: WyrdTestServer, tmp_path: Path
) -> None:
    source = tree(
        tmp_path / "source",
        {
            "docs/guide.md": "# Guide\nfirst release\n",
            "docs/deep/notes.txt": "nested\n",
            "docs-old/stale.txt": "sibling prefix\n",
            "policy.pdf": CORRUPT_PDF,
            "blob.bin": bytes(range(256)),
            "unselected.txt": "x" * 100_000,
        },
    )
    cards.upload_artifact(source, pinned_upload("workspace-library"))
    probe = Probe(cards, wyrd_server, tmp_path / "probe", "library-desk")

    [listed, read, nested] = probe.play(
        ("fs.list", {"path": "/inputs"}),
        ("fs.read", {"path": "/inputs/guide.md"}),
        ("fs.read", {"path": "/inputs/docs/deep/notes.txt"}),
    )[0]

    assert {each["path"]: each["size_bytes"] for each in listed["files"]} == {
        "/inputs/guide.md": 22,
        "/inputs/docs/guide.md": 22,
        "/inputs/docs/deep/notes.txt": 7,
        "/inputs/policy.pdf": len(CORRUPT_PDF),
        "/inputs/blob.bin": 256,
    }
    assert lines(read) == [(1, "# Guide"), (2, "first release")]
    assert lines(nested) == [(1, "nested")]
    assert [len(each["sources"]) for each in listed["files"]] == [1] * 5

    (source / "docs/guide.md").write_text("# Guide\nsecond release\n")
    cards.upload_artifact(source, {"name": "workspace-library", "version": "1.0.1"})
    [read] = probe.play(("fs.read", {"path": "/inputs/guide.md"}))[0]
    assert lines(read)[1] == (2, "first release"), "a new version never retargets the pin"


def tool_refusal(result: dict[str, Any]) -> tuple[object, object]:
    """The code and repair rule of a tool call the tool refused, carried in its result."""
    return result.get("code"), result.get("details", {}).get("rule")


OPAQUE = b"\xff\xfe\x00LIMIT"
"""Bytes no text tool reads, which ``code.run`` still reads from the mount."""

OPAQUE_HEX = "from pathlib import Path\nPath('/inputs/data/blob.bin').read_bytes().hex()"
"""``code.run`` source returning the mounted opaque file's bytes as hex."""


def test_agent_file_tools_read_without_extraction(
    cards: Cards, wyrd_server: WyrdTestServer, tmp_path: Path
) -> None:
    raw = '{\n  "limit": 100,\n  "tiers": ["standard"]\n}\n'
    body = "".join(f"line {number}\n" for number in range(1, 2001))
    source = tree(
        tmp_path / "source",
        {"policy.json": raw, "long.txt": body, "policy.pdf": CORRUPT_PDF, "blob.bin": OPAQUE},
    )
    cards.upload_artifact(source, pinned_upload("raw-records"))
    probe = Probe(cards, wyrd_server, tmp_path / "probe", "raw-desk")

    results, record = probe.play(
        ("fs.read", {"path": "/inputs/data/policy.json"}),
        ("fs.read", {"path": "/inputs/data/long.txt"}),
        ("fs.read", {"path": "/inputs/data"}),
        ("fs.read", {"path": "/inputs/data/blob.bin"}),
        ("fs.read", {"path": "/inputs/data/policy.pdf"}),
        ("fs.search", {"query": "LIMIT"}),
        ("code.run", {"source": OPAQUE_HEX}),
    )
    [policy, first_page, directory, blob, pdf, found, ran] = results

    assert lines(policy) == list(enumerate(raw.splitlines(), start=1))
    assert policy["next"] is None, policy
    page = lines(first_page)
    assert page[0] == (1, "line 1") and 0 < len(page) < 2000, len(page)
    [resumed] = probe.play(
        ("fs.read", {"path": "/inputs/data/long.txt", "start": first_page["next"]})
    )[0]
    assert lines(resumed)[0] == (len(page) + 1, f"line {len(page) + 1}")
    assert [tool_refusal(directory), tool_refusal(blob)] == [
        (VALIDATION, "regular_file"),
        (VALIDATION, "utf8_text"),
    ]
    assert "fs.list" in directory["error"] and "fs.extract" in blob["error"], (directory, blob)
    assert recorded_codes(record) == [VALIDATION, VALIDATION], "the record keeps the model's code"
    assert lines(pdf) == [(1, "%PDF-1.7"), (2, "not a document")], "reading never extracts"
    assert hits(found) == [("/inputs/data/policy.json", 2)], "binary bytes are never searched"
    assert ran["value"] == repr(OPAQUE.hex()), "code.run reads the opaque mounted bytes"
    assert tools_called(record)[-1] == "code.run"


def test_agent_file_search_supports_regex_across_files(
    cards: Cards, wyrd_server: WyrdTestServer, tmp_path: Path
) -> None:
    source = tree(
        tmp_path / "source",
        {
            "src/refunds.py": "def refund():\n    return REFUND_LIMIT\nrefund refund\n",
            "src/deep/rules.txt": "Refund(x) applies\n",
            "src-old/stale.txt": "refund outside the selection\n",
            ".hidden/notes.txt": "refund hidden\n",
            "blob.bin": b"\xff\xfe refund",
            "many.txt": "match\n" * 4000,
        },
    )
    cards.upload_artifact(source, pinned_upload("code-tree"))
    probe = Probe(cards, wyrd_server, tmp_path / "probe", "search-desk")

    results, record = probe.play(
        ("fs.search", {"query": "refund"}),
        ("fs.search", {"query": "refund", "case_sensitive": True}),
        ("fs.search", {"query": "Refund(x)"}),
        ("fs.search", {"query": r"^def \w+\(", "regex": True}),
        ("fs.search", {"query": "refund", "path": "/inputs/src/deep"}),
        ("fs.search", {"query": "refund", "path": "/inputs/src/refunds.py"}),
        ("fs.search", {"query": "refund", "glob": "*.py"}),
        ("fs.search", {"query": "refund", "glob": "inputs/src/**/*.txt"}),
        ("fs.search", {"query": "(", "regex": True}),
        ("fs.search", {"query": "refund", "glob": "["}),
        ("fs.search", {"query": "refund", "path": "/inputs/missing"}),
        ("fs.search", {"query": "no such text"}),
        ("fs.search", {"query": "match"}),
        ("fs.extract", {"path": "/inputs/src/deep/rules.txt", "output_path": "/work/rules.json"}),
        ("fs.search", {"query": "Refund(x)", "path": "/work"}),
        ("fs.read", {"path": "/inputs/src/refunds.py"}),
    )
    [
        insensitive,
        sensitive,
        literal,
        regex,
        subtree,
        single,
        basename,
        recursive,
        bad_regex,
        bad_glob,
        bad_scope,
        none,
        many,
        _,
        scratch,
        context,
    ] = results

    py, txt, hidden = (
        "/inputs/src/refunds.py",
        "/inputs/src/deep/rules.txt",
        "/inputs/.hidden/notes.txt",
    )
    assert hits(insensitive) == [(hidden, 1), (txt, 1), (py, 1), (py, 2), (py, 3)]
    assert hits(sensitive) == [(hidden, 1), (py, 1), (py, 3)]
    assert hits(literal) == [(txt, 1)]
    assert hits(regex) == [(py, 1)]
    assert hits(subtree) == [(txt, 1)]
    assert hits(single) == [(py, 1), (py, 2), (py, 3)]
    assert hits(basename) == [(py, 1), (py, 2), (py, 3)]
    assert hits(recursive) == [(txt, 1)]
    assert [tool_refusal(each) for each in (bad_regex, bad_glob, bad_scope)] == [
        (VALIDATION, "search_pattern"),
        (VALIDATION, "search_glob"),
        (VALIDATION, "workspace_missing"),
    ]
    assert recorded_codes(record) == [VALIDATION] * 3, "the record keeps the model's code"
    assert none == {"matches": [], "truncated": False}
    assert many["truncated"] is True and 0 < len(many["matches"]) < 4000, len(many["matches"])
    [(path, _)] = hits(scratch)
    assert path == "/work/rules.json" and scratch["matches"][0]["sources"] == []
    assert lines(context)[1] == (2, "    return REFUND_LIMIT")
    assert tools_called(record)[-3:] == ["fs.extract", "fs.search", "fs.read"]

    wyrd_server.flush_bifrost()
    query: dict[str, object] = {
        "sql": "SELECT count(*) AS judged FROM bifrost.verification.results "
        "WHERE service_name = 'search-desk'",
        "max_rows": 1,
    }
    [found, read, queried], record = probe.play(
        ("fs.search", {"query": "REFUND_LIMIT"}),
        ("fs.read", {"path": "/inputs/src/refunds.py", "start": 1}),
        ("bifrost.query", query),
    )
    assert hits(found) == [(py, 2)]
    assert lines(read)[0] == (2, "    return REFUND_LIMIT"), "the read starts at the hit"
    assert (queried["columns"][0]["name"], queried["rows"]) == ("judged", [[1]]), (
        "the earlier probe is retained and queryable"
    )
    assert tools_called(record) == ["fs.search", "fs.read", "bifrost.query"]


def test_agent_extract_tool_writes_explicit_output(
    cards: Cards, wyrd_server: WyrdTestServer, tmp_path: Path
) -> None:
    source = tree(
        tmp_path / "source",
        {"handbook.md": "# Handbook\nEscalate.\n", "policy.pdf": CORRUPT_PDF, "blob.bin": b"\x00"},
    )
    cards.upload_artifact(source, pinned_upload("extractable-docs"))
    probe = Probe(cards, wyrd_server, tmp_path / "probe", "extract-desk")

    results, record = probe.play(
        ("fs.extract", {"path": "/inputs/docs/handbook.md", "output_path": "/work/handbook.json"}),
        ("fs.read", {"path": "/work/handbook.json"}),
        ("fs.extract", {"path": "/inputs/docs/handbook.md", "output_path": "/work/handbook.json"}),
        ("fs.extract", {"path": "/inputs/docs/policy.pdf", "output_path": "/work/policy.json"}),
        ("fs.extract", {"path": "/inputs/docs/blob.bin", "output_path": "/work/blob.json"}),
        ("fs.extract", {"path": "/inputs/docs", "output_path": "/work/docs.json"}),
        ("fs.extract", {"path": "/inputs/docs/handbook.md", "output_path": "/inputs/x.json"}),
        ("fs.extract", {"path": "/inputs/docs/handbook.md", "output_path": "/work/../x.json"}),
        ("fs.list", {"path": "/work"}),
    )
    [extracted, read, *refused, listed] = results

    assert extracted["output_path"] == "/work/handbook.json"
    assert len(extracted["sources"]) == 1 and extracted["extractor_version"], extracted
    text = "\n".join(text for _, text in lines(read))
    assert json.loads(text) == {"format": "markdown", "text": "# Handbook\nEscalate.\n"}
    assert all(not unit.get("sources") for unit in read["units"]), "scratch is no citation source"
    assert [tool_refusal(each) for each in refused] == [
        (VALIDATION, "output_exists"),
        (VALIDATION, "extraction_refused"),
        (VALIDATION, "extractable_document"),
        (VALIDATION, "regular_file"),
        (VALIDATION, "output_path"),
        (VALIDATION, "output_path"),
    ]
    assert recorded_codes(record) == [VALIDATION] * 6, "the record keeps the model's code"
    assert [each["path"] for each in listed["files"]] == ["/work/handbook.json"]


def test_artifact_workspace_refusals_preserve_authority(
    cards: Cards,
    wyrd_server: WyrdTestServer,
    receiver: Receiver,
    other_tenant: str,
    tmp_path: Path,
) -> None:
    secret = f"SECRET-{secrets.token_hex(8)}"
    source = tree(tmp_path / "source", {"docs/guide.md": f"{secret}\n", "notes.txt": "n\n"})
    ref = cards.upload_artifact(source, pinned_upload("guarded-docs"))
    asked = len(receiver.to("/v1/chat/completions"))

    refused = {
        case: refusal(lambda case=case: declare(cards, f"cards/artifact_workspaces/{case}.yaml"))[0]
        for case in (
            "wrong-kind-check",
            "missing-card-check",
            "outside-inputs-check",
            "colliding-mounts-check",
            "unknown-field-check",
        )
    }
    assert refused == {
        "wrong-kind-check": "WYRD_LOADER_400_INVALID_ENVELOPE",
        "missing-card-check": "WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY",
        "outside-inputs-check": "WYRD_LOADER_400_INVALID_ENVELOPE",
        "colliding-mounts-check": "WYRD_LOADER_400_INVALID_ENVELOPE",
        "unknown-field-check": "WYRD_LOADER_400_INVALID_ENVELOPE",
    }
    for case in refused:
        assert not cards.list(kind="Verifier", name=case).refs, case
    foreign = Cards(
        WyrdClient(
            credential=wyrd_server.bootstrap_service_in_tenant(
                other_tenant, ["admin"], name="artifact-foreigner"
            )
        )
    )
    assert {
        "declare": refusal(
            lambda: declare(foreign, "cards/artifact_workspaces/denied-desk-check.yaml")
        )[0],
        "inventory": refusal(lambda: foreign.list_artifacts(ref))[0],
    } == {
        "declare": "WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY",
        "inventory": "WYRD_REGISTRY_404_CARD_NOT_FOUND",
    }

    probe = Probe(cards, wyrd_server, tmp_path / "prefix", "prefix-desk")
    code, _ = refusal(lambda: probe.play(("fs.read", {"path": "/inputs/x"})))
    assert code == "WYRD_VERIFICATION_400_INPUT_INVALID", "a prefix stops at a / boundary"

    probe = Probe(cards, wyrd_server, tmp_path / "denied", "denied-desk", {"tool": "fs.search"})
    [read, found, ungranted], record = probe.play(
        ("fs.read", {"path": "/inputs/guide.md"}),
        ("fs.search", {"query": "nothing"}),
        ("fs.extract", {"path": "/inputs/guide.md", "output_path": "/inputs/guide.md"}),
    )
    assert read.get("code") == DENIED, read
    assert found == {"matches": [], "truncated": False}
    assert ungranted.get("code") == DENIED, "a declared tool still needs its grant"
    undeclared = probe.state.run().observe.verify(
        probe.verifier, script=[{"tool": "fs.list", "arguments": {"path": "/inputs"}}]
    )
    assert "is not attached" in step(undeclared, "probe").get("reason", ""), undeclared.steps
    assert all(
        secret not in json.dumps(each.body) for each in receiver.to("/v1/chat/completions")[asked:]
    ), "no refused path hands the protected line to the model"
    assert tools_called(record)[:2] == ["fs.read", "fs.search"]
