import inspect
import json

import pytest

from wyrd import (
    AnthropicSettings,
    GeminiSettings,
    OpenAISettings,
    Prompt,
    PromptCard,
    WyrdError,
)
from wyrd.prompt import (
    AnthropicSettings as PromptAnthropicSettings,
    GeminiSettings as PromptGeminiSettings,
    OpenAISettings as PromptOpenAISettings,
)


def test_openai_seed_and_logit_bias_from_dict_model_settings() -> None:
    prompt = Prompt.openai_chat(
        "gpt-4o",
        messages="hello",
        model_settings={"seed": 42, "logit_bias": {"123": -100}},
    )

    body = prompt.request.model_dump()
    assert body["seed"] == 42
    assert body["logit_bias"] == {"123": -100}
    assert "settings" not in body


def test_openai_unmodeled_field_passthrough() -> None:
    prompt = Prompt.openai_chat(
        "gpt-4o",
        messages="hello",
        model_settings={"future_knob": {"enabled": True}},
    )

    assert prompt.request.model_dump()["future_knob"] == {"enabled": True}


def test_typed_openai_settings_round_trip() -> None:
    settings = OpenAISettings(seed=7, metadata={"purpose": "unit"})
    prompt = Prompt.openai_chat("gpt-4o", messages="hello", model_settings=settings)

    assert prompt.request.model_dump()["seed"] == 7
    assert prompt.model_settings.to_dict()["metadata"] == {"purpose": "unit"}


def test_provider_settings_mismatch() -> None:
    with pytest.raises(WyrdError) as error:
        Prompt.openai_chat(
            "gpt-4o",
            messages="hello",
            model_settings=AnthropicSettings(max_tokens=128),
        )

    assert error.value.code == "WYRD_PROMPT_400_SETTINGS_PROVIDER_MISMATCH"


def test_model_settings_decode_error() -> None:
    with pytest.raises(WyrdError) as error:
        Prompt.openai_chat(
            "gpt-4o",
            messages="hello",
            model_settings={"temperature": "hot"},
        )

    assert error.value.code == "WYRD_PROMPT_400_SETTINGS_DECODE"


def test_anthropic_default_max_tokens() -> None:
    prompt = Prompt.anthropic("claude-sonnet-4-5", messages="hello")

    assert prompt.request.model_dump()["max_tokens"] == 4096
    assert prompt.model_settings.to_dict()["max_tokens"] == 4096


def test_gemini_thinking_config() -> None:
    prompt = Prompt.gemini(
        "gemini-2.5-pro",
        messages="hello",
        model_settings={
            "generation_config": {
                "thinking_config": {"include_thoughts": True, "thinking_budget": 128}
            }
        },
    )

    assert prompt.request.model_dump()["generation_config"]["thinking_config"] == {
        "include_thoughts": True,
        "thinking_budget": 128,
    }


def test_model_settings_getter_round_trip() -> None:
    prompt = Prompt.openai_chat(
        "gpt-4o",
        messages="hello",
        model_settings=OpenAISettings(seed=11),
    )

    assert isinstance(prompt.model_settings, OpenAISettings)
    assert json.loads(prompt.model_settings.model_dump_json())["seed"] == 11
    assert Prompt.raw("openai", "gpt-4o", b'{"model":"gpt-4o"}').model_settings is None


def test_cache_sugar_and_model_settings_precedence() -> None:
    prompt = Prompt.openai_chat(
        "gpt-4o",
        messages="hello",
        cache="cache-from-sugar",
        model_settings={"prompt_cache_key": "cache-from-settings"},
    )

    assert prompt.request.model_dump()["prompt_cache_key"] == "cache-from-settings"


def test_yaml_prompt_load_with_model_settings(tmp_path) -> None:
    path = tmp_path / "prompt.yaml"
    path.write_text(
        """
provider: openai
model: gpt-4o
messages: hello
model_settings:
  seed: 99
"""
    )

    prompt = Prompt.load(path)
    assert prompt.request.model_dump()["seed"] == 99


def test_old_sampling_kwargs_are_absent() -> None:
    signature = inspect.signature(Prompt.openai_chat)

    assert "model_settings" in signature.parameters
    assert "temperature" not in signature.parameters
    assert "top_p" not in signature.parameters
    assert "max_tokens" not in signature.parameters


def test_settings_classes_import_from_root_and_prompt_module() -> None:
    assert OpenAISettings is PromptOpenAISettings
    assert AnthropicSettings is PromptAnthropicSettings
    assert GeminiSettings is PromptGeminiSettings


def test_vertex_reuses_gemini_settings() -> None:
    prompt = Prompt.vertex(
        "gemini-2.5-pro",
        messages="hello",
        model_settings=GeminiSettings(generation_config={"max_output_tokens": 64}),
    )

    assert isinstance(prompt.model_settings, GeminiSettings)
    assert prompt.request.model_dump()["generation_config"]["max_output_tokens"] == 64


def test_prompt_card_accepts_model_settings_union() -> None:
    prompt = Prompt.openai_chat("gpt-4o", messages="hello")

    card = PromptCard(prompt, model_settings=OpenAISettings(seed=77))

    assert isinstance(card.model_settings, OpenAISettings)
    assert card.prompt.request.model_dump()["seed"] == 77
    assert card.model_settings.to_dict()["seed"] == 77


def test_prompt_card_model_settings_provider_mismatch() -> None:
    prompt = Prompt.openai_chat("gpt-4o", messages="hello")

    with pytest.raises(WyrdError) as error:
        PromptCard(prompt, model_settings=AnthropicSettings(max_tokens=128))

    assert error.value.code == "WYRD_PROMPT_400_SETTINGS_PROVIDER_MISMATCH"


def test_yaml_prompt_card_load_with_model_settings(tmp_path) -> None:
    path = tmp_path / "prompt_card.yaml"
    path.write_text(
        """
apiVersion: wyrd/v1
kind: Prompt
metadata:
  name: yaml-prompt
  version: 0.1.0
spec:
  type: Prompt
  prompt:
    request:
      model: gpt-4o
      messages:
        - role: user
          content: hello
      seed: 123
      future_knob:
        enabled: true
    model: gpt-4o
"""
    )

    card = PromptCard.load(path)

    assert card.prompt.request.model_dump()["seed"] == 123
    assert card.prompt.request.model_dump()["future_knob"] == {"enabled": True}
    assert card.model_settings.to_dict()["seed"] == 123
