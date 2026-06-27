//! Wyrd client transport runtime.
//!
//! Module map:
//! - [`config`] — `TransportConfig`, `GrpcConfig`, `HttpConfig`, `MockConfig`.
//! - [`credential`] — ADC-style credential resolution chain.
//! - [`grpc`] — generic authed gRPC channel (stub; logic lands in 07).
//! - [`http`] — async `reqwest` HTTP transport (gated on `transport-http`).
//! - [`mock`] — in-memory loopback transport for tests.

pub mod config;
pub mod credential;
pub mod grpc;
pub mod http;
pub mod mock;

pub use config::{GrpcConfig, HttpConfig, MockConfig, TransportConfig};
pub use credential::{CredentialChain, CredentialSource, ResolvedCredential};
#[cfg(feature = "transport-http")]
pub use http::{ArrowResponse, HttpTransport};
pub use mock::{MockRecord, MockTransport};
