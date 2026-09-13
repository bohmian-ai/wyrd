from __future__ import annotations

import pydantic
import pytest
from wyrd import Prompt, WyrdError
from wyrd.prompt import ResponseFormat


def test_output_dict_of_types_builds_response_format() -> None:
    prompt = Prompt(
        messages=["Plan it."],
        model="gpt-test",
        provider="openai",
        output={"summary": str, "steps": list[str]},
    )
    fmt = prompt.request.model_dump()["response_format"]

    assert fmt["type"] == "json_schema"
    schema = fmt["json_schema"]["schema"]
    assert schema["properties"]["summary"] == {"type": "string"}
    assert schema["properties"]["steps"] == {
        "type": "array",
        "items": {"type": "string"},
    }
    assert sorted(schema["required"]) == ["steps", "summary"]


def test_output_pydantic_model_uses_model_json_schema() -> None:
    class Plan(pydantic.BaseModel):
        summary: str
        steps: list[str]

    prompt = Prompt(
        messages=["Plan it."],
        model="gpt-test",
        provider="openai",
        output=Plan,
    )
    fmt = prompt.request.model_dump()["response_format"]

    assert fmt["type"] == "json_schema"
    assert fmt["json_schema"]["name"] == "Plan"


def test_output_raw_schema_dict_passes_through() -> None:
    raw = {
        "type": "object",
        "properties": {"x": {"type": "string"}},
        "required": ["x"],
    }
    prompt = Prompt(messages=["Hi."], model="gpt-test", provider="openai", output=raw)
    fmt = prompt.request.model_dump()["response_format"]

    assert fmt["json_schema"]["schema"]["properties"]["x"] == {"type": "string"}


def test_output_wins_over_response_format() -> None:
    prompt = Prompt(
        messages=["Hi."],
        model="gpt-test",
        provider="openai",
        response_format=ResponseFormat.json_object(),
        output={"x": str},
    )
    fmt = prompt.request.model_dump()["response_format"]

    assert fmt["type"] == "json_schema"


def test_output_invalid_class_raises() -> None:
    class NotAModel:
        pass

    with pytest.raises(WyrdError) as exc:
        Prompt(messages=["Hi."], model="gpt-test", provider="openai", output=NotAModel)

    assert exc.value.code in {
        "WYRD_PROMPT_422_INVALID_OUTPUT_SCHEMA",
        "WYRD_PROMPT_422_PYDANTIC_REQUIRED",
        "SKALD_PROMPT_422_VALIDATION",
        "WYRD_PROMPT_400_DRAFT_INVALID",
    }
