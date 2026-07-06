//! Wyrd server boot and routing primitives.

pub mod app;
pub mod auth;
pub mod boot;
pub mod components;
pub mod config;
pub mod grpc;
pub mod http;
pub mod postgres;
pub mod state;

#[cfg(test)]
pub(crate) mod test_support;

pub use app::{BootExit, WyrdServer, run, serve};
pub use boot::{ServerBootError, StateOverrides, build_state, spawn_storage_sweeper};
pub use config::{ServeMode, WyrdServerConfig};
pub use http::build_router;
pub use postgres::{ServerPostgres, ServerPostgresError};
pub use state::AppState;
