"""Opt-in live checks that Wyrd Prompts lower to requests current models accept.

Each case authors one portable or native Prompt, renders it with the public
``Prompt.render``, appends the fixed run input as the application's own turn,
and posts the native body straight to the provider with no gateway in between,
so a failure here is a Prompt lowering defect or a provider refusal, never a
gateway defect. Portable cases cover the common use cases each provider's
current model supports, not every setting on every model. Requests use the
same conventional credential variables this lane checks.

Run with ``mise run test:gateway:smoke:live``.
"""

from __future__ import annotations

import json
from typing import Any, Literal

import httpx
import pytest
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

# One structured-output schema for every provider: closed, nested, nullable,
# and `$ref`-bearing, the shape OpenAI strict mode demands. A portable Prompt
# authors it once; lowering must carry it to each provider's JSON Schema field.
ANSWER_SCHEMA: dict[str, Any] = {
    "type": "object",
    "properties": {
        "answer": {"type": "string"},
        "note": {"anyOf": [{"type": "string"}, {"type": "null"}]},
        "items": {"type": "array", "items": {"$ref": "#/$defs/item"}},
    },
    "required": ["answer", "note", "items"],
    "additionalProperties": False,
    "$defs": {
        "item": {
            "type": "object",
            "properties": {"id": {"type": "integer"}},
            "required": ["id"],
            "additionalProperties": False,
        }
    },
}

# Settings each provider's model needs on every portable Prompt: Anthropic
# requires an explicit output budget.
BASE_SETTINGS: dict[str, dict[str, Any]] = {
    "openai": {},
    "anthropic": {"max_output_tokens": MAX_OUTPUT_TOKENS},
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


def endpoint(api: str) -> tuple[str, str, dict[str, str]]:
    """The lane provider, provider URL, and auth headers for the rendered ``api`` body."""
    if api in ("open_ai_chat_completion", "open_ai_responses"):
        path = "chat/completions" if api == "open_ai_chat_completion" else "responses"
        key = credential("openai")
        return "openai", f"https://api.openai.com/v1/{path}", {"authorization": f"Bearer {key}"}
    if api == "anthropic_message":
        headers = {"x-api-key": credential("anthropic"), "anthropic-version": "2023-06-01"}
        return "anthropic", "https://api.anthropic.com/v1/messages", headers
    if api == "gemini_generate_content":
        url = f"https://generativelanguage.googleapis.com/v1beta/models/{model('gemini')}:generateContent"
        return "gemini", url, {"x-goog-api-key": credential("gemini")}
    assert api == "jev_system_one", f"no provider endpoint for {api}"
    return (
        "jev",
        "https://api.typesafe.ai/v1/systemone",
        {"authorization": f"Bearer {credential('jev')}"},
    )


def with_input(api: str, body: dict[str, Any]) -> dict[str, Any]:
    """``body`` with the run input appended as the next user turn of its native shape.

    A Jev ``systemone`` body asks typed questions about its own state and has
    no turn to append.
    """
    if api in ("open_ai_chat_completion", "anthropic_message"):
        body["messages"].append({"role": "user", "content": INPUT})
    elif api == "open_ai_responses":
        body["input"].append({"role": "user", "content": INPUT})
    elif api == "gemini_generate_content":
        body["contents"].append({"role": "user", "parts": [{"text": INPUT}]})
    return body


def call(authored: Prompt) -> tuple[str, dict[str, Any]]:
    """Render ``authored``, add the run input, and post it to its provider once.

    Returns the rendered API tag and the provider's decoded reply. Only the
    provider and status reach the failure message, never a credential.
    """
    rendered = authored.render().model_dump()
    api = rendered["provider"]
    provider, url, headers = endpoint(api)
    reply = httpx.post(url, json=with_input(api, rendered["body"]), headers=headers, timeout=120)
    assert reply.status_code == 200, f"{provider} answered {reply.status_code}: {reply.text}"
    return api, reply.json()


def text(api: str, reply: dict[str, Any]) -> str:
    """The assistant text of the ``api`` provider ``reply``."""
    if api == "open_ai_chat_completion":
        return reply["choices"][0]["message"]["content"] or ""
    if api == "open_ai_responses":
        return "".join(
            part["text"]
            for item in reply["output"]
            if item["type"] == "message"
            for part in item["content"]
            if part["type"] == "output_text"
        )
    if api == "anthropic_message":
        return "".join(block["text"] for block in reply["content"] if block["type"] == "text")
    return "".join(part.get("text", "") for part in reply["candidates"][0]["content"]["parts"])


def assert_text(authored: Prompt) -> None:
    """Assert ``authored``'s provider answered the run input with non-empty text."""
    assert text(*call(authored)).strip(), "the provider returned no text"


@pytest.mark.smoke
@pytest.mark.parametrize("provider", PORTABLE)
def test_portable_text(provider: Portable) -> None:
    """A portable Prompt holding one user turn answers the run input with text."""
    assert_text(portable(provider))


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
    assert_text(authored)


@pytest.mark.smoke
@pytest.mark.parametrize("provider", PORTABLE)
def test_portable_max_output_tokens(provider: Portable) -> None:
    """``max_output_tokens`` lowers to the provider's own output-budget field."""
    assert_text(portable(provider, settings={"max_output_tokens": MAX_OUTPUT_TOKENS}))


@pytest.mark.smoke
@pytest.mark.parametrize("provider", PORTABLE)
def test_portable_response_format(provider: Portable) -> None:
    """One closed, nested JSON Schema reaches every provider and decodes."""
    answer = json.loads(text(*call(portable(provider, response_format=ANSWER_SCHEMA))))
    assert isinstance(answer["answer"], str)


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
    assert_text(authored)


@pytest.mark.smoke
def test_native_responses() -> None:
    """A native OpenAI Responses body runs as written."""
    authored = prompt(
        "openai",
        format="responses",
        body={"instructions": SYSTEM, "input": [], "max_output_tokens": MAX_OUTPUT_TOKENS},
    )
    assert_text(authored)


@pytest.mark.smoke
def test_native_messages() -> None:
    """A native Anthropic Messages body runs as written."""
    authored = prompt(
        "anthropic",
        format="messages",
        body={"system": SYSTEM, "messages": [], "max_tokens": MAX_OUTPUT_TOKENS},
    )
    assert_text(authored)


@pytest.mark.smoke
def test_native_generate_content() -> None:
    """A native Gemini GenerateContent body runs as written, with the
    role-less ``system_instruction`` Gemini's own API accepts."""
    authored = prompt(
        "gemini",
        format="generate_content",
        body={
            "system_instruction": {"parts": [{"text": SYSTEM}]},
            "contents": [],
        },
    )
    assert_text(authored)


@pytest.mark.smoke
def test_native_system_one() -> None:
    """A native Jev ``systemone`` body answers its typed questions."""
    authored = prompt(
        "jev",
        format="system_one",
        body={"state": JEV_STATE, "questions": JEV_QUESTIONS},
    )
    _, reply = call(authored)
    assert_jev_answers(reply["answers"])
