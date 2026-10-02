#### begin imports ####
import builtins
from typing import Literal, TypeAlias, TypedDict

#### end of imports ####

OperatorProvider: TypeAlias = Literal["slack", "pager_duty", "http"]
"""The provider an Operator connection authenticates to."""

OperatorConnectionStatus: TypeAlias = Literal["active", "disabled"]
"""A connection's lifecycle state; Operators naming a disabled one fail closed."""

class HttpBearerAuth(TypedDict):
    """Write-only bearer credential for an HTTP connection."""

    scheme: Literal["bearer"]
    token: str

class HttpBasicAuth(TypedDict):
    """Write-only basic credential; ``username`` must not contain ``:``."""

    scheme: Literal["basic"]
    username: str
    password: str

class HttpHeaderAuth(TypedDict):
    """Write-only custom-header credential; ``name`` must not be server-owned."""

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
    """Create a Slack connection with its bot token."""

    provider: Literal["slack"]
    name: str
    workspace_id: str
    bot_token: str

class CreatePagerDutyConnectionRequest(TypedDict):
    """Create a PagerDuty connection with its integration key."""

    provider: Literal["pager_duty"]
    name: str
    integration_key: str

class CreateHttpConnectionRequest(TypedDict):
    """Create an HTTP connection for one HTTPS origin and credential."""

    provider: Literal["http"]
    name: str
    origin: str
    auth: HttpConnectionAuth

CreateOperatorConnectionRequest: TypeAlias = (
    CreateSlackConnectionRequest | CreatePagerDutyConnectionRequest | CreateHttpConnectionRequest
)
"""A provider-tagged connection create request: config plus its secret."""

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
    """Update an HTTP connection; omitted fields are preserved."""

    provider: Literal["http"]

UpdateOperatorConnectionRequest: TypeAlias = (
    UpdateSlackConnectionRequest | UpdatePagerDutyConnectionRequest | UpdateHttpConnectionRequest
)
"""A connection update naming the stored connection's provider."""

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
"""A connection's redacted metadata with provider config flattened alongside it."""

class OperatorConnections:
    """Tenant-scoped Operator connection handle.

    Every call blocks with the GIL released until the server answers and
    returns the redacted ``OperatorConnectionView`` wire object:
    ``connection_id``, ``provider``, ``name``, ``status``, the provider's
    nonsecret config fields flattened alongside them, ``created_at``, and
    ``updated_at``. No call ever returns a secret.
    """

    def __init__(
        self,
        server_url: str | None = None,
        credential: str | None = None,
        tenant: str | None = None,
    ) -> None:
        """Build a handle; omitted arguments fall through the client configuration.

        ``tenant`` is the optional tenant route key that selects one
        server\'s saved login or the workload-token tenant; an explicit credential, access token, or API key already names its tenant and refuses it. No network call
        happens here.

        Raises:
            WyrdError: When the server URL or credential cannot be resolved.
        """
        ...

    def create(self, request: CreateOperatorConnectionRequest) -> OperatorConnectionView:
        """Create one connection from a ``CreateOperatorConnectionRequest``.

        ``request`` carries ``provider`` (``slack``, ``pager_duty``, or
        ``http``), ``name``, the provider's config, and its secret.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for an off-contract request,
                ``WYRD_PERMISSION_403_DENIED_RBAC`` without ``operators:write``,
                ``WYRD_OPERATOR_409_CONNECTION_CONFLICT`` for a taken name, and
                ``WYRD_OPERATOR_503_KEY_UNAVAILABLE`` when no key is configured.
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

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for a non-UUID ID,
                ``WYRD_PERMISSION_403_DENIED_RBAC`` without ``operators:read``,
                and ``WYRD_OPERATOR_404_CONNECTION_NOT_FOUND`` for an unknown
                connection in the caller's tenant.
        """
        ...

    def update(
        self, connection_id: str, request: UpdateOperatorConnectionRequest
    ) -> OperatorConnectionView:
        """Update config, re-enable, or rotate the secret of one connection.

        ``request`` is an ``UpdateOperatorConnectionRequest`` naming the same
        ``provider``; omitted fields are preserved.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for an off-contract argument,
                ``WYRD_PERMISSION_403_DENIED_RBAC`` without ``operators:write``,
                and ``WYRD_OPERATOR_404_CONNECTION_NOT_FOUND`` for an unknown
                connection.
        """
        ...

    def disable(self, connection_id: str) -> OperatorConnectionView:
        """Disable one connection; Operators naming it fail closed.

        Raises:
            WyrdError: ``WYRD_PERMISSION_403_DENIED_RBAC`` without
                ``operators:write`` and ``WYRD_OPERATOR_404_CONNECTION_NOT_FOUND``
                for an unknown connection.
        """
        ...

__all__ = ["OperatorConnections"]
