# AUTO-GENERATED STUB FILE. DO NOT EDIT.
# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value
#### begin imports ####
from typing import Literal, TypeAlias, TypedDict

from ..client import WyrdClient

#### end of imports ####

PrincipalKind: TypeAlias = Literal["user", "service", "agent"]
"""The kind of an assignable principal."""

RoleSource: TypeAlias = Literal["idp", "direct"]
"""Where a Role assignment comes from: the identity provider at login, or a
tenant administrator's direct grant."""

class RoleAssignment(TypedDict):
    """One Role a principal holds and its source."""

    role: str
    source: RoleSource

class PrincipalRoles(TypedDict):
    """A principal's Role assignments, ordered by Role and then source."""

    principal_id: str
    kind: PrincipalKind
    roles: list[RoleAssignment]

class RoleAssignmentChange(TypedDict):
    """The outcome of a direct grant or revoke.

    ``changed`` is ``False`` when the assignment already matched; ``roles`` is
    the principal's complete resulting assignment set.
    """

    principal_id: str
    kind: PrincipalKind
    role: str
    changed: bool
    roles: list[RoleAssignment]

class _PrincipalCardRefOptional(TypedDict, total=False):
    """Optional fields of ``PrincipalCardRef``."""

    space: str
    uid: str

class PrincipalCardRef(_PrincipalCardRefOptional):
    """The Card a Card-bound Service or Agent principal is bound to.

    The same reference shape Card operations accept; ``space`` and ``uid`` are
    omitted when the server reports none.
    """

    kind: str
    name: str
    version: str

class PrincipalSummary(TypedDict):
    """One assignable principal; ``email`` is set for users, ``name`` for
    Services and Agents, and ``card_ref`` for Card-bound Services and Agents.
    ``card_ref`` is ``None`` for users and unbound Services."""

    principal_id: str
    kind: PrincipalKind
    status: Literal["active", "suspended"]
    email: str | None
    name: str | None
    card_ref: PrincipalCardRef | None

class PrincipalPage(TypedDict):
    """One page of principals; ``next`` is the ``after`` of the following
    page, or ``None`` on the last page."""

    principals: list[PrincipalSummary]
    next: str | None

class _CreateServicePrincipalOptional(TypedDict, total=False):
    """Optional fields of ``CreateServicePrincipalRequest``."""

    description: str | None

class CreateServicePrincipalRequest(_CreateServicePrincipalOptional):
    """Create an unbound Service principal holding ``roles``."""

    name: str
    roles: list[str]

class CreateServicePrincipalResponse(TypedDict):
    """The created principal and its first plaintext credential, returned once."""

    principal_id: str
    credential: str

class IssuedCredential(TypedDict):
    """A newly issued credential and its plaintext secret, returned once."""

    id: str
    credential: str

class CredentialMetadata(TypedDict):
    """Credential metadata; the secret is never returned."""

    id: str
    prefix: str
    created_at: str
    expires_at: str | None
    revoked_at: str | None
    last_used_at: str | None

class CredentialList(TypedDict):
    """A principal's credentials."""

    credentials: list[CredentialMetadata]

class RevokePrincipalRequest(TypedDict):
    """Revoke a principal of ``principal_kind`` for an audited ``reason``."""

    principal_kind: PrincipalKind
    reason: str

class Principals:
    """Tenant principal handle: Service principals, their credentials,
    principal discovery, and direct Role assignment.

    Every call blocks with the GIL released until the server answers. A Role
    change reaches the principal at its next token; tokens already issued keep
    the Roles they were signed with.
    """

    def __init__(self, client: WyrdClient | None = None) -> None:
        """Build a handle; no network call happens here.

        Args:
            client: the ``WyrdClient`` to act as. Omitted, the ambient client
                resolves from ``[client]`` in the Wyrd ``config.toml``, then
                the environment, then the saved ``wyrd auth login``.

        Raises:
            WyrdError: ``WYRD_CLIENT_401_NO_CREDENTIALS`` when no ambient
                credential resolves, or a client configuration error.

        """
        ...

    def create_service_principal(
        self, request: CreateServicePrincipalRequest
    ) -> CreateServicePrincipalResponse:
        """Create an unbound Service principal and its first credential.

        Args:
            request: the principal ``name``, its ``roles``, and an optional
                ``description``.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for an off-contract
                request; ``WYRD_PERMISSION_403_DENIED_RBAC`` without
                ``service_accounts:write``.

        """
        ...

    def issue_credential(self, principal_id: str) -> IssuedCredential:
        """Issue one more credential for a Service or Agent principal.

        Args:
            principal_id: the principal to issue for.

        Raises:
            WyrdError: ``WYRD_PERMISSION_403_DENIED_RBAC`` without
                ``service_accounts:write``;
                ``WYRD_AUTH_404_PRINCIPAL_NOT_FOUND`` for an unknown principal.

        """
        ...

    def list_credentials(self, principal_id: str) -> CredentialList:
        """List a principal's credential metadata.

        Args:
            principal_id: the principal whose credentials to list.

        Raises:
            WyrdError: ``WYRD_PERMISSION_403_DENIED_RBAC`` without
                ``service_accounts:write``;
                ``WYRD_AUTH_404_PRINCIPAL_NOT_FOUND`` for an unknown principal.

        """
        ...

    def revoke_credential(self, principal_id: str, credential_id: str) -> None:
        """Revoke one credential of a principal.

        Args:
            principal_id: the credential's principal.
            credential_id: the credential to revoke.

        Raises:
            WyrdError: ``WYRD_PERMISSION_403_DENIED_RBAC`` without
                ``service_accounts:write``, or a not-found error when the
                credential is not the principal's.

        """
        ...

    def revoke_principal(self, principal_id: str, request: RevokePrincipalRequest) -> None:
        """Revoke a principal and every credential it holds.

        Args:
            principal_id: the principal to revoke.
            request: the principal's kind and the audited reason.

        Raises:
            WyrdError: ``WYRD_PERMISSION_403_DENIED_RBAC`` without the revoke
                permission, or a not-found error for an unknown principal.

        """
        ...

    def list(
        self,
        *,
        kind: PrincipalKind | None = None,
        email: str | None = None,
        name: str | None = None,
        limit: int | None = None,
        after: str | None = None,
    ) -> PrincipalPage:
        """One page of assignable principals matching exact filters, ordered by id.

        Tenant administrators and system principals are never listed.

        Args:
            kind: only principals of this kind.
            email: only the user with this exact email.
            name: only Services or Agents with this exact name.
            limit: page size from 1 to 200; omitted, 100.
            after: the previous page's ``next``.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for an invalid filter or
                limit; ``WYRD_PERMISSION_403_DENIED_RBAC`` without
                ``service_accounts:write``.

        """
        ...

    def roles(self, principal_id: str) -> PrincipalRoles:
        """A principal's Role assignments.

        Args:
            principal_id: the principal to read.

        Raises:
            WyrdError: ``WYRD_PERMISSION_403_DENIED_RBAC`` without
                ``service_accounts:write``;
                ``WYRD_AUTH_404_PRINCIPAL_NOT_FOUND`` for a principal that is
                not assignable.

        """
        ...

    def grant_role(self, principal_id: str, role: str) -> RoleAssignmentChange:
        """Idempotently grant a direct Role.

        Args:
            principal_id: the principal to grant to.
            role: the built-in or custom Role name.

        Raises:
            WyrdError: ``WYRD_PERMISSION_403_DENIED_RBAC`` unless the caller is
                a tenant administrator;
                ``WYRD_AUTH_404_PRINCIPAL_NOT_FOUND`` for a principal that is
                not assignable, or a validation error for an unknown Role.

        """
        ...

    def revoke_role(self, principal_id: str, role: str) -> RoleAssignmentChange:
        """Idempotently revoke a direct Role; identity-provider assignments stay.

        Args:
            principal_id: the principal to revoke from.
            role: the Role name.

        Raises:
            WyrdError: ``WYRD_PERMISSION_403_DENIED_RBAC`` unless the caller is
                a tenant administrator;
                ``WYRD_AUTH_404_PRINCIPAL_NOT_FOUND`` for a principal that is
                not assignable, or a validation error for an unknown Role.

        """
        ...

__all__ = ["Principals"]
