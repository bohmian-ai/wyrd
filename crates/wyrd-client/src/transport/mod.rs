//! Wyrd client transport runtime config.
//!
//! Module map:
//! - [`config`] owns `TransportConfig`, variant configs, and `QueueConfig`.
//! - [`queue`] owns the `Flushable` trait consumed by per-record queues.
//! - [`grpc`], [`http`], and [`mock`] are driver module placeholders for the
//!   runtime implementation that lands after this primitive surface.

pub mod config;
pub mod grpc;
pub mod http;
pub mod mock;
pub mod queue;

pub use config::{GrpcConfig, HttpConfig, MockConfig, QueueConfig, TransportConfig};
pub use queue::Flushable;
