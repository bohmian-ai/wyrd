import json
import re
from pathlib import Path

import pytest
from wyrd.prompt import Prompt, PromptCard


def make_card() -> PromptCard:
    return PromptCard(
        Prompt.openai_chat("gpt-4o", messages="Hello {{name}}", model_settings={"seed": 7}),
        space="growth",
        name="lead-scoring",
        version="0.1.0",
    )


@pytest.mark.wyrd_covers("python:PromptCard.save")
@pytest.mark.wyrd_covers("python:PromptCard.load")
def test_save_then_load_json_roundtrip(tmp_path: Path) -> None:
    card = make_card()
    path = tmp_path / "prompt.json"

    card.save(path)
    loaded = PromptCard.load(path)

    assert json.loads(loaded.model_dump_json()) == json.loads(card.model_dump_json())


@pytest.mark.wyrd_covers("python:PromptCard.save")
@pytest.mark.wyrd_covers("python:PromptCard.from_path")
def test_save_then_from_path_yaml_roundtrip_and_no_type_field(tmp_path: Path) -> None:
    card = make_card()
    path = tmp_path / "prompt.yaml"

    card.save(path)
    loaded = PromptCard.from_path(path)

    assert json.loads(loaded.model_dump_json()) == json.loads(card.model_dump_json())
    assert "type: Prompt" not in path.read_text()


@pytest.mark.wyrd_covers("python:PromptCard.model_dump_json")
@pytest.mark.wyrd_covers("python:PromptCard.model_validate_json")
def test_model_dump_json_then_model_validate_json_roundtrip() -> None:
    card = make_card()

    loaded = PromptCard.model_validate_json(card.model_dump_json())

    assert json.loads(loaded.model_dump_json()) == json.loads(card.model_dump_json())


def test_json_and_yaml_roundtrip_equivalence(tmp_path: Path) -> None:
    card = make_card()
    json_path = tmp_path / "prompt.json"
    yaml_path = tmp_path / "prompt.yaml"

    card.save(json_path)
    card.save(yaml_path)

    assert json.loads(PromptCard.load(json_path).model_dump_json()) == json.loads(
        PromptCard.load(yaml_path).model_dump_json()
    )


def test_content_hash_format_and_stability(tmp_path: Path) -> None:
    card = make_card()
    path = tmp_path / "prompt.json"
    before = card.content_hash

    card.save(path)
    after = PromptCard.load(path).content_hash

    assert re.fullmatch(r"sha256:[0-9a-f]{64}", before)
    assert before == after


def test_promptcard_parameters_and_bound_state() -> None:
    unbound = make_card()
    bound = PromptCard(Prompt.openai_chat("gpt-4o", messages="Hello"))

    assert unbound.parameters == ["name"]
    assert not unbound.is_fully_bound
    assert bound.parameters == []
    assert bound.is_fully_bound
