//! Router mount for authz-check and principal management.

use crate::state::AppState;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

/// Build authz-check and principal management routes for the `/v1` group.
pub fn authz_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(super::check::check_authz))
        .routes(routes!(crate::auth::revoke::revoke_principal))
}
