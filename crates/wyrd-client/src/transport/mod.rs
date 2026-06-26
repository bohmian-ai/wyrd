//! Wyrd client transport runtime config.
//!
//! Module map:
//! - [`config`] - `TransportConfig`, `GrpcConfig`, `HttpConfig`,
//!   `MockConfig`, `QueueConfig`.
//! - [`queue`] - the `Flushable` trait consumed by per-record queues.

pub mod config;
pub mod credential;
pub mod queue;

pub use config::{GrpcConfig, HttpConfig, MockConfig, QueueConfig, TransportConfig};
pub use credential::{CredentialChain, CredentialSource, ResolvedCredential};
pub use queue::Flushable;
