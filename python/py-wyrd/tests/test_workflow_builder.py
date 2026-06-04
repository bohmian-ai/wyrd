from wyrd import Agent, Prompt, Workflow


def _build_agent(name: str) -> Agent:
    return Agent(
        prompt=Prompt.openai_chat("gpt-4o-mini", messages=["plan"]),
        name=name,
        version="0.1.0",
    )


def test_workflow_builder_add_after_accepts_agent() -> None:
    planner = _build_agent("planner")
    writer = _build_agent("writer")

    workflow = Workflow(name="research").add(planner).add_after(writer, after=planner)

    assert list(workflow.steps) == ["planner", "writer"]


def test_workflow_builder_add_after_accepts_step_id_string() -> None:
    planner = _build_agent("planner")
    writer = _build_agent("writer")

    workflow = Workflow(name="research").add(planner).add_after(writer, after="planner")

    assert list(workflow.steps) == ["planner", "writer"]


def test_workflow_builder_add_after_accepts_mixed_list() -> None:
    planner = _build_agent("planner")
    researcher = _build_agent("researcher")
    writer = _build_agent("writer")

    workflow = (
        Workflow(name="research")
        .add(planner)
        .add(researcher)
        .add_after(writer, after=[planner, "researcher"])
    )

    assert list(workflow.steps) == ["planner", "researcher", "writer"]
