//! Embedding helper aliases for provider clients.
//!
//! Embedding requests are native `ProviderRequest` variants in this crate's
//! client surface, so the concrete provider clients send them directly.

pub use super::{GoogleClient, OpenAiClient, VertexClient};
