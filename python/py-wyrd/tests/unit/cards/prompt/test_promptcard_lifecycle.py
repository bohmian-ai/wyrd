import gc

from wyrd.prompt import Prompt, PromptCard


def test_dropping_promptcard_does_not_invalidate_held_prompt() -> None:
    prompt = Prompt.openai_chat("gpt-4o", messages="Hello {{name}}")
    card = PromptCard(prompt)
    held = card.prompt

    del card
    gc.collect()

    assert held.render(name="Ada").provider == "openai"


def test_round_tripping_card_preserves_live_prompt_handle() -> None:
    card = PromptCard(Prompt.openai_chat("gpt-4o", messages="Hello"))
    loaded = PromptCard.model_validate_json(card.model_dump_json())

    assert isinstance(card.prompt, Prompt)
    assert isinstance(loaded.prompt, Prompt)
    assert loaded.prompt.provider == "openai"
    assert loaded.prompt.model == "gpt-4o"
