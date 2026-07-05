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
pub mod storage;

pub use app::{build_app, run, serve};
pub use boot::{
    ServerBootError, build_app_state, build_app_state_from_config, spawn_storage_sweeper,
};
pub use config::WyrdServerConfig;
pub use http::build_router;
pub use postgres::{ServerPostgres, ServerPostgresError};
pub use state::AppState;
