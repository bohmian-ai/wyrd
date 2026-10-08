"""A Service judges its components' behavior in real time with ``run.observe.verify``.

The ``assistant`` Service binds two Eval Verifiers to its ``agent`` and a PSI
Drift Verifier, fitted from the latency baseline, to its ``model``.
"""

from pathlib import Path

import pytest
from wyrd import WyrdError
from wyrd.bifrost import Bifrost
from wyrd.client import WyrdClient
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer

from .support import Count, Receiver, hydrated

pytestmark = pytest.mark.integration

HEALTHY_LATENCY = [{"latency": (row * 37) % 100} for row in range(100)]
"""One hundred latencies spread like the healthy baseline's 0 to 99 ms."""

SLOW_LATENCY = [{"latency": 99}] * 100
"""One hundred latencies all at the slow end of the baseline."""


def test_agent_answer_passes_its_verifier(assistant: WyrdState) -> None:
    judgment = assistant.run("agent").observe.verify("answer-is-yes", {"answer": "yes"})
    assert (judgment.verdict, judgment.passed, judgment.kind) == ("passed", True, "eval_assertion")
    assert judgment.verifier.name == "answer-is-yes"
    assert judgment.subject.uid == assistant.card_ref("agent").uid


def test_agent_answer_fails_its_verifier(assistant: WyrdState) -> None:
    judgment = assistant.run("agent").observe.verify("answer-is-yes", {"answer": "no"})
    assert (judgment.verdict, judgment.passed) == ("failed", False)


def test_judged_answer_passes_the_llm_judge(assistant: WyrdState, receiver: Receiver) -> None:
    asked = len(receiver.to("/v1/chat/completions"))
    judgment = assistant.run("agent").observe.verify("answer-is-judged", {"answer": "yes"})
    assert judgment.verdict == "passed"
    assert len(receiver.to("/v1/chat/completions")) == asked + 1


def test_model_latency_like_the_baseline_passes_its_verifier(assistant: WyrdState) -> None:
    judgment = assistant.run("model").observe.verify("latency-drift", HEALTHY_LATENCY)
    assert (judgment.passed, judgment.kind) == (True, "drift_psi")
    assert judgment.subject.uid == assistant.card_ref("model").uid


def test_model_latency_drift_is_judged_failed(assistant: WyrdState) -> None:
    judgment = assistant.run("model").observe.verify("latency-drift", SLOW_LATENCY)
    assert (judgment.verdict, judgment.kind) == ("failed", "drift_psi")
    assert judgment.subject.uid == assistant.card_ref("model").uid


def test_verify_before_baseline_ready_is_refused(unfitted_assistant: WyrdState) -> None:
    with pytest.raises(WyrdError) as raised:
        unfitted_assistant.run("model").observe.verify("tier-drift", [{"tier": "gold"}] * 5)
    assert raised.value.code == "WYRD_VERIFICATION_409_BASELINE_NOT_READY"


def test_unbound_verifier_fails_locally(assistant: WyrdState) -> None:
    with pytest.raises(WyrdError) as raised:
        assistant.run("agent").observe.verify("not-bound-here", {"answer": "yes"})
    assert raised.value.code == "WYRD_SDK_404_UNKNOWN_VERIFIER"


def test_input_of_the_wrong_shape_fails_locally(assistant: WyrdState) -> None:
    with pytest.raises(WyrdError) as raised:
        assistant.run("agent").observe.verify("answer-is-yes", [{"answer": "yes"}])
    assert raised.value.code == "WYRD_SDK_400_INVALID_OBSERVATION"


def test_caller_without_evals_run_is_refused(
    assistant_bundle: Path, wyrd_server: WyrdTestServer
) -> None:
    workload = wyrd_server.bootstrap_service(["workload"], name="workload-only")
    state = hydrated(assistant_bundle, WyrdClient(credential=workload))
    with pytest.raises(WyrdError) as raised:
        state.run("agent").observe.verify("answer-is-yes", {"answer": "yes"})
    assert raised.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"


def test_another_tenant_cannot_verify_the_assistant(
    assistant_bundle: Path, wyrd_server: WyrdTestServer, other_tenant: str
) -> None:
    other = wyrd_server.bootstrap_service_in_tenant(other_tenant, ["admin"], name="other-service")
    state = hydrated(assistant_bundle, WyrdClient(credential=other))
    with pytest.raises(WyrdError) as raised:
        state.run("agent").observe.verify("answer-is-yes", {"answer": "yes"})
    assert raised.value.code == "WYRD_VERIFICATION_404_TARGET_NOT_FOUND"


def test_verify_records_no_observation(
    assistant: WyrdState, wyrd_server: WyrdTestServer, bifrost: Bifrost
) -> None:
    run = assistant.run("agent")
    run.observe.verify("answer-is-yes", {"answer": "yes"})
    # Publish anything the judgment wrote so an observation would be visible.
    wyrd_server.flush_bifrost()
    [recorded] = bifrost.sql(
        "SELECT COUNT(*) AS n FROM vala.eval.observations WHERE run_id = $1",
        [run.run_id],
        model=Count,
    )
    assert recorded.n == 0
