"""``httpx`` and ``httpx2`` authentication that sends a fresh Wyrd access token on every request."""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING

try:
    import httpx
    import httpx2
except ImportError as missing:
    raise ImportError(
        "WyrdClient.http_auth needs the gateway extra: uv add 'wyrd-ai[gateway]' (httpx and httpx2)"
    ) from missing

from wyrd._wyrd import build_wyrd_error
from wyrd.otel import _active_run

if TYPE_CHECKING:
    from wyrd.client import WyrdClient

_PROVIDER_KEY_HEADERS = ("Authorization", "x-api-key", "x-goog-api-key")
"""Headers that, on a client wired to Wyrd, hold only a placeholder provider key."""

_CONTEXT_HEADERS = (
    "x-wyrd-workload-uid",
    "wyrd-card-uid",
    "wyrd-run-id",
)
"""Identity headers a state-bound adapter owns; an explicit differing value conflicts."""


class HttpAuth(httpx.Auth, httpx2.Auth):
    """Authenticate a stock ``httpx`` or ``httpx2`` client to Wyrd.

    The OpenAI and Anthropic SDKs build on ``httpx2`` and google-genai on
    ``httpx``; both run the same auth-flow protocol, so one instance serves
    either. Each request drops the provider key headers and carries
    ``X-Wyrd-Access-Token: Bearer <token>`` from ``client.access_token()``,
    asked for on that request, so a long-lived client keeps working after any
    one access token expires, and ``x-wyrd-workload-uid`` when the client runs
    as a workload root. Obtain one from ``WyrdClient.http_auth``.

    One from ``WyrdState.http_auth`` also sends the state's Service as
    ``x-wyrd-workload-uid``, its configured ``x-wyrd-deployment-environment``,
    and the state's innermost active Run as ``wyrd-card-uid`` and
    ``wyrd-run-id`` (the Service and no Run outside one), plus the active W3C
    trace context unless the request names its own ``traceparent``. That
    context is read when the request starts, before the token is, and an
    explicit identity header that differs from it fails the request locally
    with ``WYRD_CLIENT_400_CONFIG_INVALID``. The deployment environment is a
    pass-through correlation tag, never an identity: the state's configured
    tag replaces an explicit one, and an unconfigured state sends the
    request's own tag, if any, unchanged.
    """

    def __init__(
        self, client: WyrdClient, state: tuple[int, str | None, str | None] | None = None
    ) -> None:
        """Authenticate as ``client``, in ``state``'s context when given.

        Args:
            client: The client whose access token every request carries.
            state: The bound state's token, Service UID, and deployment environment.
        """
        self._client = client
        self._state = state

    def _context(self, request) -> dict[str, str]:
        """Snapshot the state-bound headers for ``request``, refusing a conflicting one."""
        if self._state is None:
            return {}
        token, service_uid, environment = self._state
        run = _active_run(token)
        context: dict[str, str] = {}
        subject = run[0] if run else service_uid
        if subject is not None:
            context["wyrd-card-uid"] = subject
        if service_uid is not None:
            context["x-wyrd-workload-uid"] = service_uid
        if run is not None:
            context["wyrd-run-id"] = run[1]
        if environment is not None:
            context["x-wyrd-deployment-environment"] = environment
        for name in _CONTEXT_HEADERS:
            explicit = request.headers.get(name)
            if explicit is not None and explicit != context.get(name):
                raise build_wyrd_error(
                    "WYRD_CLIENT_400_CONFIG_INVALID",
                    f"{name} {explicit!r} conflicts with the state-bound context; "
                    "select the intended state or Run instead",
                    {"header": name},
                )
        if "traceparent" not in request.headers:
            context.update(_trace_context())
        return context

    # The flows keep their bases' signatures: each base types them with its
    # own Request class, and one instance serves both, so ``auth_flow``,
    # which both bases also define, is the synchronous flow.
    def sync_auth_flow(self, request):
        """Swap the provider key headers for a fresh token on one synchronous request.

        Args:
            request: The outgoing ``httpx`` or ``httpx2`` request.
        """
        context = self._context(request)
        _authenticate(request, self._client.access_token(), self._client.workload_uid)
        request.headers.update(context)
        yield request

    auth_flow = sync_auth_flow

    async def async_auth_flow(self, request):
        """Swap the provider key headers for a fresh token on one asynchronous request.

        The token is fetched in a worker thread so the event loop never blocks.

        Args:
            request: The outgoing ``httpx`` or ``httpx2`` request.
        """
        context = self._context(request)
        _authenticate(
            request, await asyncio.to_thread(self._client.access_token), self._client.workload_uid
        )
        request.headers.update(context)
        yield request


def _authenticate(request, token: str, workload_uid: str | None) -> None:
    for name in _PROVIDER_KEY_HEADERS:
        request.headers.pop(name, None)
    request.headers["X-Wyrd-Access-Token"] = f"Bearer {token}"
    if workload_uid is not None:
        request.headers["x-wyrd-workload-uid"] = workload_uid


def _trace_context() -> dict[str, str]:
    try:
        from opentelemetry import propagate
    except ImportError:
        return {}
    carrier: dict[str, str] = {}
    propagate.inject(carrier)
    return carrier
