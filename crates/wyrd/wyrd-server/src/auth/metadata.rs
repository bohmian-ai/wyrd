//! RFC 8414 authorization server metadata at
//! `/.well-known/oauth-authorization-server`.
//!
//! The metadata describes the token, device authorization, and revocation
//! endpoints on the origin the request arrived at ([`RequestOrigin`]), so
//! nothing is configured. Browser sign-in is not an OAuth client flow: it
//! uses Wyrd's own browser-session operations under `/auth/browser`, so no
//! authorization endpoint or response type is advertised.

use axum::Json;
use axum::extract::State;
use wyrd_spec::auth::{AbsoluteUrl, AuthorizationServerMetadata};
use wyrd_spec::error::{WyrdError, WyrdProblem};

use crate::auth::oauth::GRANT_TYPES;
use crate::auth::{RequestOrigin, auth_not_configured};
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// `GET /.well-known/oauth-authorization-server` — RFC 8414 §3 metadata,
/// with every endpoint on the origin the request arrived at.
///
/// # Errors
/// Returns problem JSON when auth is not configured or the request names no
/// usable host.
#[utoipa::path(
    get,
    path = "/.well-known/oauth-authorization-server",
    responses(
        (status = 200, description = "Authorization server metadata",
          body = AuthorizationServerMetadata),
        (status = 400, description = "The request names no usable host \
          (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 500, description = "Auth is not configured (WYRD_SPEC_500_INTERNAL)",
          body = WyrdProblem)
    ),
    security(()),
    tag = "Auth"
)]
pub async fn metadata(
    State(state): State<AppState>,
    RequestOrigin(origin): RequestOrigin,
) -> Result<Json<AuthorizationServerMetadata>, WyrdErrorResponse> {
    state
        .auth
        .human_connections
        .as_ref()
        .ok_or_else(auth_not_configured)?;
    let origin = origin.origin().ascii_serialization();
    let url = |path: &str| {
        AbsoluteUrl::new(format!("{origin}{path}")).map_err(|error| {
            WyrdErrorResponse::from(WyrdError::Internal {
                message: format!("the request origin does not form an endpoint URL: {error}"),
                details: serde_json::json!({}),
            })
        })
    };
    let strings = |values: &[&str]| values.iter().map(|value| (*value).to_owned()).collect();
    let auth_methods = strings(&["none"]);
    Ok(Json(AuthorizationServerMetadata {
        issuer: url("")?,
        token_endpoint: url("/auth/token")?,
        device_authorization_endpoint: url("/auth/device_authorization")?,
        revocation_endpoint: url("/auth/revoke")?,
        response_types_supported: Vec::new(),
        grant_types_supported: strings(&GRANT_TYPES),
        token_endpoint_auth_methods_supported: Vec::clone(&auth_methods),
        revocation_endpoint_auth_methods_supported: auth_methods,
    }))
}
