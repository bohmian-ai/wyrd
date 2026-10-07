"""An Agent's prompt slot references a registered Prompt Card or carries one inline."""

from pathlib import Path

import pytest
from wyrd.prompt import Prompt, PromptReference, WyrdError


def test_card_reference_round_trips_its_card_ref() -> None:
    ref = PromptReference.card("support-prompt", "1.2.3", space="prod")

    decoded = PromptReference.model_validate_json(ref.model_dump_json())

    assert decoded.kind == "card"
    assert (decoded.card_ref.space, decoded.card_ref.name, decoded.card_ref.version) == (
        "prod",
        "support-prompt",
        "1.2.3",
    )


def test_inline_reference_round_trips_its_prompt() -> None:
    ref = PromptReference.inline(Prompt.openai_chat("gpt-4o", messages="Hello"))

    decoded = PromptReference.model_validate_json(ref.model_dump_json())

    assert (decoded.kind, decoded.prompt.model) == ("inline", "gpt-4o")


def test_reference_of_an_unknown_kind_is_refused(fixtures_dir: Path) -> None:
    text = (fixtures_dir / "invalid" / "prompt" / "url-prompt-reference.json").read_text()

    with pytest.raises(WyrdError) as error:
        PromptReference.model_validate_json(text)

    assert error.value.code == "WYRD_SPEC_500_INTERNAL"
