//! Router mount for authz-check.

use axum::Router;
use axum::routing::post;

use crate::state::AppState;

/// Mount authz-check routes into an existing `/v1` router.
pub fn mount(router: Router<AppState>, _: &AppState) -> Router<AppState> {
    router.route("/authz/check", post(super::check::check_authz))
}
