"""Tenant gateway administration: provider credentials, deployments, and policies."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from wyrd._wyrd.gateway import Gateway

from ._policy import (
    ApiKeyHeaderAuth,
    BearerAuth,
    EnvironmentCredentialSource,
    ExternalSecretCredentialSource,
    FallbackRule,
    FallbackScope,
    GatewayBudget,
    GatewayBudgetPeriod,
    GatewayCaptureMode,
    GatewayCapturePolicy,
    GatewayCapturePolicyWrite,
    GatewayFallbackPolicy,
    GatewayGovernancePolicy,
    GatewayLimit,
    GatewayLimitSubject,
    GatewayModelPricing,
    GatewayOperation,
    GatewayPayloadField,
    GatewayPolicySubject,
    GatewayPolicyTarget,
    GatewayPriceRate,
    ModelFallbackScope,
    ModelRef,
    ModelTarget,
    OpenAiCompatibleAdapter,
    OperationFallbackScope,
    PrincipalSubject,
    ProviderAdapter,
    ProviderAuth,
    ProviderCredentialSource,
    ProviderCredentialSourceView,
    ProviderCredentialState,
    ProviderCredentialView,
    ProviderDeployment,
    ProviderTarget,
    RoleSubject,
    UnknownCostPolicy,
    VertexAdapter,
)

if TYPE_CHECKING:
    from wyrd.gateway._auth import GatewayAuth

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


def __getattr__(name: str) -> Any:
    """Load ``GatewayAuth`` on first use, so ``httpx`` stays optional."""
    if name == "GatewayAuth":
        from wyrd.gateway._auth import GatewayAuth

        return GatewayAuth
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
