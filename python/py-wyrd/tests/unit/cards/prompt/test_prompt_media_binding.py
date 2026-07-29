import json

import pytest
from wyrd.prompt import MediaRef, Prompt, WyrdError


def assert_code(error: pytest.ExceptionInfo[WyrdError], code: str) -> None:
    assert error.value.code == code


def test_media_variables_first_seen_deduped_order() -> None:
    prompt = Prompt.openai_chat(
        "gpt-4o", messages=["${media:logo}", "${media:doc}", "${media:logo}"]
    )

    assert prompt.media_variables == ["logo", "doc"]


def test_bind_media_returns_new_prompt_and_bind_media_mut_mutates() -> None:
    prompt = Prompt("see ${media:logo}", "gpt-4o", provider="openai")
    bound = prompt.bind_media("logo", MediaRef.image_url("https://example.test/logo.png"))

    assert prompt.media_variables == ["logo"]
    assert bound.media_variables == []

    prompt.bind_media_mut("logo", MediaRef.image_url("https://example.test/logo.png"))
    assert prompt.media_variables == []


def test_bind_media_openai_anthropic_gemini_and_vertex_native_replacement() -> None:
    openai = Prompt("see ${media:img}", "gpt-4o", provider="openai").bind_media(
        "img", MediaRef.image_url("https://example.test/logo.png")
    )
    anthropic = Prompt("see ${media:img}", "claude-sonnet-4", provider="anthropic").bind_media(
        "img", MediaRef.image_base64("image/png", "QUJD")
    )
    gemini = Prompt("see ${media:img}", "gemini-2.5-pro", provider="gemini").bind_media(
        "img", MediaRef.image_base64("image/png", "QUJD")
    )
    vertex = Prompt("see ${media:img}", "gemini-2.5-pro", provider="vertex").bind_media(
        "img", MediaRef.image_base64("image/png", "QUJD")
    )

    assert openai.request.model_dump()["messages"][0]["content"][1]["type"] == "image_url"
    assert anthropic.request.model_dump()["messages"][0]["content"][1]["type"] == "image"
    assert "inline_data" in gemini.request.model_dump()["contents"][0]["parts"][1]
    assert "inline_data" in vertex.request.model_dump()["contents"][0]["parts"][1]


def test_bind_media_missing_placeholder_raises_exact_code() -> None:
    prompt = Prompt("see ${media:logo}", "gpt-4o", provider="openai")

    with pytest.raises(WyrdError) as error:
        prompt.bind_media("missing", MediaRef.image_url("https://example.test/logo.png"))

    assert_code(error, "WYRD_PROMPT_422_UNDECLARED_MEDIA_PLACEHOLDER")


def test_system_message_media_rejection_raises_exact_code() -> None:
    with pytest.raises(WyrdError) as error:
        Prompt("hi", "claude-sonnet-4", provider="anthropic", system="${media:logo}")

    assert_code(error, "WYRD_PROMPT_400_MEDIA_IN_SYSTEM_MESSAGE")


def test_text_and_media_binding_are_independent() -> None:
    prompt = Prompt("Hi {{name}} ${media:logo}", "gpt-4o", provider="openai")
    text_bound = prompt.bind("name", "Ada")

    assert text_bound.variables == []
    assert text_bound.media_variables == ["logo"]
    assert "${media:logo}" in json.dumps(text_bound.request.model_dump())

    media_bound = text_bound.bind_media("logo", MediaRef.image_url("https://example.test/logo.png"))
    assert media_bound.media_variables == []
    assert "{{name}}" not in json.dumps(media_bound.request.model_dump())


def test_text_value_containing_media_token_remains_literal() -> None:
    prompt = Prompt("Hi {{name}}", "gpt-4o", provider="openai").bind("name", "${media:hack}")

    assert prompt.media_variables == []
    assert "${media:hack}" in json.dumps(prompt.request.model_dump())


def test_multiple_media_placeholders_in_one_message_bind_cleanly() -> None:
    prompt = Prompt("A ${media:a} B ${media:b}", "gpt-4o", provider="openai")
    bound = prompt.bind_media("a", MediaRef.image_url("https://example.test/a.png")).bind_media(
        "b", MediaRef.image_url("https://example.test/b.png")
    )
    content = bound.request.model_dump()["messages"][0]["content"]

    assert [part["type"] for part in content] == ["text", "image_url", "text", "image_url"]
    assert bound.media_variables == []
