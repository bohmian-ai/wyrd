"""Type fixture pinning the fixed wire records ``wyrd.verify`` returns as dictionaries.

`ty` checks this module in the `py:typecheck` lane with unused suppressions as
errors, so every valid record below must type-check and every ``ty: ignore``
line must be a real rejection. The wire views are stub-only, so they are
imported for type checking alone.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from pydantic import BaseModel
from typing_extensions import assert_type
from wyrd.state import WyrdState
from wyrd.verify import Agent, AgentRunConfig, Execution, ExecutionRoute, VerificationResult

if TYPE_CHECKING:
    from wyrd.cards import JsonValue
    from wyrd.verify import (
        CardRefTarget,
        CheckWire,
        DownloadInit,
        ExecutionState,
        InputDeclaration,
        OutcomeDetails,
        RequirementDetails,
        RequirementResult,
        StepResult,
        VerificationDetails,
        VerificationResultWire,
        VerificationVerdict,
    )


def returned_records_are_typed(execution: Execution, result: VerificationResult) -> None:
    state = execution.status()
    assert_type(state, ExecutionState)
    assert_type(result.requirements, list[RequirementResult])
    described = execution.details()
    assert_type(described, VerificationDetails)
    assert_type(described["outcome"], OutcomeDetails)
    assert_type(described["outcome"]["requirements"], list[RequirementDetails])
    assert_type(described["outcome"]["requirements"][0]["check"], CheckWire)
    assert_type(described["outcome"]["steps"], list[StepResult])
    assert_type(described["outcome"]["evidence"], dict[str, InputDeclaration])
    if "result" in state:
        assert_type(state["result"], VerificationResultWire)
        assert_type(state["result"]["outcome"]["verdict"], VerificationVerdict)


def the_actual_download_shape_is_accepted(execution: Execution) -> int:
    local: DownloadInit = {
        "plan": {"get_url": "/v1/verifier-executions/content", "ttl_secs": 0},
        "size_bytes": 4,
        "sha256": "n4bQgYhMfWWaL+qgxVrQFaO/TxsrC4Is0V1sFbDwCgg=",
    }
    expiry = execution.download({"step_id": "review", "attempt": 1}, "report")["plan"]["ttl_secs"]
    assert_type(expiry, int)
    return local["plan"]["ttl_secs"] + expiry


class Judge(BaseModel):
    score: float


def typed_reads_carry_the_model(state: WyrdState, execution: Execution) -> None:
    result = state.run().observe.verify_typed("judge", Judge, answer="yes", data_type="slot")
    assert_type(result, VerificationResult[Judge])
    assert_type(result.requirements, list[RequirementResult[Judge]])
    assert_type(result.requirements[0]["data"], Judge)
    assert_type(result.requirements[0].get("data"), Judge | None)
    assert_type(execution.wait_typed(1.0, Judge), VerificationResult[Judge])
    typed = execution.status_typed(Judge)
    assert_type(typed["result"]["outcome"]["requirements"], list[RequirementResult[Judge]])
    assert_type(execution.wait(1.0).requirements[0]["data"], dict[str, JsonValue])
    assert_type(
        execution.details_typed(Judge)["outcome"]["requirements"], list[RequirementDetails[Judge]]
    )
    execution.wait_typed(1.0, int)  # ty: ignore[invalid-argument-type]


def the_actual_status_shape_is_accepted() -> ExecutionState:
    return {
        "execution_id": "0190b2a4-0000-7000-8000-000000000000",
        "status": "completed",
        "dispatches": [],
        "result": {
            "execution_id": "0190b2a4-0000-7000-8000-000000000000",
            "verifier": {"kind": "Verifier", "name": "policy-review", "version": "1.0.0"},
            "subject": {"kind": "Agent", "name": "desk", "version": "1.0.0"},
            "outcome": {
                "verdict": "passed",
                "requirements": [
                    {
                        "id": "follows-policy",
                        "statement": "The answer follows the policy.",
                        "outcome": "passed",
                        "reason": "cited",
                        "citations": [
                            {"slot": "policy", "locator": {"page": 2}},
                            {"slot": "notes", "locator": {"lines": {"start": 1, "end": 3}}},
                        ],
                        "actual": {"any": [1, "json", None]},
                    }
                ],
            },
        },
    }


def the_actual_details_shape_is_accepted() -> VerificationDetails:
    return {
        "execution_id": "0190b2a4-0000-7000-8000-000000000000",
        "verifier": {"kind": "Verifier", "name": "policy-review", "version": "1.0.0"},
        "subject": {"kind": "Agent", "name": "desk", "version": "1.0.0"},
        "outcome": {
            "verdict": "passed",
            "requirements": [
                {
                    "id": "follows-policy",
                    "statement": "The answer follows the policy.",
                    "outcome": "passed",
                    "reason": "cited",
                    "citations": [{"slot": "policy", "locator": {"page": 2}}],
                    "check": {
                        "kind": "agent",
                        "agent": "./policy-judge.yaml",
                        "inputs": {"claim": "requirement.statement"},
                    },
                    "actual": {"any": [1, "json", None]},
                    "provenance": {
                        "kind": "agent",
                        "route": {"kind": "wyrd_gateway", "deployment": "default"},
                    },
                }
            ],
            "steps": [
                {
                    "id": "read",
                    "status": "succeeded",
                    "attempts": 1,
                    "duration_ms": 12,
                    "output": [1, {"free": "json"}],
                    "tool_calls": [{"tool": "fs.read", "arguments": {}, "returned_bytes": 3}],
                }
            ],
            "evidence": {
                "policy": {"kind": "file", "sha256": "ab", "size_bytes": 3, "format": "pdf"},
                "answer": {"kind": "json", "value": "yes"},
            },
        },
    }


def every_check_kind_is_accepted() -> list[CheckWire]:
    return [
        {
            "kind": "assert",
            "path": "steps.facts.output.within_limit",
            "operator": "equals",
            "expected": {"open": [1, "json", None]},
        },
        {
            "kind": "assert",
            "each": "steps.text.output.pages[*].text",
            "operator": {"in_range": {"min": 0, "max": 1, "inclusive": True}},
        },
        {
            "kind": "trace",
            "on": "inputs.trace",
            "span_selector": "$.spans[0].name",
            "operator": "equals",
            "expected": "lookup-policy",
        },
        {"kind": "agent", "agent": {"kind": "Agent", "name": "judge", "version": "1.0.0"}},
        {"kind": "agent", "agent": {"sibling": {"kind": "Agent", "name": "j", "version": "1"}}},
        {
            "kind": "code",
            "source": {"inline": "def check(answer): ..."},
            "entrypoint": "check",
            "inputs": {"answer": "inputs.answer"},
        },
        {
            "kind": "code",
            "runtime": "python",
            "source": {"artifact": {"card": "./checks.yaml", "path": "check.py"}},
            "entrypoint": "check",
        },
        {
            "kind": "code",
            "source": {"inline": "def check(): ..."},
            "entrypoint": "check",
            "response_format": "verdict",
        },
        {
            "kind": "code",
            "source": {"inline": "def check(): ..."},
            "entrypoint": "check",
            "response_format": {"kind": "verdict", "data": {"type": "object"}},
        },
        {
            "kind": "drift",
            "on": "inputs.latency",
            "method": "Spc",
            "signal": {"kind": "Scalar"},
            "condition": {"kind": "Always"},
            "profile": {"kind": "Spc", "sample_size": 5},
        },
    ]


def code_check_formats_are_optional(check: CheckWire) -> object:
    if check["kind"] == "code" and "response_format" in check:
        return check["response_format"]
    return None


def checks_narrow_on_kind(check: CheckWire) -> str:
    if check["kind"] == "trace":
        return check["span_selector"]
    if check["kind"] == "code":
        return check["entrypoint"]
    if check["kind"] == "drift":
        return check["method"]
    return check["kind"]


def unknown_fixed_keys_and_values_are_rejected() -> tuple[object, ...]:
    status: ExecutionState = {
        "execution_id": "id",
        "status": "done",  # ty: ignore[invalid-argument-type]
        "dispatches": [],
    }
    misspelled: ExecutionState = {
        "execution_id": "id",
        "status": "pending",
        "dispatches": [],
        "eror": {"code": "c", "message": "m"},  # ty: ignore[invalid-key]
    }
    step: StepResult = {
        "id": "read",
        "status": "succeeded",
        "attempts": "1",  # ty: ignore[invalid-argument-type]
        "duration_ms": 1,
    }
    unknown_kind: CheckWire = {  # ty: ignore[invalid-assignment]
        "kind": "judge",
        "agent": "./judge.yaml",
    }
    foreign_key: CheckWire = {  # ty: ignore[invalid-assignment]
        "kind": "code",
        "source": {"inline": "x"},
        "entrypoint": "check",
        "agent": "./judge.yaml",
    }
    wrong_type: CheckWire = {  # ty: ignore[invalid-assignment]
        "kind": "trace",
        "on": "inputs.trace",
        "span_selector": 0,
        "operator": "equals",
    }
    return status, misspelled, step, unknown_kind, foreign_key, wrong_type


def agent_routes_and_run_configs_are_closed() -> tuple[object, ...]:
    native = Agent("a", execution_route={"kind": "native"}, run_config={"max_iterations": 0})
    gateway = Agent("b").execution_route({"kind": "wyrd_gateway", "deployment": "openai"})
    typed = Agent(
        "c", execution_route=ExecutionRoute.Native, run_config=AgentRunConfig(timeout_ms=5)
    )
    unknown_route = Agent("d").execution_route({"kind": "proxy"})  # ty: ignore[invalid-argument-type]
    no_deployment = Agent("e", execution_route={"kind": "wyrd_gateway"})  # ty: ignore[invalid-argument-type]
    misspelled = Agent("f").run_config({"max_iteration": 1})  # ty: ignore[invalid-key, invalid-argument-type]
    wrong_type = Agent("g", run_config={"timeout_ms": "5"})  # ty: ignore[invalid-argument-type]
    return native, gateway, typed, unknown_route, no_deployment, misspelled, wrong_type


def sibling_targets_omit_versions_and_carry_no_uid() -> tuple[CardRefTarget, ...]:
    pinned: CardRefTarget = {"sibling": {"kind": "Agent", "name": "j", "version": "1.0.0"}}
    omitted: CardRefTarget = {"sibling": {"kind": "Agent", "name": "j"}}
    exact: CardRefTarget = {"kind": "Agent", "name": "j", "version": "1.0.0", "uid": "u"}
    with_uid: CardRefTarget = {  # ty: ignore[invalid-assignment]
        "sibling": {"kind": "Agent", "name": "j", "uid": "u"},
    }
    return pinned, omitted, exact, with_uid
