"""Rendering a Prompt substitutes its variables into the provider request."""

import pytest
from wyrd.prompt import MediaRef, Prompt, WyrdError


def test_render_substitutes_double_brace_placeholder() -> None:
    rendered = Prompt.openai_chat("gpt-4o", messages="Hello {{name}}").render(name="Ada")

    content = rendered.openai().messages[-1].content
    assert content is not None
    assert content.as_text() == "Hello Ada"


def test_render_dollar_and_double_brace_placeholders() -> None:
    rendered = Prompt.openai_chat("gpt-4o", messages="${name} {name} {{name}}").render(name="Ada")

    content = rendered.openai().messages[-1].content
    assert content is not None
    assert content.as_text() == "Ada {name} Ada"


@pytest.mark.parametrize(
    "prompt",
    [
        pytest.param(Prompt.openai_chat("gpt-4o", messages="Hello {{name}}"), id="openai"),
        pytest.param(
            Prompt.openai_responses("gpt-4.1", messages="Hello {{name}}"), id="openai-responses"
        ),
        pytest.param(
            Prompt.anthropic("claude-sonnet-4", messages="Hello {{name}}"), id="anthropic"
        ),
        pytest.param(Prompt.gemini("gemini-2.5-pro", messages="Hello {{name}}"), id="gemini"),
        pytest.param(Prompt.vertex("gemini-2.5-pro", messages="Hello {{name}}"), id="vertex"),
    ],
)
def test_render_preserves_provider_request_variant(prompt: Prompt) -> None:
    rendered = prompt.render(name="Ada")

    assert rendered.provider == prompt.request.provider


def test_render_missing_variable_raises_exact_code() -> None:
    prompt = Prompt.openai_chat("gpt-4o", messages="Hello {{name}}")

    with pytest.raises(WyrdError) as error:
        prompt.render()

    assert error.value.code == "WYRD_PROMPT_422_MISSING_VARIABLE"


def test_refusal_text_starts_with_its_message() -> None:
    with pytest.raises(WyrdError) as error:
        Prompt.openai_chat("gpt-4o", messages="Hello {{name}}").render()

    assert str(error.value).startswith(error.value.message)


def test_bind_returns_a_new_prompt_and_leaves_the_original_unbound() -> None:
    prompt = Prompt.openai_chat("gpt-4o", messages="Hello {{name}} from {{place}}")

    bound = prompt.bind("name", "Ada")

    assert (prompt.variables, bound.variables) == (["name", "place"], ["place"])


def test_bound_text_and_media_render_together() -> None:
    bound = Prompt.openai_chat(
        "gpt-4o", messages="Hello {{name}} from {{place}} ${media:logo}"
    ).bind("name", "Ada")
    bound = bound.bind_media("logo", MediaRef.image_url("https://example.test/logo.png"))
    bound.bind_mut(place="London")

    content = bound.render().openai().messages[-1].content
    assert content is not None
    parts = content.as_parts()

    assert [part.kind for part in parts] == ["text", "image_url"]
    assert parts[0].as_text() == "Hello Ada from London "
