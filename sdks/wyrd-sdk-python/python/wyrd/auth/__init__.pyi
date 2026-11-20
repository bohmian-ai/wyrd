from typing import Literal, NotRequired, TypeAlias, TypedDict

from ..client import WyrdClient

PrincipalKind: TypeAlias = Literal["user", "service"]
"""The kind of an assignable principal."""

class Unrestricted(TypedDict):
    """No workload-family limit."""

    mode: Literal["unrestricted"]

class FamilyRestriction(TypedDict):
    """Limited to roots of the one Service family ``kind``/``name``.

    The account is that family's unique scheduled runtime account.
    """

    mode: Literal["family"]
    kind: Literal["Service"]
    name: str

WorkloadRestriction: TypeAlias = Unrestricted | FamilyRestriction
"""A Service account's workload restriction."""

class CardFamilyScope(TypedDict):
    """Every version of one Card family."""

    kind: str
    name: str

class CardVersionScope(TypedDict):
    """One exact Card version, with its trusted family."""

    kind: str
    name: str
    uid: str

class _CardFamily(TypedDict):
    family: CardFamilyScope

class _CardVersion(TypedDict):
    version: CardVersionScope

class CardScope(TypedDict):
    """A binding scope over one Card family or exact version."""

    card: _CardFamily | _CardVersion

PermissionScope: TypeAlias = Literal["all"] | CardScope | dict[str, object]
"""The objects a binding reaches: ``"all"`` for the whole tenant, a
``CardScope``, or a Bifrost, gateway, GitHub connection, or tool scope as the
wire contract spells it."""

class ScopedRoleAssignment(TypedDict):
    """One role and the explicit scope it is granted at."""

    role: str
    scope: PermissionScope

class Operation(TypedDict):
    """One resource action a role bundles; roles carry no scope."""

    resource: str
    action: str

class Permission(TypedDict):
    """One compiled permission: an operation at a scope."""

    resource: str
    action: str
    scope: PermissionScope

class _PrincipalSubject(TypedDict):
    id: str
    kind: PrincipalKind

class PrincipalBindingSubject(TypedDict):
    """A binding on one account."""

    principal: _PrincipalSubject

class _GroupSubject(TypedDict):
    id: str

class GroupBindingSubject(TypedDict):
    """A binding on one local group."""

    group: _GroupSubject

BindingSubject: TypeAlias = PrincipalBindingSubject | GroupBindingSubject
"""The account or group a binding is on."""

RoleSource: TypeAlias = Literal["idp", "direct"]
"""Where a binding comes from: the identity provider at login, or a tenant
administrator."""

class DirectGrant(TypedDict):
    """A binding on the account itself."""

    kind: Literal["direct"]

class ProviderGrant(TypedDict):
    """A binding the identity provider's login projected onto the account."""

    kind: Literal["provider"]

class GroupGrant(TypedDict):
    """A binding on a group the account belongs to."""

    kind: Literal["group"]
    id: str
    name: str

GrantSource: TypeAlias = DirectGrant | ProviderGrant | GroupGrant
"""How an account receives one grant."""

class CreateRoleBindingRequest(TypedDict):
    """Bind ``role`` at ``scope`` to ``subject``."""

    subject: BindingSubject
    role: str
    scope: PermissionScope

class RoleBinding(TypedDict):
    """One binding and the permissions it compiles to."""

    id: str
    subject: BindingSubject
    role: str
    scope: PermissionScope
    source: RoleSource
    permissions: list[Permission]

class RoleBindingList(TypedDict):
    """Bindings ordered by id."""

    bindings: list[RoleBinding]

class AccessGrant(TypedDict):
    """One binding an account receives, by any source."""

    binding_id: str
    role: str
    scope: PermissionScope
    source: GrantSource
    permissions: list[Permission]

class AccessSummary(TypedDict):
    """One account's current access and its sources."""

    principal_id: str
    kind: PrincipalKind
    status: str
    workload_restriction: NotRequired[WorkloadRestriction]
    grants: list[AccessGrant]
    permissions: list[Permission]

class Role(TypedDict):
    """One role and the operations it bundles."""

    id: str
    name: str
    operations: list[Operation]
    builtin: bool

class RoleList(TypedDict):
    """Roles ordered by name."""

    roles: list[Role]

class PutRoleRequest(TypedDict):
    """A custom role's complete operation set."""

    operations: list[Operation]

class GroupMember(TypedDict):
    """One account in a group."""

    principal_id: str
    kind: PrincipalKind

class Group(TypedDict):
    """One local group and its members."""

    id: str
    name: str
    members: list[GroupMember]

class GroupList(TypedDict):
    """Groups ordered by name."""

    groups: list[Group]

class PrincipalSummary(TypedDict):
    """One assignable principal; ``email`` is set for users and ``name`` for
    Service accounts. ``workload_restriction`` is the Service account's
    restriction, and ``None`` for users."""

    principal_id: str
    kind: PrincipalKind
    status: Literal["active", "suspended"]
    email: str | None
    name: str | None
    workload_restriction: WorkloadRestriction | None

class PrincipalPage(TypedDict):
    """One page of principals; ``next`` is the ``after`` of the following
    page, or ``None`` on the last page."""

    principals: list[PrincipalSummary]
    next: str | None

class _CreateServicePrincipalOptional(TypedDict, total=False):
    """Optional fields of ``CreateServicePrincipalRequest``."""

    description: str | None
    workload_restriction: WorkloadRestriction | None

class CreateServicePrincipalRequest(_CreateServicePrincipalOptional):
    """Create a Service account bound to ``assignments``; omitting
    ``workload_restriction`` makes it unrestricted."""

    name: str
    assignments: list[ScopedRoleAssignment]

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
    name: NotRequired[str]
    """A User key's name; absent for Service and Admin keys."""
    permission_ceiling: NotRequired[list[str]]
    """A User key's sorted permission ceiling; absent for Service and Admin keys."""

class CredentialList(TypedDict):
    """A principal's credentials."""

    credentials: list[CredentialMetadata]
    next_cursor: NotRequired[str]
    """The following page's cursor; absent on the last page and complete lists."""

class CurrentUserProfile(TypedDict):
    """The calling User's safe profile; ``permissions`` are informational."""

    tenant: str
    tenant_id: str
    principal_id: str
    principal_kind: Literal["user"]
    username: str | None
    authentication: Literal["password", "sso"]
    permissions: list[str]

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
    def connect(client: WyrdClient | None = None) -> Principals:
        """Build a handle and connect it now.

        Resolves the client and completes any credential exchange it needs; a
        bearer token or fresh cached token makes no request.

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

    def create_service_principal(
        self, request: CreateServicePrincipalRequest
    ) -> CreateServicePrincipalResponse:
        """Create a Service account and its first credential.

        Args:
            request: the account ``name``, its ``roles``, an optional
                ``description``, and an optional ``workload_restriction``.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for an off-contract
                request; ``WYRD_PERMISSION_403_DENIED_RBAC`` without
                ``service_accounts:write``, or without tenant administration
                for a family restriction; ``WYRD_AUTH_409_WORKLOAD_BINDING_CONFLICT``
                when another live account already holds the family.

        """
        ...

    def issue_credential(self, principal_id: str) -> IssuedCredential:
        """Issue one more credential for a Service account.

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

    def list_credentials_page(
        self, principal_id: str, *, limit: int | None = None, cursor: str | None = None
    ) -> CredentialList:
        """Read one page of a principal's credential metadata, newest first.

        Args:
            principal_id: the principal whose credentials to list.
            limit: page size, 1 to 100; 50 when omitted.
            cursor: the previous page's ``next_cursor`` for the same principal.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for a bad limit or a cursor
                of another principal; ``WYRD_PERMISSION_403_DENIED_RBAC`` when
                the caller may not read these credentials.

        """
        ...

    def me(self) -> CurrentUserProfile:
        """Read the calling User's safe profile.

        Raises:
            WyrdError: ``WYRD_PERMISSION_403_DENIED_RBAC`` when the caller is
                not a User.

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

    def set_workload_restriction(
        self, principal_id: str, restriction: WorkloadRestriction
    ) -> PrincipalSummary:
        """Replace a Service principal's workload restriction.

        A family restriction makes the account that family's one runtime
        account and requires tenant administration (``*``).

        Args:
            principal_id: the Service principal to restrict.
            restriction: ``{"mode": "unrestricted"}`` or a family restriction.

        Returns:
            The principal with its new restriction.

        Raises:
            WyrdError: ``WYRD_PERMISSION_403_DENIED_RBAC`` without the required
                permission, ``WYRD_AUTH_409_WORKLOAD_BINDING_CONFLICT`` when the
                family already has a runtime account, or a not-found error for
                an unknown principal.

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
            name: only the Service account with this exact name.
            limit: page size from 1 to 200; omitted, 100.
            after: the previous page's ``next``.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for an invalid filter or
                limit; ``WYRD_PERMISSION_403_DENIED_RBAC`` without
                ``service_accounts:write``.

        """
        ...

    def access(self, principal_id: str) -> AccessSummary:
        """One account's bindings, their sources, and compiled permissions.

        Args:
            principal_id: the account to read.

        Raises:
            WyrdError: ``WYRD_PERMISSION_403_DENIED_RBAC`` without
                ``service_accounts:write``;
                ``WYRD_AUTH_404_PRINCIPAL_NOT_FOUND`` for an account that is
                not assignable.

        """
        ...

    def roles(self) -> RoleList:
        """Every role and the operations it bundles.

        Raises:
            WyrdError: ``WYRD_PERMISSION_403_DENIED_RBAC`` without
                ``service_accounts:write``.

        """
        ...

    def put_role(self, role: str, request: PutRoleRequest) -> Role:
        """Create or replace a custom role; built-in roles are read-only.

        Args:
            role: the custom role name.
            request: the operations the role bundles.

        Raises:
            WyrdError: ``WYRD_PERMISSION_403_DENIED_RBAC`` unless the caller is
                a tenant administrator, or a validation error for a built-in
                or empty role.

        """
        ...

    def delete_role(self, role: str) -> Role:
        """Delete a custom role no binding uses.

        Args:
            role: the custom role name.

        Raises:
            WyrdError: ``WYRD_PERMISSION_403_DENIED_RBAC`` unless the caller is
                a tenant administrator, or a validation error for a built-in,
                unknown, or still-bound role.

        """
        ...

    def bindings(
        self,
        *,
        principal_id: str | None = None,
        group_id: str | None = None,
        role: str | None = None,
    ) -> RoleBindingList:
        """Bindings matching exact filters, ordered by id.

        Args:
            principal_id: only bindings on this account.
            group_id: only bindings on this group.
            role: only bindings of this role.

        Raises:
            WyrdError: ``WYRD_PERMISSION_403_DENIED_RBAC`` without
                ``service_accounts:write``.

        """
        ...

    def bind(self, request: CreateRoleBindingRequest) -> RoleBinding:
        """Idempotently bind a role at an explicit scope; a repeat returns the same binding.

        Args:
            request: the subject, role, and scope.

        Raises:
            WyrdError: ``WYRD_PERMISSION_403_DENIED_RBAC`` unless the caller is
                a tenant administrator;
                ``WYRD_AUTH_404_PRINCIPAL_NOT_FOUND`` for an account that is
                not assignable, or a validation error for an unknown role or a
                scope the role cannot use.

        """
        ...

    def unbind(self, binding_id: str) -> RoleBinding:
        """Remove one directly administered binding; other bindings keep their authority.

        Args:
            binding_id: the binding to remove.

        Raises:
            WyrdError: ``WYRD_PERMISSION_403_DENIED_RBAC`` unless the caller is
                a tenant administrator, or a not-found error for an unknown or
                identity-provider binding.

        """
        ...

    def groups(self) -> GroupList:
        """Every local group and its members.

        Raises:
            WyrdError: ``WYRD_PERMISSION_403_DENIED_RBAC`` without
                ``service_accounts:write``.

        """
        ...

    def create_group(self, name: str) -> Group:
        """Create an empty local group.

        Args:
            name: the tenant-unique group name.

        Raises:
            WyrdError: ``WYRD_PERMISSION_403_DENIED_RBAC`` unless the caller is
                a tenant administrator, or a validation error for a taken name.

        """
        ...

    def delete_group(self, group_id: str) -> Group:
        """Delete a local group with its memberships and bindings.

        Args:
            group_id: the group to delete.

        Raises:
            WyrdError: ``WYRD_PERMISSION_403_DENIED_RBAC`` unless the caller is
                a tenant administrator, or a not-found error.

        """
        ...

    def add_group_member(self, group_id: str, principal_id: str) -> Group:
        """Add an account to a local group.

        Args:
            group_id: the group.
            principal_id: the account to add.

        Raises:
            WyrdError: ``WYRD_PERMISSION_403_DENIED_RBAC`` unless the caller is
                a tenant administrator, or a not-found error.

        """
        ...

    def remove_group_member(self, group_id: str, principal_id: str) -> Group:
        """Remove an account from a local group.

        Args:
            group_id: the group.
            principal_id: the account to remove.

        Raises:
            WyrdError: ``WYRD_PERMISSION_403_DENIED_RBAC`` unless the caller is
                a tenant administrator, or a not-found error.

        """
        ...

__all__ = ["Principals"]
