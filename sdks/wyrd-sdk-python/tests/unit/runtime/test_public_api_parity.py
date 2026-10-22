import pytest
from wyrd import Prompt, WyrdError


def _prompt() -> Prompt:
    return Prompt(["hello"], "mock-model", provider="mock")


def test_wyrd_error_direct_construction_is_plain_exception() -> None:
    error = WyrdError("WYRD_AGENT_499_CALLBACK_ABORTED", "no")

    assert error.args == ("WYRD_AGENT_499_CALLBACK_ABORTED", "no")
    assert not hasattr(error, "code")
    with pytest.raises(TypeError):
        WyrdError(code="WYRD_AGENT_499_CALLBACK_ABORTED")  # ty: ignore[unknown-argument]


def test_prompt_card_dumps_its_envelope_and_has_no_is_card() -> None:
    from wyrd.prompt import PromptCard

    card = PromptCard(_prompt(), name="parity-prompt", version="1.0.0")

    assert card.model_dump()["kind"] == "Prompt"
    assert not hasattr(card, "is_card")
