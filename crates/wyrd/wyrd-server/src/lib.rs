//! Wyrd server boot and routing primitives.

pub mod auth;
pub mod boot;
pub mod error;
pub mod middleware;
pub mod router;
pub mod state;

use axum::Router;

pub use boot::{ServerBootError, build_app_state};
pub use router::build_router;
pub use state::AppState;

/// Build the HTTP router with shared server state.
pub fn router(state: AppState) -> Router {
    build_router(state)
}

/// Basic liveness endpoint.
pub async fn healthz() -> &'static str {
    "ok"
}
