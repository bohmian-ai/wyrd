"""``httpx`` and ``httpx2`` authentication that sends a fresh Wyrd access token on every request."""

from __future__ import annotations

import asyncio

try:
    import httpx
    import httpx2
except ImportError as missing:
    raise ImportError(
        "wyrd.gateway.GatewayAuth needs the gateway extra: pip install 'wyrd[gateway]'"
        " (httpx and httpx2)"
    ) from missing

from wyrd.client import WyrdClient


class GatewayAuth(httpx.Auth, httpx2.Auth):
    """Authenticate a stock ``httpx`` or ``httpx2`` client to the Wyrd Gateway.

    The OpenAI and Anthropic SDKs build on ``httpx2`` and google-genai on
    ``httpx``; both run the same auth-flow protocol, so one instance serves
    either. Each request carries ``Authorization: Bearer <token>`` from
    ``client.access_token()``, asked for on that request, so a long-lived
    client keeps working after any one access token expires. Omitting
    ``client`` resolves the ambient ``WyrdClient()`` once, here, so every
    request shares one token cache.
    """

    def __init__(self, client: WyrdClient | None = None) -> None:
        self._client = client if client is not None else WyrdClient()

    # The flows keep their bases' signatures: each base types them with its
    # own Request class, and one instance serves both.
    def sync_auth_flow(self, request):
        request.headers["Authorization"] = f"Bearer {self._client.access_token()}"
        yield request

    async def async_auth_flow(self, request):
        token = await asyncio.to_thread(self._client.access_token)
        request.headers["Authorization"] = f"Bearer {token}"
        yield request
