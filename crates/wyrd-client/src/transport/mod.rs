//! Wyrd client transport runtime config.
//!
//! This module owns the runtime config types that parameterize
//! `WyrdClient::new()` plus per-record queue primitives.
//! Durable shared security refs (`TlsConfig`, `SecretRef`) live in
//! `wyrd-spec::security`; this module imports them.
//!
//! Module map:
//! - `config` - `TransportConfig` enum (`Grpc`, `Http`, `Mock`),
//!   `GrpcConfig`, `HttpConfig`, `MockConfig`, `QueueConfig`.
//! - `queue` - per-record queue primitives.
//!
//! Queue-specific re-exports land with the queue primitives.

pub mod config;
pub mod queue;

pub use config::{GrpcConfig, HttpConfig, MockConfig, QueueConfig, TransportConfig};
pub use queue::Flushable;
