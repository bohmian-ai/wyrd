"""Type fixture pinning the fixed wire records ``wyrd.verify`` returns as dictionaries.

`ty` checks this module in the `py:typecheck` lane with unused suppressions as
errors, so every valid record below must type-check and every ``ty: ignore``
line must be a real rejection. The wire views are stub-only, so they are
imported for type checking alone.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from typing_extensions import assert_type
from wyrd.verify import Execution, VerificationResult

if TYPE_CHECKING:
    from wyrd.verify import (
        Check,
        ExecutionState,
        InputDeclaration,
        RequirementResult,
        StepResult,
        VerificationResultWire,
        VerificationVerdict,
    )


def returned_records_are_typed(execution: Execution, result: VerificationResult) -> None:
    state = execution.status()
    assert_type(state, ExecutionState)
    assert_type(result.requirements, list[RequirementResult])
    assert_type(result.steps, list[StepResult])
    assert_type(result.evidence, dict[str, InputDeclaration])
    if "result" in state:
        assert_type(state["result"], VerificationResultWire)
        assert_type(state["result"]["outcome"]["verdict"], VerificationVerdict)


def the_actual_status_shape_is_accepted() -> ExecutionState:
    return {
        "execution_id": "0190b2a4-0000-7000-8000-000000000000",
        "status": "completed",
        "result": {
            "execution_id": "0190b2a4-0000-7000-8000-000000000000",
            "verifier": {"kind": "Verifier", "name": "policy-review", "version": "1.0.0"},
            "subject": {"kind": "Agent", "name": "desk", "version": "1.0.0", "space": "s"},
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
        },
    }


def every_check_kind_is_accepted() -> list[Check]:
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
            "kind": "drift",
            "on": "inputs.latency",
            "method": "Spc",
            "signal": {"kind": "Scalar"},
            "condition": {"kind": "Always"},
            "profile": {"kind": "Spc", "sample_size": 5},
        },
    ]


def checks_narrow_on_kind(check: Check) -> str:
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
    }
    misspelled: ExecutionState = {
        "execution_id": "id",
        "status": "pending",
        "eror": {"code": "c", "message": "m"},  # ty: ignore[invalid-key]
    }
    step: StepResult = {
        "id": "read",
        "status": "succeeded",
        "attempts": "1",  # ty: ignore[invalid-argument-type]
        "duration_ms": 1,
    }
    unknown_kind: Check = {  # ty: ignore[invalid-assignment]
        "kind": "judge",
        "agent": "./judge.yaml",
    }
    foreign_key: Check = {  # ty: ignore[invalid-assignment]
        "kind": "code",
        "source": {"inline": "x"},
        "entrypoint": "check",
        "agent": "./judge.yaml",
    }
    wrong_type: Check = {  # ty: ignore[invalid-assignment]
        "kind": "trace",
        "on": "inputs.trace",
        "span_selector": 0,
        "operator": "equals",
    }
    return status, misspelled, step, unknown_kind, foreign_key, wrong_type
