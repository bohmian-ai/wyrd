#### begin imports ####
import builtins
from collections.abc import Mapping
from typing import Any

#### end of imports ####

class OperatorConnections:
    """Tenant-scoped Operator connection handle.

    Every call blocks with the GIL released until the server answers and
    returns the redacted ``OperatorConnectionView`` wire object:
    ``connection_id``, ``provider``, ``name``, ``status``, provider ``config``,
    ``created_at``, and ``updated_at``. No call ever returns a secret.
    """

    def __init__(self, server_url: str | None = None, credential: str | None = None) -> None:
        """Build a handle; omitted arguments fall through the client configuration.

        No network call happens here.

        Raises:
            WyrdError: When the server URL or credential cannot be resolved.
        """
        ...

    def create(self, request: Mapping[str, Any]) -> dict[str, Any]:
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

    def list(self) -> builtins.list[dict[str, Any]]:
        """List the caller tenant's connections.

        Raises:
            WyrdError: ``WYRD_PERMISSION_403_DENIED_RBAC`` without ``operators:read``.
        """
        ...

    def get(self, connection_id: str) -> dict[str, Any]:
        """Read one connection.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for a non-UUID ID,
                ``WYRD_PERMISSION_403_DENIED_RBAC`` without ``operators:read``,
                and ``WYRD_OPERATOR_404_CONNECTION_NOT_FOUND`` for an unknown
                connection in the caller's tenant.
        """
        ...

    def update(self, connection_id: str, request: Mapping[str, Any]) -> dict[str, Any]:
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

    def disable(self, connection_id: str) -> dict[str, Any]:
        """Disable one connection; Operators naming it fail closed.

        Raises:
            WyrdError: ``WYRD_PERMISSION_403_DENIED_RBAC`` without
                ``operators:write`` and ``WYRD_OPERATOR_404_CONNECTION_NOT_FOUND``
                for an unknown connection.
        """
        ...

__all__ = ["OperatorConnections"]
