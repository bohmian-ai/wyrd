from wyrd.prompt import Prompt, PromptCard


def test_prompt_has_no_register() -> None:
    assert not hasattr(Prompt, "register")


def test_promptcard_instance_has_no_register() -> None:
    card = PromptCard(Prompt.openai_chat("gpt-4o", messages="hi"))

    assert not hasattr(card, "register")
