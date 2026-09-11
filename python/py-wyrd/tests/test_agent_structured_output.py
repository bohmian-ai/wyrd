from __future__ import annotations

import pytest
from wyrd import Agent, Prompt, WyrdError


def response_with(text: str) -> dict:
    return {
        "id": "replacement",
        "object": "chat.completion",
        "created": 0,
        "model": "mock-model",
        "choices": [
            {
                "index": 0,
                "message": {"role": "assistant", "content": text},
                "finish_reason": "stop",
            }
        ],
    }


def test_structured_output_parses_into_dict() -> None:
    def after_model(ctx, response):
        return response_with(r'{"foo":"A","bar":"B"}')

    agent = Agent(
        prompt=Prompt(
            messages=["Hi"],
            model="mock-model",
            provider="mock",
            output={"foo": str, "bar": str},
        ),
        after_model_callback=after_model,
    )

    run = agent.run("ignored")

    assert run.structured_output == {"foo": "A", "bar": "B"}


def test_structured_output_is_none_for_text_prompts() -> None:
    agent = Agent(prompt=Prompt(messages=["Hi"], model="mock-model", provider="mock"))
    run = agent.run("hello")

    assert run.structured_output is None


def test_structured_output_decode_failure_raises() -> None:
    def after_model(ctx, response):
        return response_with("not json")

    agent = Agent(
        prompt=Prompt(
            messages=["Hi"],
            model="mock-model",
            provider="mock",
            output={"x": str},
        ),
        after_model_callback=after_model,
    )

    with pytest.raises(WyrdError) as exc:
        agent.run("ignored")

    assert exc.value.code == "WYRD_AGENT_422_STRUCTURED_DECODE"
