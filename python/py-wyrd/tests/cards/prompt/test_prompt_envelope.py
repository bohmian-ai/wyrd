import json

import pytest
from wyrd.prompt import Prompt, PromptCard, ResponseFormat, WyrdError


def assert_code(error: pytest.ExceptionInfo[WyrdError], code: str) -> None:
    assert error.value.code == code


def assert_card_error(prompt: Prompt, code: str) -> None:
    with pytest.raises(WyrdError) as error:
        PromptCard(prompt).model_dump_json()
    assert_code(error, code)


def test_happy_path_promptcard_construction() -> None:
    card = PromptCard(Prompt.openai_chat("gpt-4o", messages="Hello {{name}}"))

    assert card.prompt.provider == "openai"
    assert card.parameters == ["name"]


def test_invalid_variable_name_raises_exact_code() -> None:
    prompt = Prompt.openai_chat("gpt-4o", messages="Hello {{name}}", variables=["bad-name"])

    assert_card_error(prompt, "WYRD_PROMPT_400_INVALID_VARIABLE_NAME")


def test_duplicate_variable_raises_exact_code() -> None:
    prompt = Prompt.openai_chat("gpt-4o", messages="Hello {{name}}", variables=["name", "name"])

    assert_card_error(prompt, "WYRD_PROMPT_409_DUPLICATE_VARIABLE")


def test_undeclared_text_placeholder_raises_exact_code() -> None:
    prompt = Prompt.openai_chat("gpt-4o", messages="Hello {{name}}", variables=[])

    assert_card_error(prompt, "WYRD_PROMPT_422_UNDECLARED_PLACEHOLDER")


def test_unreferenced_text_variable_raises_exact_code() -> None:
    prompt = Prompt.openai_chat("gpt-4o", messages="Hello", variables=["name"])

    assert_card_error(prompt, "WYRD_PROMPT_422_UNREFERENCED_VARIABLE")


def test_empty_model_raises_exact_code() -> None:
    with pytest.raises(WyrdError) as error:
        Prompt.openai_chat("", messages="Hello")

    assert_code(error, "WYRD_PROMPT_400_EMPTY_MODEL")


def test_invalid_response_schema_raises_exact_code() -> None:
    with pytest.raises(WyrdError) as error:
        ResponseFormat.json_schema("bad", [])

    assert_code(error, "WYRD_PROMPT_400_INVALID_RESPONSE_SCHEMA")


def test_undeclared_media_placeholder_raises_exact_code() -> None:
    prompt = Prompt.openai_chat("gpt-4o", messages="Hello")
    request = prompt.request.model_dump()
    request["messages"][0]["content"] = "${media:logo}"
    prompt.request = request

    assert_card_error(prompt, "WYRD_PROMPT_422_UNDECLARED_MEDIA_PLACEHOLDER")


def test_unreferenced_media_variable_raises_exact_code() -> None:
    prompt = Prompt("Describe ${media:logo}", "gpt-4o", provider="openai")
    request = prompt.request.model_dump()
    request["messages"][0]["content"] = "Describe this logo"
    prompt.request = request

    assert_card_error(prompt, "WYRD_PROMPT_422_UNREFERENCED_MEDIA_VARIABLE")


def test_promptcard_json_envelope_shape() -> None:
    card = PromptCard(Prompt.openai_chat("gpt-4o", messages="Hello"), name="hello")
    envelope = json.loads(card.model_dump_json())

    assert envelope["apiVersion"] == "wyrd/v1"
    assert envelope["kind"] == "Prompt"
    assert envelope["metadata"]["name"] == "hello"
