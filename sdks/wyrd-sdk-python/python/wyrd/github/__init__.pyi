from typing import Literal, TypeAlias, TypedDict

from ..client import WyrdClient

GithubConnectionState: TypeAlias = Literal["not_connected", "active", "suspended"]
"""A tenant connection's state; ``suspended`` follows GitHub's suspension."""

class GithubConnectResponse(TypedDict):
    """The receipt of a begun connection.

    ``authorization_url`` is GitHub's install URL, which completes only in a
    browser signed in to Wyrd as the same principal; ``expires_at`` is the
    RFC 3339 UTC time the transaction expires.
    """

    authorization_url: str
    expires_at: str

class GithubAppIdentity(TypedDict):
    """The deployment's configured GitHub App."""

    id: int
    slug: str

class GithubRepositoryAccess(TypedDict):
    """One repository the installation and the authorizing user can both reach."""

    repository_id: str
    full_name: str

class GithubConnectionView(TypedDict):
    """A tenant's redacted GitHub connection; no token, code, or key is returned.

    ``installation_id``, ``connected_at``, and ``updated_at`` are ``None``
    while ``state`` is ``not_connected``; timestamps are RFC 3339 UTC strings.
    """

    host: str
    app: GithubAppIdentity
    state: GithubConnectionState
    installation_id: int | None
    repositories: list[GithubRepositoryAccess]
    connected_at: str | None
    updated_at: str | None

class GithubConnection:
    """Tenant-scoped GitHub connection handle.

    Every call blocks with the GIL released until the server answers and
    needs ``identity_connections:write``. No call ever returns a GitHub
    token, code, or key.
    """

    def __init__(self, client: WyrdClient | None = None) -> None:
        """Build a handle; no IO or credential resolution happens here.

        The first server operation connects exactly as ``connect`` does, once
        per handle, and raises its errors.

        Args:
            client: the ``WyrdClient`` to act as, sharing its transport and
                token cache. Omitted, the ambient client resolves from
                ``[client]`` in the Wyrd ``config.toml``, then the
                environment, then the saved ``wyrd auth login``.

        """
        ...
    @staticmethod
    def connect(client: WyrdClient | None = None) -> GithubConnection:
        """Build a handle and connect it now.

        Args:
            client: the ``WyrdClient`` to act as. Omitted, the ambient client
                resolves as for the constructor.

        Raises:
            WyrdError: ``WYRD_CLIENT_401_NO_CREDENTIALS`` when no credential
                resolves; ``WYRD_CLIENT_400_CONFIG_INVALID`` for an unreadable
                ``config.toml``; the server's error for a refused credential
                exchange.

        """
        ...

    def begin(self) -> GithubConnectResponse:
        """Begin a connection and return GitHub's install URL.

        The tenant is connected only after that URL completes in a browser
        signed in to Wyrd as the same principal and GitHub confirms the
        installation and the authorizing user's access.

        Raises:
            WyrdError: ``WYRD_GITHUB_503_NOT_CONFIGURED`` when the deployment
                has no GitHub App; ``WYRD_PERMISSION_403_DENIED_RBAC`` without
                ``identity_connections:write``.

        """
        ...

    def get(self) -> GithubConnectionView:
        """Read the caller tenant's connection.

        Raises:
            WyrdError: ``WYRD_GITHUB_503_NOT_CONFIGURED`` when the deployment
                has no GitHub App; ``WYRD_PERMISSION_403_DENIED_RBAC`` without
                ``identity_connections:write``.

        """
        ...

    def disconnect(self) -> GithubConnectionView:
        """Release the caller tenant's installation and return the ``not_connected`` view.

        Raises:
            WyrdError: ``WYRD_GITHUB_503_NOT_CONFIGURED`` when the deployment
                has no GitHub App; ``WYRD_PERMISSION_403_DENIED_RBAC`` without
                ``identity_connections:write``; ``WYRD_GITHUB_404_NOT_CONNECTED``
                when nothing is connected.

        """
        ...

__all__ = ["GithubConnection"]
