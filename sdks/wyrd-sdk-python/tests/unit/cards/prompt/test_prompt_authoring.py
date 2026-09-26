import json

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
    WyrdError,
)


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
    prompt = Prompt("What logo is this? ${media:logo}", "gpt-4o", provider="openai")
    assert prompt.media_variables == ["logo"]

    bound = prompt.bind_media("logo", MediaRef.image_url("https://example.test/logo.png"))
    body = bound.request.model_dump()
    image = body["messages"][0]["content"][1]["image_url"]["url"]
    assert image == "https://example.test/logo.png"
    assert bound.media_variables == []


def test_bind_media_requires_existing_placeholder() -> None:
    prompt = Prompt("What logo is this?", "gpt-4o", provider="openai")

    with pytest.raises(WyrdError):
        prompt.bind_media("logo", MediaRef.image_url("https://example.test/logo.png"))


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


def test_every_provider_builder_constructs_promptcard_and_round_trips_json() -> None:
    prompts = [
        Prompt.openai_chat("gpt-4o", messages="hello"),
        Prompt.openai_responses("gpt-4.1", messages="hello"),
        Prompt.anthropic("claude-sonnet-4", messages="hello"),
        Prompt.gemini("gemini-2.5-pro", messages="hello"),
        Prompt.vertex("gemini-2.5-pro", messages="hello"),
        Prompt.raw("openai", "gpt-4o", b'{"model":"gpt-4o","messages":["hello"]}'),
    ]

    for prompt in prompts:
        card = PromptCard(prompt)
        expected_provider = "google" if prompt.provider == "vertex" else prompt.provider
        assert (
            PromptCard.model_validate_json(card.model_dump_json()).prompt.provider
            == expected_provider
        )


def test_role_helpers_cover_system_user_assistant_and_tool_result() -> None:
    prompt = (
        Prompt.openai_chat("gpt-4o")
        .system("system")
        .user("user")
        .assistant("assistant")
        .tool_result("call-1", "result")
    )
    roles = [message["role"] for message in prompt.request.model_dump()["messages"]]

    assert roles == ["system", "user", "assistant", "tool"]


def test_message_inputs_accept_string_and_list_of_strings() -> None:
    string_prompt = Prompt.anthropic("claude-sonnet-4", messages="hello")
    list_prompt = Prompt.gemini("gemini-2.5-pro", messages=["hello", "world"])

    assert string_prompt.request.model_dump()["messages"][0]["content"][0]["text"] == "hello"
    assert len(list_prompt.request.model_dump()["contents"]) == 2


def test_provider_settings_constructor_smoke() -> None:
    cases = [
        (
            Prompt.openai_chat(
                "gpt-4o",
                messages="hello",
                model_settings=OpenAISettings(seed=1, max_completion_tokens=64),
            ),
            "seed",
            1,
        ),
        (
            Prompt.openai_responses(
                "gpt-4.1",
                messages="hello",
                model_settings=OpenAIResponsesSettings(reasoning={"effort": "medium"}),
            ),
            "reasoning",
            {"effort": "medium"},
        ),
        (
            Prompt.anthropic(
                "claude-sonnet-4",
                messages="hello",
                model_settings=AnthropicSettings(max_tokens=128),
            ),
            "max_tokens",
            128,
        ),
        (
            Prompt.gemini(
                "gemini-2.5-pro",
                messages="hello",
                model_settings=GeminiSettings(generation_config={"temperature": 0.2}),
            ),
            "generation_config",
            {"temperature": 0.2},
        ),
        (
            Prompt.vertex(
                "gemini-2.5-pro",
                messages="hello",
                model_settings=GeminiSettings(generation_config={"max_output_tokens": 64}),
            ),
            "generation_config",
            {"max_output_tokens": 64},
        ),
    ]

    for prompt, key, value in cases:
        if isinstance(value, float):
            assert prompt.request.model_dump()[key] == pytest.approx(value)
        elif key == "generation_config" and "temperature" in value:
            assert prompt.request.model_dump()[key]["temperature"] == pytest.approx(
                value["temperature"]
            )
        else:
            assert prompt.request.model_dump()[key] == value


def test_openai_responses_input_getter_projects_text_and_item_forms() -> None:
    items_prompt = Prompt.openai_responses("gpt-4.1", messages="Hello")
    body = json.loads(items_prompt.model_dump_json())
    body["request"]["input"] = "Hello"
    text_prompt = Prompt.model_validate_json(json.dumps(body))

    for prompt in (items_prompt, text_prompt):
        (item,) = prompt.request.openai_responses().input
        assert item.kind == "message"
        assert item.as_message_role() == "user"
    assert text_prompt.request.model_dump()["input"] == "Hello"
