import json

import pytest
from wyrd.prompt import Prompt, PromptRef, WyrdError


def test_promptref_card_roundtrip() -> None:
    ref = PromptRef.card("support-prompt", "1.2.3", space="prod")
    decoded = PromptRef.model_validate_json(ref.model_dump_json())

    assert decoded.kind == "card"
    assert decoded.model_dump()["kind"] == "card"
    assert decoded.model_dump()["value"]["name"] == "support-prompt"
    assert decoded.model_dump()["value"]["version"] == "1.2.3"
    assert decoded.model_dump()["value"]["space"] == "prod"


def test_promptref_inline_roundtrip() -> None:
    ref = PromptRef.inline(Prompt.openai_chat("gpt-4o", messages="Hello"))
    decoded = PromptRef.model_validate_json(ref.model_dump_json())

    assert decoded.kind == "inline"
    assert decoded.model_dump()["kind"] == "inline"
    assert decoded.model_dump()["value"]["model"] == "gpt-4o"


def test_promptref_kind_tag_only_card_or_inline() -> None:
    ref = PromptRef.card("support-prompt", "1.2.3", space="prod")
    assert ref.kind in {"card", "inline"}

    payload = json.loads(ref.model_dump_json())
    payload["kind"] = "url"
    with pytest.raises(WyrdError):
        PromptRef.model_validate_json(json.dumps(payload))
