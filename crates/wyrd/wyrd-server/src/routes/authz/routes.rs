//! Router mount for authz-check and principal management.

use axum::Router;
use axum::routing::post;

use crate::state::AppState;

/// Standalone authz-check and principal management router for the `/v1` group.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/authz/check", post(super::check::check_authz))
        .route(
            "/principals/{id}/revoke",
            post(crate::auth::revoke::revoke_principal),
        )
}
