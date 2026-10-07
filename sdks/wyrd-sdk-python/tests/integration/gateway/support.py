"""Shared local provider upstream and evidence readers for gateway journeys.

One recording stdlib HTTP server stands in for every built-in provider the
test server's adapters reach: OpenAI under ``/v1``, Anthropic Messages, Gemini
and Vertex ``GenerateContent``, and OpenAI embeddings and images. Journeys
point a real ``WyrdTestServer`` at it, drive unmodified clients through the
server, and read the published ``vala.gateway.calls`` rows once.
"""

from __future__ import annotations

import base64
import json
import struct
from http.server import BaseHTTPRequestHandler
from typing import Any, Literal

from wyrd import cli
from wyrd.bifrost import Bifrost
from wyrd.client import WyrdClient
from wyrd.gateway import Gateway, GatewayOperation, ProviderAdapter, ProviderAuth
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
            answer = {
                **EMBEDDING,
                "data": [{"object": "embedding", "index": 0, "embedding": packed}],
            }
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


def deploy(
    server: WyrdTestServer,
    provider: Literal["openai", "anthropic", "gemini", "vertex"],
    model: str,
    capabilities: list[GatewayOperation],
    adapter: ProviderAdapter | None = None,
) -> None:
    """Store a provider's ``Environment`` credential and one built-in deployment of ``model``.

    The credential goes through in-process ``wyrd gateway credential put`` and
    the deployment through the public Python ``Gateway``, both as the harness
    admin, reading the operator binding the test server assigns to ``provider``.
    """
    gateway = Gateway(server_url=server.base_url, credential=server.api_key)
    credential = f"{provider}-key"
    cli.put_provider_credential(
        {
            "name": credential,
            "provider": provider,
            "source": {"environment": {"binding": BINDINGS[provider]}},
        },
        server=server.base_url,
    )
    header = {"anthropic": "x-api-key", "gemini": "x-goog-api-key"}.get(provider)
    auth: ProviderAuth = (
        {"api_key_header": {"header": header, "credential": credential}}
        if header
        else {"bearer": {"credential": credential}}
    )
    if adapter is None:
        assert provider != "vertex", "a Vertex deployment needs its project and location adapter"
        adapter = provider
    gateway.put_deployment(
        {
            "name": model.replace(".", "-"),
            "model": {"provider": provider, "model": model},
            "adapter": adapter,
            "auth": auth,
            "capabilities": capabilities,
            "routing_weight": 1,
        }
    )


def access_token(api_key: str) -> str:
    """A Wyrd access token for ``api_key``, from the public client the way a third-party caller gets one."""
    return WyrdClient(credential=api_key).access_token()


def calls(server: WyrdTestServer, where: str, columns: str) -> list[dict[str, Any]]:
    """Publish the gateway's captured calls, then read the ``vala.gateway.calls`` rows matching ``where``."""
    server.flush_bifrost()
    query = f"SELECT {columns} FROM vala.gateway.calls WHERE {where} ORDER BY started_at"
    return Bifrost().sql(query).to_arrow().to_pylist()


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
