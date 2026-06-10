from pathlib import Path

from wyrd import Agent, Prompt, Workflow


def _build_agent(name: str) -> Agent:
    return Agent(
        prompt=Prompt.openai_chat("gpt-4o-mini", messages=["plan"]),
        name=name,
        version="0.1.0",
    )


def test_workflow_save_load_round_trips_yaml(tmp_path: Path) -> None:
    path = tmp_path / "research.yaml"
    planner = _build_agent("planner")
    writer = _build_agent("writer")
    workflow = Workflow.sequential("research", planner, writer)
    workflow.set_version("0.1.0")

    workflow.save(path)
    loaded = Workflow.load(path)

    yaml_body = path.read_text()
    assert "apiVersion: wyrd/v1" in yaml_body
    assert "kind: Workflow" in yaml_body
    assert "planner" in yaml_body
    assert "writer" in yaml_body
    assert list(loaded.steps) == ["planner", "writer"]
