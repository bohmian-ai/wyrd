"""A Service judges its components' behavior with ``run.observe.verify`` and ``submit``.

The ``assistant`` Service binds the ``answer-check`` assertion and the
``answer-judged`` Agent check to its ``agent``, and the scheduled
``latency-drift`` window Verifier to its ``model``. Only ``verifier:run`` on
the named Verifier authorizes a verify; the local model provider stands in
for the judge and agrees with every answer, citing it.
"""

from __future__ import annotations

import json
import secrets
import time
from pathlib import Path

import httpx
import pytest
from opentelemetry import trace
from opentelemetry.sdk.trace import TracerProvider
from wyrd import WyrdError, verify
from wyrd.auth import Principals
from wyrd.bifrost import Bifrost
from wyrd.cards import Cards
from wyrd.client import WyrdClient
from wyrd.gateway import Gateway
from wyrd.operators import OperatorConnections
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer, cli

from .support import (
    BASELINE_TIMEOUT_SECONDS,
    FRAME_CLAIM,
    LIVE_REVIEW,
    UNBADGED,
    Receiver,
    Retained,
    ancestry,
    binding_ids,
    binding_status,
    client_of,
    download,
    group_binding,
    group_execution,
    hydrated,
    register,
    service_key,
    timeline,
)

pytestmark = pytest.mark.integration

WAIT_SECONDS = 90.0
"""How long a test waits for a queued execution."""

DENIED = "WYRD_PERMISSION_403_DENIED_RBAC"

CALLS = "/v1/chat/completions"
"""The local model provider's route every judge call reaches."""


def verifier_run(cards: Cards, verifier: str) -> dict[str, object]:
    """The ``verifier:run`` grant on exactly the registered ``verifier``."""
    uid = register(cards, f"cards/verify_in_real_time/{verifier}.yaml")[verifier].uid
    return {"resource": "verifier", "action": "run", "scope": {"verifier": str(uid)}}


def principal(service: str = "assistant") -> str:
    """The ``service`` principal id; ``assistant``'s is the one most judgments here are requested as."""
    return cli.issue_key(
        kind="Service", name=service, version="1.0.0", space="default"
    ).principal_id


def result_with_its_judge_calls(
    bifrost: Bifrost, execution_id: str, run_id: str | None, call_id: str | None = None
) -> list[dict]:
    """The result ``execution_id`` wrote beside the judge calls it joins on its Card.

    Calls are narrowed to ``run_id``, or to ``call_id`` for a judgment without
    a Run, so other stories' calls on the same Card do not join.
    """
    on, value = ("c.run_id", run_id) if call_id is None else ("c.call_id", call_id)
    return (
        bifrost.sql(
            "SELECT r.sub_principal_id AS result_subject, r.act_principal_id AS result_actor, "
            "r.run_id AS result_run, c.sub_principal_id AS call_subject, "
            "c.act_principal_id AS call_actor, c.run_id AS call_run "
            "FROM vala.verification.results r "
            "JOIN vala.gateway.calls c ON c.card_uid = r.card_uid "
            f"WHERE r.execution_id = $1 AND {on} = $2",
            [execution_id, value],
        )
        .to_arrow()
        .to_pylist()
    )


def assert_called_as_system(joined: list[dict], caller: str, run_id: str | None) -> None:
    """Assert one result and judge call, both for ``caller``: the verify by ``caller``, the call by SYSTEM."""
    [row] = joined
    assert (row["result_subject"], row["call_subject"], row["result_run"], row["call_run"]) == (
        caller,
        caller,
        run_id,
        run_id,
    ), row
    assert row["result_actor"] == caller, "the caller performs its direct verify"
    assert row["call_actor"] not in (None, caller), "SYSTEM performs the judgment"


def verify_over_http(server: WyrdTestServer, cards: Cards, traceparent: str) -> str:
    """Judge ``yes`` with ``answer-judged`` over plain HTTP as the Service, with no Run.

    Returns the result's ``execution_id``.
    """
    registered = register(cards, "cards/verify_in_real_time/assistant.yaml")
    token = WyrdClient(credential=service_key("assistant")).access_token()
    response = httpx.post(
        f"{server.base_url}/v1/verification/execute",
        headers={"x-wyrd-access-token": f"Bearer {token}", "traceparent": traceparent},
        json={
            "verifier_card_uid": str(registered["answer-judged"].uid),
            "card_uid": str(registered["assistant-agent"].uid),
            "inputs": {"answer": {"kind": "json", "value": "yes"}},
        },
        timeout=60,
    )
    result = response.json()
    assert response.status_code == 200, result
    assert result["outcome"]["verdict"] == "passed", result
    return result["execution_id"]


def test_agent_answer_passes_its_verifier(assistant: WyrdState) -> None:
    run = assistant.run("agent")

    result = run.observe.verify("answer-check", "yes")

    assert result.verdict == "passed"
    assert result.verifier.name == "answer-check"
    assert result.subject == assistant.card_ref("agent")
    assert result.run_id == run.run_id
    [requirement] = result.requirements
    assert (requirement["id"], requirement["outcome"]) == ("says-yes", "passed")


def test_agent_answer_fails_its_verifier(assistant: WyrdState) -> None:
    result = assistant.run("agent").observe.verify("answer-check", "no")

    assert result.verdict == "failed"
    [requirement] = result.requirements
    assert (requirement["outcome"], requirement["actual"]) == ("failed", "no")


def test_judged_answer_passes_the_agent_check(assistant: WyrdState, receiver: Receiver) -> None:
    asked = len(receiver.to(CALLS))

    result = assistant.run("agent").observe.verify("answer-judged", "yes")

    assert result.verdict == "passed"
    [requirement] = result.requirements
    assert requirement["outcome"] == "passed"
    assert requirement["citations"] == [{"slot": "answer", "locator": {"json_pointer": ""}}]
    assert requirement["provenance"]["kind"] == "agent"
    assert requirement["provenance"]["agent"]["name"] == "judge-agent"
    [judged] = receiver.to(CALLS)[asked:]
    assert "Claim: The answer agrees with the customer" in str(judged.body), judged.body


def test_judge_call_is_attributed_to_the_run_and_card(
    assistant: WyrdState, wyrd_server: WyrdTestServer, bifrost: Bifrost, metadata_capture: None
) -> None:
    run = assistant.run("agent")
    result = run.observe.verify("answer-judged", "yes")
    wyrd_server.flush_bifrost()

    caller = principal()
    calls = (
        bifrost.sql(
            "SELECT sub_principal_id, act_principal_id, card_uid FROM vala.gateway.calls "
            "WHERE run_id = $1",
            [run.run_id],
        )
        .to_arrow()
        .to_pylist()
    )
    [call] = calls
    assert (call["sub_principal_id"], call["card_uid"]) == (caller, str(result.subject.uid))
    assert call["act_principal_id"] not in (None, caller)
    assert_called_as_system(
        result_with_its_judge_calls(bifrost, result.execution_id, run.run_id), caller, run.run_id
    )


def test_judge_call_without_a_run_joins_the_callers_trace_attributed_to_the_card(
    assistant_bundle: Path,
    wyrd_server: WyrdTestServer,
    cards: Cards,
    bifrost: Bifrost,
    metadata_capture: None,
) -> None:
    trace_id, span_id = secrets.token_hex(16), secrets.token_hex(8)

    execution_id = verify_over_http(wyrd_server, cards, f"00-{trace_id}-{span_id}-01")

    spans = timeline(wyrd_server, trace_id)
    [attempt] = [span for span in spans if span["scope"]["name"] == "wyrd.gateway"]
    assert ancestry(spans, attempt)[-1].get("parent_span_id") == span_id, spans
    call_id = attempt["attributes"]["gateway_call_id"]
    [call] = (
        bifrost.sql(
            "SELECT sub_principal_id, run_id, card_uid FROM vala.gateway.calls WHERE call_id = $1",
            [call_id],
        )
        .to_arrow()
        .to_pylist()
    )
    caller = principal()
    agent = register(cards, "cards/verify_in_real_time/assistant.yaml")["assistant-agent"]
    assert (call["sub_principal_id"], call["run_id"], call["card_uid"]) == (
        caller,
        None,
        str(agent.uid),
    )
    assert_called_as_system(
        result_with_its_judge_calls(bifrost, execution_id, None, call_id), caller, None
    )


def test_traced_sdk_verify_joins_the_callers_trace(
    assistant: WyrdState, wyrd_server: WyrdTestServer, metadata_capture: None
) -> None:
    tracer = TracerProvider().get_tracer("journey")
    with tracer.start_as_current_span("caller") as caller:
        assistant.run("agent").observe.verify("answer-judged", "yes")

    context = caller.get_span_context()
    spans = timeline(wyrd_server, format(context.trace_id, "032x"))
    attempts = [span for span in spans if span["scope"]["name"] == "wyrd.gateway"]
    assert len(attempts) == 1, spans
    assert ancestry(spans, attempts[0])[-1].get("parent_span_id") == format(
        context.span_id, "016x"
    ), [(span["name"], span["span_id"], span.get("parent_span_id")) for span in spans]


def test_traced_sdk_submit_links_its_attempt_to_the_callers_trace(
    assistant: WyrdState, wyrd_server: WyrdTestServer, metadata_capture: None
) -> None:
    tracer = TracerProvider().get_tracer("journey")
    with tracer.start_as_current_span("caller") as caller:
        execution = assistant.run("agent").observe.submit("answer-judged", "yes")
    assert isinstance(execution, verify.Execution)
    execution.wait(WAIT_SECONDS)

    trace_id = format(caller.get_span_context().trace_id, "032x")
    spans = timeline(wyrd_server, trace_id)
    [attempt] = [span for span in spans if span["name"] == "verification.attempt"]
    assert attempt["trace_id"] != trace_id, attempt
    assert [link["trace_id"] for link in attempt["links"]] == [trace_id], attempt
    judged = [span for span in spans if span["scope"]["name"] == "wyrd.gateway"]
    assert len(judged) == 1, spans
    assert ancestry(spans, judged[0])[-1]["span_id"] == attempt["span_id"], [
        (span["name"], span["trace_id"], span["span_id"], span.get("parent_span_id"))
        for span in spans
    ]


def test_malformed_traceparent_is_ignored_by_the_judge_call(
    assistant_bundle: Path,
    wyrd_server: WyrdTestServer,
    cards: Cards,
    receiver: Receiver,
    metadata_capture: None,
) -> None:
    trace_id = secrets.token_hex(16)
    asked = len(receiver.to(CALLS))

    verify_over_http(wyrd_server, cards, f"00-{trace_id}-{secrets.token_hex(8)}")

    assert len(receiver.to(CALLS)) == asked + 1
    assert timeline(wyrd_server, trace_id) == [], "nothing joined the caller's trace"


def test_sdk_requests_carry_the_active_trace(wyrd_server: WyrdTestServer) -> None:
    gateway = Gateway(client_of(wyrd_server))
    tracer = TracerProvider().get_tracer("journey")
    with tracer.start_as_current_span("caller") as caller:
        gateway.capture_policy()

    context = caller.get_span_context()
    spans = timeline(wyrd_server, format(context.trace_id, "032x"))
    assert [
        span["name"]
        for span in spans
        if span.get("parent_span_id") == format(context.span_id, "016x")
    ] == ["request"], [
        (span["name"], span["span_id"], span.get("parent_span_id")) for span in spans
    ]


def test_submitted_answer_is_judged_in_the_background_and_resumes_by_id(
    assistant: WyrdState, assistant_key: str
) -> None:
    run = assistant.run("agent")

    execution = run.observe.submit("answer-check", "no")
    assert isinstance(execution, verify.Execution)
    waited = execution.wait(WAIT_SECONDS)
    resumed = verify.Execution.connect(execution.id, WyrdClient(credential=assistant_key)).wait(
        WAIT_SECONDS
    )

    assert waited.verdict == "failed"
    assert waited.execution_id == execution.id
    assert waited.run_id == run.run_id
    assert (resumed.execution_id, resumed.verdict, resumed.requirements) == (
        waited.execution_id,
        waited.verdict,
        waited.requirements,
    )


def test_another_tenant_cannot_resume_an_execution(
    assistant: WyrdState, wyrd_server: WyrdTestServer, other_tenant: str
) -> None:
    execution = assistant.run("agent").observe.submit("answer-check", "yes")
    assert isinstance(execution, verify.Execution)
    execution.wait(WAIT_SECONDS)
    foreign = wyrd_server.bootstrap_service_in_tenant(other_tenant, ["admin"], name="other-resumer")

    with pytest.raises(WyrdError) as refused:
        verify.Execution.connect(execution.id, WyrdClient(credential=foreign)).wait(2.0)
    assert refused.value.code == "WYRD_SDK_504_VERIFICATION_WAIT_TIMEOUT"


def test_window_verifier_records_samples_and_refuses_verify(assistant: WyrdState) -> None:
    assistant.start_bifrost()
    observe = assistant.run("model").observe

    with pytest.raises(WyrdError) as refused:
        observe.verify("latency-drift", {"latency": 12.5})
    submitted = observe.submit("latency-drift", {"latency": 12.5})
    assistant.shutdown()

    assert refused.value.code == "WYRD_VERIFICATION_422_INPUT_UNSUPPORTED"
    assert isinstance(submitted, verify.Recorded)


def test_unfitted_baseline_is_skipped_on_schedule(
    unfitted_assistant: WyrdState, wyrd_server: WyrdTestServer, cards: Cards
) -> None:
    unfitted_assistant.start_bifrost()
    observe = unfitted_assistant.run("model").observe
    for _ in range(5):
        observe.submit("tier-drift", "gold")
    unfitted_assistant.shutdown()
    [binding_id] = binding_ids(cards, unfitted_assistant.root_ref)
    wyrd_server.make_binding_due(binding_id)
    due = binding_status(wyrd_server, binding_id)["next_run_at"]

    deadline = time.monotonic() + BASELINE_TIMEOUT_SECONDS
    while (status := binding_status(wyrd_server, binding_id))["next_run_at"] == due:
        assert time.monotonic() < deadline, "the due occurrence was not scheduled"
        time.sleep(0.5)

    assert status["readiness"] == "baseline_not_ready"
    assert status.get("last_execution_id") is None


def test_unbound_verifier_fails_locally(assistant: WyrdState) -> None:
    with pytest.raises(WyrdError) as raised:
        assistant.run("agent").observe.verify("not-bound-here", "yes")
    assert raised.value.code == "WYRD_SDK_404_UNKNOWN_VERIFIER"


def test_input_that_does_not_fit_its_slot_fails_locally(assistant: WyrdState) -> None:
    observe = assistant.run("agent").observe
    with pytest.raises(WyrdError) as unknown:
        observe.verify("answer-check", reply="yes")
    with pytest.raises(WyrdError) as file:
        observe.verify("answer-check", answer=verify.File(b"yes"))
    assert unknown.value.code == file.value.code == "WYRD_VERIFICATION_400_INPUT_INVALID"


def test_caller_without_the_verifier_grant_is_refused(
    assistant_bundle: Path, wyrd_server: WyrdTestServer, cards: Cards
) -> None:
    operators_only = wyrd_server.scoped_api_key("py_operators_only", ["operators:invoke"])
    other = wyrd_server.scoped_api_key("py_other_verifier", [verifier_run(cards, "answer-judged")])
    for key in [operators_only, other]:
        state = hydrated(assistant_bundle, WyrdClient(credential=key))
        with pytest.raises(WyrdError) as raised:
            state.run("agent").observe.verify("answer-check", "yes")
        assert raised.value.code == DENIED


def test_retired_verification_contracts_are_refused(
    assistant_bundle: Path, wyrd_server: WyrdTestServer, bifrost: Bifrost
) -> None:
    for action in ["run", "invoke"]:
        stale = wyrd_server.scoped_api_key(
            f"py_evals_{action}", [{"resource": "evals", "action": action, "scope": "all"}]
        )
        state = hydrated(assistant_bundle, WyrdClient(credential=stale))
        with pytest.raises(WyrdError) as raised:
            state.run("agent").observe.verify("answer-check", "yes")
        assert raised.value.code == "WYRD_SPEC_500_INTERNAL", action

    assert all(table["namespace"] != "vala.eval" for table in bifrost.list_tables())

    def refusals(namespace: str, name: str) -> list[str]:
        codes = []
        for call in (
            lambda: bifrost.sql(f"SELECT * FROM {namespace}.{name}"),
            lambda: bifrost.describe_table(namespace, name),
            lambda: Bifrost().use_table_by_name(f"{namespace}.{name}"),
        ):
            with pytest.raises(WyrdError) as raised:
                call()
            codes.append(raised.value.code)
        return codes

    for name in ["result_items", "observations"]:
        assert refusals("vala.eval", name) == refusals("vala.absent", name), name
    bifrost.sql("SELECT COUNT(*) AS n FROM vala.verification.results")


def test_exact_verifier_grant_verifies_only_that_verifier(
    assistant_bundle: Path, wyrd_server: WyrdTestServer, cards: Cards
) -> None:
    key = wyrd_server.scoped_api_key("py_exact_verifier", [verifier_run(cards, "answer-check")])
    run = hydrated(assistant_bundle, WyrdClient(credential=key)).run("agent")
    assert run.observe.verify("answer-check", "yes").verdict == "passed"
    with pytest.raises(WyrdError) as raised:
        run.observe.verify("answer-judged", "yes")
    assert raised.value.code == DENIED


def test_another_tenant_cannot_verify_the_assistant(
    assistant_bundle: Path, wyrd_server: WyrdTestServer, other_tenant: str
) -> None:
    other = wyrd_server.bootstrap_service_in_tenant(other_tenant, ["admin"], name="other-service")
    run = hydrated(assistant_bundle, WyrdClient(credential=other)).run("agent")
    for verifier in ["answer-check", "answer-judged"]:
        with pytest.raises(WyrdError) as raised:
            run.observe.verify(verifier, "yes")
        assert raised.value.code == "WYRD_VERIFICATION_404_TARGET_NOT_FOUND"


def enqueue_over_http(server: WyrdTestServer, key: str, idempotency_key: str, body: dict) -> str:
    """Enqueue ``body`` over plain HTTP as ``key`` with ``idempotency_key``; return the execution."""
    response = httpx.post(
        f"{server.base_url}/v1/verification/enqueue",
        headers={
            "x-wyrd-access-token": f"Bearer {WyrdClient(credential=key).access_token()}",
            "idempotency-key": idempotency_key,
        },
        json=body,
        timeout=60,
    )
    accepted = response.json()
    assert response.status_code == 202, accepted
    return accepted["execution_id"]


def test_grouped_support_resumes(wyrd_server: WyrdTestServer, cards: Cards, tmp_path: Path) -> None:
    registered = register(cards, "cards/verify_in_real_time/support.yaml")
    bundle = download(cards, "support", tmp_path)
    key = service_key("support")
    binding_id = group_binding(wyrd_server, cards, registered["support"])
    state = WyrdState.from_path(bundle, WyrdClient(credential=key))
    state.start_bifrost()
    group = f"conv-{secrets.token_hex(4)}"
    turns = [{"turn": turn, "answer": f"answer {turn}"} for turn in range(1, 7)]
    first = state.run("agent")
    for turn in turns[:3]:
        first.observe.verify("turn-check", turn=turn, group_id=group)
    resumed = state.run("agent")
    resumed.observe.verify("turn-check", turns[3], group_id=group)
    queued = resumed.observe.submit("turn-check", turns[4], group_id=group)
    assert isinstance(queued, verify.Execution)
    queued.wait(WAIT_SECONDS)

    def body(turn: dict) -> dict:
        return {
            "verifier_card_uid": str(registered["turn-check"].uid),
            "card_uid": str(registered["assistant-agent"].uid),
            "group_id": group,
            "inputs": {"turn": {"kind": "json", "value": turn}},
        }

    idempotency_key = f"support-turn-{group}"
    replayed = enqueue_over_http(wyrd_server, key, idempotency_key, body(turns[5]))
    assert enqueue_over_http(wyrd_server, key, idempotency_key, body(turns[5])) == replayed
    conflict = enqueue_over_http(wyrd_server, key, idempotency_key, body({"answer": ""}))
    admin = client_of(wyrd_server)
    with pytest.raises(WyrdError) as conflicted:
        verify.Execution.connect(conflict, admin).wait(WAIT_SECONDS)
    assert conflicted.value.code == "WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT"
    verify.Execution.connect(replayed, admin).wait(WAIT_SECONDS)
    WyrdState.from_path(bundle, admin).run("agent").observe.verify(
        "turn-check", {"answer": ""}, group_id=group
    )
    other = wyrd_server.scoped_api_key("py_group_other", [verifier_run(cards, "answer-check")])
    with pytest.raises(WyrdError) as denied:
        WyrdState.from_path(bundle, WyrdClient(credential=other)).run("agent").observe.verify(
            "turn-check", {"answer": ""}, group_id=group
        )
    assert denied.value.code == DENIED
    state.shutdown()

    judged = group_execution(wyrd_server, binding_id, WAIT_SECONDS)

    assert (judged["group_id"], judged["owner_card_uid"]) == (group, str(registered["support"].uid))
    coverage = judged["group"]
    assert coverage["members"] == 12, "six inputs and six results"
    assert len(coverage["sources"]["turns"]["execution_ids"]) == 6
    assert coverage["sources"]["turns"]["execution_ids"][5] == replayed
    outcome = judged["result"]["outcome"]
    assert outcome["evidence"]["turns"] == {"kind": "json", "value": turns}
    assert outcome["verdict"] == "passed"
    assert binding_status(wyrd_server, binding_id)["last_execution_id"] == judged["execution_id"]
    with pytest.raises(WyrdError) as refused:
        register(cards, "cards/verify_in_real_time/support-unsourced.yaml")
    assert refused.value.code == "WYRD_SPEC_400_TRIGGER_ACTIVATION_MISMATCH"
    assert "verified_by[0].runs_on" in str(refused.value)


def test_grouped_refund_changes_verdict(
    wyrd_server: WyrdTestServer, cards: Cards, receiver: Receiver, tmp_path: Path
) -> None:
    admin = client_of(wyrd_server)
    OperatorConnections(admin).create(
        {
            "provider": "http",
            "name": "refund-hooks",
            "origin": receiver.url,
            "auth": {"scheme": "bearer", "token": "refund-hook-token"},
        }
    )
    registered = register(cards, "cards/verify_in_real_time/refund.yaml")
    bundle = download(cards, "refund", tmp_path)
    binding_id = group_binding(wyrd_server, cards, registered["refund"])
    state = WyrdState.from_path(bundle, WyrdClient(credential=service_key("refund")))
    state.start_bifrost()
    group = f"order-{secrets.token_hex(4)}"
    opened = time.monotonic()

    def turn(overdue: bool) -> None:
        state.run("agent").observe.verify(
            "refund-turn", {"order": "A-1", "overdue": overdue}, group_id=group
        )

    turn(False)
    turn(False)
    promised = group_execution(wyrd_server, binding_id, WAIT_SECONDS)
    for overdue in [False, True, False]:
        turn(overdue)
    corrected = group_execution(wyrd_server, binding_id, WAIT_SECONDS, promised["execution_id"])
    turn(False)
    unchanged = group_execution(wyrd_server, binding_id, WAIT_SECONDS, corrected["execution_id"])

    assert [
        (
            len(review["group"]["sources"]["facts"]["execution_ids"]),
            review["result"]["outcome"]["verdict"],
            len(review["dispatches"]),
        )
        for review in [promised, corrected, unchanged]
    ] == [(2, "passed", 0), (5, "failed", 1), (6, "failed", 0)], (
        "only the verdict change dispatches"
    )
    history = (
        Bifrost(client=admin)
        .sql(
            "SELECT execution_id, result_id, verdict FROM vala.verification.results "
            "WHERE binding_id = $1 ORDER BY ended_at",
            [binding_id],
        )
        .to_arrow()
        .to_pylist()
    )
    assert [(row["execution_id"], row["verdict"]) for row in history] == [
        (promised["execution_id"], "passed"),
        (corrected["execution_id"], "failed"),
        (unchanged["execution_id"], "failed"),
    ], "every review stays retained"
    alert = receiver.wait_for("/hooks/refund-review", WAIT_SECONDS)
    assert alert.body == {
        "execution_id": corrected["execution_id"],
        "result_id": history[1]["result_id"],
        "verdict": "failed",
        "group_id": group,
        "previous_result_id": history[0]["result_id"],
        "previous_verdict": "passed",
    }

    # The fixture's 40 s finality, counted from the group's first turn, must
    # pass before the late turn; waiting for it is the scenario.
    time.sleep(max(0.0, opened + 41 - time.monotonic()))
    turn(True)
    state.shutdown()
    quiet = time.monotonic() + 8
    while time.monotonic() < quiet:
        wyrd_server.flush_bifrost()
        status = binding_status(wyrd_server, binding_id)
        assert status["last_execution_id"] == unchanged["execution_id"], "late turns open no review"
        time.sleep(0.5)
    assert len(receiver.to("/hooks/refund-review")) == 1
    with pytest.raises(WyrdError) as refused:
        register(cards, "cards/verify_in_real_time/refund-on-failure.yaml")
    assert refused.value.code == "WYRD_SPEC_400_TRIGGER_ACTIVATION_MISMATCH"
    assert "verified_by[1].on_failure" in str(refused.value)


@pytest.mark.usefixtures("assistant_bundle")
def test_grouped_coding_uses_declared_sources(
    wyrd_server: WyrdTestServer,
    cards: Cards,
    receiver: Receiver,
    tmp_path: Path,
    fresh_tracer_provider: None,
) -> None:
    registered = register(cards, "cards/verify_in_real_time/coding.yaml")
    bundle = download(cards, "coding", tmp_path)
    binding_id = group_binding(wyrd_server, cards, registered["coding"])
    wyrd_server.scoped_api_key("py_live_reader", ["tool:invoke", "bifrost_query:read"])
    principals = Principals(client_of(wyrd_server))
    coding = principal("coding")
    principals.grant_role(coding, "py_live_reader")
    state = WyrdState.from_path(bundle, WyrdClient(credential=service_key("coding")))
    state.start_bifrost()
    state.start_telemetry()
    tracer = trace.get_tracer("journey.coding")
    session = tracer.start_span("coding-session")
    with trace.use_span(session), tracer.start_as_current_span("edit-file"):
        pass
    trace_id = format(session.get_span_context().trace_id, "032x")
    group = f"session-{secrets.token_hex(4)}"
    calls = [
        {"call": "prompt", "tests": "passed"},
        {"call": "edit", "tests": "failed"},
        {"call": "test", "tests": "passed"},
    ]
    run = state.run("agent")
    verdicts = [
        run.observe.verify(
            "coding-turn", call=call, group_id=group, **({"trace": trace_id} if index == 0 else {})
        ).verdict
        for index, call in enumerate(calls)
    ]
    assert verdicts == ["passed", "failed", "passed"]
    empty = run.observe.verify("empty-each", [])
    assert empty.verdict == "inconclusive"
    assert [
        (requirement["id"], requirement["outcome"], requirement["reason"])
        for requirement in empty.requirements
    ] == [
        ("listed-ok", "inconclusive", "empty_selection"),
        ("items-ok", "inconclusive", "empty_selection"),
    ]
    with trace.use_span(session), tracer.start_as_current_span("late-test-run"):
        pass
    session.end()
    state.flush()

    judged = group_execution(wyrd_server, binding_id, WAIT_SECONDS)

    outcome = judged["result"]["outcome"]
    assert outcome["evidence"]["calls"] == {"kind": "json", "value": calls}
    assert outcome["evidence"]["traces"] == {"kind": "json", "value": [trace_id, None, None]}
    assert outcome["evidence"]["tests"] == {
        "kind": "json",
        "value": ["passed", "failed", "passed"],
    }
    assert [
        (requirement["id"], requirement["outcome"]) for requirement in outcome["requirements"]
    ] == [("every-call-tested", "failed"), ("spans-reviewed", "passed")], outcome["requirements"]
    assert outcome["verdict"] == "failed"

    def reviews() -> list[str]:
        return [
            text
            for delivery in receiver.to(CALLS)
            if LIVE_REVIEW in (text := json.dumps(delivery.body)) and trace_id in text
        ]

    [ask] = [text for text in reviews() if '"role": "tool"' not in text]
    assert "edit-file" not in ask, "the trace slot carries its ID only"
    [read] = [text for text in reviews() if '"role": "tool"' in text]
    assert "edit-file" in read and "late-test-run" in read, "the check reads the spans live"

    principals.revoke_role(coding, "py_live_reader")
    later = f"session-{secrets.token_hex(4)}"
    run.observe.verify("coding-turn", call=calls[0], trace=trace_id, group_id=later)
    revoked = group_execution(wyrd_server, binding_id, WAIT_SECONDS, judged["execution_id"])
    assert revoked["group_id"] == later
    [denied] = [text for text in reviews() if '"role": "tool"' in text and DENIED in text]
    assert "edit-file" not in denied, "a denied read returns nothing"
    state.shutdown()

    for name, kind, code, reason in [
        ("coding-child", "Service", "WYRD_LOADER_400_INVALID_ENVELOPE", "child fields"),
        ("coding-unbound", "Service", "WYRD_SPEC_400_VALIDATION", "matches 0 bindings"),
        ("support-ambiguous", "Service", "WYRD_SPEC_400_VALIDATION", "matches 2 bindings"),
        ("support-undeclared", "Service", "WYRD_SPEC_400_VALIDATION", "does not declare"),
        ("coding-cycle", "Service", "WYRD_SPEC_400_VALIDATION", "forms a source cycle"),
        ("reserved-typed", "Verifier", "WYRD_LOADER_400_INVALID_ENVELOPE", "inputs.group_id"),
        ("reserved-source", "Verifier", "WYRD_LOADER_400_INVALID_ENVELOPE", "inputs.group_id"),
    ]:
        with pytest.raises(WyrdError) as refused:
            register(cards, f"cards/verify_in_real_time/{name}.yaml")
        assert refused.value.code == code, name
        assert reason in f"{refused.value} {refused.value.details}", name
        with pytest.raises(WyrdError) as absent:
            cards.resolve_latest(kind, "default", name)
        assert absent.value.code == "WYRD_REGISTRY_404_CARD_NOT_FOUND", name
    register(cards, "cards/verify_in_real_time/coding-external.yaml")


def frame(directory: Path, label: str) -> Path:
    """A PNG frame in ``directory`` carrying the nine-byte ``label`` after its signature and a newline.

    Nine bytes start the label on a base64 boundary, so its base64 appears
    verbatim in the image data URL the judge receives.
    """
    assert len(label) == 9, "a frame label is nine bytes"
    path = directory / f"{label}.png"
    path.write_bytes(b"\x89PNG\r\n\x1a\n\n" + label.encode())
    return path


def submit_frames(state: WyrdState, group: str, paths: list[Path]) -> list[verify.Execution]:
    """Submit each frame at ``paths`` to ``frame-verifier`` in one Run under ``group``."""
    run = state.run("agent")
    executions = []
    for path in paths:
        queued = run.observe.submit("frame-verifier", verify.File(path), group_id=group)
        assert isinstance(queued, verify.Execution)
        executions.append(queued)
    return executions


@pytest.mark.usefixtures("assistant_bundle")
def test_grouped_sampled_frames_are_bounded(
    wyrd_server: WyrdTestServer, cards: Cards, receiver: Receiver, tmp_path: Path
) -> None:
    registered = register(cards, "cards/verify_in_real_time/frames.yaml")
    register(cards, "cards/verify_in_real_time/frames-other.yaml")
    binding_id = group_binding(wyrd_server, cards, registered["frames"])
    bundle = download(cards, "frames", tmp_path / "frames")
    state = WyrdState.from_path(bundle, WyrdClient(credential=service_key("frames")))
    state.start_bifrost()
    other = WyrdState.from_path(
        download(cards, "frames-other", tmp_path / "other"),
        WyrdClient(credential=service_key("frames-other")),
    )
    other.start_bifrost()
    events = {
        name: f"{name}-{secrets.token_hex(4)}" for name in ("empty", "failing", "over", "door")
    }

    def reported(event: str) -> None:
        state.run("agent").observe.verify(
            "event-fact", {"event_id": event, "detected": True}, group_id=event
        )

    def judged_after(after: str | None) -> verify.ExecutionState:
        state.flush()
        return group_execution(wyrd_server, binding_id, WAIT_SECONDS, after)

    def badged(judged: verify.ExecutionState) -> tuple[str, str]:
        outcome = judged["result"]["outcome"]
        [requirement] = outcome["requirements"]
        return outcome["verdict"], requirement["reason"]

    reported(events["empty"])
    judged = judged_after(None)
    assert judged["group_id"] == events["empty"]
    assert badged(judged) == ("inconclusive", "empty_selection")
    assert judged["result"]["outcome"]["evidence"]["outcomes"] == {"kind": "json", "value": []}

    paths = [frame(tmp_path, f"fail-{index:04}") for index in range(19)]
    paths.insert(7, frame(tmp_path, UNBADGED))
    executions = submit_frames(state, events["failing"], paths)
    for foreign in submit_frames(other, events["failing"], [frame(tmp_path, "other-001")]):
        foreign.wait(WAIT_SECONDS)
    unauthorized = wyrd_server.scoped_api_key(
        "py_frames_denied", [verifier_run(cards, "answer-check")]
    )
    with pytest.raises(WyrdError) as denied:
        WyrdState.from_path(bundle, WyrdClient(credential=unauthorized)).run(
            "agent"
        ).observe.submit(
            "frame-verifier", verify.File(frame(tmp_path, "denied-01")), group_id=events["failing"]
        )
    assert denied.value.code == DENIED
    reported(events["failing"])
    for execution in executions:
        execution.wait(WAIT_SECONDS)
    judged = judged_after(judged["execution_id"])
    assert judged["group_id"] == events["failing"]
    coverage = judged["group"]["sources"]
    # Frames ingested in one batch share its ingest time, so their list
    # order is by execution ID rather than by submission.
    listed = coverage["outcomes"]["execution_ids"]
    assert len(listed) == 20
    assert set(listed) == {execution.id for execution in executions}, (
        "only the Service's own frames"
    )
    assert len(coverage["event"]["execution_ids"]) == 1
    expected = ["failed" if id == executions[7].id else "passed" for id in listed]
    outcome = judged["result"]["outcome"]
    assert outcome["evidence"]["outcomes"] == {"kind": "json", "value": expected}
    assert outcome["verdict"] == "failed"

    paths = [frame(tmp_path, f"over-{index:04}") for index in range(21)]
    executions = submit_frames(state, events["over"], paths)
    reported(events["over"])
    for execution in executions:
        execution.wait(WAIT_SECONDS)
    judged = judged_after(judged["execution_id"])
    assert judged["group_id"] == events["over"]
    assert badged(judged) == (
        "inconclusive",
        "source_execution_limit_exceeded: "
        "verifiers.frame-verifier.requirements.person-wearing-badge.outcome "
        "selected more than 20 source executions",
    )

    paths = [frame(tmp_path, f"door-{index:04}") for index in range(20)]
    executions = submit_frames(state, events["door"], paths)
    reported(events["door"])
    for execution in executions:
        execution.wait(WAIT_SECONDS)
    judged = judged_after(judged["execution_id"])
    assert judged["group_id"] == events["door"]
    assert sorted(judged["group"]["sources"]["outcomes"]["execution_ids"]) == sorted(
        execution.id for execution in executions
    )
    assert judged["result"]["outcome"]["evidence"]["outcomes"] == {
        "kind": "json",
        "value": ["passed"] * 20,
    }
    assert judged["result"]["outcome"]["verdict"] == "passed"
    frames_judged = [d for d in receiver.to(CALLS) if FRAME_CLAIM in json.dumps(d.body)]
    assert len(frames_judged) == 20 + 1 + 21 + 20, "every frame is judged once"
    state.shutdown()
    other.shutdown()


def group_reviews(server: WyrdTestServer, binding_id: str, groups: list[str]) -> list[str]:
    """The executions of ``binding_id``'s retained reviews of ``groups``, oldest first."""
    server.flush_bifrost()
    rows = Bifrost(client=client_of(server)).sql(
        "SELECT execution_id FROM vala.verification.results "
        "WHERE binding_id = $1 AND group_id IN ($2, $3) ORDER BY ended_at",
        [binding_id, *groups],
        model=Retained,
    )
    return [row.execution_id for row in rows]


@pytest.mark.usefixtures("assistant_bundle")
def test_grouped_conversations_recover(
    wyrd_server: WyrdTestServer,
    cards: Cards,
    assistant: WyrdState,
    other_tenant: str,
    tmp_path: Path,
) -> None:
    registered = register(cards, "cards/verify_in_real_time/support.yaml")
    key = service_key("support")
    binding_id = group_binding(wyrd_server, cards, registered["support"])
    state = WyrdState.from_path(
        download(cards, "support", tmp_path / "a"), WyrdClient(credential=key)
    )
    state.start_bifrost()
    assistant.start_bifrost()
    suffix = secrets.token_hex(4)
    a, b = f"conv-a-{suffix}", f"conv-b-{suffix}"

    def turn(answer: str) -> dict:
        return {"answer": answer}

    sampled = assistant.run("model").observe.submit("latency-drift", {"latency": 12.5})
    assert isinstance(sampled, verify.Recorded)
    state.run("agent").observe.verify("turn-check", turn("a1"), group_id=a)
    state.run("agent").observe.verify("turn-check", turn("b1"), group_id=b)
    queued = state.run("agent").observe.submit("turn-check", turn("a2"), group_id=a)
    assert isinstance(queued, verify.Execution)
    state.run("agent").observe.verify("turn-check", turn(""), group_id=b)
    solo = state.run("agent").observe.verify("turn-check", turn("solo"))
    queued.wait(WAIT_SECONDS)
    newer = register(cards, "cards/verify_in_real_time/turn-check-newer.yaml")
    late = enqueue_over_http(
        wyrd_server,
        key,
        f"support-newer-{suffix}",
        {
            "verifier_card_uid": str(newer["turn-check"].uid),
            "card_uid": str(registered["assistant-agent"].uid),
            "group_id": a,
            "inputs": {"turn": {"kind": "json", "value": turn("newer")}},
        },
    )
    admin = client_of(wyrd_server)
    verify.Execution.connect(late, admin).wait(WAIT_SECONDS)
    tenant_b = client_of(
        wyrd_server,
        wyrd_server.bootstrap_service_in_tenant(other_tenant, ["admin"], name=f"b-admin-{suffix}"),
    )
    b_cards = Cards(tenant_b)
    register(b_cards, "cards/verify_in_real_time/support.yaml")
    b_key = cli.issue_key(
        kind="Service", name="support", version="1.0.0", space="default", client=tenant_b
    ).key
    b_state = WyrdState.from_path(
        download(b_cards, "support", tmp_path / "b"), WyrdClient(credential=b_key)
    )
    b_state.start_bifrost()
    b_state.run("agent").observe.verify("turn-check", turn("tenant b"), group_id=a)
    for each in (state, assistant, b_state):
        each.shutdown()

    deadline = time.monotonic() + WAIT_SECONDS
    while len(reviews := group_reviews(wyrd_server, binding_id, [a, b])) < 2:
        assert time.monotonic() < deadline, "both conversations are reviewed"
        time.sleep(0.5)
    judged = {}
    for execution_id in reviews:
        review = verify.Execution.connect(execution_id, admin).status()
        outcome = review["result"]["outcome"]
        judged[review["group_id"]] = (
            len(review["group"]["sources"]["turns"]["execution_ids"]),
            outcome["evidence"]["turns"],
            outcome["verdict"],
        )
    assert judged == {
        a: (2, {"kind": "json", "value": [turn("a1"), turn("a2")]}, "passed"),
        b: (2, {"kind": "json", "value": [turn("b1"), turn("")]}, "failed"),
    }, "each conversation judges only its own pinned turns"
    ungrouped = Bifrost(client=admin).sql(
        "SELECT execution_id FROM vala.verification.results "
        "WHERE execution_id = $1 AND group_id IS NULL",
        [solo.execution_id],
        model=Retained,
    )
    assert len(ungrouped) == 1, "the ungrouped turn joins no group"
    quiet = time.monotonic() + 8
    while time.monotonic() < quiet:
        assert group_reviews(wyrd_server, binding_id, [a, b]) == reviews, "idle ticks review once"
        time.sleep(0.5)
