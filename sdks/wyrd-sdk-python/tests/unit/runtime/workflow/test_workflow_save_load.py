"""A Workflow saved as a Workflow Card YAML file loads back unchanged."""

from pathlib import Path

from wyrd import Agent, Prompt, Workflow


def _build_agent(name: str) -> Agent:
    # Unversioned Agents save inline, so the file loads back without a registry.
    # A versioned Agent would save as a registry reference instead.
    return Agent(
        prompt=Prompt.openai_chat("gpt-4o-mini", messages=["plan"]),
        name=name,
    )


def test_workflow_save_load_round_trips_yaml(tmp_path: Path) -> None:
    path = tmp_path / "research.yaml"
    planner = _build_agent("planner")
    writer = _build_agent("writer")
    workflow = Workflow.sequential("research", planner, writer).with_outputs(
        {"brief": "steps.writer.output.text"}
    )
    workflow.set_version("0.1.0")

    workflow.save(path)
    loaded = Workflow.from_path(path)

    assert (loaded.name, loaded.version, list(loaded.steps)) == (
        "research",
        "0.1.0",
        ["planner", "writer"],
    )
    assert loaded.to_yaml() == workflow.to_yaml()
