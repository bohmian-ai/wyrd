import pytest

from wyrd import Agent, Prompt, WyrdError, tool


def test_validate_registrable_rejects_runtime_local_tools() -> None:
    @tool(name="lookup")
    def lookup(q: str) -> str:
        return q

    agent = Agent(
        prompt=Prompt.openai_chat("gpt-4o-mini", messages=["hello"]),
        tools=[lookup],
        name="planner-agent",
        version="0.3.0",
    )

    with pytest.raises(WyrdError) as error:
        agent.validate_registrable()

    assert error.value.code == "WYRD_AGENT_422_RUNTIME_LOCAL_TOOLS_NOT_REGISTRABLE"
