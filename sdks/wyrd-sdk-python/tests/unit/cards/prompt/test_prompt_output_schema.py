"""A Prompt's ``output`` becomes the JSON schema its provider is asked to follow."""

from typing import Any

import pydantic
import pytest
from wyrd import Prompt, WyrdError
from wyrd.prompt import ResponseFormat


def test_output_dict_of_types_builds_a_strict_object_schema() -> None:
    prompt = Prompt(
        messages=["Plan it."],
        model="gpt-test",
        provider="openai",
        output={"summary": str, "steps": list[str]},
    )

    assert prompt.response_schema == {
        "type": "object",
        "properties": {
            "summary": {"type": "string"},
            "steps": {"type": "array", "items": {"type": "string"}},
        },
        "required": ["summary", "steps"],
        "additionalProperties": False,
    }


def test_output_pydantic_model_names_the_schema_after_the_class() -> None:
    class Plan(pydantic.BaseModel):
        summary: str
        steps: list[str]

    prompt = Prompt(messages=["Plan it."], model="gpt-test", provider="openai", output=Plan)

    assert prompt.response_schema_name == "Plan"


def test_output_raw_schema_dict_passes_through() -> None:
    raw = {"type": "object", "properties": {"x": {"type": "string"}}, "required": ["x"]}

    prompt = Prompt(messages=["Hi."], model="gpt-test", provider="openai", output=raw)

    assert prompt.response_schema == {**raw, "additionalProperties": False}


def test_output_keeps_an_authored_additional_properties() -> None:
    raw = {"type": "object", "properties": {"x": {"type": "string"}}, "additionalProperties": True}

    prompt = Prompt(messages=["go"], model="gpt-test", provider="openai", output=raw)

    assert prompt.response_schema == raw


def test_output_wins_over_response_format() -> None:
    prompt = Prompt(
        messages=["Hi."],
        model="gpt-test",
        provider="openai",
        response_format=ResponseFormat.json_object(),
        output={"x": str},
    )

    assert prompt.response_schema is not None


class NotAModel:
    pass


@pytest.mark.parametrize(
    "output", [pytest.param(NotAModel, id="plain-class"), pytest.param(42, id="int")]
)
def test_output_that_is_not_a_schema_is_refused(output: Any) -> None:
    with pytest.raises(WyrdError) as exc:
        Prompt(messages=["Hi."], model="gpt-test", provider="openai", output=output)

    assert exc.value.code == "WYRD_PROMPT_400_DRAFT_INVALID"
