//! CLI login handoff and refresh revocation contracts.
//!
//! `wyrd auth login` begins a tenant human SSO login with
//! `POST /auth/cli-handoffs`, sends the person's system browser to the
//! returned provider URL, and polls `POST /auth/cli-handoffs/{id}/claim` with
//! the CLI-held verifier until the common callback has completed the login.
//! The browser never sees the verifier or a Wyrd token. `wyrd auth logout`
//! revokes its saved refresh chain with `POST /auth/revoke`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::DataTenantId;
use crate::auth::{AbsoluteUrl, PrincipalId, SecretBearer};
use crate::ids::TenantSlug;

/// `POST /auth/cli-handoffs` request: begin a CLI login.
///
/// The route key is pre-login routing context only, exactly as for
/// `POST /auth/login`; it never becomes tenant authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct CreateCliHandoff {
    /// The tenant's route key (its slug).
    pub tenant_route_key: TenantSlug,
}

/// `POST /auth/cli-handoffs` response: a begun CLI login.
///
/// `poll_verifier` is returned exactly once and only its SHA-256 is stored;
/// the CLI keeps it in memory and presents it to claim or cancel the handoff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct CliHandoff {
    /// Server-issued handoff id; it names the login in later calls.
    pub handoff_id: Uuid,
    /// Provider authorization URL to open in the person's browser. It carries
    /// the login state but no verifier and no Wyrd token.
    pub login_url: AbsoluteUrl,
    /// 256-bit CLI-held secret that alone can claim or cancel the handoff.
    pub poll_verifier: SecretBearer,
    /// When the handoff stops being claimable.
    pub expires_at: DateTime<Utc>,
}

/// Body of `POST /auth/cli-handoffs/{id}/claim` and
/// `POST /auth/cli-handoffs/{id}/cancel`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct CliHandoffProof {
    /// The tenant's route key the handoff was begun at.
    pub tenant_route_key: TenantSlug,
    /// The verifier [`CliHandoff::poll_verifier`] returned.
    pub poll_verifier: SecretBearer,
}

/// `POST /auth/cli-handoffs/{id}/claim` response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum CliHandoffClaim {
    /// The browser sign-in has not completed yet; poll again after the
    /// interval.
    Pending {
        /// Seconds to wait before the next claim.
        retry_after_seconds: u32,
    },
    /// The login completed and this claim consumed it.
    Complete(CliLogin),
}

/// The Wyrd user credential a completed CLI handoff hands to its verifier
/// holder, once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct CliLogin {
    /// The deployment's configured public origin.
    pub server_origin: AbsoluteUrl,
    /// The tenant the credential belongs to.
    pub tenant_id: DataTenantId,
    /// The tenant `User` the credential acts as.
    pub principal_id: PrincipalId,
    /// Short-lived Wyrd access token.
    pub access_token: SecretBearer,
    /// Rotating Wyrd refresh token, renewed through `POST /auth/token`.
    pub refresh_token: SecretBearer,
    /// Access-token expiry.
    pub access_expires_at: DateTime<Utc>,
}

/// Body of `POST /auth/revoke`: end the login a refresh token belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct RevokeRefreshToken {
    /// A refresh token of the login to end; a rotated ancestor of the current
    /// token names the same login.
    pub refresh_token: SecretBearer,
}

#[cfg(test)]
mod tests {
    use super::{CliHandoffClaim, CliHandoffProof};

    /// Pending and complete claims are told apart by `status`, and a proof
    /// refuses unknown fields.
    #[test]
    fn claim_status_is_tagged_and_proofs_are_closed() {
        let pending: CliHandoffClaim = serde_json::from_value(
            serde_json::json!({"status": "pending", "retry_after_seconds": 2}),
        )
        .expect("pending decodes");
        assert_eq!(
            pending,
            CliHandoffClaim::Pending {
                retry_after_seconds: 2
            }
        );
        let extra = serde_json::from_value::<CliHandoffProof>(serde_json::json!({
            "tenant_route_key": "acme", "poll_verifier": "v", "tenant_id": "x"
        }));
        assert!(extra.is_err());
    }
}
