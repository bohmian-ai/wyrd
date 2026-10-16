"""A Service judges its components' behavior with ``run.observe.verify`` and ``submit``.

The ``assistant`` Service binds the ``answer-check`` assertion and the
``answer-judged`` Agent check to its ``agent``, and the scheduled
``latency-drift`` window Verifier to its ``model``. Only ``verifier:run`` on
the named Verifier authorizes a verify; the local model provider stands in
for the judge and agrees with every answer, citing it.
"""

import secrets
from pathlib import Path

import httpx
import pytest
from opentelemetry.sdk.trace import TracerProvider
from wyrd import WyrdError, verify
from wyrd.bifrost import Bifrost
from wyrd.cards import Cards
from wyrd.client import WyrdClient
from wyrd.gateway import Gateway
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer, cli

from .support import (
    BASELINE_TIMEOUT_SECONDS,
    Receiver,
    ancestry,
    binding_ids,
    client_of,
    hydrated,
    register,
    scheduled_result,
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


def principal() -> str:
    """The ``assistant`` Service's principal id, which every judgment here is requested as."""
    return cli.issue_key(
        kind="Service", name="assistant", version="1.0.0", space="default"
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
            f"WHERE r.result_id = $1 AND {on} = $2",
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


def test_unfitted_baseline_is_judged_inconclusive_on_schedule(
    unfitted_assistant: WyrdState, wyrd_server: WyrdTestServer, cards: Cards
) -> None:
    unfitted_assistant.start_bifrost()
    observe = unfitted_assistant.run("model").observe
    for _ in range(5):
        observe.submit("tier-drift", "gold")
    unfitted_assistant.shutdown()
    [binding_id] = binding_ids(cards, unfitted_assistant.root_ref)

    result = scheduled_result(wyrd_server, binding_id, BASELINE_TIMEOUT_SECONDS)

    assert result.verdict == "inconclusive"
    assert result.verifier.name == "tier-drift"


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
    evals_only = wyrd_server.scoped_api_key("py_evals_only", ["evals:run"])
    other = wyrd_server.scoped_api_key("py_other_verifier", [verifier_run(cards, "answer-judged")])
    for key in [evals_only, other]:
        state = hydrated(assistant_bundle, WyrdClient(credential=key))
        with pytest.raises(WyrdError) as raised:
            state.run("agent").observe.verify("answer-check", "yes")
        assert raised.value.code == DENIED


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
