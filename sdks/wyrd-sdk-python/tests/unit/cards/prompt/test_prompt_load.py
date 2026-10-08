"""A Prompt dumped to YAML or JSON loads back unchanged."""

from pathlib import Path

import pytest
from wyrd.prompt import Prompt, WyrdError


@pytest.mark.parametrize("suffix", ["yaml", "json"])
@pytest.mark.parametrize(
    "prompt",
    [
        pytest.param(
            Prompt.openai_chat(
                "gpt-4o",
                system="System {{tone}}",
                messages="Hello {{name}}",
                model_settings={"seed": 7},
            ),
            id="openai",
        ),
        pytest.param(
            Prompt.openai_responses(
                "gpt-4.1",
                instructions="System {{tone}}",
                messages="Hello {{name}}",
                model_settings={"reasoning": {"effort": "medium"}},
            ),
            id="openai-responses",
        ),
        pytest.param(
            Prompt.anthropic(
                "claude-sonnet-4",
                system="System {{tone}}",
                messages="Hello {{name}}",
                model_settings={"max_tokens": 256},
            ),
            id="anthropic",
        ),
        pytest.param(
            Prompt.gemini(
                "gemini-2.5-pro",
                system="System {{tone}}",
                messages="Hello {{name}}",
                model_settings={"generation_config": {"temperature": 0.2}},
            ),
            id="gemini",
        ),
        pytest.param(
            Prompt.vertex(
                "gemini-2.5-pro",
                system="System {{tone}}",
                messages="Hello {{name}}",
                model_settings={"generation_config": {"max_output_tokens": 64}},
            ),
            id="vertex",
        ),
    ],
)
def test_dumped_prompt_loads_back_unchanged(tmp_path: Path, prompt: Prompt, suffix: str) -> None:
    path = tmp_path / f"prompt.{suffix}"
    prompt.dump(path)

    assert Prompt.load(path).model_dump() == prompt.model_dump()


def test_prompt_load_raw_v1_preserves_provider_and_body(tmp_path: Path) -> None:
    path = tmp_path / "raw.json"
    prompt = Prompt.raw("openai", "gpt-4o", b'{"model":"gpt-4o","future":true}')
    prompt.dump(path)

    loaded = Prompt.load(path)

    assert loaded.provider == "openai"
    assert loaded.request.model_dump()["body"]["body"]["future"] is True


def test_missing_prompt_file_is_refused(tmp_path: Path) -> None:
    with pytest.raises(WyrdError) as error:
        Prompt.load(tmp_path / "missing.yaml")

    assert error.value.code == "WYRD_PROMPT_500_LOADER_IO"


@pytest.mark.parametrize("name", ["prompt.txt", "prompt"])
def test_prompt_file_without_a_yaml_or_json_extension_is_refused(tmp_path: Path, name: str) -> None:
    path = tmp_path / name
    path.touch()

    with pytest.raises(WyrdError) as error:
        Prompt.load(path)

    assert error.value.code == "WYRD_PROMPT_400_LOADER_BAD_EXTENSION"
