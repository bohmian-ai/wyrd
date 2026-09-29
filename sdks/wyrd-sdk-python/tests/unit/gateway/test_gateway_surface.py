"""Public surface tests for the gateway administration client."""

from __future__ import annotations

import pytest

METHODS = [
    "credential",
    "credentials",
    "put_deployment",
    "deployment",
    "deployments",
    "delete_deployment",
    "put_fallback_policy",
    "fallback_policy",
    "delete_fallback_policy",
    "put_governance_policy",
    "governance_policy",
    "delete_governance_policy",
    "put_capture_policy",
    "capture_policy",
]

MUTATIONS = ["put_credential", "revoke_credential", "delete_credential"]

SENTINEL = "sk-live-python-surface-sentinel"


def test_gateway_is_exported_with_every_administration_method():
    from wyrd._wyrd.gateway import Gateway as NativeGateway
    from wyrd.gateway import Gateway

    assert Gateway is NativeGateway
    for name in METHODS:
        assert callable(getattr(Gateway, name)), name


def test_gateway_offers_no_provider_credential_mutation():
    """Credential mutation is a CLI and scoped MCP path, not an SDK one.

    The restriction is structural rather than a runtime source check: the
    methods do not exist on the native class, so no dict a caller builds at
    runtime can reach a managed-secret write from Python.
    """
    from wyrd.gateway import Gateway

    for name in MUTATIONS:
        assert not hasattr(Gateway, name), name


def test_gateway_invalid_body_raises_validation_without_echoing_it():
    """A rejected body names its argument and repeats none of its content.

    A gateway write body can carry a provider key. The sentinel below sits
    where a number is required, so serde's own message would quote it verbatim;
    none of it may reach the raised error. Validation also happens before
    transport: the unroutable port would fail with a different code.
    """
    from wyrd import WyrdError
    from wyrd.gateway import Gateway

    gateway = Gateway(server_url="http://127.0.0.1:9", credential="unused")
    with pytest.raises(WyrdError) as captured:
        gateway.put_deployment(
            {
                "name": "openai-primary",
                "model": {"provider": "openai", "model": "gpt-4o"},
                "adapter": "openai",
                "auth": {"bearer": {"credential": "openai-key"}},
                "capabilities": ["chat_completions"],
                "routing_weight": SENTINEL,
            }
        )
    error = captured.value
    assert error.code == "WYRD_SPEC_400_VALIDATION"
    assert error.details["field"] == "deployment"
    assert SENTINEL not in str(error)
    assert SENTINEL not in error.message
    assert SENTINEL not in str(error.details)
