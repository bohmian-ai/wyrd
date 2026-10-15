from ..client import WyrdClient
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
    def connect(client: WyrdClient | None = None) -> Gateway:
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
