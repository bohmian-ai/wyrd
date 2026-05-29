import json

import pytest
from wyrd.prompt import Prompt, ProviderRequest, ResponseFormat, WyrdError


def test_prompt_new_builds_openai_chat_by_default() -> None:
    prompt = Prompt("Hello {{name}}", "gpt-4o", provider="openai")

    assert prompt.provider == "openai"
    assert prompt.model == "gpt-4o"
    assert prompt.variables == ["name"]
    body = prompt.request.model_dump()
    assert body["model"] == "gpt-4o"
    assert body["messages"][0]["content"] == "Hello {{name}}"


def test_prompt_new_selects_openai_responses() -> None:
    prompt = Prompt(
        "Summarize {{topic}}",
        "gpt-4.1",
        provider="openai",
        operation="responses",
    )

    body = prompt.request.model_dump()
    assert body["input"][0]["content"][0]["text"] == "Summarize {{topic}}"
    assert prompt.variables == ["topic"]


def test_bind_returns_new_prompt_and_bind_mut_updates_in_place() -> None:
    prompt = Prompt("Hello {{name}} from {{place}}", "claude-sonnet-4", provider="anthropic")

    bound = prompt.bind("name", "Ada")
    assert prompt.variables == ["name", "place"]
    assert bound.variables == ["place"]
    assert "Ada" in bound.model_dump_json()

    bound.bind_mut(place="London")
    assert bound.variables == []
    rendered = bound.render()
    assert isinstance(rendered, ProviderRequest)
    assert "London" in rendered.model_dump_json()


def test_bind_media_replaces_native_media_placeholder() -> None:
    prompt = Prompt("What logo is this?", "gpt-4o", provider="openai").user(
        Prompt.openai_image_url("{{logo}}")
    )

    bound = prompt.bind_media("logo", "https://example.test/logo.png")
    body = bound.request.model_dump()
    image = body["messages"][1]["content"][0]["image_url"]["url"]
    assert image == "https://example.test/logo.png"


def test_bind_media_requires_existing_placeholder() -> None:
    prompt = Prompt("What logo is this?", "gpt-4o", provider="openai")

    with pytest.raises(WyrdError):
        prompt.bind_media("logo", "https://example.test/logo.png")


def test_prompt_response_format_and_str_are_json_inspectable() -> None:
    response_format = ResponseFormat.json_schema(
        "answer",
        {"type": "object", "properties": {"answer": {"type": "string"}}},
    )
    prompt = Prompt(
        "Answer",
        "gpt-4o",
        provider="openai",
        response_format=response_format,
    )

    assert json.loads(str(prompt))["model"] == "gpt-4o"
    assert "response_format" in prompt.request.model_dump()
