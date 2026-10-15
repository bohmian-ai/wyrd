"""An Agent authored in Python, saved as a Card, and run offline."""

from pathlib import Path

import pytest
from wyrd import Agent, AgentCard, Prompt, PromptReference, RunConfig, WyrdError, tool
from wyrd.agent import MockProvider


@tool(name="t", description="echoes text")
def echo_text(input: str) -> str:
    return input


def test_saved_agent_loads_and_runs_on_the_mock_provider(tmp_path: Path) -> None:
    path = tmp_path / "agent.yaml"
    Agent(
        prompt=Prompt(["hello"], "mock-model", provider="mock"),
        tools=[echo_text],
        name="planner",
        version="0.3.0",
        run_config=RunConfig(max_iterations=3),
    ).save(path)

    loaded = Agent.from_yaml(path)

    assert (loaded.name, loaded.version, loaded.tool_names) == ("planner", "0.3.0", ["t"])
    assert "draft the doc" in loaded.run("draft the doc").output


def test_journey_to_card_round_trip(tmp_path: Path) -> None:
    path = tmp_path / "agent.yaml"
    agent = Agent(
        prompt=Prompt.openai_chat("gpt-4o-mini", messages=["hello"]),
        name="planner",
        version="0.3.0",
    )

    card = agent.to_card()

    assert isinstance(card, AgentCard)
    assert (card.name, card.version) == ("planner", "0.3.0")
    assert card.prompt_ref.prompt is not None
    assert card.prompt_ref.prompt.model == "gpt-4o-mini"

    agent.save(path)
    loaded = Agent.from_yaml(path)
    loaded_card = loaded.to_card()

    assert loaded_card.model_dump() == card.model_dump()


def test_agent_card_json_round_trip() -> None:
    prompt = PromptReference.inline(Prompt.openai_chat("gpt-4o-mini", messages=["hello"]))
    card = AgentCard(
        prompt,
        execution_route={"kind": "native"},
        space="research",
        name="planner",
        version="0.3.0",
    )

    payload = card.model_dump()
    restored = AgentCard.model_validate_json(card.model_dump_json())

    assert payload["apiVersion"] == "wyrd/v1"
    assert payload["kind"] == "Agent"
    metadata = payload["metadata"]
    assert isinstance(metadata, dict)
    assert metadata["space"] == "research"
    assert restored.uid == card.uid
    assert restored.name == card.name
    assert restored.prompt_ref.kind == "inline"
    assert isinstance(restored.prompt_ref.prompt, Prompt)
    assert restored.prompt_ref.card_ref is None


def test_agent_card_preserves_unresolved_registered_prompt_reference() -> None:
    prompt_ref = PromptReference.card(
        "planner-prompt",
        "0.3.0",
        space="research",
        uid="018f90f5-8e1b-7c4a-a834-4d2d4df6e9c2",
    )
    restored = AgentCard.model_validate_json(
        AgentCard(
            prompt_ref, execution_route={"kind": "native"}, space="research", name="planner"
        ).model_dump_json()
    )

    with pytest.raises(WyrdError):
        restored.prompt()
    assert restored.prompt_ref.prompt is None
    assert restored.prompt_ref.card_ref is not None
    assert restored.prompt_ref.card_ref.name == "planner-prompt"
    assert restored.prompt_ref.card_ref.version == "0.3.0"


def test_mock_provider_returns_canned_responses_in_order_then_echoes() -> None:
    mock = MockProvider(["first answer"])
    mock.push("second answer")
    agent = Agent(
        prompt=Prompt(["hello"], "mock-model", provider="mock"),
        mock_provider=mock,
    )

    assert mock.remaining == 2
    assert agent.run("q1").output == "first answer"
    assert agent.run("q2").output == "second answer"
    assert mock.remaining == 0
    assert "q3" in agent.run("q3").output


def test_mock_provider_rejects_provider_base_url() -> None:
    with pytest.raises(WyrdError) as error:
        Agent(
            prompt=Prompt(["hello"], "mock-model", provider="mock"),
            mock_provider=MockProvider(),
            provider_base_url="http://localhost:1",
        )

    assert error.value.code == "WYRD_AGENT_502_PROVIDER"


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
        calls.append(("before_model", ctx.agent_id))
        return None

    def after_model(ctx, response):
        calls.append(("after_model", ctx.agent_id))
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


def test_agent_keeps_its_gateway_route_through_save_load_and_run_refusal(
    tmp_path: Path,
) -> None:
    """A gateway Agent keeps its route through YAML and JSON, and refuses to run locally.

    A local Agent has no gateway caller, so a run on a ``wyrd_gateway`` route,
    declared or as a per-run override, is refused before any provider IO; the
    override never changes the Agent's own route.
    """
    route = {"kind": "wyrd_gateway", "deployment": "openai-primary"}
    mock = MockProvider(["native answer"])
    prompt = Prompt(messages="hi", model="m", provider="mock")
    agent = Agent(prompt=prompt, name="routed", version="1.0.0", execution_route=route)
    path = tmp_path / "routed.yaml"
    agent.save(path)

    loaded = Agent.from_yaml(path)
    validated = Agent.model_validate_json(agent.model_dump_json())

    for restored in (loaded, validated):
        assert restored.to_card().spec["execution_route"] == route
    with pytest.raises(WyrdError) as captured:
        Agent(prompt=prompt, mock_provider=mock, execution_route=route).run("hi")
    assert captured.value.code == "WYRD_AGENT_422_INVALID_ARGUMENT"
    native = Agent(prompt=prompt, name="native", version="1.0.0", mock_provider=mock)
    with pytest.raises(WyrdError) as captured:
        native.run("hi", execution_route=route)
    assert captured.value.code == "WYRD_AGENT_422_INVALID_ARGUMENT"
    assert native.to_card().spec["execution_route"] == {"kind": "native"}
    assert native.run("hi").output == "native answer", "no refused run reached the mock"
    with pytest.raises(WyrdError) as captured:
        Agent(prompt=prompt, execution_route={"kind": "external"})
    assert captured.value.code == "WYRD_SPEC_400_VALIDATION"
