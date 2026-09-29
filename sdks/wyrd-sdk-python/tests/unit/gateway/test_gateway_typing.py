"""Type fixture pinning the gateway administration wire shapes as real types.

`ty` checks this module in the `py:typecheck` lane, so each assignment is a
static assertion that the public TypedDict and Literal projections accept the
contract shape without an `Any` hop. The runtime asserts keep it a unit test.
"""

from __future__ import annotations

from wyrd.gateway import (
    GatewayCapturePolicy,
    GatewayFallbackPolicy,
    GatewayGovernancePolicy,
    ProviderCredentialSourceView,
    ProviderCredentialView,
    ProviderDeployment,
)


def test_gateway_contract_shapes_are_typed_dicts() -> None:
    view: ProviderCredentialView = {
        "name": "openai-key",
        "provider": "openai",
        "source": {"environment": {"binding": "OPENAI_API_KEY"}},
        "state": "revoked",
        "created_at": "2026-07-01T00:00:00Z",
        "updated_at": "2026-07-01T00:00:00Z",
        "rotated_at": None,
        "revoked_at": "2026-07-02T00:00:00Z",
    }
    deployment: ProviderDeployment = {
        "name": "vertex-flash",
        "model": {"provider": "vertex", "model": "gemini-2.5-flash"},
        "adapter": {"vertex": {"project": "p", "location": "us-central1"}},
        "auth": {"api_key_header": {"credential": "openai-key", "header": "x-api-key"}},
        "capabilities": ["chat_completions", "embeddings"],
        "routing_weight": 1,
    }
    fallback: GatewayFallbackPolicy = {
        "rules": [
            {
                "scope": {"operation": {"operation": "responses"}},
                "candidates": [deployment["model"]],
            }
        ]
    }
    governance: GatewayGovernancePolicy = {
        "limits": [
            {
                "subject": {"principal": {"principal_id": "p1"}},
                "target": "all",
                "requests_per_minute": 60,
                "tokens_per_minute": None,
                "concurrent_calls": None,
            }
        ],
        "budgets": [
            {
                "subject": {"role": {"role_name": "analyst"}},
                "period": "calendar_month_utc",
                "amount": "10",
                "currency": "USD",
            }
        ],
        "pricing": [],
        "unknown_cost": "reject",
    }
    capture: GatewayCapturePolicy = {"mode": "payload", "payload_fields": ["request"], "version": 2}

    assert view["source"] == {"environment": {"binding": "OPENAI_API_KEY"}}
    assert deployment["adapter"] != "openai"
    assert fallback["rules"][0]["candidates"][0]["provider"] == "vertex"
    assert governance["budgets"][0]["period"] == "calendar_month_utc"
    assert capture["version"] == 2


def test_managed_secrets_are_readable_but_not_submittable() -> None:
    """The SDK projects the redacted managed source and exports no write type.

    Credential mutation is a CLI and scoped MCP administration path, so the
    package exposes a source view and no write body at all.
    """
    import wyrd.gateway

    assert not hasattr(wyrd.gateway, "ProviderCredentialWrite")
    source: ProviderCredentialSourceView = "managed_secret"
    view: ProviderCredentialView = {
        "name": "openai-key",
        "provider": "openai",
        "source": source,
        "state": "active",
        "created_at": "2026-07-01T00:00:00Z",
        "updated_at": "2026-07-01T00:00:00Z",
        "rotated_at": None,
        "revoked_at": None,
    }

    assert view["source"] == "managed_secret"
