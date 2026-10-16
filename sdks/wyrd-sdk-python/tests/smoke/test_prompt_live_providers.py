"""Opt-in live checks that Wyrd Prompts lower to requests current models accept.

Each case authors one portable or native Prompt, wraps it in the public
``wyrd.agent.Agent`` on its default ``native`` route, and runs it straight
against the provider with no gateway in between, so a failure here is a Prompt
lowering defect or a provider refusal, never a gateway defect. Portable cases
cover the common use cases each provider's current model supports, not every
setting on every model. The Agent's provider clients read the same
conventional credential variables this lane checks.

Run with ``mise run test:gateway:smoke:live``.
"""

from __future__ import annotations

import json
from typing import Any, Literal

import pytest
from wyrd.agent import Agent, AgentRun, local_registry, tool
from wyrd.prompt import Prompt

from .conftest import JEV_QUESTIONS, JEV_STATE, assert_jev_answers, credential, model

Portable = Literal["openai", "anthropic", "gemini"]

PORTABLE = ["openai", "anthropic", "gemini"]

# Lane provider → the provider name a Prompt is authored with.
AUTHORED_PROVIDERS = {
    "openai": "openai",
    "anthropic": "anthropic",
    "gemini": "google",
    "jev": "jev",
}

# Output budget, generous for a one-word answer because a thinking model spends
# it on reasoning first.
MAX_OUTPUT_TOKENS = 512

SYSTEM = "You answer every question with the single word: ready"
INPUT = "Reply with the single word: ready"

# Structured-output schema per provider, each in the form that provider's
# structured-output field accepts: OpenAI strict mode and Anthropic require
# the closed object, and Gemini's ``responseSchema`` refuses
# ``additionalProperties``.
CLOSED_ANSWER_SCHEMA: dict[str, Any] = {
    "type": "object",
    "properties": {"answer": {"type": "string"}},
    "required": ["answer"],
    "additionalProperties": False,
}
ANSWER_SCHEMAS = {
    "openai": CLOSED_ANSWER_SCHEMA,
    "anthropic": CLOSED_ANSWER_SCHEMA,
    "gemini": {k: v for k, v in CLOSED_ANSWER_SCHEMA.items() if k != "additionalProperties"},
}

# Settings each provider's model needs on every portable Prompt: Anthropic
# requires an explicit output budget.
BASE_SETTINGS: dict[str, dict[str, Any]] = {
    "openai": {},
    "anthropic": {"max_output_tokens": MAX_OUTPUT_TOKENS},
    "gemini": {},
}

# Settings each provider's model needs before it accepts function tools: a
# reasoning OpenAI model refuses them while it reasons.
TOOL_SETTINGS: dict[str, dict[str, Any]] = {
    "openai": {"reasoning_effort": "none"},
    "anthropic": {},
    "gemini": {},
}


def prompt(provider: str, **fields: Any) -> Prompt:
    """Compile the authored Prompt ``fields`` for ``provider``'s current model.

    The provider's credential is checked first, so a provider without one is
    skipped before anything about it is compiled or called. The Prompt goes through ``Prompt.model_validate_json``, the same
    compilation a stored Prompt Card body takes, so the authored form under
    test is exactly the dictionary each case writes.
    """
    credential(provider)
    authored = {"provider": AUTHORED_PROVIDERS[provider], "model": model(provider), **fields}
    return Prompt.model_validate_json(json.dumps(authored))


def portable(provider: Portable, **fields: Any) -> Prompt:
    """A portable Prompt for ``provider``, carrying ``fields`` over its base settings.

    A portable Prompt needs its own content, so one without system text opens
    with a greeting turn before the run input.
    """
    if "system" not in fields:
        fields.setdefault("messages", [{"role": "user", "content": "Hello."}])
    settings = {**BASE_SETTINGS[provider], **fields.pop("settings", {})}
    if settings:
        fields["settings"] = settings
    return prompt(provider, **fields)


def run(authored: Prompt) -> AgentRun:
    """Run ``authored`` once through a native-route Agent."""
    return Agent(prompt=authored).run(INPUT)


def assert_text(answer: AgentRun) -> None:
    """Assert ``answer`` finished with non-empty assistant text."""
    assert answer.error is None, f"the run failed: {answer.error}"
    assert answer.output.strip(), "the run returned no text"


@pytest.mark.smoke
@pytest.mark.parametrize("provider", PORTABLE)
def test_portable_text(provider: Portable) -> None:
    """A portable Prompt holding one user turn answers the run input with text."""
    assert_text(run(portable(provider)))


@pytest.mark.smoke
@pytest.mark.parametrize("provider", PORTABLE)
def test_portable_system_and_history(provider: Portable) -> None:
    """System text plus a multi-turn user/assistant history lowers and answers."""
    authored = portable(
        provider,
        system=SYSTEM,
        messages=[
            {"role": "user", "content": "Are you there?"},
            {"role": "assistant", "content": "ready"},
        ],
    )
    assert_text(run(authored))


@pytest.mark.smoke
@pytest.mark.parametrize("provider", PORTABLE)
def test_portable_max_output_tokens(provider: Portable) -> None:
    """``max_output_tokens`` lowers to the provider's own output-budget field."""
    authored = portable(provider, settings={"max_output_tokens": MAX_OUTPUT_TOKENS})
    assert_text(run(authored))


@pytest.mark.smoke
@pytest.mark.parametrize("provider", PORTABLE)
def test_portable_response_format(provider: Portable) -> None:
    """A portable JSON Schema lands in the structured-output field and decodes."""
    authored = portable(provider, response_format=ANSWER_SCHEMAS[provider])
    answer = run(authored)
    assert answer.error is None, f"the run failed: {answer.error}"
    assert answer.structured_output is not None, "the run decoded no structured output"
    assert isinstance(answer.structured_output["answer"], str)


@pytest.mark.smoke
@pytest.mark.parametrize("provider", PORTABLE)
def test_portable_local_tool_completes_the_loop(provider: Portable) -> None:
    """The model calls one local ``@tool`` and the Agent loop answers with its result.

    The tool records each call, so passing proves a real call was decoded,
    executed, and fed back before the model's final text. A scoped registry
    lets each provider's case declare the same tool name.
    """
    authored = portable(
        provider,
        system="Always call get_weather before answering a weather question.",
        settings=TOOL_SETTINGS[provider],
    )
    calls: list[str] = []
    with local_registry():

        @tool(name="get_weather", description="Return the current weather for a city.")
        def get_weather(city: str) -> str:
            calls.append(city)
            return f"It is sunny in {city}."

        answer = Agent(prompt=authored, tools=[get_weather]).run("What is the weather in Paris?")
    assert calls, "the model never called the local tool"
    assert answer.iterations >= 2, "the loop ended without answering the tool result"
    assert_text(answer)


@pytest.mark.smoke
def test_native_chat_completion() -> None:
    """A native OpenAI Chat Completions body runs as written."""
    authored = prompt(
        "openai",
        format="chat_completion",
        body={
            "messages": [{"role": "system", "content": SYSTEM}],
            "max_completion_tokens": MAX_OUTPUT_TOKENS,
        },
    )
    assert_text(run(authored))


@pytest.mark.smoke
def test_native_responses() -> None:
    """A native OpenAI Responses body runs as written."""
    authored = prompt(
        "openai",
        format="responses",
        body={"instructions": SYSTEM, "input": [], "max_output_tokens": MAX_OUTPUT_TOKENS},
    )
    assert_text(run(authored))


@pytest.mark.smoke
def test_native_messages() -> None:
    """A native Anthropic Messages body runs as written."""
    authored = prompt(
        "anthropic",
        format="messages",
        body={"system": SYSTEM, "messages": [], "max_tokens": MAX_OUTPUT_TOKENS},
    )
    assert_text(run(authored))


@pytest.mark.smoke
def test_native_generate_content() -> None:
    """A native Gemini GenerateContent body runs as written.

    The ``role`` on ``system_instruction`` is Wyrd's requirement, not
    Gemini's: the native body decoder refuses a system instruction without
    one (finding PRM-2).
    """
    authored = prompt(
        "gemini",
        format="generate_content",
        body={
            "system_instruction": {"role": "user", "parts": [{"text": SYSTEM}]},
            "contents": [],
        },
    )
    assert_text(run(authored))


@pytest.mark.smoke
def test_native_system_one() -> None:
    """A native Jev ``systemone`` body answers its typed questions."""
    authored = prompt(
        "jev",
        format="system_one",
        body={"state": JEV_STATE, "questions": JEV_QUESTIONS},
    )
    answer = run(authored)
    assert answer.error is None, f"the run failed: {answer.error}"
    assert_jev_answers(json.loads(answer.output))
