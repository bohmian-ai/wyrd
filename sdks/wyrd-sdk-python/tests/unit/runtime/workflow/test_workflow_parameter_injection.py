from __future__ import annotations

import pytest
from wyrd import Agent, Prompt, Workflow, WorkflowRun, WyrdError
from wyrd.agent import WorkflowRunDict, WorkflowRunError, WorkflowStepResult


def _planner() -> Agent:
    return Agent(
        prompt=Prompt(
            messages=[r'{"foo":"A","bar":"B"}'],
            model="mock-model",
            provider="mock",
            output={"foo": str, "bar": str},
        ),
        name="planner",
    )


def _writer(message: str) -> Agent:
    return Agent(
        prompt=Prompt(messages=[message], model="mock-model", provider="mock"),
        name="writer",
    )


def test_explicit_workflow_bindings() -> None:
    workflow = (
        Workflow.sequential("demo", _planner(), _writer("pick ${foo} for ${topic}"))
        .with_inputs({"topic": "default"})
        .with_step_inputs(
            "writer",
            {"foo": "steps.planner.output.structured.foo", "topic": "input.topic"},
        )
        .with_outputs(
            {
                "pick": "steps.writer.output.text",
                "plan": "steps.planner.output.structured",
                "topic": "input.topic",
            }
        )
    )
    workflow.validate()

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
    workflow = Workflow.sequential("demo", _planner(), _writer("pick ${foo}")).with_outputs(
        {"pick": "steps.writer.output.text"}
    )

    with pytest.raises(WyrdError) as exc:
        workflow.validate()

    assert exc.value.code == "WYRD_WORKFLOW_422_VALIDATION"
    assert exc.value.details is not None
    assert exc.value.details["field"] == "steps[1].inputs.foo"


def test_workflow_text_input_requires_declared_string_input() -> None:
    declared = (
        Workflow.sequential("demo", _writer("got ${input}"))
        .with_inputs({"input": ""})
        .with_step_inputs("writer", {"input": "input.input"})
        .with_outputs({"got": "steps.writer.output.text"})
    )
    undeclared = Workflow.sequential("demo", _writer("static")).with_outputs(
        {"got": "steps.writer.output.text"}
    )

    assert declared.run("hi").outputs == {"got": "got hi"}
    with pytest.raises(WyrdError) as exc:
        undeclared.run("hi")
    assert exc.value.code == "WYRD_WORKFLOW_422_RUN_REQUEST"


def test_workflow_rejects_undeclared_or_mistyped_input() -> None:
    workflow = (
        Workflow.sequential("demo", _writer("n=${n}"))
        .with_inputs({"n": 1})
        .with_step_inputs("writer", {"n": "input.n"})
        .with_outputs({"text": "steps.writer.output.text"})
    )

    assert workflow.run({"n": 7}).outputs == {"text": "n=7"}
    for bad in ({"other": 1}, {"n": "seven"}):
        with pytest.raises(WyrdError) as exc:
            workflow.run(bad)
        assert exc.value.code == "WYRD_WORKFLOW_422_RUN_REQUEST"


def test_workflow_rejects_invalid_binding_source() -> None:
    workflow = Workflow.sequential("demo", _writer("x ${v}"))

    with pytest.raises(WyrdError):
        workflow.with_step_inputs("writer", {"v": "${input.v}"})
    with pytest.raises(WyrdError):
        workflow.with_step_inputs("missing", {"v": "input.v"})
