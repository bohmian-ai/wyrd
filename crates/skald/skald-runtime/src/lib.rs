//! Native Skald runtime dispatch.
//!
//! The runtime resolves a provider from a native [`skald_spec::ProviderRequest`],
//! sends it through a registered provider client, and returns the native
//! [`skald_spec::ProviderResponse`]. Callers read responses through
//! `ProviderResponse::adapter()`; this crate does not define a neutral run
//! output.

pub mod dispatch;
pub mod error;
pub mod mock;
pub mod provider;
pub mod runtime;

pub use dispatch::dispatch;
pub use error::{SkaldRuntimeError, SkaldRuntimeResult};
pub use mock::{MockExchange, MockExpectation, MockProvider};
pub use provider::{Provider, ProviderRegistry};
pub use runtime::{RuntimeConfig, SkaldRuntime};
