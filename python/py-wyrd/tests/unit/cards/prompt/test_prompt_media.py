from pathlib import Path

import pytest
from wyrd.prompt import MediaRef, Prompt, WyrdError


def assert_code(error: pytest.ExceptionInfo[WyrdError], code: str) -> None:
    assert error.value.code == code


def test_openai_eager_image_url_base64_and_path(tmp_path: Path) -> None:
    image = tmp_path / "logo.png"
    image.write_bytes(b"\x89PNG\r\n\x1a\n")

    url_prompt = (
        Prompt.openai_chat("gpt-4o")
        .user("look")
        .user(Prompt.openai_image_url("https://example.test/logo.png"))
    )
    base64_prompt = Prompt.openai_chat("gpt-4o").user(
        Prompt.openai_file_data("data:image/png;base64,QUJD", filename="logo.png")
    )
    path_prompt = Prompt("look ${media:logo}", "gpt-4o", provider="openai").bind_media(
        "logo", MediaRef.image_path(image)
    )

    assert url_prompt.request.model_dump()["messages"][1]["content"][0]["type"] == "image_url"
    assert base64_prompt.request.model_dump()["messages"][0]["content"][0]["type"] == "file"
    assert path_prompt.request.model_dump()["messages"][0]["content"][1]["image_url"][
        "url"
    ].startswith("data:image/png;base64,")


def test_openai_rejects_document_url() -> None:
    prompt = Prompt("read ${media:doc}", "gpt-4o", provider="openai")

    with pytest.raises(WyrdError) as error:
        prompt.bind_media("doc", MediaRef.document_url("https://example.test/doc.pdf"))

    assert_code(error, "WYRD_PROMPT_400_UNSUPPORTED_MEDIA_FOR_PROVIDER")


def test_anthropic_eager_image_and_document_blocks() -> None:
    prompt = (
        Prompt.anthropic("claude-sonnet-4")
        .user(Prompt.anthropic_image_url("https://example.test/logo.png"))
        .user(Prompt.anthropic_image_base64("image/png", "QUJD"))
        .user(Prompt.anthropic_document_text("text/plain", "hello", title="doc"))
    )
    content = prompt.request.model_dump()["messages"]

    assert content[0]["content"][0]["type"] == "image"
    assert content[1]["content"][0]["source"]["type"] == "base64"
    assert content[2]["content"][0]["type"] == "document"


def test_anthropic_late_document_url_and_base64() -> None:
    url_prompt = Prompt("read ${media:doc}", "claude-sonnet-4", provider="anthropic").bind_media(
        "doc", MediaRef.document_url("https://example.test/doc.pdf")
    )
    b64_prompt = Prompt("read ${media:doc}", "claude-sonnet-4", provider="anthropic").bind_media(
        "doc", MediaRef.document_base64("application/pdf", "QUJD")
    )

    assert url_prompt.request.model_dump()["messages"][0]["content"][1]["type"] == "document"
    assert (
        b64_prompt.request.model_dump()["messages"][0]["content"][1]["source"]["type"] == "base64"
    )


def test_gemini_inline_data_and_gs_file_uri() -> None:
    inline = Prompt.gemini("gemini-2.5-pro").user(Prompt.google_inline_data("image/png", "QUJD"))
    gs = Prompt("see ${media:img}", "gemini-2.5-pro", provider="gemini").bind_media(
        "img", MediaRef.image_url("gs://bucket/logo.png", mime_type="image/png")
    )

    assert "inline_data" in inline.request.model_dump()["contents"][0]["parts"][0]
    assert gs.request.model_dump()["contents"][0]["parts"][1]["file_data"]["file_uri"].startswith(
        "gs://"
    )


def test_gemini_rejects_ordinary_https_media_uri() -> None:
    prompt = Prompt("see ${media:img}", "gemini-2.5-pro", provider="gemini")

    with pytest.raises(WyrdError) as error:
        prompt.bind_media(
            "img", MediaRef.image_url("https://example.test/logo.png", mime_type="image/png")
        )

    assert_code(error, "WYRD_PROMPT_400_UNSUPPORTED_MEDIA_FOR_PROVIDER")


def test_vertex_mirrors_gemini_media_behavior() -> None:
    prompt = Prompt("see ${media:img}", "gemini-2.5-pro", provider="vertex")
    bound = prompt.bind_media("img", MediaRef.image_base64("image/png", "QUJD"))

    assert "inline_data" in bound.request.model_dump()["contents"][0]["parts"][1]


def test_gemini_url_missing_mime_raises_exact_code() -> None:
    prompt = Prompt("see ${media:img}", "gemini-2.5-pro", provider="gemini")

    with pytest.raises(WyrdError) as error:
        prompt.bind_media("img", MediaRef.image_url("gs://bucket/logo.png"))

    assert_code(error, "WYRD_PROMPT_400_INVALID_MEDIA_TYPE")


def test_oversized_file_directory_and_system_media_raise_exact_codes(tmp_path: Path) -> None:
    oversize = tmp_path / "oversize.png"
    oversize.write_bytes(b"0" * (20 * 1024 * 1024 + 1))

    with pytest.raises(WyrdError) as too_large:
        MediaRef.image_path(oversize)
    assert_code(too_large, "WYRD_PROMPT_400_MEDIA_TOO_LARGE")

    with pytest.raises(WyrdError) as directory:
        MediaRef.image_path(tmp_path)
    assert_code(directory, "WYRD_PROMPT_400_MEDIA_NOT_REGULAR_FILE")

    with pytest.raises(WyrdError) as system:
        Prompt("hello", "gpt-4o", provider="openai", system="${media:logo}")
    assert_code(system, "WYRD_PROMPT_400_MEDIA_IN_SYSTEM_MESSAGE")
