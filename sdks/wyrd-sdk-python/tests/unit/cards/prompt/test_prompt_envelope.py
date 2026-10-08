"""A PromptCard built from a Prompt."""

from wyrd.prompt import Prompt, PromptCard


def test_happy_path_promptcard_construction() -> None:
    card = PromptCard(Prompt.openai_chat("gpt-4o", messages="Hello {{name}}"))

    assert card.prompt.provider == "openai"
    assert card.parameters == ["name"]


def test_promptcard_takes_its_name() -> None:
    assert PromptCard(Prompt.openai_chat("gpt-4o", messages="Hello"), name="hello").name == "hello"
