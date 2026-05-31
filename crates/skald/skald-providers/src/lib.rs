//! Native HTTP provider clients for Skald.
//!
//! This crate owns provider HTTP transport, auth, retry, streaming decode, and
//! concrete provider clients. Clients serialize native `skald-spec` request
//! variants directly and decode native response variants.

#![allow(clippy::module_name_repetitions)]

pub mod auth;
pub mod clients;
pub mod error;
pub mod raw;
pub mod retry;
pub mod stream;
pub mod trait_;
pub mod transport;

pub use clients::{AnthropicClient, GoogleClient, OpenAiClient, VertexClient};
pub use error::{ProviderError, ProviderResult};
pub use retry::RetryPolicy;
pub use trait_::{ProviderClient, ProviderStream};
pub use transport::{HttpTransport, TransportConfig};
