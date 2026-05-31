import json

import pytest
from wyrd.prompt import MediaRef, Prompt, ProviderRequest, WyrdError


def assert_code(error: pytest.ExceptionInfo[WyrdError], code: str) -> None:
    assert error.value.code == code


def rendered_text(request: ProviderRequest) -> str:
    body = request.model_dump()
    if "messages" in body:
        content = body["messages"][-1]["content"]
        if isinstance(content, list):
            return "".join(part.get("text", "") for part in content)
        return content
    if "input" in body:
        return body["input"][-1]["content"][0]["text"]
    return body["contents"][-1]["parts"][0]["text"]


def provider_prompts() -> list[Prompt]:
    return [
        Prompt.openai_chat("gpt-4o", messages="Hello {{name}}"),
        Prompt.openai_responses("gpt-4.1", messages="Hello {{name}}"),
        Prompt.anthropic("claude-sonnet-4", messages="Hello {{name}}"),
        Prompt.gemini("gemini-2.5-pro", messages="Hello {{name}}"),
        Prompt.vertex("gemini-2.5-pro", messages="Hello {{name}}"),
    ]


def test_render_substitutes_double_brace_placeholder() -> None:
    rendered = Prompt.openai_chat("gpt-4o", messages="Hello {{name}}").render(name="Ada")

    assert rendered_text(rendered) == "Hello Ada"


def test_render_dollar_and_single_brace_remain_literal() -> None:
    rendered = Prompt.openai_chat("gpt-4o", messages="${name} {name} {{name}}").render(name="Ada")

    assert rendered_text(rendered) == "${name} {name} Ada"


@pytest.mark.parametrize("prompt", provider_prompts())
def test_render_preserves_provider_request_variant(prompt: Prompt) -> None:
    rendered = prompt.render(name="Ada")

    assert rendered.provider == prompt.provider


def test_render_missing_variable_raises_exact_code() -> None:
    prompt = Prompt.openai_chat("gpt-4o", messages="Hello {{name}}")

    with pytest.raises(WyrdError) as error:
        prompt.render()

    assert_code(error, "WYRD_PROMPT_422_MISSING_VARIABLE")


def test_bind_bind_mut_and_render_compose_without_mutating_original() -> None:
    prompt = Prompt.anthropic(
        "claude-sonnet-4", messages="Hello {{name}} from {{place}} ${media:logo}"
    )
    bound = prompt.bind("name", "Ada")

    assert prompt.variables == ["name", "place"]
    assert bound.variables == ["place"]

    bound = bound.bind_media("logo", MediaRef.image_url("https://example.test/logo.png"))
    bound.bind_mut(place="London")
    rendered = bound.render()

    assert bound.variables == []
    assert bound.media_variables == []
    assert "Ada" in json.dumps(rendered.model_dump())
    assert "London" in json.dumps(rendered.model_dump())
    assert prompt.media_variables == ["logo"]
