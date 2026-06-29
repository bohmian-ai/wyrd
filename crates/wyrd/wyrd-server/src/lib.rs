//! Wyrd server boot and routing primitives.

pub mod auth;
pub mod auth_boot;
pub mod boot;
pub mod config;
pub mod error;
pub mod grpc;
pub mod health;
pub mod issuer_boot;
pub mod middleware;
pub mod router;
pub mod routes;
pub mod serve;
pub mod shutdown;
pub mod state;
pub mod storage;

use axum::Router;

pub use boot::{
    ServerBootError, build_app_state, build_app_state_from_config, spawn_storage_sweeper,
};
pub use config::WyrdServerConfig;
pub use router::build_router;
pub use serve::serve;
pub use state::AppState;

/// Build the HTTP router with shared server state.
pub fn router(state: AppState) -> Router {
    build_router(state)
}

/// Basic liveness endpoint.
pub async fn healthz() -> &'static str {
    "ok"
}
