//! Device authorization (RFC 8628) and token revocation (RFC 7009) contracts.
//!
//! `wyrd auth login` asks `POST /auth/device_authorization` for a device code
//! and a user code, shows the user code, and opens the verification URL. The
//! person approves the code there and signs in through the tenant's normal
//! browser login, while the CLI polls `POST /auth/token` with the
//! `urn:ietf:params:oauth:grant-type:device_code` grant until it receives the
//! Wyrd user credential. The browser never sees the device code or a Wyrd
//! token. Logout revokes a refresh token with `POST /auth/revoke`. Both
//! requests are form-encoded, and unrecognized parameters are ignored.

use serde::{Deserialize, Serialize};

use crate::auth::{AbsoluteUrl, SecretBearer};
use crate::ids::TenantSlug;

/// `POST /auth/device_authorization` parameters (RFC 8628 §3.1), besides
/// the client's `client_id`.
///
/// `tenant` is the one extension parameter: pre-login routing context only,
/// exactly as for `GET /auth/authorize`; it never becomes tenant authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct DeviceAuthorizationRequest {
    /// The tenant's route key (its slug).
    pub tenant: TenantSlug,
}

/// `POST /auth/device_authorization` response (RFC 8628 §3.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct DeviceAuthorization {
    /// Secret the client polls the token endpoint with; returned once and
    /// stored only as its SHA-256.
    pub device_code: SecretBearer,
    /// Short code the person confirms on the verification page.
    pub user_code: String,
    /// Verification page where the person enters the user code.
    pub verification_uri: AbsoluteUrl,
    /// Verification page with the user code filled in.
    pub verification_uri_complete: AbsoluteUrl,
    /// Seconds until the device code expires.
    pub expires_in: u64,
    /// Minimum seconds between token polls.
    pub interval: u64,
}

/// `POST /auth/revoke` parameters (RFC 7009 §2.1), besides the client's
/// authentication or `client_id`. The optional `token_type_hint` is accepted
/// and ignored: only refresh tokens are revocable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct TokenRevocationRequest {
    /// A refresh token of the login to end; a rotated ancestor of the current
    /// token names the same login.
    pub token: SecretBearer,
}
