//! Wyrd client transport runtime config.
//!
//! This module owns the runtime config types that parameterize
//! `WyrdClient::new()` plus the per-record-queue `Flushable` trait.
//! Durable shared security refs (`TlsConfig`, `SecretRef`) live in
//! `wyrd-spec::security`; this module imports them.
//!
//! Module map:
//! - `config` — `TransportConfig` enum (`Grpc`, `Http`, `Mock`),
//!   `GrpcConfig`, `HttpConfig`, `MockConfig`, `QueueConfig`.
//! - `queue` — the `Flushable` trait consumed by per-record queues.
//!
//! Re-exports below are populated in their owning commits (3 onward).
//! Commit 8 carries the final consolidated re-export block.

pub mod config;
pub mod queue;
