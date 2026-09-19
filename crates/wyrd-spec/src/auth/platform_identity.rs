//! Wire contract for the platform-scope human identity plane.
//!
//! One deployment-owned OIDC connection, and the human platform principals
//! registered against it. Provider secrets travel inbound only: no response
//! here carries one, which is why the configure request and the connection view
//! are separate types rather than one round-tripped shape.

use serde::{Deserialize, Serialize};

use crate::auth::PrincipalId;

/// How Wyrd authenticates to the platform provider's token endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case", tag = "method")]
pub enum PlatformClientAuth {
    /// HTTP Basic with a client secret.
    SecretBasic {
        /// The client secret, sealed before storage and never returned.
        secret: String,
    },
    /// Client secret in the request body.
    SecretPost {
        /// The client secret, sealed before storage and never returned.
        secret: String,
    },
    /// A public client with no secret.
    Public,
}

/// Install or replace the deployment's platform OIDC connection.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct ConfigurePlatformOidcRequest {
    /// Issuer URL, pinned as the `iss` of every accepted token.
    pub issuer_url: String,
    /// JWKS endpoint signing keys are fetched from.
    pub jwks_uri: String,
    /// Audience pinned as the `aud` of every accepted token.
    pub expected_audience: String,
    /// Wyrd's client identifier at the provider.
    pub client_id: String,
    /// How Wyrd authenticates to the provider.
    pub client_auth: PlatformClientAuth,
    /// JWKS cache lifetime in seconds.
    #[serde(default = "default_jwks_ttl_secs")]
    pub jwks_ttl_secs: i64,
}

/// Default JWKS cache lifetime: long enough to avoid hammering the provider,
/// short enough that a key rotation is picked up without an operator action.
const fn default_jwks_ttl_secs() -> i64 {
    300
}

/// The configured connection, with nothing secret in it.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct PlatformOidcConnectionView {
    /// Issuer URL.
    pub issuer_url: String,
    /// JWKS endpoint.
    pub jwks_uri: String,
    /// Expected audience.
    pub expected_audience: String,
    /// Client identifier.
    pub client_id: String,
    /// Client authentication method name only — never the secret itself.
    pub client_auth: String,
    /// JWKS cache lifetime in seconds.
    pub jwks_ttl_secs: i64,
}

/// Pre-register a human platform administrator.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct RegisterPlatformAdminRequest {
    /// Operator-facing name, unique across the deployment.
    pub name: String,
    /// Claim value matched on this administrator's first login only. After that
    /// the pinned subject resolves them and this is no longer consulted.
    pub match_claim: String,
}

/// A registered human platform administrator, awaiting first login.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct RegisterPlatformAdminResponse {
    /// Durable principal id, stable across every later login.
    pub principal_id: PrincipalId,
}

/// Begin a platform federated login.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct PlatformLoginRequest {
    /// Where the provider returns the browser after authentication.
    pub redirect_uri: String,
}

/// Complete a platform federated login.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct PlatformCallbackRequest {
    /// Authorization code returned by the provider.
    pub code: String,
    /// Opaque state key issued when the login began.
    pub state: String,
}
