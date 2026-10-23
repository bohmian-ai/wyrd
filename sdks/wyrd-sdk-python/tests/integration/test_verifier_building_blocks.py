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
then answers a 100-dollar limit, the policy judge passes every answer with a
citation unless the answer names a scripted case, and the ``tool-review``
investigator calls the Card, Bifrost, and code tools at once and then
finishes. Scripted answer texts also fail the reader's provider, schema, and
deadline and the code step and check; scoped and delegated principals
exercise every tool grant.
"""

from __future__ import annotations

import json
from collections.abc import Iterator, Sequence
from pathlib import Path
from typing import TYPE_CHECKING, Any, cast

import pytest
from opentelemetry import trace
from pydantic import BaseModel, RootModel
from wyrd import WyrdError, verify
from wyrd.auth import Principals
from wyrd.bifrost import Bifrost
from wyrd.cards import Cards
from wyrd.client import WyrdClient
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer

from .support import (
    FIXTURES,
    Receiver,
    client_of,
    declare,
    pull,
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


class RetainedOutcome(BaseModel):
    """One retained result's outcome as JSON text."""

    outcome: str


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
    desk: WyrdState, wyrd_server: WyrdTestServer, receiver: Receiver, fresh_tracer_provider: None
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
    assert [call["tool"] for call in rules["tool_calls"]] == ["fs.read"]
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
    desk: WyrdState, wyrd_server: WyrdTestServer, fresh_tracer_provider: None
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

    resumed = verify.Execution.connect(execution.id, WyrdClient(credential=policy_desk_key))
    result = resumed.wait(WAIT_SECONDS)
    status = resumed.status()

    assert status["status"] == "completed"
    assert status["result"]["execution_id"] == execution.id
    assert "error" not in status
    assert result.verdict == "failed"
    assert requirement(result, "refund-within-limit")["outcome"] == "failed"


def test_every_format_extracts_its_locked_output(desk: WyrdState) -> None:
    evidence = FIXTURES / "cards/verifier_building_blocks/samples"
    for name, expected in DOCUMENTS.items():
        result = desk.run().observe.verify("document-formats", verify.File(evidence / name))

        assert step(result, "read")["output"] == expected, name
        assert requirement(result, "extracted")["outcome"] == "passed", name


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
    desk: WyrdState, wyrd_server: WyrdTestServer, fresh_tracer_provider: None
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
    actor = WyrdClient(
        credential=wyrd_server.scoped_api_key("tool_delegate", grants({"tool": TOOLS[0]}))
    )
    delegated = actor.on_behalf_of(subject.access_token(), audience="wyrd")

    result = investigate(policy_desk_bundle, delegated, "delegated")

    assert_tools(result, receiver, "delegated", (TOOLS[0],))
    wyrd_server.flush_bifrost()
    attributed = Bifrost(client=client_of(wyrd_server)).sql(
        "SELECT sub_principal_id, act_principal_id "
        "FROM vala.verification.results WHERE execution_id = $1",
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

    assert retained(wyrd_server, waited.execution_id) == status["result"]["outcome"]

    for marker, data in EXACT_DATA.items():
        text = f"{marker} refunds"
        assert requirement(scored_review(desk, text), "well-formed")["data"] == data, marker
        execution = submit_scored(desk, text)
        waited = execution.wait(WAIT_SECONDS)
        assert requirement(waited, "well-formed")["data"] == data, marker
        status = execution.status()
        assert status["result"]["outcome"]["requirements"] == waited.requirements, marker
        assert retained(wyrd_server, waited.execution_id) == status["result"]["outcome"], marker


def retained(wyrd_server: WyrdTestServer, execution_id: str) -> object:
    """The retained outcome of ``execution_id``, read back from Bifrost after publishing."""
    wyrd_server.flush_bifrost()
    [row] = Bifrost(client=client_of(wyrd_server)).sql(
        "SELECT to_json(outcome) AS outcome FROM vala.verification.results WHERE execution_id = $1",
        [execution_id],
        model=RetainedOutcome,
    )
    return json.loads(row.outcome)


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
