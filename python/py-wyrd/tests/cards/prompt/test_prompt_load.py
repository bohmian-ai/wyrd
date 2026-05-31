import json
from pathlib import Path

import pytest
import yaml
from wyrd.prompt import Prompt, WyrdError


def assert_code(error: pytest.ExceptionInfo[WyrdError], code: str) -> None:
    assert error.value.code == code


def prompt_cases() -> list[tuple[str, Prompt]]:
    return [
        (
            "openai_chat",
            Prompt.openai_chat(
                "gpt-4o",
                system="System {{tone}}",
                messages="Hello {{name}}",
                model_settings={"seed": 7},
            ),
        ),
        (
            "openai_responses",
            Prompt.openai_responses(
                "gpt-4.1",
                instructions="System {{tone}}",
                messages="Hello {{name}}",
                model_settings={"reasoning": {"effort": "medium"}},
            ),
        ),
        (
            "anthropic",
            Prompt.anthropic(
                "claude-sonnet-4",
                system="System {{tone}}",
                messages="Hello {{name}}",
                model_settings={"max_tokens": 256},
            ),
        ),
        (
            "gemini",
            Prompt.gemini(
                "gemini-2.5-pro",
                system="System {{tone}}",
                messages="Hello {{name}}",
                model_settings={"generation_config": {"temperature": 0.2}},
            ),
        ),
        (
            "vertex",
            Prompt.vertex(
                "gemini-2.5-pro",
                system="System {{tone}}",
                messages="Hello {{name}}",
                model_settings={"generation_config": {"max_output_tokens": 64}},
            ),
        ),
        ("raw_v1", Prompt.raw("openai", "gpt-4o", b'{"model":"gpt-4o","messages":["hi"]}')),
    ]


def message_text(prompt: Prompt) -> str:
    body = prompt.request.model_dump()
    if "messages" in body:
        content = body["messages"][-1]["content"]
        if isinstance(content, list):
            return " ".join(part.get("text", "") for part in content)
        return content
    if "input" in body:
        return body["input"][-1]["content"][0]["text"]
    if "contents" in body:
        return body["contents"][-1]["parts"][0]["text"]
    if "body" in body:
        return json.dumps(body["body"])
    return json.dumps(body)


def system_text(prompt: Prompt) -> str:
    system = prompt.system_messages
    return json.dumps(system)


@pytest.mark.parametrize(("name", "prompt"), prompt_cases())
def test_prompt_load_yaml_and_json_preserve_deep_fields(
    tmp_path: Path, name: str, prompt: Prompt
) -> None:
    yaml_path = tmp_path / f"{name}.yaml"
    json_path = tmp_path / f"{name}.json"
    prompt.dump(yaml_path)
    prompt.dump(json_path)

    yaml_loaded = Prompt.load(yaml_path)
    json_loaded = Prompt.load(json_path)

    expected_provider = "google" if name == "vertex" else prompt.provider
    assert yaml_loaded.provider == expected_provider
    assert yaml_loaded.model == prompt.model
    assert json_loaded.model_dump() == yaml_loaded.model_dump()
    if name != "raw_v1":
        assert "Hello {{name}}" in message_text(yaml_loaded)
        assert "System" in system_text(yaml_loaded)
        assert yaml_loaded.model_settings is not None


def test_prompt_load_raw_v1_preserves_provider_and_body(tmp_path: Path) -> None:
    path = tmp_path / "raw.json"
    prompt = Prompt.raw("openai", "gpt-4o", b'{"model":"gpt-4o","future":true}')
    prompt.dump(path)

    loaded = Prompt.load(path)

    assert loaded.provider == "openai"
    assert loaded.request.model_dump()["body"]["future"] is True


def test_prompt_load_error_codes(tmp_path: Path) -> None:
    missing = tmp_path / "missing.yaml"
    bad_extension = tmp_path / "prompt.txt"
    bad_extension.write_text("provider: openai\n")
    extensionless = tmp_path / "prompt"
    extensionless.write_text("provider: openai\n")

    with pytest.raises(WyrdError) as missing_error:
        Prompt.load(missing)
    assert_code(missing_error, "WYRD_PROMPT_500_LOADER_IO")

    with pytest.raises(WyrdError) as bad_extension_error:
        Prompt.load(bad_extension)
    assert_code(bad_extension_error, "WYRD_PROMPT_400_LOADER_BAD_EXTENSION")

    with pytest.raises(WyrdError) as extensionless_error:
        Prompt.load(extensionless)
    assert_code(extensionless_error, "WYRD_PROMPT_400_LOADER_BAD_EXTENSION")


def test_prompt_load_declarative_yaml_with_model_settings(tmp_path: Path) -> None:
    path = tmp_path / "declarative.yaml"
    path.write_text(
        yaml.safe_dump(
            {
                "provider": "openai",
                "model": "gpt-4o",
                "messages": "hello",
                "model_settings": {"seed": 99},
            }
        )
    )

    prompt = Prompt.load(path)

    assert prompt.provider == "openai"
    assert prompt.model == "gpt-4o"
    assert prompt.request.model_dump()["seed"] == 99
