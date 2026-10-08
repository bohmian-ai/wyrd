"""A PromptCard saves, loads, and serializes without change."""

import re
from pathlib import Path

import pytest
from wyrd.prompt import Prompt, PromptCard


@pytest.fixture
def card() -> PromptCard:
    """An OpenAI lead-scoring Prompt Card with one unbound variable."""
    return PromptCard(
        Prompt.openai_chat("gpt-4o", messages="Hello {{name}}", model_settings={"seed": 7}),
        space="growth",
        name="lead-scoring",
        version="0.1.0",
    )


def test_save_then_load_json_roundtrip(tmp_path: Path, card: PromptCard) -> None:
    path = tmp_path / "prompt.json"

    card.save(path)
    loaded = PromptCard.load(path)

    assert loaded.model_dump() == card.model_dump()


def test_save_then_from_path_yaml_roundtrip(tmp_path: Path, card: PromptCard) -> None:
    path = tmp_path / "prompt.yaml"

    card.save(path)
    loaded = PromptCard.from_path(path)

    assert loaded.model_dump() == card.model_dump()


def test_model_dump_json_then_model_validate_json_roundtrip(card: PromptCard) -> None:

    loaded = PromptCard.model_validate_json(card.model_dump_json())

    assert loaded.model_dump() == card.model_dump()


def test_content_hash_format_and_stability(tmp_path: Path, card: PromptCard) -> None:
    path = tmp_path / "prompt.json"
    before = card.content_hash

    card.save(path)
    after = PromptCard.load(path).content_hash

    assert re.fullmatch(r"sha256:[0-9a-f]{64}", before)
    assert before == after


def test_promptcard_parameters_and_bound_state(card: PromptCard) -> None:
    unbound = card
    bound = PromptCard(Prompt.openai_chat("gpt-4o", messages="Hello"))

    assert unbound.parameters == ["name"]
    assert not unbound.is_fully_bound
    assert bound.parameters == []
    assert bound.is_fully_bound
