"""A PromptCard authored in Python or as declarative YAML."""

from pathlib import Path

import pytest
from wyrd.prompt import OpenAISettings, Prompt, PromptCard, WyrdError


@pytest.fixture
def lead_scoring(fixtures_dir: Path) -> Path:
    """The declarative OpenAI lead-scoring Prompt Card file."""
    return fixtures_dir / "authoring" / "prompt" / "lead-scoring.yaml"


@pytest.fixture
def support_agent(fixtures_dir: Path) -> Path:
    """A declarative Anthropic Prompt Card file that omits apiVersion and space."""
    return fixtures_dir / "authoring" / "prompt" / "support-agent.yaml"


def test_promptcard_constructor_with_python_prompt() -> None:
    prompt = Prompt("Summarize {{doc}}.", "gpt-4o", provider="openai")
    card = PromptCard(prompt, space="growth", name="lead-scoring", version="0.1.0")

    assert card.space == "growth"
    assert card.name == "lead-scoring"
    assert card.version == "0.1.0"
    assert card.prompt.provider == "openai"
    assert card.prompt.model == "gpt-4o"


def test_declarative_openai_card_loads_its_identity_and_model(lead_scoring: Path) -> None:
    card = PromptCard.from_path(lead_scoring)

    assert (card.space, card.name, card.prompt.provider, card.prompt.model) == (
        "growth",
        "lead-scoring",
        "openai",
        "gpt-4o",
    )


def test_declarative_card_extracts_its_variables(lead_scoring: Path) -> None:
    assert PromptCard.from_path(lead_scoring).parameters == ["persona", "doc"]


def test_declarative_card_applies_its_model_settings(lead_scoring: Path) -> None:
    card = PromptCard.from_path(lead_scoring)

    assert isinstance(card.prompt.model_settings, OpenAISettings)
    assert card.prompt.model_settings.to_dict()["temperature"] == pytest.approx(0.2)


def test_declarative_anthropic_card_loads_its_provider_and_model(support_agent: Path) -> None:
    card = PromptCard.from_path(support_agent)

    assert (card.prompt.provider, card.prompt.model) == ("anthropic", "claude-3-5-sonnet-20241022")


def test_omitted_api_version_defaults_to_wyrd_v1(support_agent: Path) -> None:
    assert PromptCard.from_path(support_agent).model_dump()["apiVersion"] == "wyrd/v1"


def test_declarative_card_saves_and_loads_unchanged(tmp_path: Path, lead_scoring: Path) -> None:
    card = PromptCard.from_path(lead_scoring)
    card.save(tmp_path / "saved.yaml")

    assert PromptCard.from_path(tmp_path / "saved.yaml").model_dump() == card.model_dump()


def test_unknown_provider_is_refused(fixtures_dir: Path) -> None:
    with pytest.raises(WyrdError) as error:
        PromptCard.from_path(fixtures_dir / "invalid" / "prompt" / "unknown-provider.yaml")

    assert error.value.code == "WYRD_SPEC_400_VALIDATION"


def test_load_is_alias_for_from_path(lead_scoring: Path) -> None:
    assert (
        PromptCard.load(lead_scoring).model_dump()
        == PromptCard.from_path(lead_scoring).model_dump()
    )
