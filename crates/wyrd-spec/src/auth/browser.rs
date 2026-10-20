//! Browser-session contracts for `/auth/browser`.
//!
//! The server alone creates, resolves, expires and revokes browser sessions.
//! A session is an opaque random identifier carried only in a host-only
//! HttpOnly cookie and stored only by digest; no operation here returns it,
//! an OAuth token, or a key. Login and recovery accept a tenant route key plus
//! the applicable recovery input, never a principal or permissions. Session
//! metadata is the safe projection a browser page may render.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::DataTenantId;
use crate::auth::{AbsoluteUrl, PrincipalId, PrincipalKindTag, SecretBearer};
use crate::ids::TenantSlug;

/// Body of `POST /auth/browser/login`: begin tenant SSO in this browser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct BrowserLoginRequest {
    /// The tenant's route key; pre-login routing context only.
    pub tenant: TenantSlug,
}

/// Response of `POST /auth/browser/login`.
///
/// The response also sets the short-lived login-transaction cookie the
/// provider callback requires; the provider state exists only inside the URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct BrowserLoginResponse {
    /// The tenant provider's authorization URL to send the browser to.
    pub authorization_url: AbsoluteUrl,
}

/// Body of `POST /auth/browser/api-key`: recover tenant access with an API
/// key.
///
/// The key itself establishes its tenant; `tenant` must name that same
/// tenant or the request is refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct BrowserApiKeyRequest {
    /// The tenant's route key the recovery page was opened for.
    pub tenant: TenantSlug,
    /// The recovery API key; verified, never stored.
    #[schemars(with = "String")]
    pub api_key: SecretBearer,
}

/// Query of `GET /auth/browser/session`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::IntoParams))]
#[cfg_attr(feature = "server", into_params(parameter_in = Query))]
#[serde(deny_unknown_fields)]
pub struct BrowserSessionQuery {
    /// Resolve only this tenant's session; without it every presented
    /// tenant session is resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<TenantSlug>,
}

/// Body of `POST /auth/browser/logout`: end this browser's session for one
/// tenant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct BrowserLogoutRequest {
    /// The tenant route key whose session cookie is revoked and cleared.
    pub tenant: TenantSlug,
}

/// How a browser session was created.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum BrowserCredential {
    /// The tenant's identity provider signed the person in.
    Sso,
    /// A recovery API key signed the browser in.
    ApiKey,
}

/// Safe metadata of one live tenant browser session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct BrowserSessionView {
    /// The tenant's route key.
    pub tenant: TenantSlug,
    /// The tenant's isolation key.
    pub tenant_id: DataTenantId,
    /// The tenant's display name.
    pub tenant_name: String,
    /// The signed-in principal.
    pub principal_id: PrincipalId,
    /// That principal's kind.
    pub principal_kind: PrincipalKindTag,
    /// How the session was created.
    pub credential: BrowserCredential,
    /// The principal's current operations as `resource:action` names,
    /// resolved for this request; informational only, never authority.
    pub permissions: Vec<String>,
    /// The session's absolute expiry; it never slides.
    pub expires_at: DateTime<Utc>,
}

/// Response of `GET /auth/browser/session`: every live presented session,
/// one per tenant, never merged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct BrowserSessionsResponse {
    /// The live sessions, in the order their cookies were presented.
    pub sessions: Vec<BrowserSessionView>,
}
