"""``${media:name}`` placeholders bind independently of text variables."""

import json

import pytest
from wyrd.prompt import MediaRef, Prompt, WyrdError

LOGO = MediaRef.image_url("https://example.test/logo.png")


def test_media_variables_first_seen_deduped_order() -> None:
    prompt = Prompt.openai_chat(
        "gpt-4o", messages=["${media:logo}", "${media:doc}", "${media:logo}"]
    )

    assert prompt.media_variables == ["logo", "doc"]


def test_bind_media_returns_new_prompt_and_bind_media_mut_mutates() -> None:
    prompt = Prompt("see ${media:logo}", "gpt-4o", provider="openai")
    bound = prompt.bind_media("logo", LOGO)

    assert prompt.media_variables == ["logo"]
    assert bound.media_variables == []

    prompt.bind_media_mut("logo", LOGO)
    assert prompt.media_variables == []


def test_openai_bound_media_becomes_an_image_url_part() -> None:
    prompt = Prompt("see ${media:img}", "gpt-4o", provider="openai").bind_media("img", LOGO)

    content = prompt.request.openai().messages[0].content
    assert content is not None
    assert content.as_parts()[1].kind == "image_url"


@pytest.mark.parametrize("provider", ["google", "vertex"])
def test_google_bound_media_becomes_an_inline_data_part(provider: str) -> None:
    prompt = Prompt("see ${media:img}", "gemini-2.5-pro", provider=provider).bind_media(
        "img", MediaRef.image_base64("image/png", "QUJD")
    )

    assert prompt.request.gemini().contents[0].parts[1].kind == "inline_data"


def test_anthropic_bound_media_becomes_an_image_block() -> None:
    prompt = Prompt("see ${media:img}", "claude-sonnet-4", provider="anthropic").bind_media(
        "img", MediaRef.image_base64("image/png", "QUJD")
    )

    assert prompt.request.messages[0]["content"][1]["type"] == "image"


def test_bind_media_missing_placeholder_raises_exact_code() -> None:
    prompt = Prompt("see ${media:logo}", "gpt-4o", provider="openai")

    with pytest.raises(WyrdError) as error:
        prompt.bind_media("missing", LOGO)

    assert error.value.code == "WYRD_PROMPT_422_UNDECLARED_MEDIA_PLACEHOLDER"


@pytest.mark.parametrize("provider", ["openai", "anthropic", "google"])
def test_portable_binding_refuses_the_other_media_kind(provider: str) -> None:
    prompt = Prompt.model_validate_json(
        json.dumps(
            {
                "provider": provider,
                "model": "model-1",
                "settings": {"max_output_tokens": 8},
                "messages": [
                    {
                        "role": "user",
                        "content": [
                            {"type": "image", "source": "${media:photo}"},
                            {"type": "document", "source": "${media:report}"},
                        ],
                    }
                ],
            }
        )
    )
    image = MediaRef.image_base64("image/png", "iVBORw0KGgo=")
    document = MediaRef.document_base64("application/pdf", "JVBERi0=")

    for name, media in [("photo", document), ("report", image)]:
        with pytest.raises(WyrdError) as error:
            prompt.bind_media_mut(name, media)
        assert error.value.code == "WYRD_PROMPT_400_INVALID_MEDIA_TYPE"
    assert prompt.media_variables == ["photo", "report"]

    bound = prompt.bind_media("photo", image).bind_media("report", document)
    assert bound.media_variables == []


def test_render_with_unbound_media_raises_missing_media_variable() -> None:
    with pytest.raises(WyrdError) as error:
        Prompt("x ${media:m}", "gpt-4o", provider="openai").render()

    assert error.value.code == "WYRD_PROMPT_422_MISSING_MEDIA_VARIABLE"


def test_binding_text_leaves_the_media_placeholder_unbound() -> None:
    text_bound = Prompt("Hi {{name}} ${media:logo}", "gpt-4o", provider="openai").bind(
        "name", "Ada"
    )

    assert (text_bound.variables, text_bound.media_variables) == ([], ["logo"])


def test_binding_media_after_text_completes_the_request() -> None:
    text_bound = Prompt("Hi {{name}} ${media:logo}", "gpt-4o", provider="openai").bind(
        "name", "Ada"
    )

    content = text_bound.bind_media("logo", LOGO).request.openai().messages[0].content
    assert content is not None
    parts = content.as_parts()

    assert [part.kind for part in parts] == ["text", "image_url"]
    assert parts[0].as_text() == "Hi Ada "


def test_text_value_containing_media_token_remains_literal() -> None:
    prompt = Prompt("Hi {{name}}", "gpt-4o", provider="openai").bind("name", "${media:hack}")

    assert prompt.media_variables == []
    content = prompt.request.openai().messages[0].content
    assert content is not None
    assert content.as_text() == "Hi ${media:hack}"


def test_multiple_media_placeholders_in_one_message_bind_cleanly() -> None:
    prompt = Prompt("A ${media:a} B ${media:b}", "gpt-4o", provider="openai")
    bound = prompt.bind_media("a", MediaRef.image_url("https://example.test/a.png")).bind_media(
        "b", MediaRef.image_url("https://example.test/b.png")
    )

    content = bound.request.openai().messages[0].content
    assert content is not None
    parts = content.as_parts()
    assert [part.kind for part in parts] == ["text", "image_url", "text", "image_url"]
