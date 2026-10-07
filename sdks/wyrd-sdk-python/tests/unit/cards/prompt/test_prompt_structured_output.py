import pytest
from pydantic import BaseModel
from wyrd.prompt import Prompt, ResponseFormat, WyrdError


class Ingredient(BaseModel):
    name: str
    grams: int


class Recipe(BaseModel):
    title: str
    ingredients: list[Ingredient]


def provider_prompts(response_format: type[BaseModel]) -> list[Prompt]:
    return [
        Prompt.openai_chat("gpt-4o", messages="Recipe", response_format=response_format),
        Prompt.openai_responses("gpt-4.1", messages="Recipe", response_format=response_format),
        Prompt.anthropic("claude-sonnet-4", messages="Recipe", response_format=response_format),
        Prompt.gemini("gemini-2.5-pro", messages="Recipe", response_format=response_format),
        Prompt.vertex("gemini-2.5-pro", messages="Recipe", response_format=response_format),
    ]


@pytest.mark.parametrize("prompt", provider_prompts(Recipe))
def test_pydantic_basemodel_schema_extraction(prompt: Prompt) -> None:
    schema = prompt.response_schema
    round_tripped = Prompt.model_validate_json(prompt.model_dump_json())

    assert schema is not None
    assert schema["type"] == "object"
    assert "ingredients" in schema["properties"]
    assert prompt.response_schema_name == "response"
    assert round_tripped.response_schema == schema
    assert round_tripped.response_schema_name == "response"


def test_response_schema_is_none_without_structured_output() -> None:
    prompt = Prompt.openai_chat("gpt-4o", messages="Recipe")

    assert prompt.response_schema is None
    assert prompt.response_schema_name is None


def test_response_format_json_schema_rejects_non_object_schema() -> None:
    with pytest.raises(WyrdError) as error:
        ResponseFormat.json_schema("bad", "not-object")  # ty: ignore[invalid-argument-type]

    assert error.value.code == "WYRD_PROMPT_400_INVALID_RESPONSE_SCHEMA"
