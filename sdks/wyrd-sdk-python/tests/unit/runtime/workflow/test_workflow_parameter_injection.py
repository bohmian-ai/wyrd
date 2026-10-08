from __future__ import annotations

import json
from typing import Any

import pytest
from wyrd import Workflow, WorkflowRun, WyrdError
from wyrd.agent import WorkflowRunDict, WorkflowRunError, WorkflowStepResult

PLAN_SCHEMA = {
    "type": "object",
    "properties": {"foo": {"type": "string"}, "bar": {"type": "string"}},
    "required": ["foo", "bar"],
    "additionalProperties": False,
}


def _agent(message: str, variables: list[str], structured: bool = False) -> dict[str, Any]:
    """An inline mock-provider Agent whose Prompt sends ``message``."""
    body: dict[str, Any] = {
        "model": "mock-model",
        "messages": [{"role": "user", "content": message}],
    }
    prompt: dict[str, Any] = {
        "model": "mock-model",
        "provider": {"custom": "mock"},
        "variables": variables,
        "request": {"provider": "open_ai_chat_completion", "body": body},
    }
    if structured:
        schema = {"name": "structured_output", "schema": PLAN_SCHEMA}
        body["response_format"] = {"type": "json_schema", "json_schema": {**schema, "strict": True}}
        prompt["response_type"] = {"json_schema": schema}
    return {"prompt": prompt}


def _workflow(
    steps: list[dict[str, Any]],
    outputs: dict[str, str],
    inputs: dict[str, dict[str, Any]] | None = None,
) -> Workflow:
    """A ``demo`` Workflow authored as YAML (JSON is YAML) and loaded with ``from_yaml``."""
    return Workflow.from_yaml(
        json.dumps(
            {
                "apiVersion": "wyrd/v1",
                "kind": "Workflow",
                "metadata": {"name": "demo", "version": "1.0.0"},
                "spec": {"inputs": inputs or {}, "steps": steps, "outputs": outputs},
            }
        )
    )


def _step(step_id: str, agent: dict[str, Any], **fields: Any) -> dict[str, Any]:
    """One Agent step with optional ``inputs`` and ``depends_on``."""
    return {"id": step_id, "action": {"type": "agent", "target": agent}, **fields}


PLANNER = _step("planner", _agent('{"foo":"A","bar":"B"}', [], structured=True))


def test_explicit_workflow_bindings() -> None:
    workflow = _workflow(
        [
            PLANNER,
            _step(
                "writer",
                _agent("pick ${foo} for ${topic}", ["foo", "topic"]),
                depends_on=["planner"],
                inputs={"foo": "steps.planner.output.structured.foo", "topic": "input.topic"},
            ),
        ],
        {
            "pick": "steps.writer.output.text",
            "plan": "steps.planner.output.structured",
            "topic": "input.topic",
        },
        {"topic": {"type": "str", "value": "default"}},
    )

    run = workflow.run({"topic": "rust"})

    assert isinstance(run, WorkflowRun)
    assert run.status == "succeeded"
    assert run.error is None
    assert run.outputs == {
        "pick": "pick A for rust",
        "plan": {"foo": "A", "bar": "B"},
        "topic": "rust",
    }
    assert run.steps["planner"]["structured_output"] == {"foo": "A", "bar": "B"}
    assert run.steps["planner"]["attempts"] == 1
    assert run.steps["writer"]["text"] == "pick A for rust"
    snapshot: WorkflowRunDict = run.to_dict()
    planner: WorkflowStepResult = snapshot["steps"]["planner"]
    assert snapshot["outputs"] == run.outputs
    assert set(snapshot) == WorkflowRunDict.__required_keys__
    assert set(planner) == WorkflowStepResult.__required_keys__
    assert planner["status"] == "succeeded"
    error: WorkflowRunError | None = snapshot["error"]
    assert error is None
    assert run.run_id

    assert workflow.run().outputs["topic"] == "default"


def test_workflow_dependencies_inject_no_data() -> None:
    workflow = _workflow(
        [PLANNER, _step("writer", _agent("pick ${foo}", ["foo"]), depends_on=["planner"])],
        {"pick": "steps.writer.output.text"},
    )

    with pytest.raises(WyrdError) as exc:
        workflow.run()

    assert exc.value.code == "WYRD_WORKFLOW_422_VALIDATION"
    assert exc.value.details["field"] == "steps[1].inputs.foo"


def test_workflow_text_input_requires_declared_string_input() -> None:
    declared = _workflow(
        [_step("writer", _agent("got ${input}", ["input"]), inputs={"input": "input.input"})],
        {"got": "steps.writer.output.text"},
        {"input": {"type": "str", "value": ""}},
    )
    undeclared = _workflow(
        [_step("writer", _agent("static", []))], {"got": "steps.writer.output.text"}
    )

    assert declared.run("hi").outputs == {"got": "got hi"}
    with pytest.raises(WyrdError) as exc:
        undeclared.run("hi")
    assert exc.value.code == "WYRD_WORKFLOW_422_RUN_REQUEST"


def test_workflow_rejects_undeclared_or_mistyped_input() -> None:
    workflow = _workflow(
        [_step("writer", _agent("n=${n}", ["n"]), inputs={"n": "input.n"})],
        {"text": "steps.writer.output.text"},
        {"n": {"type": "int", "value": 1}},
    )

    assert workflow.run({"n": 7}).outputs == {"text": "n=7"}
    for bad in ({"other": 1}, {"n": "seven"}):
        with pytest.raises(WyrdError) as exc:
            workflow.run(bad)
        assert exc.value.code == "WYRD_WORKFLOW_422_RUN_REQUEST"


def test_workflow_rejects_invalid_binding_source() -> None:
    with pytest.raises(WyrdError):
        _workflow(
            [_step("writer", _agent("x ${v}", ["v"]), inputs={"v": "${input.v}"})],
            {"text": "steps.writer.output.text"},
            {"v": {"type": "str", "value": ""}},
        )
