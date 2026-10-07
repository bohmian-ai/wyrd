"""Media placed in a Prompt becomes each provider's native media part."""

from pathlib import Path

import pytest
from wyrd.prompt import MediaRef, Prompt, WyrdError

LOGO_URL = "https://example.test/logo.png"


def test_openai_eager_image_url_is_an_image_part() -> None:
    prompt = Prompt.openai_chat("gpt-4o").user("look").user(Prompt.openai_image_url(LOGO_URL))

    assert prompt.request.openai().messages[1].content.as_parts()[0].as_image_url().url == LOGO_URL


def test_openai_eager_file_data_is_a_file_part() -> None:
    prompt = Prompt.openai_chat("gpt-4o").user(
        Prompt.openai_file_data("data:image/png;base64,QUJD", filename="logo.png")
    )

    assert (
        prompt.request.openai().messages[0].content.as_parts()[0].as_file().filename == "logo.png"
    )


def test_openai_image_path_binds_as_a_data_url(tmp_path: Path) -> None:
    image = tmp_path / "logo.png"
    image.write_bytes(b"\x89PNG\r\n\x1a\n")

    prompt = Prompt("look ${media:logo}", "gpt-4o", provider="openai").bind_media(
        "logo", MediaRef.image_path(image)
    )

    url = prompt.request.openai().messages[0].content.as_parts()[1].as_image_url().url
    assert url.startswith("data:image/png;base64,")


def test_openai_rejects_document_url() -> None:
    prompt = Prompt("read ${media:doc}", "gpt-4o", provider="openai")

    with pytest.raises(WyrdError) as error:
        prompt.bind_media("doc", MediaRef.document_url("https://example.test/doc.pdf"))

    assert error.value.code == "WYRD_PROMPT_400_UNSUPPORTED_MEDIA_FOR_PROVIDER"


def test_anthropic_eager_media_become_image_and_document_blocks() -> None:
    prompt = (
        Prompt.anthropic("claude-sonnet-4")
        .user(Prompt.anthropic_image_url(LOGO_URL))
        .user(Prompt.anthropic_image_base64("image/png", "QUJD"))
        .user(Prompt.anthropic_document_text("text/plain", "hello", title="doc"))
    )

    blocks = [
        (message["content"][0]["type"], message["content"][0]["source"]["type"])
        for message in prompt.request.messages
    ]

    assert blocks == [("image", "url"), ("image", "base64"), ("document", "text")]


@pytest.mark.parametrize(
    ("media", "source_type"),
    [
        pytest.param(MediaRef.document_url("https://example.test/doc.pdf"), "url", id="url"),
        pytest.param(MediaRef.document_base64("application/pdf", "QUJD"), "base64", id="base64"),
    ],
)
def test_anthropic_late_document_binds_as_a_document_block(
    media: MediaRef, source_type: str
) -> None:
    prompt = Prompt("read ${media:doc}", "claude-sonnet-4", provider="anthropic").bind_media(
        "doc", media
    )

    block = prompt.request.messages[0]["content"][1]
    assert (block["type"], block["source"]["type"]) == ("document", source_type)


def test_gemini_eager_inline_data_is_an_inline_data_part() -> None:
    prompt = Prompt.gemini("gemini-2.5-pro").user(Prompt.google_inline_data("image/png", "QUJD"))

    assert prompt.request.gemini().contents[0].parts[0].kind == "inline_data"


def test_gemini_gs_uri_binds_as_a_file_data_part() -> None:
    prompt = Prompt("see ${media:img}", "gemini-2.5-pro", provider="gemini").bind_media(
        "img", MediaRef.image_url("gs://bucket/logo.png", mime_type="image/png")
    )

    assert prompt.request.gemini().contents[0].parts[1].kind == "file_data"


def test_gemini_rejects_ordinary_https_media_uri() -> None:
    prompt = Prompt("see ${media:img}", "gemini-2.5-pro", provider="gemini")

    with pytest.raises(WyrdError) as error:
        prompt.bind_media("img", MediaRef.image_url(LOGO_URL, mime_type="image/png"))

    assert error.value.code == "WYRD_PROMPT_400_UNSUPPORTED_MEDIA_FOR_PROVIDER"


def test_vertex_mirrors_gemini_media_behavior() -> None:
    prompt = Prompt("see ${media:img}", "gemini-2.5-pro", provider="vertex")
    bound = prompt.bind_media("img", MediaRef.image_base64("image/png", "QUJD"))

    assert bound.request.gemini().contents[0].parts[1].kind == "inline_data"


def test_gemini_url_missing_mime_raises_exact_code() -> None:
    prompt = Prompt("see ${media:img}", "gemini-2.5-pro", provider="gemini")

    with pytest.raises(WyrdError) as error:
        prompt.bind_media("img", MediaRef.image_url("gs://bucket/logo.png"))

    assert error.value.code == "WYRD_PROMPT_400_INVALID_MEDIA_TYPE"


def test_oversized_media_file_is_refused(tmp_path: Path) -> None:
    oversize = tmp_path / "oversize.png"
    oversize.write_bytes(b"0" * (20 * 1024 * 1024 + 1))

    with pytest.raises(WyrdError) as error:
        MediaRef.image_path(oversize)

    assert error.value.code == "WYRD_PROMPT_400_MEDIA_TOO_LARGE"


def test_directory_media_path_is_refused(tmp_path: Path) -> None:
    with pytest.raises(WyrdError) as error:
        MediaRef.image_path(tmp_path)

    assert error.value.code == "WYRD_PROMPT_400_MEDIA_NOT_REGULAR_FILE"


def test_media_in_the_system_message_is_refused() -> None:
    with pytest.raises(WyrdError) as error:
        Prompt("hello", "gpt-4o", provider="openai", system="${media:logo}")

    assert error.value.code == "WYRD_PROMPT_400_MEDIA_IN_SYSTEM_MESSAGE"
