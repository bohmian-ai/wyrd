#### begin imports ####

from typing import Literal

#### end of imports ####

class WyrdClient:
    """Authenticated Wyrd client that can act for another principal.

    One client carries the HTTP and gRPC transport plus the credential every
    request presents. Pass it to ``Bifrost(client=...)`` to reuse its
    authentication; its own credential is the actor in ``on_behalf_of``.
    """

    def __init__(
        self,
        server_url: str | None = None,
        credential: str | None = None,
        grpc_url: str | None = None,
    ) -> None:
        """Build a client, resolving every omitted value locally.

        Construction resolves the credential but makes no network call.

        Args:
            server_url: the HTTP server URL; a trailing ``/`` is dropped.
                Resolved from ``WYRD_SERVER_URL`` and then
                ``http://localhost:8080`` if omitted.
            credential: the API key or bearer token. Resolved through
                ``WYRD_ACCESS_TOKEN`` → ``WYRD_WORKLOAD_TOKEN`` + tenant →
                ``WYRD_API_KEY`` → ``~/.config/wyrd/credentials.toml``
                ``[default].api_key`` if omitted.
            grpc_url: the gRPC endpoint. Resolved from ``WYRD_GRPC_URL`` and
                then the effective server URL's scheme and host on port
                ``50051`` if omitted.

        Raises:
            WyrdError: ``WYRD_CLIENT_401_NO_CREDENTIALS`` when no credential
                resolves; ``WYRD_CLIENT_503_TRANSPORT_DOWN`` when the HTTP
                transport cannot be built.

        """
        ...

    @property
    def server_url(self) -> str:
        """The effective HTTP server URL this client sends requests to."""
        ...

    @property
    def grpc_url(self) -> str:
        """The effective gRPC endpoint, resolved as described in ``WyrdClient()``."""
        ...

    def on_behalf_of(
        self,
        subject_token: str,
        audience: Literal["wyrd", "bifrost"] = "bifrost",
    ) -> WyrdClient:
        """Return a client that acts for the holder of ``subject_token``.

        The server issues a short-lived token whose subject is the inbound
        principal, whose actor is this client, and whose permissions are the
        intersection of both. The first RFC 8693 exchange runs here, so a
        refusal raises at the call site; the returned client keeps the
        delegated token in memory only, re-exchanges before expiry, and shares
        this client's connection pools.

        Args:
            subject_token: the inbound principal's access token.
            audience: where the delegated token is accepted: ``"bifrost"``
                for the Bifrost ingest and query surfaces only, or ``"wyrd"``
                for every Wyrd tenant surface.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for an unknown audience, or
                the server's stable code when the exchange is refused, such as
                an invalid subject or actor token or a policy denial.

        """
        ...

__all__ = ["WyrdClient"]
