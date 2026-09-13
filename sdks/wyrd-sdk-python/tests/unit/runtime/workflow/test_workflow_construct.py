import pytest
from wyrd import Agent, Prompt, Workflow, WyrdError


def _build_agent(name: str) -> Agent:
    return Agent(
        prompt=Prompt.openai_chat("gpt-4o-mini", messages=["plan"]),
        name=name,
        version="0.1.0",
    )


def test_workflow_constructs_empty_with_metadata() -> None:
    workflow = Workflow(name="research", version="0.1.0")

    assert workflow.name == "research"
    assert workflow.version == "0.1.0"
    assert list(workflow.steps) == []


def test_workflow_sequential_links_agents_in_order() -> None:
    workflow = Workflow.sequential("research", _build_agent("planner"), _build_agent("writer"))

    assert list(workflow.steps) == ["planner", "writer"]


def test_workflow_parallel_no_dependencies() -> None:
    workflow = Workflow.parallel(
        "fanout",
        _build_agent("alpha"),
        _build_agent("bravo"),
        _build_agent("charlie"),
    )

    assert list(workflow.steps) == ["alpha", "bravo", "charlie"]


def test_workflow_sequential_rejects_non_agent_positional() -> None:
    with pytest.raises(WyrdError) as exc:
        Workflow.sequential("research", "not-an-agent")
    assert exc.value.code == "WYRD_WORKFLOW_422_VALIDATION"
    assert exc.value.details["argument"] == "agents"
