"""Unit contracts for the Python Card registry boundary."""

from pathlib import Path

import pytest
import wyrd
from wyrd.cards import AgentCard, Cards, VersionBump
from wyrd.prompt import Prompt, PromptCard, PromptReference


def _offline_cards() -> Cards:
    """Construct a credential-complete client whose endpoint must never be used."""
    return Cards(server_url="http://127.0.0.1:1", api_key="unit-test-key")


def test_agent_cards_are_not_registerable() -> None:
    """The unscoped registry rejects envelope-only Agent holders locally."""
    card = AgentCard(
        PromptReference.inline(Prompt.openai_chat("gpt-4o", messages="hello")),
        space="unit",
        name="agent",
        version="0.1.0",
    )
    with pytest.raises(wyrd.WyrdError, match="DataCard, ModelCard, or PromptCard"):
        _offline_cards().register(card)


def test_registry_stub_documents_patch_default_and_exact_delete() -> None:
    """Hand-authored registry contracts describe the native version policy."""
    stub = Path(__file__).parents[3] / "python" / "wyrd" / "stubs" / "cards.pyi"
    text = stub.read_text(encoding="utf-8")
    assert "RegisterableCard: TypeAlias = DataCard | ModelCard | PromptCard" in text
    assert "VersionBump.Patch" in text
    assert "name`, and the exact `version`" in text


def test_exact_pin_and_explicit_bump_are_rejected_before_network() -> None:
    """An exact authored pin cannot be combined with a caller bump intent."""
    card = PromptCard(
        Prompt.openai_chat("gpt-4o", messages="hello"),
        space="unit",
        name="prompt",
        version="1.2.3",
    )
    with pytest.raises(wyrd.WyrdError, match="exact metadata.version pin"):
        _offline_cards().prompt.register(card, version_bump=VersionBump.Minor)
