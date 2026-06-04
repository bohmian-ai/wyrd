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
#[cfg(feature = "python")]
pub mod python;
pub mod runtime;

pub use dispatch::{dispatch, dispatch_stream};
pub use error::{SkaldRuntimeError, SkaldRuntimeResult};
pub use mock::{MockExchange, MockExpectation, MockProvider};
pub use provider::{
    Provider, ProviderRegistry, default_registry, refresh_default_registry_from_env,
};
#[cfg(feature = "python")]
pub use python::python_register;
pub use runtime::{RuntimeConfig, SkaldRuntime};
