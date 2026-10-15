"""``httpx`` and ``httpx2`` authentication that sends a fresh Wyrd access token on every request."""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING

try:
    import httpx
    import httpx2
except ImportError as missing:
    raise ImportError(
        "WyrdClient.http_auth needs the gateway extra: pip install 'wyrd[gateway]'"
        " (httpx and httpx2)"
    ) from missing

if TYPE_CHECKING:
    from wyrd.client import WyrdClient

_PROVIDER_KEY_HEADERS = ("Authorization", "x-api-key", "x-goog-api-key")
"""Headers that, on a client wired to Wyrd, hold only a placeholder provider key."""


class HttpAuth(httpx.Auth, httpx2.Auth):
    """Authenticate a stock ``httpx`` or ``httpx2`` client to Wyrd.

    The OpenAI and Anthropic SDKs build on ``httpx2`` and google-genai on
    ``httpx``; both run the same auth-flow protocol, so one instance serves
    either. Each request drops the provider key headers and carries
    ``X-Wyrd-Access-Token: Bearer <token>`` from ``client.access_token()``,
    asked for on that request, so a long-lived client keeps working after any
    one access token expires. Obtain one from ``WyrdClient.http_auth``.
    """

    def __init__(self, client: WyrdClient) -> None:
        """Authenticate as ``client``.

        Args:
            client: The client whose access token every request carries.
        """
        self._client = client

    # The flows keep their bases' signatures: each base types them with its
    # own Request class, and one instance serves both.
    def sync_auth_flow(self, request):
        """Swap the provider key headers for a fresh token on one synchronous request.

        Args:
            request: The outgoing ``httpx`` or ``httpx2`` request.
        """
        _authenticate(request, self._client.access_token())
        yield request

    async def async_auth_flow(self, request):
        """Swap the provider key headers for a fresh token on one asynchronous request.

        The token is fetched in a worker thread so the event loop never blocks.

        Args:
            request: The outgoing ``httpx`` or ``httpx2`` request.
        """
        _authenticate(request, await asyncio.to_thread(self._client.access_token))
        yield request


def _authenticate(request, token: str) -> None:
    for name in _PROVIDER_KEY_HEADERS:
        request.headers.pop(name, None)
    request.headers["X-Wyrd-Access-Token"] = f"Bearer {token}"
