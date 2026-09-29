"""Tenant gateway administration: provider credentials, deployments, and policies."""

from __future__ import annotations

from typing import Literal, TypedDict

from wyrd._wyrd.gateway import Gateway

GatewayOperation = Literal[
    "chat_completions", "responses", "embeddings", "images", "audio", "batches"
]
ProviderCredentialState = Literal["active", "revoked"]
GatewayCaptureMode = Literal["disabled", "metadata", "payload"]
GatewayPayloadField = Literal["request", "response"]
GatewayBudgetPeriod = Literal["calendar_day_utc", "calendar_month_utc"]
UnknownCostPolicy = Literal["reject", "allow_unpriced"]


class ModelRef(TypedDict):
    """One exact provider-native model."""

    provider: str
    model: str


class _EnvironmentBinding(TypedDict):
    binding: str


class EnvironmentCredentialSource(TypedDict):
    """Operator-configured environment variable or mounted-file binding."""

    environment: _EnvironmentBinding


class _ExternalSecret(TypedDict):
    backend: str
    reference: str


class ExternalSecretCredentialSource(TypedDict):
    """Reference resolved from an operator-configured secret backend."""

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
    """Redacted provider credential; never carries secret material."""

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
    """Google Vertex GenerateContent protocol."""

    vertex: _VertexLocation


class _OpenAiCompatibleBase(TypedDict):
    base_url: str


class OpenAiCompatibleAdapter(TypedDict):
    """Standard OpenAI-compatible routes under a tenant base URL."""

    openai_compatible: _OpenAiCompatibleBase


ProviderAdapter = Literal["openai", "anthropic", "gemini"] | VertexAdapter | OpenAiCompatibleAdapter


class _BearerAuth(TypedDict):
    credential: str


class BearerAuth(TypedDict):
    """``Authorization: Bearer <credential>``."""

    bearer: _BearerAuth


class _ApiKeyHeaderAuth(TypedDict):
    header: str
    credential: str


class ApiKeyHeaderAuth(TypedDict):
    """A named API-key header carrying the credential."""

    api_key_header: _ApiKeyHeaderAuth


ProviderAuth = Literal["none"] | BearerAuth | ApiKeyHeaderAuth


class ProviderDeployment(TypedDict):
    """Named provider deployment; also the ``PUT`` body."""

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


class FallbackRule(TypedDict):
    """Ordered fallback candidates for one scope."""

    scope: FallbackScope
    candidates: list[ModelRef]


class GatewayFallbackPolicy(TypedDict):
    """Tenant fallback policy."""

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
    """Rate and concurrency limit for one subject and target."""

    subject: GatewayLimitSubject
    target: GatewayPolicyTarget
    requests_per_minute: int | None
    tokens_per_minute: int | None
    concurrent_calls: int | None


class GatewayBudget(TypedDict):
    """Spending budget for one subject and period."""

    subject: GatewayPolicySubject
    period: GatewayBudgetPeriod
    amount: str
    currency: str


class GatewayPriceRate(TypedDict):
    """Price for one provider billing dimension."""

    dimension: str
    unit: str
    price: str


class GatewayModelPricing(TypedDict):
    """One immutable pricing version for a model."""

    model: ModelRef
    version: str
    currency: str
    effective_at: str
    active: bool
    rates: list[GatewayPriceRate]


class GatewayGovernancePolicy(TypedDict):
    """Tenant limits, budgets, pricing, and unknown-cost admission."""

    limits: list[GatewayLimit]
    budgets: list[GatewayBudget]
    pricing: list[GatewayModelPricing]
    unknown_cost: UnknownCostPolicy


class GatewayCapturePolicyWrite(TypedDict):
    """``PUT /v1/admin/gateway/capture-policy`` body."""

    mode: GatewayCaptureMode
    payload_fields: list[GatewayPayloadField]


class GatewayCapturePolicy(TypedDict):
    """Effective capture policy and its content version."""

    mode: GatewayCaptureMode
    payload_fields: list[GatewayPayloadField]
    version: int


__all__ = [
    "ApiKeyHeaderAuth",
    "BearerAuth",
    "EnvironmentCredentialSource",
    "ExternalSecretCredentialSource",
    "FallbackRule",
    "FallbackScope",
    "Gateway",
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
