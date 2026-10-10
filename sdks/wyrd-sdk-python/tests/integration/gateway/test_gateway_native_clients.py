"""Unmodified Anthropic and Google GenAI clients call the Wyrd gateway natively.

Each client holds a Wyrd access token in its own API-key slot, the way a
LiteLLM-style proxy is used. The server's built-in adapters reach the shared
recording upstream, which proves the operator provider key, never the caller
token, travels upstream, and each call is recorded in ``vala.gateway.calls``.
"""

from __future__ import annotations

import anthropic
import pytest
from anthropic.types import MessageParam, TextBlock
from google import genai
from google.genai import errors as genai_errors
from google.genai import types as genai_types
from wyrd.gateway import Gateway
from wyrd.testing import WyrdTestServer

from ..support import Receiver, client_of
from .support import (
    PROVIDER_KEY,
    access_token,
    assert_upstream_credentials,
    calls,
    deploy,
    usage,
)

pytestmark = pytest.mark.integration

MESSAGES: list[MessageParam] = [{"role": "user", "content": "hi"}]


@pytest.fixture
def anthropic_gateway(
    gateway_server: tuple[WyrdTestServer, Receiver],
) -> tuple[WyrdTestServer, Receiver]:
    """A server with one Anthropic chat deployment and metadata capture."""
    server, upstream = gateway_server
    deploy(server, "anthropic", "claude-sonnet-5", ["chat_completions"])
    Gateway(client_of(server)).put_capture_policy({"mode": "metadata", "payload_fields": []})
    return server, upstream


@pytest.fixture
def gemini_gateway(
    gateway_server: tuple[WyrdTestServer, Receiver],
) -> tuple[WyrdTestServer, Receiver]:
    """A server with one Gemini chat deployment and metadata capture."""
    server, upstream = gateway_server
    deploy(server, "gemini", "gemini-2.5-flash", ["chat_completions"])
    Gateway(client_of(server)).put_capture_policy({"mode": "metadata", "payload_fields": []})
    return server, upstream


def anthropic_client(
    server: WyrdTestServer, token: str, default_headers: dict[str, str] | None = None
) -> anthropic.Anthropic:
    """The official Anthropic client pointed at the gateway with ``token`` as its API key."""
    return anthropic.Anthropic(
        base_url=server.base_url, api_key=token, max_retries=0, default_headers=default_headers
    )


def gemini_client(server: WyrdTestServer, token: str, **headers: str) -> genai.Client:
    """The official Google GenAI client pointed at the gateway with ``token`` as its API key."""
    options = genai_types.HttpOptions(
        base_url=server.base_url, headers=headers or None, retry_options=None
    )
    return genai.Client(api_key=token, http_options=options)


def test_anthropic_client_calls_and_streams_through_the_gateway(
    anthropic_gateway: tuple[WyrdTestServer, Receiver],
) -> None:
    server, upstream = anthropic_gateway
    token = server.access_token()
    client = anthropic_client(server, token)

    message = client.messages.create(model="claude-sonnet-5", max_tokens=16, messages=MESSAGES)
    with client.messages.stream(
        model="claude-sonnet-5", max_tokens=16, messages=MESSAGES
    ) as stream:
        streamed = "".join(stream.text_stream)
        final = stream.get_final_message()

    (block,) = message.content
    assert isinstance(block, TextBlock)
    assert block.text == "hi"
    assert (message.usage.input_tokens, message.usage.output_tokens) == (5, 3)
    assert (streamed, final.stop_reason, final.usage.output_tokens) == ("hi", "end_turn", 3)
    assert len(upstream.deliveries) == 2
    assert_upstream_credentials(upstream, "x-api-key", PROVIDER_KEY, token)


def test_anthropic_refusals_never_reach_the_provider(
    anthropic_gateway: tuple[WyrdTestServer, Receiver],
) -> None:
    server, upstream = anthropic_gateway
    token = server.access_token()
    viewer_key = server.bootstrap_service(["viewer"], name="native-reader")

    with pytest.raises(anthropic.PermissionDeniedError) as denied:
        anthropic_client(server, access_token(server, viewer_key)).messages.create(
            model="claude-sonnet-5", max_tokens=16, messages=MESSAGES
        )
    ambiguous_client = anthropic_client(
        server, token, default_headers={"Authorization": f"Bearer {token}"}
    )
    with pytest.raises(anthropic.BadRequestError) as ambiguous:
        ambiguous_client.messages.create(model="claude-sonnet-5", max_tokens=16, messages=MESSAGES)

    assert denied.value.response.json()["error"]["code"] == "WYRD_PERMISSION_403_DENIED_RBAC"
    assert ambiguous.value.response.json()["error"]["code"] == "WYRD_AUTH_400_BAD_TOKEN_FORMAT"
    assert upstream.deliveries == []


def test_anthropic_calls_are_recorded_with_their_usage(
    anthropic_gateway: tuple[WyrdTestServer, Receiver],
) -> None:
    server, _ = anthropic_gateway
    client = anthropic_client(server, server.access_token())
    client.messages.create(model="claude-sonnet-5", max_tokens=16, messages=MESSAGES)
    with client.messages.stream(
        model="claude-sonnet-5", max_tokens=16, messages=MESSAGES
    ) as stream:
        stream.get_final_message()

    rows = calls(
        server,
        "ingress_dialect = 'anthropic_messages'",
        "caller_principal_id, usage, streaming, outcome",
    )

    assert len({row["caller_principal_id"] for row in rows} - {None}) == 1
    assert sorted(row["streaming"] for row in rows) == [False, True]
    assert {row["outcome"] for row in rows} == {"succeeded"}
    assert usage(rows) == {("input_tokens", "5"), ("output_tokens", "3")}


def test_gemini_client_calls_and_streams_through_the_gateway(
    gemini_gateway: tuple[WyrdTestServer, Receiver],
) -> None:
    server, upstream = gemini_gateway
    token = server.access_token()
    client = gemini_client(server, token)

    answer = client.models.generate_content(model="gemini-2.5-flash", contents="hi")
    chunks = list(client.models.generate_content_stream(model="gemini-2.5-flash", contents="hi"))

    assert answer.text == "hi"
    assert answer.usage_metadata is not None
    assert answer.usage_metadata.prompt_token_count == 7
    assert "".join(chunk.text or "" for chunk in chunks) == "hi"
    final_candidates = chunks[-1].candidates
    assert final_candidates is not None
    assert final_candidates[0].finish_reason == genai_types.FinishReason.STOP
    assert [delivery.path for delivery in upstream.deliveries] == [
        "/v1beta/models/gemini-2.5-flash:generateContent",
        "/v1beta/models/gemini-2.5-flash:streamGenerateContent?alt=sse",
    ]
    assert_upstream_credentials(upstream, "x-goog-api-key", PROVIDER_KEY, token)


def test_gemini_refusals_never_reach_the_provider(
    gemini_gateway: tuple[WyrdTestServer, Receiver],
) -> None:
    server, upstream = gemini_gateway
    token = server.access_token()
    viewer_key = server.bootstrap_service(["viewer"], name="native-reader")

    reader = gemini_client(server, access_token(server, viewer_key))
    ambiguous_client = gemini_client(server, token, Authorization=f"Bearer {token}")
    with pytest.raises(genai_errors.ClientError) as denied:
        reader.models.generate_content(model="gemini-2.5-flash", contents="hi")
    with pytest.raises(genai_errors.ClientError) as ambiguous:
        ambiguous_client.models.generate_content(model="gemini-2.5-flash", contents="hi")

    assert (denied.value.code, denied.value.status) == (403, "PERMISSION_DENIED")
    assert (
        denied.value.details["error"]["details"][0]["reason"] == "WYRD_PERMISSION_403_DENIED_RBAC"
    )
    assert ambiguous.value.details["error"]["details"][0]["reason"] == (
        "WYRD_AUTH_400_BAD_TOKEN_FORMAT"
    )
    assert upstream.deliveries == []


def test_gemini_calls_are_recorded_with_their_usage(
    gemini_gateway: tuple[WyrdTestServer, Receiver],
) -> None:
    server, _ = gemini_gateway
    client = gemini_client(server, server.access_token())
    client.models.generate_content(model="gemini-2.5-flash", contents="hi")
    list(client.models.generate_content_stream(model="gemini-2.5-flash", contents="hi"))

    rows = calls(
        server,
        "ingress_dialect = 'gemini_generate_content'",
        "caller_principal_id, usage, streaming, outcome",
    )

    assert len({row["caller_principal_id"] for row in rows} - {None}) == 1
    assert sorted(row["streaming"] for row in rows) == [False, True]
    assert {row["outcome"] for row in rows} == {"succeeded"}
    assert usage(rows) == {("input_tokens", "7"), ("output_tokens", "2")}
