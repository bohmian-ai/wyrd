//! Durable shared security primitives.
//!
//! `TlsConfig` and `SecretRef` are consumed by both the external client
//! transport surface and the server-internal alert router. The types are pure
//! data, PyO3-free, async-free, and WASM-safe.

pub mod secret_ref;
pub mod tls;

#[cfg(any(test, feature = "test-utils"))]
pub use secret_ref::InlineSecret;
pub use secret_ref::SecretRef;
pub use tls::TlsConfig;
