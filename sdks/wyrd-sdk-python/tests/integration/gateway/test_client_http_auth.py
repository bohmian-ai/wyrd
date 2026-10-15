"""Stock clients reach the gateway and ``/mcp`` with only ``WyrdClient.http_auth``.

Every client keeps a placeholder provider key, as one wired to Wyrd does;
``http_auth`` drops it and sends the client's current access token as
``X-Wyrd-Access-Token``, so the gateway sees exactly one credential.
"""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING, Any

import anthropic
import httpx2
import openai
import pytest
from anthropic.types import TextBlock
from wyrd.client import WyrdClient
from wyrd.gateway import Gateway

from ..support import Receiver, client_of
from .support import PROVIDER_KEY, assert_upstream_credentials, deploy

if TYPE_CHECKING:
    from pathlib import Path

    from wyrd.testing import WyrdTestServer

pytestmark = pytest.mark.integration

MESSAGES: list[Any] = [{"role": "user", "content": "hi"}]


@pytest.fixture
def backends(gateway_server: tuple[WyrdTestServer, Receiver]) -> tuple[WyrdTestServer, Receiver]:
    """A server with one OpenAI and one Anthropic chat deployment."""
    server, upstream = gateway_server
    deploy(server, "openai", "gpt-4o", ["chat_completions"])
    deploy(server, "anthropic", "claude-sonnet-5", ["chat_completions"])
    Gateway(client_of(server)).put_capture_policy({"mode": "metadata", "payload_fields": []})
    return server, upstream


def test_openai_client_calls_the_gateway_with_http_auth(
    backends: tuple[WyrdTestServer, Receiver],
) -> None:
    server, upstream = backends
    wyrd = client_of(server)
    client = openai.OpenAI(
        base_url=f"{server.base_url}/v1",
        api_key="wyrd",
        http_client=openai.DefaultHttpxClient(auth=wyrd.http_auth),
        max_retries=0,
    )

    completion = client.chat.completions.create(model="openai/gpt-4o", messages=MESSAGES)

    assert completion.choices[0].message.content == "hi"
    assert_upstream_credentials(
        upstream, "authorization", f"Bearer {PROVIDER_KEY}", wyrd.access_token()
    )


def test_async_openai_client_calls_the_gateway_with_http_auth(
    backends: tuple[WyrdTestServer, Receiver],
) -> None:
    server, _ = backends

    async def ask() -> str | None:
        client = openai.AsyncOpenAI(
            base_url=f"{server.base_url}/v1",
            api_key="wyrd",
            http_client=openai.DefaultAsyncHttpxClient(auth=client_of(server).http_auth),
            max_retries=0,
        )
        completion = await client.chat.completions.create(model="openai/gpt-4o", messages=MESSAGES)
        return completion.choices[0].message.content

    assert asyncio.run(ask()) == "hi"


def test_anthropic_client_calls_the_gateway_with_http_auth(
    backends: tuple[WyrdTestServer, Receiver],
) -> None:
    server, upstream = backends
    wyrd = client_of(server)
    client = anthropic.Anthropic(
        base_url=server.base_url,
        api_key="wyrd",
        http_client=anthropic.DefaultHttpxClient(auth=wyrd.http_auth),
        max_retries=0,
    )

    message = client.messages.create(model="claude-sonnet-5", max_tokens=16, messages=MESSAGES)

    (block,) = message.content
    assert isinstance(block, TextBlock)
    assert block.text == "hi"
    assert_upstream_credentials(upstream, "x-api-key", PROVIDER_KEY, wyrd.access_token())


def test_httpx_reaches_mcp_with_http_auth(gateway_server: tuple[WyrdTestServer, Receiver]) -> None:
    server, _ = gateway_server
    with httpx2.Client(auth=client_of(server).http_auth) as http:
        response = http.post(
            f"{server.base_url}/mcp",
            headers={
                "accept": "text/event-stream, application/json",
                "mcp-protocol-version": "2026-07-28",
                "mcp-method": "server/discover",
                "authorization": "Bearer wyrd",
            },
            json={
                "jsonrpc": "2.0",
                "id": 1,
                "method": "server/discover",
                "params": {
                    "_meta": {
                        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                        "io.modelcontextprotocol/clientCapabilities": {},
                    }
                },
            },
        )

    assert response.status_code == 200, response.text
    assert '"result"' in response.text


def test_http_auth_authenticates_from_the_environment_alone(
    backends: tuple[WyrdTestServer, Receiver], monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    server, _ = backends
    monkeypatch.setenv("WYRD_CONFIG_HOME", str(tmp_path))
    monkeypatch.setenv("WYRD_SERVER_URL", server.base_url)
    monkeypatch.setenv("WYRD_API_KEY", server.api_key)
    for name in ("WYRD_ACCESS_TOKEN", "WYRD_WORKLOAD_TOKEN", "WYRD_TENANT"):
        monkeypatch.delenv(name, raising=False)
    client = openai.OpenAI(
        base_url=f"{server.base_url}/v1",
        api_key="wyrd",
        http_client=openai.DefaultHttpxClient(auth=WyrdClient().http_auth),
        max_retries=0,
    )

    completion = client.chat.completions.create(model="openai/gpt-4o", messages=MESSAGES)

    assert completion.choices[0].message.content == "hi"
