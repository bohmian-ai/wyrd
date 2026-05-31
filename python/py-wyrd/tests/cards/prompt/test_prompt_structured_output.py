import json

import pytest
from pydantic import BaseModel
from wyrd.prompt import Prompt, ResponseFormat, WyrdError


class Ingredient(BaseModel):
    name: str
    grams: int


class Recipe(BaseModel):
    title: str
    ingredients: list[Ingredient]


def schema_from_request(prompt: Prompt) -> dict:
    body = prompt.request.model_dump()
    if "response_format" in body:
        return body["response_format"]["json_schema"]["schema"]
    if "text" in body:
        return body["text"]["format"]["schema"]
    if "generation_config" in body:
        return body["generation_config"]["response_schema"]
    return prompt.model_dump()["response_type"]["json_schema"]["schema"]


def provider_prompts(response_format: object) -> list[Prompt]:
    return [
        Prompt.openai_chat("gpt-4o", messages="Recipe", response_format=response_format),
        Prompt.openai_responses("gpt-4.1", messages="Recipe", response_format=response_format),
        Prompt.anthropic("claude-sonnet-4", messages="Recipe", response_format=response_format),
        Prompt.gemini("gemini-2.5-pro", messages="Recipe", response_format=response_format),
        Prompt.vertex("gemini-2.5-pro", messages="Recipe", response_format=response_format),
    ]


@pytest.mark.parametrize("prompt", provider_prompts(Recipe))
def test_pydantic_basemodel_schema_extraction(prompt: Prompt) -> None:
    schema = schema_from_request(prompt)
    round_tripped = Prompt.from_json(prompt.model_dump_json())

    assert schema["type"] == "object"
    assert "ingredients" in schema["properties"]
    assert schema_from_request(round_tripped) == schema


def test_response_format_json_schema_rejects_non_object_schema() -> None:
    with pytest.raises(WyrdError) as error:
        ResponseFormat.json_schema("bad", "not-object")

    assert error.value.code == "WYRD_PROMPT_400_INVALID_RESPONSE_SCHEMA"


def test_structured_output_model_dump_json_preserves_schema() -> None:
    prompt = Prompt.openai_chat("gpt-4o", messages="Recipe", response_format=Recipe)
    decoded = json.loads(prompt.model_dump_json())

    assert decoded["response_type"]["json_schema"]["schema"] == schema_from_request(prompt)
