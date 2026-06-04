from __future__ import annotations

import pytest
from wyrd import Agent, Prompt, Workflow, WyrdError


def test_workflow_dict_input_binds_first_step() -> None:
    agent = Agent(
        prompt=Prompt(messages=["hello ${name}"], model="mock-model", provider="mock"),
        name="greeter",
    )
    wf = Workflow.sequential("demo", agent)
    run = wf.run({"name": "Steven"})

    assert run.final_output == "hello Steven"
    assert run.parameters == {}


def test_workflow_string_input_binds_input_var() -> None:
    agent = Agent(
        prompt=Prompt(messages=["got ${input}"], model="mock-model", provider="mock"),
        name="greeter",
    )
    wf = Workflow.sequential("demo", agent)
    run = wf.run("hi")

    assert run.final_output == "got hi"


def test_workflow_passes_structured_output_downstream() -> None:
    planner = Agent(
        prompt=Prompt(
            messages=[r'{"foo":"A","bar":"B"}'],
            model="mock-model",
            provider="mock",
            output={"foo": str, "bar": str},
        ),
        name="planner",
    )
    writer = Agent(
        prompt=Prompt(messages=["pick ${foo}"], model="mock-model", provider="mock"),
        name="writer",
    )
    wf = Workflow.sequential("demo", planner, writer)
    run = wf.run("topic")

    assert run.parameters == {"foo": "A", "bar": "B"}
    assert run.final_output == "pick A"


def test_workflow_collision_upstream_wins() -> None:
    planner = Agent(
        prompt=Prompt(
            messages=[r'{"foo":"upstream"}'],
            model="mock-model",
            provider="mock",
            output={"foo": str},
        ),
        name="planner",
    )
    writer = Agent(
        prompt=Prompt(messages=["use ${foo}"], model="mock-model", provider="mock"),
        name="writer",
    )
    wf = Workflow.sequential("demo", planner, writer)
    run = wf.run({"foo": "input-val"})

    assert run.parameters["foo"] == "upstream"
    assert run.final_output == "use upstream"


def test_workflow_missing_parameter_raises() -> None:
    agent = Agent(
        prompt=Prompt(messages=["use ${absent}"], model="mock-model", provider="mock"),
        name="writer",
    )
    wf = Workflow.sequential("demo", agent)

    with pytest.raises(WyrdError) as exc:
        wf.run("topic")

    assert exc.value.code == "SKALD_WORKFLOW_422_MISSING_PARAMETER"
