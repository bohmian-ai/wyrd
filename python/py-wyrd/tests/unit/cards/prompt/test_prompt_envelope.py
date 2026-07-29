import json

from wyrd.prompt import Prompt, PromptCard


def test_happy_path_promptcard_construction() -> None:
    card = PromptCard(Prompt.openai_chat("gpt-4o", messages="Hello {{name}}"))

    assert card.prompt.provider == "openai"
    assert card.parameters == ["name"]


def test_promptcard_json_envelope_shape() -> None:
    card = PromptCard(Prompt.openai_chat("gpt-4o", messages="Hello"), name="hello")
    envelope = json.loads(card.model_dump_json())

    assert envelope["apiVersion"] == "wyrd/v1"
    assert envelope["kind"] == "Prompt"
    assert envelope["metadata"]["name"] == "hello"
