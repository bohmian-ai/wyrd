# AUTO-GENERATED STUB FILE. DO NOT EDIT.
# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value
#### begin imports ####

from typing import Literal

#### end of imports ####

class WyrdClient:
    """Authenticated Wyrd client that can act for another principal.

    Construction resolves omitted values from the environment and then
    ``~/.config/wyrd/credentials.toml``. The client's own credential is the
    actor in ``on_behalf_of``.
    """

    def __init__(
        self,
        server_url: str | None = None,
        credential: str | None = None,
        grpc_url: str | None = None,
    ) -> None:
        """Build a client from optional transport overrides.

        Raises:
            WyrdError: ``WYRD_CLIENT_401_NO_CREDENTIALS`` when no credential
                resolves.
        """
        ...

    def on_behalf_of(
        self,
        subject_token: str,
        audience: Literal["wyrd", "bifrost"] = "bifrost",
    ) -> WyrdClient:
        """Return a client that acts for the holder of ``subject_token``.

        The server issues a short-lived token whose subject is the inbound
        principal, whose actor is this client, and whose permissions are the
        intersection of both. The first exchange runs here; the returned client
        re-exchanges before expiry.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for an unknown audience, or
                the server's stable code when the exchange is refused.
        """
        ...

__all__ = ["WyrdClient"]
