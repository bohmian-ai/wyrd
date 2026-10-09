"""``GatewayAuth`` asks its client for a token on every request, through both httpx families."""

from __future__ import annotations

import asyncio
import itertools
from typing import TYPE_CHECKING, cast

import httpx
import httpx2
from wyrd.gateway import GatewayAuth

if TYPE_CHECKING:
    from wyrd.client import WyrdClient


class Minting:
    """A stand-in client that mints a new token on each ask."""

    def __init__(self) -> None:
        self._tokens = (f"token-{n}" for n in itertools.count(1))

    def access_token(self) -> str:
        return next(self._tokens)


def bearer(request: httpx.Request) -> httpx.Response:
    return httpx.Response(200, text=request.headers["authorization"])


def bearer2(request: httpx2.Request) -> httpx2.Response:
    return httpx2.Response(200, text=request.headers["authorization"])


def test_each_httpx_request_carries_a_fresh_token() -> None:
    auth = GatewayAuth(cast("WyrdClient", Minting()))
    with httpx.Client(auth=auth, transport=httpx.MockTransport(bearer)) as client:
        sent = [client.get("http://gateway/v1/models").text for _ in range(2)]
    assert sent == ["Bearer token-1", "Bearer token-2"]


def test_each_httpx2_request_carries_a_fresh_token() -> None:
    auth = GatewayAuth(cast("WyrdClient", Minting()))
    with httpx2.Client(auth=auth, transport=httpx2.MockTransport(bearer2)) as client:
        sent = [client.get("http://gateway/v1/models").text for _ in range(2)]
    assert sent == ["Bearer token-1", "Bearer token-2"]


def test_async_requests_carry_a_fresh_token() -> None:
    async def send_twice() -> list[str]:
        auth = GatewayAuth(cast("WyrdClient", Minting()))
        async with httpx.AsyncClient(auth=auth, transport=httpx.MockTransport(bearer)) as client:
            return [(await client.get("http://gateway/v1/models")).text for _ in range(2)]

    assert asyncio.run(send_twice()) == ["Bearer token-1", "Bearer token-2"]
