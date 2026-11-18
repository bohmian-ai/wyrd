"""Offline Jev decision checks through public ``wyrd.verify``: an invalid
``verdict_transform`` is refused where it is written or when its graph is
validated, naming the field, and the corrected definition validates.

Nothing here reaches a server; validation is the shared Rust validation.
"""

from typing import Any

import pytest
from wyrd import WyrdError
from wyrd.prompt import Prompt
from wyrd.verify import (
    Action,
    Agent,
    AgentRunConfig,
    Check,
    Component,
    ExecutionRoute,
    Requirement,
    Service,
    Verifier,
)

QUESTIONS: dict[str, Any] = {
    "refund": {
        "type": "choice",
        "instructions": "Does the reply follow the refund policy?",
        "criteria": {"compliant": "It follows the policy.", "violation": "It breaks the policy."},
    },
    "helpful": {"type": "noul", "instructions": "Does the reply help the customer?"},
    "compliance": {
        "type": "score",
        "instructions": "How compliant is the reply?",
        "criteria": ["Low", "Medium", "High"],
    },
}

CHECK = "requirements[0].check.verdict_transform"


def judge(prompt: Prompt | None = None) -> Agent:
    return Agent(
        "jev-judge",
        "1.0.0",
        prompt=prompt or Prompt.jev("jev-latest", state="${context}", questions=QUESTIONS),
        execution_route=ExecutionRoute.wyrd_gateway(deployment="jev"),
        run_config=AgentRunConfig(max_iterations=1),
    )


def check(transform: dict[str, Any], agent: Agent | None = None) -> Check:
    return Check.agent(
        agent or judge(), inputs={"context": "inputs.reply"}, verdict_transform=transform
    )


def desk(verifier: Verifier, agent: Agent | None = None) -> Service:
    return Service("desk", "1.0.0").add_component(
        Component("jev-judge", agent or judge()).verified_by([verifier])
    )


def verifier(judged: Check) -> Verifier:
    return Verifier(
        "desk-policy",
        "1.0.0",
        inputs={"reply": str},
        requirements=[Requirement("judged", "The reply is judged.", judged)],
    )


def refusal(error: pytest.ExceptionInfo[WyrdError]) -> str:
    assert error.value.code == "WYRD_LOADER_400_INVALID_ENVELOPE", error.value
    return " ".join(diagnostic["message"] for diagnostic in error.value.details["diagnostics"])


@pytest.mark.parametrize(
    ("transform", "field"),
    [
        (
            {
                "kind": "probability",
                "question": "helpful",
                "expected": True,
                "pass_at": 0.2,
                "fail_at": 0.8,
            },
            "verdict_transform.fail_at",
        ),
        (
            {
                "kind": "probability",
                "question": "helpful",
                "expected": True,
                "pass_at": 1.5,
                "fail_at": 0.1,
            },
            "verdict_transform.fail_at",
        ),
        (
            {"kind": "score", "question": "compliance", "operator": "contains", "expected": 2},
            "verdict_transform.operator",
        ),
        ({"kind": "choice", "question": " "}, "verdict_transform.question"),
        (
            {
                "kind": "probability",
                "question": "helpful",
                "expected": "yes",
                "pass_at": 0.9,
                "fail_at": 0.1,
            },
            "verdict_transform",
        ),
        ({"kind": "verdict", "question": "helpful"}, "verdict_transform"),
        ({"kind": "choice", "question": "refund", "outcome": {}}, "verdict_transform"),
    ],
)
def test_verdict_transform_refuses_a_malformed_policy_where_it_is_written(
    transform: dict[str, Any], field: str
) -> None:
    with pytest.raises(WyrdError) as refused:
        check(transform)

    assert field in refused.value.message, refused.value


@pytest.mark.parametrize(
    ("transform", "field"),
    [
        ({"kind": "choice", "question": "tone"}, f"{CHECK}.question"),
        (
            {
                "kind": "probability",
                "question": "refund",
                "expected": True,
                "pass_at": 0.9,
                "fail_at": 0.1,
            },
            f"{CHECK}.kind",
        ),
        (
            {"kind": "score", "question": "helpful", "operator": "equals", "expected": 1},
            f"{CHECK}.kind",
        ),
        ({"kind": "choice", "question": "refund"}, f"{CHECK}.outcomes"),
        (
            {"kind": "choice", "question": "refund", "outcomes": {"compliant": "passed"}},
            f"{CHECK}.outcomes",
        ),
        (
            {
                "kind": "choice",
                "question": "refund",
                "outcomes": {"compliant": "passed", "violation": "failed", "other": "failed"},
            },
            f"{CHECK}.outcomes.other",
        ),
    ],
)
def test_validation_refuses_a_policy_its_jev_prompt_cannot_answer(
    transform: dict[str, Any], field: str
) -> None:
    service = desk(verifier(check(transform)))

    with pytest.raises(WyrdError) as refused:
        service.validate()

    assert field in refusal(refused)


def test_validation_refuses_a_policy_on_a_prompt_without_decisions() -> None:
    chat = judge(Prompt.openai_chat("gpt-4o", messages="Judge {{context}}"))
    service = desk(verifier(check({"kind": "choice", "question": "refund"}, chat)), chat)

    with pytest.raises(WyrdError) as refused:
        service.validate()

    assert "give the Agent a Jev system_one Prompt" in refusal(refused)


def test_a_corrected_policy_validates_after_its_refusal() -> None:
    policy = verifier(check({"kind": "choice", "question": "refund"}))
    service = desk(policy)
    with pytest.raises(WyrdError) as refused:
        service.validate()
    assert f"{CHECK}.outcomes" in refusal(refused)

    policy.requirements(
        [
            Requirement(
                "judged",
                "The reply is judged.",
                check(
                    {
                        "kind": "choice",
                        "question": "refund",
                        "outcomes": {"compliant": "passed", "violation": "failed"},
                    }
                ),
            )
        ]
    )
    desk(policy).validate()


def test_an_agent_action_takes_no_verdict_transform() -> None:
    with pytest.raises(TypeError, match="verdict_transform"):
        Action.agent(judge(), verdict_transform={"kind": "choice", "question": "refund"})


def test_a_jev_prompt_refuses_a_named_response_format() -> None:
    prompt = {
        "provider": "jev",
        "model": "jev-latest",
        "format": "system_one",
        "response_format": "verdict",
        "body": {"state": "${context}", "questions": QUESTIONS},
    }

    with pytest.raises(WyrdError) as refused:
        Agent(
            "jev-judge",
            "1.0.0",
            prompt=prompt,
            execution_route=ExecutionRoute.wyrd_gateway(deployment="jev"),
        ).validate()

    assert "this native format has no structured output" in refusal(refused)
