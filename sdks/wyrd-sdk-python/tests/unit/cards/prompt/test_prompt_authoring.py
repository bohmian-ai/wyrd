"""Authoring a Prompt for each provider through its builders and settings."""

import json
from pathlib import Path
from typing import Any

import pytest
from wyrd.prompt import (
    AnthropicSettings,
    GeminiSettings,
    MediaRef,
    OpenAIResponsesSettings,
    OpenAISettings,
    Prompt,
    PromptCard,
    ProviderRequest,
    ResponseFormat,
)


def test_prompt_new_builds_openai_chat_by_default() -> None:
    prompt = Prompt("Hello {{name}}", "gpt-4o", provider="openai")

    request = prompt.request.openai()
    assert (prompt.provider, request.model, prompt.variables) == ("openai", "gpt-4o", ["name"])
    content = request.messages[0].content
    assert content is not None
    assert content.as_text() == "Hello {{name}}"


def test_prompt_new_selects_openai_responses() -> None:
    prompt = Prompt("Summarize {{topic}}", "gpt-4.1", provider="openai", operation="responses")

    (item,) = prompt.request.openai_responses().input
    assert (item.kind, prompt.variables) == ("message", ["topic"])


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
    prompt = Prompt("What logo is this? ${media:logo}", "gpt-4o", provider="openai")

    bound = prompt.bind_media("logo", MediaRef.image_url("https://example.test/logo.png"))

    content = bound.request.openai().messages[0].content
    assert content is not None
    part = content.as_parts()[1]
    assert part.as_image_url().url == "https://example.test/logo.png"


def test_prompt_str_is_json_with_its_model() -> None:
    prompt = Prompt("Answer", "gpt-4o", provider="openai")

    assert json.loads(str(prompt))["model"] == "gpt-4o"


def test_response_format_json_schema_sets_the_response_schema() -> None:
    schema = {"type": "object", "properties": {"answer": {"type": "string"}}}

    prompt = Prompt(
        "Answer",
        "gpt-4o",
        provider="openai",
        response_format=ResponseFormat.json_schema("answer", schema),
    )

    assert prompt.response_schema_name == "answer"


@pytest.mark.parametrize(
    ("prompt", "provider"),
    [
        pytest.param(Prompt.openai_chat("gpt-4o", messages="hello"), "openai", id="openai"),
        pytest.param(
            Prompt.openai_responses("gpt-4.1", messages="hello"), "openai", id="openai-responses"
        ),
        pytest.param(
            Prompt.anthropic("claude-sonnet-4", messages="hello"), "anthropic", id="anthropic"
        ),
        pytest.param(Prompt.gemini("gemini-2.5-pro", messages="hello"), "google", id="gemini"),
        pytest.param(Prompt.vertex("gemini-2.5-pro", messages="hello"), "vertex", id="vertex"),
        pytest.param(
            Prompt.raw("openai", "gpt-4o", b'{"model":"gpt-4o","messages":["hello"]}'),
            "openai",
            id="raw",
        ),
    ],
)
def test_every_provider_builder_round_trips_through_a_promptcard(
    prompt: Prompt, provider: str
) -> None:
    card = PromptCard(prompt)

    assert PromptCard.model_validate_json(card.model_dump_json()).prompt.provider == provider


def test_role_helpers_cover_system_user_assistant_and_tool_result() -> None:
    prompt = (
        Prompt.openai_chat("gpt-4o")
        .system("system")
        .user("user")
        .assistant("assistant")
        .tool_result("call-1", "result")
    )

    assert [message.role for message in prompt.request.openai().messages] == [
        "system",
        "user",
        "assistant",
        "tool",
    ]


def test_message_string_becomes_one_anthropic_text_block() -> None:
    prompt = Prompt.anthropic("claude-sonnet-4", messages="hello")

    assert prompt.request.messages[0]["content"] == [{"type": "text", "text": "hello"}]


def test_message_list_becomes_one_gemini_content_turn_per_string() -> None:
    prompt = Prompt.gemini("gemini-2.5-pro", messages=["hello", "world"])

    assert [content.parts[0].as_text() for content in prompt.request.gemini().contents] == [
        "hello",
        "world",
    ]


@pytest.mark.parametrize(
    ("prompt", "key", "value"),
    [
        pytest.param(
            Prompt.openai_chat(
                "gpt-4o",
                messages="hello",
                model_settings=OpenAISettings(seed=1, max_completion_tokens=64),
            ),
            "seed",
            1,
            id="openai",
        ),
        pytest.param(
            Prompt.openai_responses(
                "gpt-4.1",
                messages="hello",
                model_settings=OpenAIResponsesSettings(reasoning={"effort": "medium"}),
            ),
            "reasoning",
            {"effort": "medium"},
            id="openai-responses",
        ),
        pytest.param(
            Prompt.anthropic(
                "claude-sonnet-4",
                messages="hello",
                model_settings=AnthropicSettings(max_tokens=128),
            ),
            "max_tokens",
            128,
            id="anthropic",
        ),
        pytest.param(
            Prompt.gemini(
                "gemini-2.5-pro",
                messages="hello",
                model_settings=GeminiSettings(generation_config={"candidate_count": 2}),
            ),
            "generation_config",
            {"candidate_count": 2},
            id="gemini",
        ),
        pytest.param(
            Prompt.vertex(
                "gemini-2.5-pro",
                messages="hello",
                model_settings=GeminiSettings(generation_config={"max_output_tokens": 64}),
            ),
            "generation_config",
            {"max_output_tokens": 64},
            id="vertex",
        ),
    ],
)
def test_provider_settings_land_in_the_request(prompt: Prompt, key: str, value: Any) -> None:
    assert prompt.request.model_dump()["body"][key] == value


def test_openai_responses_item_input_reads_as_one_user_message() -> None:
    (item,) = Prompt.openai_responses("gpt-4.1", messages="Hello").request.openai_responses().input

    assert (item.kind, item.as_message_role()) == ("message", "user")


def test_openai_responses_text_input_reads_as_one_user_message(fixtures_dir: Path) -> None:
    prompt = Prompt.load(fixtures_dir / "authoring" / "prompt" / "responses-text-input.yaml")

    (item,) = prompt.request.openai_responses().input
    assert (item.kind, item.as_message_role()) == ("message", "user")
