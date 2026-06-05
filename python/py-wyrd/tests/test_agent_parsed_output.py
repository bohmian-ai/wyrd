"""AgentRun.parsed returns a typed model instance."""

from __future__ import annotations

import pydantic
import pytest
from wyrd import Agent, Prompt, WyrdError


def _response(text: str) -> dict:
    return {
        "id": "mock",
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


def test_parsed_returns_pydantic_instance():
    class Plan(pydantic.BaseModel):
        summary: str
        steps: list[str]

    agent = Agent(
        prompt=Prompt(
            messages=["Plan it."],
            model="mock-model",
            provider="mock",
            output=Plan,
        ),
        after_model_callback=lambda ctx, r: _response(
            '{"summary":"Rust is great","steps":["learn ownership"]}'
        ),
    )
    run = agent.run("go")
    assert isinstance(run.parsed, Plan)
    assert run.parsed.summary == "Rust is great"
    assert run.parsed.steps == ["learn ownership"]


def test_structured_output_still_dict_when_parsed_present():
    class Plan(pydantic.BaseModel):
        summary: str

    agent = Agent(
        prompt=Prompt(
            messages=["go"], model="mock-model", provider="mock", output=Plan
        ),
        after_model_callback=lambda ctx, r: _response('{"summary":"hello"}'),
    )
    run = agent.run("go")
    assert isinstance(run.structured_output, dict)
    assert run.structured_output["summary"] == "hello"
    assert isinstance(run.parsed, Plan)


def test_parsed_is_none_for_text_prompt():
    agent = Agent(
        prompt=Prompt(messages=["go"], model="mock-model", provider="mock")
    )
    run = agent.run("go")
    assert run.parsed is None
    assert run.structured_output is None


def test_parsed_via_agent_output_type():
    """Agent(output_type=Plan) parses when schema is on the Prompt."""

    class Plan(pydantic.BaseModel):
        summary: str

    agent = Agent(
        prompt=Prompt(
            messages=["go"],
            model="mock-model",
            provider="mock",
            output={"summary": str},
        ),
        after_model_callback=lambda ctx, r: _response('{"summary":"from agent"}'),
        output_type=Plan,
    )
    run = agent.run("go")
    assert isinstance(run.parsed, Plan)
    assert run.parsed.summary == "from agent"


def test_parsed_via_run_output_type_overrides_agent():
    """run(output_type=...) overrides Agent(output_type=...)."""

    class Plan(pydantic.BaseModel):
        summary: str

    class Other(pydantic.BaseModel):
        summary: str

    agent = Agent(
        prompt=Prompt(
            messages=["go"],
            model="mock-model",
            provider="mock",
            output={"summary": str},
        ),
        after_model_callback=lambda ctx, r: _response('{"summary":"run-level"}'),
        output_type=Other,
    )
    run = agent.run("go", output_type=Plan)
    assert isinstance(run.parsed, Plan)


def test_parsed_decode_failure_raises():
    """model_validate_json failure surfaces as WyrdError."""

    class Strict(pydantic.BaseModel):
        required_field: str

    agent = Agent(
        prompt=Prompt(
            messages=["go"],
            model="mock-model",
            provider="mock",
            output=Strict,
        ),
        after_model_callback=lambda ctx, r: _response('{"wrong_field":"value"}'),
    )
    with pytest.raises((WyrdError, pydantic.ValidationError)):
        agent.run("go")


def test_generic_callable_parsed():
    """Non-Pydantic callable works via cls(**dict) path."""

    class MyResult:
        def __init__(self, value: str):
            self.value = value

    agent = Agent(
        prompt=Prompt(
            messages=["go"],
            model="mock-model",
            provider="mock",
            output={"value": str},
        ),
        after_model_callback=lambda ctx, r: _response('{"value":"hello"}'),
        output_type=MyResult,
    )
    run = agent.run("go")
    assert isinstance(run.parsed, MyResult)
    assert run.parsed.value == "hello"
