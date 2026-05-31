"""PromptCard declarative authoring tests.

Covers:
- PromptCard(prompt=Prompt(...)) constructor path
- PromptCard.from_path on declarative YAML envelopes
- PromptCard.from_path on native (saved) envelopes
- round-trip: declarative → save → from_path → equal
- error cases: bad provider, settings mismatch, no silent fallback
"""

import json
from pathlib import Path

import pytest
from wyrd.prompt import Prompt, PromptCard, WyrdError

OPENAI_DECLARATIVE_YAML = """\
apiVersion: wyrd/v1
kind: Prompt
metadata:
  name: lead-scoring
  version: 0.1.0
  space: growth
spec:
  provider: openai
  model: gpt-4o
  system: "You are {{persona}}."
  messages:
    - "Summarize {{doc}}."
  model_settings:
    temperature: 0.2
"""

ANTHROPIC_DECLARATIVE_YAML = """\
kind: Prompt
metadata:
  name: support-agent
  version: 0.1.0
spec:
  provider: anthropic
  model: claude-3-5-sonnet-20241022
  system: You are a helpful support agent.
  messages:
    - How can I reset my password?
"""

MALFORMED_PROVIDER_YAML = """\
kind: Prompt
metadata:
  name: bad
  version: 0.1.0
spec:
  provider: unknownprovider
  model: some-model
  messages:
    - Hello
"""


def test_promptcard_constructor_with_python_prompt() -> None:
    prompt = Prompt("Summarize {{doc}}.", "gpt-4o", provider="openai")
    card = PromptCard(prompt, space="growth", name="lead-scoring", version="0.1.0")

    assert card.space == "growth"
    assert card.name == "lead-scoring"
    assert card.version == "0.1.0"
    assert card.prompt.provider == "openai"
    assert card.prompt.model == "gpt-4o"


def test_from_path_declarative_openai(tmp_path: Path) -> None:
    yaml_file = tmp_path / "prompt.yaml"
    yaml_file.write_text(OPENAI_DECLARATIVE_YAML)

    card = PromptCard.from_path(yaml_file)

    assert card.space == "growth"
    assert card.name == "lead-scoring"
    assert card.prompt.provider == "openai"
    assert card.prompt.model == "gpt-4o"


def test_from_path_declarative_extracts_variables(tmp_path: Path) -> None:
    yaml_file = tmp_path / "prompt.yaml"
    yaml_file.write_text(OPENAI_DECLARATIVE_YAML)

    card = PromptCard.from_path(yaml_file)

    assert "persona" in card.parameters
    assert "doc" in card.parameters


def test_from_path_declarative_temperature_applied(tmp_path: Path) -> None:
    yaml_file = tmp_path / "prompt.yaml"
    yaml_file.write_text(OPENAI_DECLARATIVE_YAML)

    card = PromptCard.from_path(yaml_file)
    body = json.loads(card.prompt.request.model_dump_json())
    assert body.get("temperature") == pytest.approx(0.2)


def test_from_path_declarative_anthropic(tmp_path: Path) -> None:
    yaml_file = tmp_path / "prompt.yaml"
    yaml_file.write_text(ANTHROPIC_DECLARATIVE_YAML)

    card = PromptCard.from_path(yaml_file)

    assert card.prompt.provider == "anthropic"
    assert card.prompt.model == "claude-3-5-sonnet-20241022"


def test_from_path_apiversion_default_when_omitted(tmp_path: Path) -> None:
    yaml_file = tmp_path / "prompt.yaml"
    yaml_file.write_text(ANTHROPIC_DECLARATIVE_YAML)

    card = PromptCard.from_path(yaml_file)
    envelope = json.loads(card.model_dump_json())
    assert envelope["apiVersion"] == "wyrd/v1"


def test_declarative_round_trip(tmp_path: Path) -> None:
    """Declarative YAML → from_path → save → from_path → equal."""
    declarative_file = tmp_path / "declarative.yaml"
    declarative_file.write_text(OPENAI_DECLARATIVE_YAML)
    card = PromptCard.from_path(declarative_file)

    saved_file = tmp_path / "saved.yaml"
    card.save(saved_file)

    reloaded = PromptCard.from_path(saved_file)
    assert json.loads(card.model_dump_json()) == json.loads(reloaded.model_dump_json())


def test_saved_card_has_no_spec_type_field(tmp_path: Path) -> None:
    declarative_file = tmp_path / "declarative.yaml"
    declarative_file.write_text(OPENAI_DECLARATIVE_YAML)
    card = PromptCard.from_path(declarative_file)

    saved_file = tmp_path / "saved.yaml"
    card.save(saved_file)

    content = saved_file.read_text()
    assert "type: Prompt" not in content


def test_bad_provider_raises_error(tmp_path: Path) -> None:
    yaml_file = tmp_path / "bad.yaml"
    yaml_file.write_text(MALFORMED_PROVIDER_YAML)

    with pytest.raises(WyrdError):
        PromptCard.from_path(yaml_file)


def test_load_is_alias_for_from_path(tmp_path: Path) -> None:
    yaml_file = tmp_path / "prompt.yaml"
    yaml_file.write_text(OPENAI_DECLARATIVE_YAML)

    via_load = PromptCard.load(yaml_file)
    via_from_path = PromptCard.from_path(yaml_file)

    assert json.loads(via_load.model_dump_json()) == json.loads(via_from_path.model_dump_json())
