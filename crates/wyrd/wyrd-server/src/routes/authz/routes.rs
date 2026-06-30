//! Router mount for authz-check and principal management.

use axum::Router;
use axum::routing::post;

use crate::state::AppState;

/// Mount authz-check and principal management routes into an existing `/v1` router.
pub fn mount(router: Router<AppState>, _: &AppState) -> Router<AppState> {
    router
        .route("/authz/check", post(super::check::check_authz))
        .route(
            "/principals/{id}/revoke",
            post(crate::auth::revoke::revoke_principal),
        )
}
