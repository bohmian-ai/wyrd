//! Wyrd client transport runtime.
//!
//! Module map:
//! - [`config`] — `TransportConfig`, `GrpcConfig`, `HttpConfig`, `MockConfig`.
//! - [`credential`] — ADC-style credential resolution chain.
//! - [`grpc`] — generic authed gRPC channel (gated on `transport-grpc`).
//! - [`http`] — async `reqwest` HTTP transport (gated on `transport-http`).
//! - [`mock`] — in-memory loopback transport for tests.

pub mod config;
pub mod credential;
pub mod grpc;
pub mod http;
pub mod mock;

/// Exponential backoff delay in milliseconds for a 0-based retry `attempt`.
///
/// Shared by the HTTP retry loop and the gRPC connect-retry loop so both paths
/// back off on the same schedule: attempt 0 → 100 ms, 1 → 1 s, ≥2 → 5 s (cap).
pub(crate) fn backoff_ms(attempt: u32) -> u64 {
    const DELAYS: &[u64] = &[100, 1_000];
    DELAYS.get(attempt as usize).copied().unwrap_or(5_000)
}

pub use config::{GrpcConfig, HttpConfig, MockConfig, TransportConfig};
pub use credential::{CredentialChain, CredentialSource, ResolvedCredential};
pub use grpc::GrpcConnection;
pub use http::{ArrowResponse, HttpTransport};
pub use mock::{MockRecord, MockTransport};
