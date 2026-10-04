# AUTO-GENERATED STUB FILE. DO NOT EDIT.
# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value
#### begin imports ####
import builtins
from typing import Literal, TypeAlias, TypedDict

#### end of imports ####

OperatorProvider: TypeAlias = Literal["slack", "pager_duty", "http"]
"""The provider an Operator connection authenticates to."""

OperatorConnectionStatus: TypeAlias = Literal["active", "disabled"]
"""A connection's lifecycle state; Operators naming a disabled one fail closed."""

class HttpBearerAuth(TypedDict):
    """Write-only bearer credential for an HTTP connection.

    Sent as ``Authorization: Bearer <token>``; ``token`` must be non-empty.
    """

    scheme: Literal["bearer"]
    token: str

class HttpBasicAuth(TypedDict):
    """Write-only basic credential.

    ``username`` must be non-empty without ``:`` and ``password`` non-empty.
    """

    scheme: Literal["basic"]
    username: str
    password: str

class HttpHeaderAuth(TypedDict):
    """Write-only custom-header credential.

    ``name`` is a case-insensitive HTTP field name that must not be a
    server-owned header; ``value`` must be non-empty.
    """

    scheme: Literal["header"]
    name: str
    value: str

HttpConnectionAuth: TypeAlias = HttpBearerAuth | HttpBasicAuth | HttpHeaderAuth
"""An HTTP connection credential, tagged by ``scheme``."""

class HttpSchemeView(TypedDict):
    """Redacted bearer or basic auth metadata; the credential is never read back."""

    scheme: Literal["bearer", "basic"]

class HttpHeaderSchemeView(TypedDict):
    """Redacted custom-header auth metadata naming only the header."""

    scheme: Literal["header"]
    name: str

HttpAuthScheme: TypeAlias = HttpSchemeView | HttpHeaderSchemeView
"""Redacted HTTP auth metadata a connection view returns."""

class CreateSlackConnectionRequest(TypedDict):
    """Create a Slack connection with its bot token.

    ``workspace_id`` is the non-empty Slack workspace (team) ID the token
    belongs to; ``bot_token`` is a non-empty bot token with ``chat:write``.
    """

    provider: Literal["slack"]
    name: str
    workspace_id: str
    bot_token: str

class CreatePagerDutyConnectionRequest(TypedDict):
    """Create a PagerDuty connection with its non-empty Events API v2 Global
    Integration key."""

    provider: Literal["pager_duty"]
    name: str
    integration_key: str

class CreateHttpConnectionRequest(TypedDict):
    """Create an HTTP connection for one HTTPS origin and credential.

    ``origin`` is ``https://host[:port]`` with no path, query, fragment, or
    userinfo; plain ``http`` is accepted only for a loopback host. It is the
    only origin the credential may be sent to.
    """

    provider: Literal["http"]
    name: str
    origin: str
    auth: HttpConnectionAuth

CreateOperatorConnectionRequest: TypeAlias = (
    CreateSlackConnectionRequest | CreatePagerDutyConnectionRequest | CreateHttpConnectionRequest
)
"""A provider-tagged connection create request: config plus its secret.

``name`` is immutable and unique per provider within the tenant: 3 to 64 of
``a-z``, ``0-9``, ``_``, ``-``, starting with a letter. Unknown keys are
rejected.
"""

class _UpdateSlackFields(TypedDict, total=False):
    workspace_id: str
    bot_token: str
    status: OperatorConnectionStatus

class UpdateSlackConnectionRequest(_UpdateSlackFields):
    """Update a Slack connection; omitted fields are preserved."""

    provider: Literal["slack"]

class _UpdatePagerDutyFields(TypedDict, total=False):
    integration_key: str
    status: OperatorConnectionStatus

class UpdatePagerDutyConnectionRequest(_UpdatePagerDutyFields):
    """Update a PagerDuty connection; omitted fields are preserved."""

    provider: Literal["pager_duty"]

class _UpdateHttpFields(TypedDict, total=False):
    origin: str
    auth: HttpConnectionAuth
    status: OperatorConnectionStatus

class UpdateHttpConnectionRequest(_UpdateHttpFields):
    """Update an HTTP connection; omitted fields are preserved.

    A supplied ``auth`` replaces the credential and may change its scheme and
    header name.
    """

    provider: Literal["http"]

UpdateOperatorConnectionRequest: TypeAlias = (
    UpdateSlackConnectionRequest | UpdatePagerDutyConnectionRequest | UpdateHttpConnectionRequest
)
"""A connection update naming the stored connection's provider.

A supplied secret replaces the stored one atomically, ``status`` ``"active"``
re-enables a disabled connection, and the name cannot be changed.
"""

class _OperatorConnectionViewBase(TypedDict):
    connection_id: str
    name: str
    status: OperatorConnectionStatus
    created_at: str
    updated_at: str

class SlackConnectionView(_OperatorConnectionViewBase):
    """A redacted Slack connection; the bot token is never returned."""

    provider: Literal["slack"]
    workspace_id: str

class PagerDutyConnectionView(_OperatorConnectionViewBase):
    """A redacted PagerDuty connection; the integration key is never returned."""

    provider: Literal["pager_duty"]

class HttpConnectionView(_OperatorConnectionViewBase):
    """A redacted HTTP connection; only the auth scheme is returned."""

    provider: Literal["http"]
    origin: str
    auth: HttpAuthScheme

OperatorConnectionView: TypeAlias = (
    SlackConnectionView | PagerDutyConnectionView | HttpConnectionView
)
"""A connection's redacted metadata with provider config flattened alongside it.

``connection_id`` is the server-minted UUIDv7 that management calls address;
``created_at`` and ``updated_at`` are RFC 3339 UTC strings.
"""

class OperatorConnections:
    """Tenant-scoped Operator connection handle.

    Every call blocks with the GIL released until the server answers and
    returns the redacted ``OperatorConnectionView`` wire object:
    ``connection_id``, ``provider``, ``name``, ``status``, the provider's
    nonsecret config fields flattened alongside them, ``created_at``, and
    ``updated_at``. No call ever returns a secret.
    """

    def __init__(self, server_url: str | None = None, credential: str | None = None) -> None:
        """Build a handle; no network call happens here.

        Args:
            server_url: the Wyrd server URL. Resolved from ``WYRD_SERVER_URL``
                and then ``http://localhost:8080`` if omitted.
            credential: the API key or bearer token. Resolved through
                ``WYRD_ACCESS_TOKEN`` → ``WYRD_WORKLOAD_TOKEN`` + tenant →
                ``WYRD_API_KEY`` → ``~/.config/wyrd/credentials.toml``
                ``[default].api_key`` if omitted.

        Raises:
            WyrdError: ``WYRD_CLIENT_401_NO_CREDENTIALS`` when no credential
                resolves, or a client configuration error.

        """
        ...

    def create(self, request: CreateOperatorConnectionRequest) -> OperatorConnectionView:
        """Create one connection.

        Args:
            request: ``provider`` (``slack``, ``pager_duty``, or ``http``),
                ``name``, the provider's config, and its secret; see
                ``CreateOperatorConnectionRequest``.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for an off-contract request
                such as an invalid name or origin;
                ``WYRD_OPERATOR_400_INVALID_CONNECTION`` for an empty secret or
                workspace ID or an invalid header name;
                ``WYRD_PERMISSION_403_DENIED_RBAC`` without ``operators:write``;
                ``WYRD_OPERATOR_409_CONNECTION_CONFLICT`` for a taken provider
                and name; ``WYRD_OPERATOR_503_KEY_UNAVAILABLE`` when the server
                has no usable encryption key.

        """
        ...

    def list(self) -> builtins.list[OperatorConnectionView]:
        """List the caller tenant's connections.

        Raises:
            WyrdError: ``WYRD_PERMISSION_403_DENIED_RBAC`` without ``operators:read``.

        """
        ...

    def get(self, connection_id: str) -> OperatorConnectionView:
        """Read one connection.

        Args:
            connection_id: the UUIDv7 ``connection_id`` from a view.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` when ``connection_id`` is
                not a UUIDv7, ``WYRD_PERMISSION_403_DENIED_RBAC`` without
                ``operators:read``, and ``WYRD_OPERATOR_404_CONNECTION_NOT_FOUND``
                for an unknown connection in the caller's tenant.

        """
        ...

    def update(
        self, connection_id: str, request: UpdateOperatorConnectionRequest
    ) -> OperatorConnectionView:
        """Update config, re-enable, or rotate the secret of one connection.

        Args:
            connection_id: the UUIDv7 ``connection_id`` from a view.
            request: an ``UpdateOperatorConnectionRequest`` naming the stored
                connection's ``provider``; omitted fields are preserved.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for an off-contract argument,
                ``WYRD_OPERATOR_400_INVALID_CONNECTION`` when ``provider``
                differs from the stored one or a supplied value is invalid,
                ``WYRD_PERMISSION_403_DENIED_RBAC`` without ``operators:write``,
                ``WYRD_OPERATOR_404_CONNECTION_NOT_FOUND`` for an unknown
                connection, and ``WYRD_OPERATOR_503_KEY_UNAVAILABLE`` when a
                new secret cannot be encrypted.

        """
        ...

    def disable(self, connection_id: str) -> OperatorConnectionView:
        """Disable one connection; Operators naming it fail closed until it is
        re-enabled with ``update(..., {"provider": ..., "status": "active"})``.

        Args:
            connection_id: the UUIDv7 ``connection_id`` from a view.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` when ``connection_id`` is
                not a UUIDv7, ``WYRD_PERMISSION_403_DENIED_RBAC`` without
                ``operators:write``, and ``WYRD_OPERATOR_404_CONNECTION_NOT_FOUND``
                for an unknown connection.

        """
        ...

__all__ = ["OperatorConnections"]
