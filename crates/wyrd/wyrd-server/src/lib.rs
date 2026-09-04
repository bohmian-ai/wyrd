//! Wyrd server boot and routing primitives.

pub mod app;
pub mod audit;
pub mod auth;
pub mod bifrost;
pub mod boot;
pub mod components;
pub mod config;
pub mod grpc;
pub mod http;
pub mod mcp;
pub mod openapi;
pub mod oracle;
mod otlp_decode;
mod otlp_json;
mod otlp_logs_decode;
mod otlp_logs_json;
mod otlp_metrics_decode;
mod otlp_metrics_json;
mod otlp_trace_json;
pub mod postgres;
pub mod query;
pub mod state;
pub mod vala_query;

#[cfg(test)]
pub(crate) mod test_support;

pub use app::metrics::{TelemetryRuntimeError, WyrdTelemetryRuntime};
#[cfg(feature = "test-support")]
pub use app::metrics::{install_capture_runtime, start_capture_forge_role};
#[cfg(feature = "test-support")]
pub use app::run_forge_worker_process_for_test;
pub use app::{BootExit, BoundServer, WyrdServer, run, serve};
pub use boot::{ServerBootError, StateOverrides, build_state, spawn_storage_sweeper};
#[cfg(feature = "test-support")]
pub use config::BifrostTarget;
pub use config::{ServeMode, WyrdServerConfig};
pub use http::build_router;
pub use postgres::{ServerPostgres, ServerPostgresError};
pub use state::AppState;
