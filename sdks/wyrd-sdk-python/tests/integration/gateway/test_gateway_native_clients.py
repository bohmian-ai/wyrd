"""Unmodified Anthropic and Google GenAI clients against the Wyrd gateway.

Each journey points the official client at a real server with a Wyrd access
token in the client's own API-key slot, the way a LiteLLM-style proxy is used.
The server's built-in adapters reach a stdlib mock upstream that records every
request, so the journeys prove native responses and streams, refusals without
dispatch, the operator provider key (never the caller token) upstream, and an
eventual ``vala.gateway.calls`` row attributed to the caller with the mock's
usage.
"""

from __future__ import annotations

import anthropic
import pytest
from google import genai
from google.genai import errors as genai_errors
from google.genai import types as genai_types
from wyrd.gateway import Gateway
from wyrd.testing import WyrdTestServer

from .support import (
    Received,
    assert_upstream_credentials,
    call_rows,
    deploy,
    exchange,
    principal,
    usage,
)


def _caller_rows(server: WyrdTestServer, token: str, dialect: str, count: int) -> list[dict]:
    """Poll the caller-visible ``dialect`` rows, non-streaming first."""
    rows = call_rows(
        server,
        token,
        f"ingress_dialect = '{dialect}'",
        count,
        "caller_principal_id, usage, streaming, outcome",
    )
    return sorted(rows, key=lambda row: row["streaming"])


def _deploy(server: WyrdTestServer, provider: str, model: str) -> None:
    """Configure one built-in chat deployment and metadata capture."""
    deploy(server, provider, model, ["chat_completions"])
    Gateway(server_url=server.base_url, credential=server.api_key).put_capture_policy(
        {"mode": "metadata", "payload_fields": []}
    )


@pytest.mark.integration
def test_anthropic_client_calls_gateway_natively(
    gateway_server: tuple[WyrdTestServer, Received],
) -> None:
    server, received = gateway_server
    _deploy(server, "anthropic", "claude-sonnet-5")
    token = server.access_token()
    client = anthropic.Anthropic(base_url=server.base_url, api_key=token, max_retries=0)

    message = client.messages.create(
        model="claude-sonnet-5", max_tokens=16, messages=[{"role": "user", "content": "hi"}]
    )
    assert message.content[0].text == "hi"
    assert (message.usage.input_tokens, message.usage.output_tokens) == (5, 3)

    with client.messages.stream(
        model="claude-sonnet-5", max_tokens=16, messages=[{"role": "user", "content": "hi"}]
    ) as stream:
        assert "".join(stream.text_stream) == "hi"
        final = stream.get_final_message()
    assert final.stop_reason == "end_turn"
    assert final.usage.output_tokens == 3
    assert len(received) == 2
    assert_upstream_credentials(received, "x-api-key", token)

    reader = server.bootstrap_service(["reader"], name="native-reader")
    with pytest.raises(anthropic.PermissionDeniedError) as denied:
        anthropic.Anthropic(
            base_url=server.base_url, api_key=exchange(server, reader), max_retries=0
        ).messages.create(
            model="claude-sonnet-5", max_tokens=16, messages=[{"role": "user", "content": "hi"}]
        )
    assert denied.value.body["error"]["code"] == "WYRD_PERMISSION_403_DENIED_RBAC"
    conflicting = anthropic.Anthropic(
        base_url=server.base_url,
        api_key=token,
        default_headers={"Authorization": f"Bearer {token}"},
        max_retries=0,
    )
    with pytest.raises(anthropic.BadRequestError) as ambiguous:
        conflicting.messages.create(
            model="claude-sonnet-5", max_tokens=16, messages=[{"role": "user", "content": "hi"}]
        )
    assert ambiguous.value.body["error"]["code"] == "WYRD_AUTH_400_BAD_TOKEN_FORMAT"
    assert len(received) == 2, "refusals never reach the provider"

    rows = _caller_rows(server, token, "anthropic_messages", 2)
    assert {row["caller_principal_id"] for row in rows} == {principal(token)}
    assert [row["streaming"] for row in rows] == [False, True]
    assert all(row["outcome"] == "succeeded" for row in rows)
    assert usage(rows) == {("input_tokens", "5"), ("output_tokens", "3")}


@pytest.mark.integration
def test_google_genai_client_calls_gateway_natively(
    gateway_server: tuple[WyrdTestServer, Received],
) -> None:
    server, received = gateway_server
    _deploy(server, "gemini", "gemini-2.5-flash")
    token = server.access_token()
    options = genai_types.HttpOptions(base_url=server.base_url, retry_options=None)
    client = genai.Client(api_key=token, http_options=options)

    answer = client.models.generate_content(model="gemini-2.5-flash", contents="hi")
    assert answer.text == "hi"
    assert answer.usage_metadata is not None
    assert answer.usage_metadata.prompt_token_count == 7

    chunks = list(client.models.generate_content_stream(model="gemini-2.5-flash", contents="hi"))
    assert "".join(chunk.text or "" for chunk in chunks) == "hi"
    assert chunks[-1].candidates[0].finish_reason == genai_types.FinishReason.STOP
    assert len(received) == 2
    assert received[1][0].endswith(":streamGenerateContent?alt=sse")
    assert_upstream_credentials(received, "x-goog-api-key", token)

    reader = server.bootstrap_service(["reader"], name="native-reader")
    denied_client = genai.Client(api_key=exchange(server, reader), http_options=options)
    with pytest.raises(genai_errors.ClientError) as denied:
        denied_client.models.generate_content(model="gemini-2.5-flash", contents="hi")
    assert denied.value.code == 403
    assert denied.value.status == "PERMISSION_DENIED"
    assert (
        denied.value.details["error"]["details"][0]["reason"] == "WYRD_PERMISSION_403_DENIED_RBAC"
    )
    conflicting = genai.Client(
        api_key=token,
        http_options=genai_types.HttpOptions(
            base_url=server.base_url, headers={"Authorization": f"Bearer {token}"}
        ),
    )
    with pytest.raises(genai_errors.ClientError) as ambiguous:
        conflicting.models.generate_content(model="gemini-2.5-flash", contents="hi")
    assert ambiguous.value.code == 400
    assert len(received) == 2, "refusals never reach the provider"

    rows = _caller_rows(server, token, "gemini_generate_content", 2)
    assert {row["caller_principal_id"] for row in rows} == {principal(token)}
    assert [row["streaming"] for row in rows] == [False, True]
    assert all(row["outcome"] == "succeeded" for row in rows)
    assert usage(rows) == {("input_tokens", "7"), ("output_tokens", "2")}
