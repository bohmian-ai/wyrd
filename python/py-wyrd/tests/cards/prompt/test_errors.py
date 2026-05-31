from pathlib import Path

import pytest
from wyrd.prompt import AnthropicSettings, MediaRef, Prompt, PromptCard, ResponseFormat, WyrdError


def assert_code(error: pytest.ExceptionInfo[WyrdError], code: str) -> None:
    assert error.value.code == code
    assert str(error.value).startswith(error.value.message)


def test_prompt_envelope_error_codes() -> None:
    cases = [
        (
            lambda: PromptCard(
                Prompt.openai_chat("gpt-4o", messages="hi {{name}}", variables=[])
            ).model_dump_json(),
            "WYRD_PROMPT_422_UNDECLARED_PLACEHOLDER",
        ),
        (
            lambda: Prompt.openai_chat("gpt-4o", messages="hi {{name}}").render(),
            "WYRD_PROMPT_422_MISSING_VARIABLE",
        ),
        (
            lambda: ResponseFormat.json_schema("bad", []),
            "WYRD_PROMPT_400_INVALID_RESPONSE_SCHEMA",
        ),
    ]

    for fn, code in cases:
        with pytest.raises(WyrdError) as error:
            fn()
        assert_code(error, code)


def test_media_error_codes(tmp_path: Path) -> None:
    oversized = tmp_path / "oversized.png"
    oversized.write_bytes(b"0" * (20 * 1024 * 1024 + 1))
    cases = [
        (
            lambda: Prompt("x ${media:m}", "gpt-4o", provider="openai").render(),
            "WYRD_PROMPT_422_MISSING_MEDIA_VARIABLE",
        ),
        (
            lambda: Prompt("x ${media:m}", "gpt-4o", provider="openai").bind_media(
                "missing", MediaRef.image_url("https://example.test/x.png")
            ),
            "WYRD_PROMPT_422_UNDECLARED_MEDIA_PLACEHOLDER",
        ),
        (
            lambda: Prompt("x ${media:m}", "gpt-4o", provider="openai").bind_media(
                "m", MediaRef.document_url("https://example.test/x.pdf")
            ),
            "WYRD_PROMPT_400_UNSUPPORTED_MEDIA_FOR_PROVIDER",
        ),
        (
            lambda: Prompt("x ${media:m}", "gemini-2.5-pro", provider="gemini").bind_media(
                "m", MediaRef.image_url("gs://bucket/x.png")
            ),
            "WYRD_PROMPT_400_INVALID_MEDIA_TYPE",
        ),
        (lambda: MediaRef.image_path(tmp_path), "WYRD_PROMPT_400_MEDIA_NOT_REGULAR_FILE"),
        (lambda: MediaRef.image_path(oversized), "WYRD_PROMPT_400_MEDIA_TOO_LARGE"),
    ]

    for fn, code in cases:
        with pytest.raises(WyrdError) as error:
            fn()
        assert_code(error, code)


def test_loader_and_settings_error_codes(tmp_path: Path) -> None:
    bad = tmp_path / "prompt.txt"
    bad.write_text("provider: openai\n")

    cases = [
        (lambda: Prompt.load(tmp_path / "missing.yaml"), "WYRD_PROMPT_500_LOADER_IO"),
        (lambda: Prompt.load(bad), "WYRD_PROMPT_400_LOADER_BAD_EXTENSION"),
        (
            lambda: Prompt.openai_chat(
                "gpt-4o", messages="hi", model_settings=AnthropicSettings(max_tokens=128)
            ),
            "WYRD_PROMPT_400_SETTINGS_PROVIDER_MISMATCH",
        ),
        (
            lambda: Prompt.openai_chat(
                "gpt-4o", messages="hi", model_settings={"temperature": "hot"}
            ),
            "WYRD_PROMPT_400_SETTINGS_DECODE",
        ),
    ]

    for fn, code in cases:
        with pytest.raises(WyrdError) as error:
            fn()
        assert_code(error, code)
