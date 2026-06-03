from pathlib import Path

from wyrd import Agent, Prompt


def test_agent_save_load_round_trips_yaml(tmp_path: Path) -> None:
    path = tmp_path / "planner.yaml"
    agent = Agent(
        prompt=Prompt.openai_chat("gpt-4o-mini", messages=["hello"]),
        name="planner-agent",
        version="0.3.0",
        space="research",
    )

    agent.save(path)
    loaded = Agent.from_yaml(path)

    assert loaded.to_yaml_string() == path.read_text()
    assert "apiVersion: wyrd/v1" in loaded.to_yaml_string()
    forbidden_version_key = "version" + "_req"
    assert forbidden_version_key not in loaded.to_yaml_string()
