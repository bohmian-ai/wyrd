import importlib

from wyrd import Agent, Prompt


def test_agent_is_native_pyclass_without_python_wrapper() -> None:
    agent = Agent(prompt=Prompt.openai_chat("gpt-4o-mini", messages=["hello"]))

    assert Agent.__module__ == "wyrd.agent"
    assert Agent.__name__ == "Agent"
    assert type(agent).__module__ == "wyrd.agent"
    assert not hasattr(agent, "_inner")


def test_old_agent_inner_classes_are_not_importable() -> None:
    module = importlib.import_module("wyrd.agent")
    old_names = [
        "_Agent" + "BuilderInner",
        "_Agent" + "WithMetaInner",
        "_Agent" + "Inner",
    ]

    for name in old_names:
        assert not hasattr(module, name)


def test_agent_to_card_returns_mapping() -> None:
    agent = Agent(
        prompt=Prompt.openai_chat("gpt-4o-mini", messages=["hello"]),
        name="planner-agent",
        version="0.3.0",
    )

    card = agent.to_card()

    assert isinstance(card, dict)
    assert card["kind"] == "Agent"
