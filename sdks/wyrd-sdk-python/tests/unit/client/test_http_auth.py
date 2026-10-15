"""``WyrdClient.http_auth`` swaps provider keys for a fresh Wyrd access token on every request."""

from __future__ import annotations

import asyncio
import itertools
from typing import TYPE_CHECKING, cast

import httpx
import httpx2
from wyrd import WyrdClient
from wyrd.client._http_auth import HttpAuth

if TYPE_CHECKING:
    from collections.abc import Mapping

PLACEHOLDERS = {"Authorization": "Bearer wyrd", "x-api-key": "wyrd", "x-goog-api-key": "wyrd"}


class Minting:
    """A stand-in client that mints a new token on each ask."""

    def __init__(self) -> None:
        self._tokens = (f"token-{n}" for n in itertools.count(1))

    def access_token(self) -> str:
        return next(self._tokens)


def credentials(headers: Mapping[str, str]) -> str:
    """The credential headers a request reached the transport with."""
    names = ("authorization", "x-api-key", "x-goog-api-key", "x-wyrd-access-token")
    return repr({name: headers[name] for name in names if name in headers})


def test_http_auth_on_a_client_replaces_provider_keys_with_its_access_token() -> None:
    client = WyrdClient(server_url="http://127.0.0.1:9", credential="access-token-1")
    echo = httpx2.MockTransport(
        lambda request: httpx2.Response(200, text=credentials(request.headers))
    )
    with httpx2.Client(auth=client.http_auth, transport=echo, headers=PLACEHOLDERS) as http:
        sent = http.get("http://wyrd/v1/models").text
    assert sent == repr({"x-wyrd-access-token": "Bearer access-token-1"})


def test_each_httpx_request_carries_a_fresh_token() -> None:
    echo = httpx.MockTransport(
        lambda request: httpx.Response(200, text=credentials(request.headers))
    )
    auth = HttpAuth(cast("WyrdClient", Minting()))
    with httpx.Client(auth=auth, transport=echo, headers=PLACEHOLDERS) as http:
        sent = [http.get("http://wyrd/mcp").text for _ in range(2)]
    assert sent == [repr({"x-wyrd-access-token": f"Bearer token-{n}"}) for n in (1, 2)]


def test_async_requests_carry_a_fresh_token() -> None:
    echo = httpx2.MockTransport(
        lambda request: httpx2.Response(200, text=credentials(request.headers))
    )

    async def send_twice() -> list[str]:
        auth = HttpAuth(cast("WyrdClient", Minting()))
        async with httpx2.AsyncClient(auth=auth, transport=echo, headers=PLACEHOLDERS) as http:
            return [(await http.get("http://wyrd/v1/models")).text for _ in range(2)]

    assert asyncio.run(send_twice()) == [
        repr({"x-wyrd-access-token": f"Bearer token-{n}"}) for n in (1, 2)
    ]
