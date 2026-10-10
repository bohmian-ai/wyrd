# AUTO-GENERATED STUB FILE. DO NOT EDIT.
# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value
from typing import Literal, TypedDict

import httpx
import httpx2

from ..client import WyrdClient

class GatewayAuth(httpx.Auth, httpx2.Auth):
    """Authenticate a stock ``httpx`` or ``httpx2`` client to the Wyrd Gateway.

    The OpenAI and Anthropic SDKs build on ``httpx2`` and google-genai on
    ``httpx``; one instance serves either. Each request carries
    ``Authorization: Bearer <token>`` from ``client.access_token()``, asked for
    on that request, so a long-lived client keeps working after any one access
    token expires. Needs the ``gateway`` extra (``pip install 'wyrd[gateway]'``).
    """

    def __init__(self, client: WyrdClient) -> None:
        """Authenticate as ``client``.

        Args:
            client: The client whose access token every request carries.
        """
        ...

GatewayOperation = Literal[
    "chat_completions", "responses", "embeddings", "images", "audio", "batches"
]
ProviderCredentialState = Literal["active", "revoked"]
GatewayCaptureMode = Literal["disabled", "metadata", "payload"]
"""``"disabled"`` publishes nothing to Bifrost, ``"metadata"`` publishes call
metadata only, and ``"payload"`` adds the selected redacted payload fields."""
GatewayPayloadField = Literal["request", "response"]
GatewayBudgetPeriod = Literal["calendar_day_utc", "calendar_month_utc"]
UnknownCostPolicy = Literal["reject", "allow_unpriced"]
"""Admission of a call whose cost cannot be bounded: ``"reject"`` refuses it;
``"allow_unpriced"`` admits it as unpriced when no budget applies."""

class ModelRef(TypedDict):
    """One exact provider-native model.

    ``provider`` is a lowercase Wyrd name (3 to 64 of ``a-z``, ``0-9``, ``_``,
    ``-``, starting with a letter); ``openai``, ``anthropic``, ``gemini``, and
    ``vertex`` are reserved for the built-in adapters. ``model`` is the
    provider's own identifier, 1 to 255 bytes without control characters, and
    may contain ``/``.
    """

    provider: str
    model: str

class _EnvironmentBinding(TypedDict):
    binding: str

class EnvironmentCredentialSource(TypedDict):
    """Operator-configured environment variable or mounted-file binding.

    ``environment.binding`` names a binding the operator configured on the
    server; the value itself never leaves the server.
    """

    environment: _EnvironmentBinding

class _ExternalSecret(TypedDict):
    backend: str
    reference: str

class ExternalSecretCredentialSource(TypedDict):
    """Reference resolved from an operator-configured secret backend.

    ``external_secret.backend`` names the configured backend and
    ``external_secret.reference`` is its opaque reference, such as a Vault KV
    v2 ``<path>#<key>``.
    """

    external_secret: _ExternalSecret

ProviderCredentialSource = EnvironmentCredentialSource | ExternalSecretCredentialSource

ProviderCredentialSourceView = ProviderCredentialSource | Literal["managed_secret"]
"""Redacted source of a read credential.

A tenant-submitted managed secret projects to the bare discriminator. There is
no write counterpart: the SDK reads provider credentials and never mutates
one. Submitting, rotating, revoking, and deleting a credential are CLI and
scoped MCP paths, because a name-only revoke or delete cannot be told apart
from one aimed at a managed secret.
"""

class ProviderCredentialView(TypedDict):
    """Redacted provider credential; never carries secret material.

    ``state`` ``"revoked"`` is terminal. Timestamps are RFC 3339 UTC strings;
    ``rotated_at`` is the last active replacement and ``revoked_at`` the
    revocation, each ``None`` when it has not happened.
    """

    name: str
    provider: str
    source: ProviderCredentialSourceView
    state: ProviderCredentialState
    created_at: str
    updated_at: str
    rotated_at: str | None
    revoked_at: str | None

class _VertexLocation(TypedDict):
    project: str
    location: str

class VertexAdapter(TypedDict):
    """Google Vertex GenerateContent protocol.

    ``vertex.project`` and ``vertex.location`` (such as ``us-central1``) are
    each 1 to 128 ASCII alphanumerics, ``-``, or ``_``. The deployment's
    provider must be ``vertex``.
    """

    vertex: _VertexLocation

class _OpenAiCompatibleBase(TypedDict):
    base_url: str

class OpenAiCompatibleAdapter(TypedDict):
    """Standard OpenAI-compatible routes under a tenant base URL.

    ``openai_compatible.base_url`` is an absolute URL. The deployment's
    provider must not be one of the reserved built-in identities.
    """

    openai_compatible: _OpenAiCompatibleBase

ProviderAdapter = Literal["openai", "anthropic", "gemini"] | VertexAdapter | OpenAiCompatibleAdapter
"""Upstream protocol family a deployment speaks.

``"openai"``, ``"anthropic"``, and ``"gemini"`` require the provider of the
same name. OpenAI-protocol adapters serve every operation; the Anthropic,
Gemini, and Vertex adapters serve ``chat_completions`` only.
"""

class _BearerAuth(TypedDict):
    credential: str

class BearerAuth(TypedDict):
    """``Authorization: Bearer <credential>``.

    ``bearer.credential`` names the tenant provider credential supplying the
    value.
    """

    bearer: _BearerAuth

class _ApiKeyHeaderAuth(TypedDict):
    header: str
    credential: str

class ApiKeyHeaderAuth(TypedDict):
    """A named API-key header carrying the credential.

    ``api_key_header.header`` is an HTTP field name, stored lowercase.
    Authorization, proxy, host, forwarding, connection, framing, cookie, and
    ``wyrd-`` headers are rejected. ``api_key_header.credential`` names the
    tenant provider credential supplying the value.
    """

    api_key_header: _ApiKeyHeaderAuth

ProviderAuth = Literal["none"] | BearerAuth | ApiKeyHeaderAuth
"""Authentication the gateway presents upstream; ``"none"`` sends none.

The caller's own Wyrd token is never forwarded.
"""

class ProviderDeployment(TypedDict):
    """Named provider deployment; the same shape is written and read.

    ``name`` (a lowercase Wyrd name) is an administration identifier only:
    inference callers select a ``ModelRef``, never a deployment.
    ``capabilities`` must be non-empty and served by ``adapter``.
    ``routing_weight`` is a positive weight among deployments serving the same
    model. A referenced credential must be active and for the same provider.
    Unknown keys are rejected.
    """

    name: str
    model: ModelRef
    adapter: ProviderAdapter
    auth: ProviderAuth
    capabilities: list[GatewayOperation]
    routing_weight: int

class _OperationScope(TypedDict):
    operation: GatewayOperation

class OperationFallbackScope(TypedDict):
    """Calls for one operation."""

    operation: _OperationScope

class _ModelScope(TypedDict):
    model: ModelRef

class ModelFallbackScope(TypedDict):
    """Calls for one exact requested model."""

    model: _ModelScope

FallbackScope = Literal["global"] | OperationFallbackScope | ModelFallbackScope
"""Where one fallback rule applies; ``"global"`` covers every call without a
more specific rule."""

class FallbackRule(TypedDict):
    """Ordered fallback candidates for one scope.

    ``candidates`` is non-empty and duplicate-free; a model-scoped rule cannot
    list its own model.
    """

    scope: FallbackScope
    candidates: list[ModelRef]

class GatewayFallbackPolicy(TypedDict):
    """Tenant fallback policy: at most one rule per scope; rules never merge.

    The default policy has no rules.
    """

    rules: list[FallbackRule]

class _PrincipalRef(TypedDict):
    principal_id: str

class PrincipalSubject(TypedDict):
    """One principal."""

    principal: _PrincipalRef

class _RoleRef(TypedDict):
    role_name: str

class RoleSubject(TypedDict):
    """Every principal holding one role."""

    role: _RoleRef

GatewayLimitSubject = Literal["tenant"] | PrincipalSubject
"""Principal set a limit applies to; limits have no role subject."""
GatewayPolicySubject = Literal["tenant"] | PrincipalSubject | RoleSubject

class _ProviderTargetRef(TypedDict):
    provider: str

class ProviderTarget(TypedDict):
    """One provider."""

    provider: _ProviderTargetRef

class _ModelTargetRef(TypedDict):
    model: ModelRef

class ModelTarget(TypedDict):
    """One exact model."""

    model: _ModelTargetRef

GatewayPolicyTarget = Literal["all"] | ProviderTarget | ModelTarget

class GatewayLimit(TypedDict):
    """Rate and concurrency limit for one subject and target.

    Each set value is a positive integer and ``None`` leaves that dimension
    unlimited; at least one must be set. At most one limit exists per subject
    and target.
    """

    subject: GatewayLimitSubject
    target: GatewayPolicyTarget
    requests_per_minute: int | None
    tokens_per_minute: int | None
    concurrent_calls: int | None

class GatewayBudget(TypedDict):
    """Spending budget for one subject and period.

    ``amount`` is a positive decimal string such as ``"100.50"`` and
    ``currency`` an ISO-4217 code of three uppercase letters. Budgets and
    active pricing must share one currency; at most one budget exists per
    subject and period.
    """

    subject: GatewayPolicySubject
    period: GatewayBudgetPeriod
    amount: str
    currency: str

class GatewayPriceRate(TypedDict):
    """Price for one provider billing dimension.

    ``dimension`` (such as ``input_tokens``) and ``unit`` (such as
    ``1m_tokens``) are each 1 to 128 bytes; ``price`` is a non-negative
    decimal string per unit.
    """

    dimension: str
    unit: str
    price: str

class GatewayModelPricing(TypedDict):
    """One immutable pricing version for a model.

    ``version`` is a 1 to 128 byte label whose content never changes: only
    ``active`` may differ on resubmission. ``effective_at`` is the RFC 3339
    admission time the entry applies from, ``active`` makes it eligible for
    newly admitted calls, and ``rates`` must be non-empty.
    """

    model: ModelRef
    version: str
    currency: str
    effective_at: str
    active: bool
    rates: list[GatewayPriceRate]

class GatewayGovernancePolicy(TypedDict):
    """Tenant limits, budgets, pricing, and unknown-cost admission.

    The default policy has no limits, budgets, or active pricing and
    ``unknown_cost`` ``"allow_unpriced"``.
    """

    limits: list[GatewayLimit]
    budgets: list[GatewayBudget]
    pricing: list[GatewayModelPricing]
    unknown_cost: UnknownCostPolicy

class GatewayCapturePolicyWrite(TypedDict):
    """Capture policy body for ``Gateway.put_capture_policy()``.

    ``payload_fields`` must be non-empty in ``"payload"`` mode and empty
    otherwise.
    """

    mode: GatewayCaptureMode
    payload_fields: list[GatewayPayloadField]

class GatewayCapturePolicy(TypedDict):
    """Effective capture policy and its content version.

    ``version`` starts at 1 and increases only when the effective content
    changes. The default policy is ``"disabled"`` at version 1.
    """

    mode: GatewayCaptureMode
    payload_fields: list[GatewayPayloadField]
    version: int

class Gateway:
    """Tenant gateway administration client.

    Request bodies are plain dicts in the ``wyrd/v1`` gateway wire shape and are
    decoded against the typed contract before sending; a rejected body raises
    ``WYRD_SPEC_400_VALIDATION`` naming the argument and its decode position and
    quoting none of the value. Cross-field rules are checked by the server and
    raise ``WYRD_GATEWAY_400_INVALID_CONFIGURATION`` naming
    ``details["field"]``. Responses are dicts. Reads need ``gateway:read``,
    writes and policy resets ``gateway:write``, and deployment deletion
    ``gateway:delete``; a missing permission raises
    ``WYRD_PERMISSION_403_DENIED_RBAC``. Every failure raises ``WyrdError``
    with a stable ``code``.

    Provider credential mutation is absent: this class has no method to submit,
    rotate, revoke, or delete a credential, so no dict built at runtime can
    reach a managed secret write. Use the Wyrd CLI or the scoped MCP tool.
    """

    def __init__(self, client: WyrdClient | None = None) -> None:
        """Connect to a Wyrd server.

        Args:
            client: the ``WyrdClient`` to act as, sharing its transport and
                token cache. Omitted, the ambient client resolves from
                ``[client]`` in the Wyrd ``config.toml``, then the
                environment, then the saved ``wyrd auth login``.

        Raises:
            WyrdError: ``WYRD_CLIENT_401_NO_CREDENTIALS`` when no ambient
                credential resolves, or a client configuration error.

        """
        ...

    def credential(self, name: str) -> ProviderCredentialView:
        """Read one redacted provider credential.

        Args:
            name: the credential name, a lowercase Wyrd name (3 to 64 of
                ``a-z``, ``0-9``, ``_``, ``-``, starting with a letter).

        Returns:
            The redacted credential view; it never carries secret material.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for an invalid name;
                ``WYRD_GATEWAY_404_RESOURCE_NOT_FOUND`` when the tenant has
                no such credential.

        """
        ...

    def credentials(self) -> list[ProviderCredentialView]:
        """List the tenant's redacted provider credentials ordered by name.

        Returns:
            Every redacted credential view.

        Raises:
            WyrdError: ``WYRD_PERMISSION_403_DENIED_RBAC`` without the
                permission, or the server's transport error.
        """
        ...

    def put_deployment(self, deployment: ProviderDeployment) -> ProviderDeployment:
        """Create or replace the deployment named by ``deployment["name"]``.

        Only newly admitted calls observe the change.

        Args:
            deployment: the complete deployment; see ``ProviderDeployment``.

        Returns:
            The stored deployment, with the auth header name lowercased.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` when the dict does not
                decode; ``WYRD_GATEWAY_400_INVALID_CONFIGURATION`` when the
                adapter, provider, or capabilities disagree, or the referenced
                credential is absent, revoked, for another provider, or not
                assigned to this tenant and endpoint.

        """
        ...

    def deployment(self, name: str) -> ProviderDeployment:
        """Read one provider deployment.

        Args:
            name: the deployment name, a lowercase Wyrd name.

        Returns:
            The stored deployment.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for an invalid name;
                ``WYRD_GATEWAY_404_RESOURCE_NOT_FOUND`` when the tenant has
                no such deployment.

        """
        ...

    def deployments(self) -> list[ProviderDeployment]:
        """List the tenant's provider deployments ordered by name.

        Returns:
            Every stored deployment.

        Raises:
            WyrdError: ``WYRD_PERMISSION_403_DENIED_RBAC`` without the
                permission, or the server's transport error.
        """
        ...

    def delete_deployment(self, name: str) -> None:
        """Delete a provider deployment; deleting an absent name succeeds.

        Args:
            name: the deployment name, a lowercase Wyrd name.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for an invalid name.

        """
        ...

    def put_fallback_policy(self, policy: GatewayFallbackPolicy) -> GatewayFallbackPolicy:
        """Replace the whole tenant fallback policy and return it.

        Args:
            policy: the complete replacement policy; see ``GatewayFallbackPolicy``.

        Returns:
            The stored fallback policy.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` when the dict does not
                decode; ``WYRD_GATEWAY_400_INVALID_CONFIGURATION`` for a
                duplicate scope or an empty, repeated, or self-listing
                candidate list.

        """
        ...

    def fallback_policy(self) -> GatewayFallbackPolicy:
        """Read the tenant fallback policy, or the empty default.

        Returns:
            The stored fallback policy.

        Raises:
            WyrdError: ``WYRD_PERMISSION_403_DENIED_RBAC`` without the
                permission, or the server's transport error.
        """
        ...

    def delete_fallback_policy(self) -> None:
        """Restore the empty default fallback policy; repeating succeeds.

        Raises:
            WyrdError: ``WYRD_PERMISSION_403_DENIED_RBAC`` without the
                permission, or the server's transport error.
        """
        ...

    def put_governance_policy(self, policy: GatewayGovernancePolicy) -> GatewayGovernancePolicy:
        """Replace the tenant governance policy and return the stored result.

        Pricing is never deleted: a stored version omitted from ``policy`` is
        retained inactive, so the returned policy can list more pricing than
        was sent.

        Args:
            policy: the complete replacement policy; see ``GatewayGovernancePolicy``.

        Returns:
            The stored governance policy, pricing included.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` when the dict does not
                decode; ``WYRD_GATEWAY_400_INVALID_CONFIGURATION`` for a
                duplicate limit, budget, or pricing key, an empty limit or rate
                list, a zero budget, mixed currencies, or a changed stored
                pricing version.

        """
        ...

    def governance_policy(self) -> GatewayGovernancePolicy:
        """Read the tenant governance policy, or the empty default.

        Returns:
            The stored governance policy.

        Raises:
            WyrdError: ``WYRD_PERMISSION_403_DENIED_RBAC`` without the
                permission, or the server's transport error.
        """
        ...

    def delete_governance_policy(self) -> None:
        """Clear limits and budgets, restore ``"allow_unpriced"``, and retire pricing.

        Retired pricing is retained; repeating succeeds.

        Raises:
            WyrdError: ``WYRD_PERMISSION_403_DENIED_RBAC`` without the
                permission, or the server's transport error.
        """
        ...

    def put_capture_policy(self, policy: GatewayCapturePolicyWrite) -> GatewayCapturePolicy:
        """Replace the tenant capture policy and return its versioned view.

        Writing the current content leaves ``version`` unchanged.

        Args:
            policy: the complete replacement policy; see ``GatewayCapturePolicyWrite``.

        Returns:
            The stored, versioned capture policy.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` when the dict does not
                decode; ``WYRD_GATEWAY_400_INVALID_CONFIGURATION`` when
                ``payload_fields`` disagrees with ``mode``.

        """
        ...

    def capture_policy(self) -> GatewayCapturePolicy:
        """Read the tenant capture policy, or the disabled version-1 default.

        Returns:
            The stored, versioned capture policy.

        Raises:
            WyrdError: ``WYRD_PERMISSION_403_DENIED_RBAC`` without the
                permission, or the server's transport error.
        """
        ...

__all__ = [
    "ApiKeyHeaderAuth",
    "BearerAuth",
    "EnvironmentCredentialSource",
    "ExternalSecretCredentialSource",
    "FallbackRule",
    "FallbackScope",
    "Gateway",
    "GatewayAuth",
    "GatewayBudget",
    "GatewayBudgetPeriod",
    "GatewayCaptureMode",
    "GatewayCapturePolicy",
    "GatewayCapturePolicyWrite",
    "GatewayFallbackPolicy",
    "GatewayGovernancePolicy",
    "GatewayLimit",
    "GatewayLimitSubject",
    "GatewayModelPricing",
    "GatewayOperation",
    "GatewayPayloadField",
    "GatewayPolicySubject",
    "GatewayPolicyTarget",
    "GatewayPriceRate",
    "ModelFallbackScope",
    "ModelRef",
    "ModelTarget",
    "OpenAiCompatibleAdapter",
    "OperationFallbackScope",
    "PrincipalSubject",
    "ProviderAdapter",
    "ProviderAuth",
    "ProviderCredentialSource",
    "ProviderCredentialSourceView",
    "ProviderCredentialState",
    "ProviderCredentialView",
    "ProviderDeployment",
    "ProviderTarget",
    "RoleSubject",
    "UnknownCostPolicy",
    "VertexAdapter",
]
