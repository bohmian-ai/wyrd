//! Wyrd server boot and routing primitives.

pub mod boot;
pub mod state;

use axum::{Router, routing::get};

pub use boot::{ServerBootError, build_app_state};
pub use state::AppState;

/// Build the HTTP router with shared server state.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .with_state(state)
}
