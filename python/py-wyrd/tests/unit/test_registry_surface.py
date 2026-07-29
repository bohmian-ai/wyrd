"""Unit contracts for the Python Card registry boundary."""

from pathlib import Path

import pytest
import wyrd
from wyrd.cards import AgentCard, Cards, Prompt, PromptReference


def test_agent_cards_are_not_registerable() -> None:
    """The unscoped registry rejects envelope-only Agent holders locally."""
    card = AgentCard(
        PromptReference.inline(Prompt.openai_chat("gpt-4o", messages="hello")),
        space="unit",
        name="agent",
        version="0.1.0",
    )
    with pytest.raises(wyrd.WyrdError, match="DataCard, ModelCard, or PromptCard"):
        Cards().register(card)


def test_registry_stub_documents_patch_default_and_exact_delete() -> None:
    """Hand-authored registry contracts describe the native version policy."""
    stub = Path(__file__).parents[2] / "python" / "wyrd" / "stubs" / "cards.pyi"
    text = stub.read_text(encoding="utf-8")
    assert "RegisterableCard: TypeAlias = DataCard | ModelCard | PromptCard" in text
    assert "VersionBump.Patch" in text
    assert "name`, and the exact `version`" in text
