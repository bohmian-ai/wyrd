"""Shared local provider upstream and evidence readers for gateway journeys.

One recording stdlib HTTP server stands in for every built-in provider the
test server's adapters reach: OpenAI under ``/v1``, Anthropic Messages, Gemini
and Vertex ``GenerateContent``, and OpenAI embeddings and images. Journeys
point a real ``WyrdTestServer`` at it, drive unmodified clients through the
server, and read the eventual ``vala.gateway.calls`` rows with bounded polling.
"""

from __future__ import annotations

import asyncio
import base64
import json
import struct
import time
from http.server import BaseHTTPRequestHandler
from typing import Any

import httpx
import pytest
from wyrd import WyrdError
from wyrd.bifrost import AsyncBifrost
from wyrd.gateway import Gateway
from wyrd.testing import WyrdTestServer

PROVIDER_KEY = "sk-native-upstream"

BINDINGS = {
    "openai": "test-provider-key",
    "anthropic": "test-anthropic-key",
    "gemini": "test-gemini-key",
    "vertex": "test-vertex-key",
}

OPENAI_USAGE = {"prompt_tokens": 11, "completion_tokens": 4, "total_tokens": 15}

OPENAI_COMPLETION = {
    "id": "chatcmpl-1",
    "object": "chat.completion",
    "created": 1,
    "model": "gpt-4o",
    "choices": [
        {
            "index": 0,
            "message": {"role": "assistant", "content": "hi"},
            "finish_reason": "stop",
            "logprobs": None,
        }
    ],
    "usage": OPENAI_USAGE,
}

OPENAI_CHUNKS = [
    {
        "id": "chatcmpl-2",
        "object": "chat.completion.chunk",
        "created": 1,
        "model": "gpt-4o",
        "choices": [{"index": 0, "delta": {"role": "assistant", "content": "h"}}],
    },
    {
        "id": "chatcmpl-2",
        "object": "chat.completion.chunk",
        "created": 1,
        "model": "gpt-4o",
        "choices": [{"index": 0, "delta": {"content": "i"}, "finish_reason": "stop"}],
    },
    {
        "id": "chatcmpl-2",
        "object": "chat.completion.chunk",
        "created": 1,
        "model": "gpt-4o",
        "choices": [],
        "usage": OPENAI_USAGE,
    },
]

ANTHROPIC_MESSAGE = {
    "id": "msg_1",
    "type": "message",
    "role": "assistant",
    "model": "claude-sonnet-5",
    "content": [{"type": "text", "text": "hi"}],
    "stop_reason": "end_turn",
    "stop_sequence": None,
    "usage": {"input_tokens": 5, "output_tokens": 3},
}

ANTHROPIC_EVENTS = [
    (
        "message_start",
        {
            "type": "message_start",
            "message": {**ANTHROPIC_MESSAGE, "content": [], "stop_reason": None},
        },
    ),
    (
        "content_block_start",
        {"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}},
    ),
    (
        "content_block_delta",
        {"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "hi"}},
    ),
    ("content_block_stop", {"type": "content_block_stop", "index": 0}),
    (
        "message_delta",
        {
            "type": "message_delta",
            "delta": {"stop_reason": "end_turn", "stop_sequence": None},
            "usage": {"output_tokens": 3},
        },
    ),
    ("message_stop", {"type": "message_stop"}),
]

GEMINI_ANSWER = {
    "candidates": [
        {
            "content": {"role": "model", "parts": [{"text": "hi"}]},
            "finishReason": "STOP",
            "index": 0,
        }
    ],
    "usageMetadata": {"promptTokenCount": 7, "candidatesTokenCount": 2, "totalTokenCount": 9},
    "modelVersion": "gemini-2.5-flash",
}

GEMINI_EVENTS = [
    {
        "candidates": [{"content": {"role": "model", "parts": [{"text": "h"}]}, "index": 0}],
        "modelVersion": "gemini-2.5-flash",
    },
    {
        "candidates": [
            {
                "content": {"role": "model", "parts": [{"text": "i"}]},
                "finishReason": "STOP",
                "index": 0,
            }
        ],
        "usageMetadata": {"promptTokenCount": 7, "candidatesTokenCount": 2, "totalTokenCount": 9},
    },
]

EMBEDDING_VECTOR = [0.25, -0.5, 0.75]

EMBEDDING = {
    "object": "list",
    "data": [{"object": "embedding", "index": 0, "embedding": EMBEDDING_VECTOR}],
    "model": "text-embedding-3-small",
    "usage": {"prompt_tokens": 6, "total_tokens": 6},
}

IMAGE_BYTES = b"\x89PNG\r\n\x1a\nwyrd-gateway-journey-image"

IMAGE = {
    "created": 1,
    "data": [{"b64_json": base64.b64encode(IMAGE_BYTES).decode()}],
    "usage": {
        "input_tokens": 9,
        "output_tokens": 13,
        "total_tokens": 22,
        "input_tokens_details": {"text_tokens": 9, "image_tokens": 0},
    },
}

Received = list[tuple[str, dict[str, str]]]


class Upstream(BaseHTTPRequestHandler):
    """Mock built-in provider API recording each request path and headers."""

    received: Received

    def do_POST(self) -> None:
        body = json.loads(self.rfile.read(int(self.headers["content-length"])))
        self.received.append((self.path, {k.lower(): v for k, v in self.headers.items()}))
        if self.path == "/v1/chat/completions" and body.get("stream"):
            self._send("text/event-stream", _sse([*OPENAI_CHUNKS, "[DONE]"]))
        elif self.path == "/v1/chat/completions":
            self._send("application/json", json.dumps(OPENAI_COMPLETION))
        elif self.path == "/v1/embeddings" and body.get("encoding_format") == "base64":
            packed = base64.b64encode(struct.pack("<3f", *EMBEDDING_VECTOR)).decode()
            answer = {**EMBEDDING, "data": [{**EMBEDDING["data"][0], "embedding": packed}]}
            self._send("application/json", json.dumps(answer))
        elif self.path == "/v1/embeddings":
            self._send("application/json", json.dumps(EMBEDDING))
        elif self.path == "/v1/images/generations":
            self._send("application/json", json.dumps(IMAGE))
        elif self.path == "/v1/messages" and body.get("stream"):
            frames = [
                f"event: {name}\ndata: {json.dumps(data)}\n\n" for name, data in ANTHROPIC_EVENTS
            ]
            self._send("text/event-stream", "".join(frames))
        elif self.path == "/v1/messages":
            self._send("application/json", json.dumps(ANTHROPIC_MESSAGE))
        elif ":streamGenerateContent" in self.path:
            self._send(
                "text/event-stream",
                "".join(f"data: {json.dumps(e)}\r\n\r\n" for e in GEMINI_EVENTS),
            )
        elif self.path.endswith(":generateContent"):
            self._send("application/json", json.dumps(GEMINI_ANSWER))
        else:
            self.send_error(404)

    def _send(self, content_type: str, text: str) -> None:
        payload = text.encode()
        self.send_response(200)
        self.send_header("content-type", content_type)
        self.send_header("content-length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def log_message(self, format: str, *args: Any) -> None:
        del format, args


def _sse(events: list[Any]) -> str:
    """Encode OpenAI server-sent events, passing the ``[DONE]`` sentinel verbatim."""
    return "".join(
        f"data: {event if isinstance(event, str) else json.dumps(event)}\n\n" for event in events
    )


def admin_headers(server: WyrdTestServer, api_key: str) -> dict[str, str]:
    """Administration headers carrying an access token exchanged from ``api_key``.

    Administration routes read the Wyrd access token, not the API key, so a
    raw-HTTP caller exchanges the key first through the same public auth route
    every client uses.
    """
    return {"x-wyrd-access-token": f"Bearer {exchange(server, api_key)}"}


def put_credential(
    server: WyrdTestServer, body: dict[str, Any], key: str | None = None
) -> httpx.Response:
    """Submit one provider credential over the same HTTP operation the CLI uses.

    The Python SDK exposes no credential mutation — submitting, rotating,
    revoking, and deleting are CLI and scoped MCP paths — so a journey that
    needs a credential in place calls the public route directly, as the CLI
    does. The response is returned unraised so a caller can assert a denial.
    """
    return httpx.put(
        f"{server.base_url}/v1/admin/gateway/provider-credentials/{body['name']}",
        json=body,
        headers=admin_headers(server, key or server.api_key),
        timeout=30.0,
    )


def delete_credential(server: WyrdTestServer, name: str) -> httpx.Response:
    """Delete one provider credential over the same HTTP operation the CLI uses.

    Deletion is a CLI and scoped MCP path for the same reason submission is:
    the name alone does not say whether a managed secret backs it. The
    response is returned unraised so a caller can assert a conflict.
    """
    return httpx.delete(
        f"{server.base_url}/v1/admin/gateway/provider-credentials/{name}",
        headers=admin_headers(server, server.api_key),
        timeout=30.0,
    )


def deploy(
    server: WyrdTestServer,
    provider: str,
    model: str,
    capabilities: list[str],
    adapter: object | None = None,
) -> None:
    """Store a provider's ``Environment`` credential and one built-in deployment of ``model``.

    The deployment goes through the public Python ``Gateway`` as the harness
    admin. The credential cannot: the SDK has no mutation method, so it uses
    the same HTTP operation the CLI does, reading the operator binding the
    test server assigns to ``provider``.
    """
    gateway = Gateway(server_url=server.base_url, credential=server.api_key)
    credential = f"{provider}-key"
    put_credential(
        server,
        {
            "name": credential,
            "provider": provider,
            "source": {"environment": {"binding": BINDINGS[provider]}},
        },
    ).raise_for_status()
    header = {"anthropic": "x-api-key", "gemini": "x-goog-api-key"}.get(provider)
    auth = (
        {"api_key_header": {"header": header, "credential": credential}}
        if header
        else {"bearer": {"credential": credential}}
    )
    gateway.put_deployment(
        {
            "name": model.replace(".", "-"),
            "model": {"provider": provider, "model": model},
            "adapter": adapter or provider,
            "auth": auth,
            "capabilities": capabilities,
            "routing_weight": 1,
        }
    )


def exchange(server: WyrdTestServer, api_key: str) -> str:
    """Exchange an API key for a Wyrd access token through the public auth route."""
    response = httpx.post(
        f"{server.base_url}/auth/token", json={"grant_type": "wyrd_api_key", "api_key": api_key}
    )
    response.raise_for_status()
    return response.json()["access_token"]


def principal(token: str) -> str:
    """Principal id carried in a Wyrd access token."""
    claims = token.split(".")[1]
    return json.loads(base64.urlsafe_b64decode(claims + "=" * (-len(claims) % 4)))["principal"][
        "id"
    ]


def call_rows(
    server: WyrdTestServer, token: str, where: str, count: int, columns: str = "*"
) -> list[dict[str, Any]]:
    """Poll ``vala.gateway.calls`` until ``count`` rows match ``where``.

    Capture publication is asynchronous, so each bounded attempt flushes Scribe
    before reading, and every read carries its own bound so one stalled query
    cannot hang the journey. A query the Oracle refuses admission to under load
    is retried like a stalled one — that refusal is the documented retryable
    429, not an answer about the rows. Ten seconds of attempts without the rows
    fails.
    """
    query = f"SELECT {columns} FROM vala.gateway.calls WHERE {where} ORDER BY started_at"

    async def read() -> list[dict[str, Any]]:
        bifrost = AsyncBifrost(server_url=server.base_url, credential=token)
        result = await asyncio.wait_for(bifrost.sql(query), timeout=10)
        return result.to_arrow().to_pylist()

    rows: list[dict[str, Any]] = []
    for _ in range(40):
        server.flush_bifrost()
        try:
            rows = asyncio.run(read())
        except TimeoutError:
            continue
        except WyrdError as error:
            if "query admission rejected" not in str(error):
                raise
            time.sleep(0.25)
            continue
        if len(rows) >= count:
            return rows
        time.sleep(0.25)
    pytest.fail(f"{count} rows matching {where} never published: {rows}")


def assert_upstream_credentials(received: Received, header: str, token: str) -> None:
    """Every upstream request carried the provider key and never the caller token."""
    assert received
    for path, headers in received:
        assert headers[header] in (PROVIDER_KEY, f"Bearer {PROVIDER_KEY}")
        assert token not in path
        assert all(token not in value for value in headers.values())
        assert not any(name.startswith("x-wyrd") for name in headers)


def usage(rows: list[dict[str, Any]]) -> set[tuple[str, str]]:
    """Distinct usage amounts across ``rows``."""
    return {(u["dimension"], u["quantity"]) for row in rows for u in row["usage"] or []}
