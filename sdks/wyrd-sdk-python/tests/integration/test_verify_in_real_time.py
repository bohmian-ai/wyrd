"""A Service judges its components' behavior in real time with ``run.observe.verify``.

The ``assistant`` Service binds two Eval Verifiers to its ``agent`` and a PSI
Drift Verifier, fitted from the latency baseline, to its ``model``.
"""

import pytest
from pydantic import BaseModel
from wyrd import WyrdError
from wyrd.bifrost import Bifrost
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer

from .conftest import Receiver

SHIFTED_LATENCY = [{"latency": 150.0 + row} for row in range(120)]
"""Latency samples far above every baseline bin."""


class Count(BaseModel):
    """One ``COUNT(*)`` result."""

    n: int


@pytest.mark.integration
def test_agent_answer_passes_its_verifier(assistant: WyrdState) -> None:
    judgment = assistant.run("agent").observe.verify("answer-is-yes", {"answer": "yes"})
    assert (judgment.verdict, judgment.passed, judgment.kind) == ("passed", True, "eval_assertion")
    assert judgment.verifier.name == "answer-is-yes"
    assert judgment.subject.uid == assistant.card_ref("agent").uid


@pytest.mark.integration
def test_agent_answer_fails_its_verifier(assistant: WyrdState) -> None:
    judgment = assistant.run("agent").observe.verify("answer-is-yes", {"answer": "no"})
    assert (judgment.verdict, judgment.passed) == ("failed", False)


@pytest.mark.integration
def test_judged_answer_passes_the_llm_judge(assistant: WyrdState, receiver: Receiver) -> None:
    asked = len(receiver.to("/v1/chat/completions"))
    judgment = assistant.run("agent").observe.verify("answer-is-judged", {"answer": "yes"})
    assert judgment.verdict == "passed"
    assert len(receiver.to("/v1/chat/completions")) == asked + 1


@pytest.mark.integration
def test_model_latency_drift_is_judged_failed(assistant: WyrdState) -> None:
    judgment = assistant.run("model").observe.verify("latency-drift", SHIFTED_LATENCY)
    assert (judgment.verdict, judgment.kind) == ("failed", "drift_psi")
    assert judgment.subject.uid == assistant.card_ref("model").uid


@pytest.mark.integration
def test_verify_before_baseline_ready_is_refused(unfitted_assistant: WyrdState) -> None:
    with pytest.raises(WyrdError) as raised:
        unfitted_assistant.run("model").observe.verify("tier-drift", [{"tier": "gold"}] * 5)
    assert raised.value.code == "WYRD_VERIFICATION_409_BASELINE_NOT_READY"


@pytest.mark.integration
def test_unbound_verifier_fails_locally(assistant: WyrdState) -> None:
    with pytest.raises(WyrdError) as raised:
        assistant.run("agent").observe.verify("latency-drift", SHIFTED_LATENCY)
    assert raised.value.code == "WYRD_SDK_404_UNKNOWN_VERIFIER"


@pytest.mark.integration
def test_caller_without_evals_run_is_refused(
    assistant: WyrdState, reader_key: str, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("WYRD_API_KEY", reader_key)
    with pytest.raises(WyrdError) as raised:
        assistant.run("agent").observe.verify("answer-is-yes", {"answer": "yes"})
    assert raised.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"


@pytest.mark.integration
def test_verify_records_no_observation(
    assistant: WyrdState, wyrd_server: WyrdTestServer, bifrost: Bifrost
) -> None:
    run = assistant.run("agent")
    run.observe.verify("answer-is-yes", {"answer": "yes"})
    wyrd_server.flush_bifrost()
    [recorded] = bifrost.sql(
        "SELECT COUNT(*) AS n FROM vala.eval.observations WHERE run_id = $1",
        [run.run_id],
        model=Count,
    )
    assert recorded.n == 0
