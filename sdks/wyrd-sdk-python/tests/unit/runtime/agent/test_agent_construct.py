"""An Agent is built from a Prompt and an optional Card identity."""

from wyrd import Agent, Prompt


def test_agent_constructs_from_prompt() -> None:
    prompt = Prompt.openai_chat("gpt-4o-mini", messages=["hello"])

    agent = Agent(prompt=prompt, name="planner-agent", version="0.3.0")

    assert agent.name == "planner-agent"
    assert agent.version == "0.3.0"
    assert agent.provider == "openai"
    assert agent.model == "gpt-4o-mini"
