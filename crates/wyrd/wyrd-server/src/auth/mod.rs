//! Authentication extractors for Wyrd HTTP handlers.

pub mod callback;
pub mod jwt_bearer;
pub mod login;
pub mod revoke;

pub(crate) use wyrd_auth::card_scope;
pub(crate) use wyrd_auth::credential_verify;
pub(crate) use wyrd_auth::{
    exchange_api_key, issue_api_key, pg_resolvers, refresh, revocation_resolver, roles,
};
pub use wyrd_auth::{permission_resolver, seed};

// --------------------------------------------------------------------------
// Shared host-header and error helpers used by multiple token-issuing paths.
// --------------------------------------------------------------------------

use axum::http::{HeaderMap, header};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::TenantSlug;

use crate::http::error::WyrdErrorResponse;

/// Extract the tenant slug from the leftmost host label (e.g. `acme` in
/// `acme.wyrd.cloud`). Returns `None` for `localhost`-style hosts.
pub(crate) fn tenant_slug_from_host(headers: &HeaderMap) -> Option<TenantSlug> {
    let host = headers.get(header::HOST)?.to_str().ok()?;
    let host = host.split(':').next().unwrap_or(host);
    let mut segments = host.split('.').filter(|segment| !segment.is_empty());
    let first = segments.next()?;
    let second = segments.next()?;
    if first == "localhost" || second == "localhost" {
        return None;
    }
    TenantSlug::new(first.to_owned()).ok()
}

pub(crate) fn invalid_token(message: &str) -> WyrdErrorResponse {
    WyrdErrorResponse::from(WyrdError::InvalidToken {
        message: message.to_owned(),
        details: serde_json::json!({}),
    })
}

pub(crate) fn auth_not_configured() -> WyrdErrorResponse {
    WyrdErrorResponse::from(WyrdError::Internal {
        message: "auth signing or verification handle is not configured".to_owned(),
        details: serde_json::json!({}),
    })
}
