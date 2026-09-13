import json

import pytest
from wyrd.prompt import Prompt, PromptReference, WyrdError


def test_promptref_card_roundtrip() -> None:
    ref = PromptReference.card("support-prompt", "1.2.3", space="prod")
    decoded = PromptReference.model_validate_json(ref.model_dump_json())

    assert decoded.kind == "card"
    assert decoded.model_dump()["kind"] == "Prompt"
    assert decoded.model_dump()["name"] == "support-prompt"
    assert decoded.model_dump()["version"] == "1.2.3"
    assert decoded.model_dump()["space"] == "prod"


def test_promptref_inline_roundtrip() -> None:
    ref = PromptReference.inline(Prompt.openai_chat("gpt-4o", messages="Hello"))
    decoded = PromptReference.model_validate_json(ref.model_dump_json())

    assert decoded.kind == "inline"
    assert decoded.model_dump()["model"] == "gpt-4o"


def test_promptref_kind_tag_only_card_or_inline() -> None:
    ref = PromptReference.card("support-prompt", "1.2.3", space="prod")
    assert ref.kind in {"card", "inline"}

    payload = json.loads(ref.model_dump_json())
    payload["kind"] = "url"
    with pytest.raises(WyrdError):
        PromptReference.model_validate_json(json.dumps(payload))
