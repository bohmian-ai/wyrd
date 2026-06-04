from __future__ import annotations

from pathlib import Path

import pytest
import wyrd
from wyrd import Agent, FinishReason, Prompt, RunConfig, tool


@tool(name="t", description="echoes text")
def echo_text(input: str) -> str:
    return input


def test_journey_author_save_load_run(tmp_path: Path) -> None:
    path = tmp_path / "agent.yaml"
    saved = Agent(
        prompt=Prompt.openai_chat("gpt-4o-mini", messages=["hello"]),
        tools=[echo_text],
        name="planner",
        version="0.3.0",
        run_config=RunConfig(max_iterations=3),
    )
    saved.save(path)
    loaded = Agent.from_yaml(path)
    runner = Agent(
        prompt=Prompt(["hello"], "mock-model", provider="mock"),
        tools=[echo_text],
        name=loaded.name,
        version=loaded.version,
        run_config=RunConfig(max_iterations=3),
    )
    run = runner.run("draft the doc")

    assert loaded.name == "planner"
    assert loaded.version == "0.3.0"
    assert loaded.tool_names == ["t"]
    assert run.finish_reason == FinishReason.ModelStopped
    assert "draft the doc" in run.output


def test_journey_to_card_round_trip(tmp_path: Path) -> None:
    path = tmp_path / "agent.yaml"
    agent = Agent(
        prompt=Prompt.openai_chat("gpt-4o-mini", messages=["hello"]),
        name="planner",
        version="0.3.0",
    )

    card = agent.to_card()

    assert card["apiVersion"] == "wyrd/v1"
    assert card["kind"] == "Agent"
    assert card["metadata"]["name"] == "planner"
    assert card["metadata"]["version"] == "0.3.0"
    assert card["spec"]["prompt"] is not None

    agent.save(path)
    loaded = Agent.from_yaml(path)
    loaded_card = loaded.to_card()

    assert loaded_card["apiVersion"] == card["apiVersion"]
    assert loaded_card["kind"] == card["kind"]
    assert loaded_card["metadata"] == card["metadata"]
    assert loaded_card["spec"] == card["spec"]


def test_journey_delegate_via_agent_delegate_tool() -> None:
    child = Agent(
        prompt=Prompt(["child"], "mock-model", provider="mock"),
        name="researcher",
        version="0.3.0",
    )

    delegate = child.as_tool(description="Research the request")

    assert "draft the doc" in delegate({"input": "draft the doc"})


def test_journey_callbacks_fire_in_registration_order() -> None:
    calls: list[tuple[str, str]] = []

    def before_agent(ctx, input_text):
        calls.append(("before_agent", input_text))
        return None

    def before_model(ctx, request):
        calls.append(("before_model", ctx["agent_id"]))
        return None

    def after_model(ctx, response):
        calls.append(("after_model", ctx["agent_id"]))
        return None

    def after_agent(ctx, run):
        calls.append(("after_agent", run.output))
        return None

    agent = Agent(
        prompt=Prompt(["hello"], "mock-model", provider="mock"),
        name="planner",
        version="0.3.0",
        before_agent_callback=before_agent,
        before_model_callback=before_model,
        after_model_callback=after_model,
        after_agent_callback=after_agent,
    )

    run = agent.run("draft the doc")

    assert "draft the doc" in run.output
    assert [name for name, _ in calls] == [
        "before_agent",
        "before_model",
        "after_model",
        "after_agent",
    ]


def test_journey_module_paths() -> None:
    agent = Agent(
        prompt=Prompt(["hello"], "mock-model", provider="mock"),
        name="planner",
        version="0.3.0",
    )

    assert Agent.__module__ == "wyrd.agent"
    assert type(agent).__module__ == "wyrd.agent"
    assert not hasattr(agent, "_inner")
    assert not hasattr(wyrd, "AgentCard")

    with pytest.raises(ImportError):
        from wyrd import AgentCard  # noqa: F401

    assert not hasattr(wyrd, "_wyrd") or "AgentCard" not in dir(getattr(wyrd, "_wyrd", object()))
