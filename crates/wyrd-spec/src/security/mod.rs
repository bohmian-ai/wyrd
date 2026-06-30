//! Durable shared security primitives.
//!
//! `TlsConfig` and `SecretRef` are consumed by both the external client
//! (`wyrd-client::transport`) and the server-internal alert router
//! (`vala-core::alert_router`). Putting them in `wyrd-spec::security` gives
//! both consumers one durable, schema-bearing definition without a
//! cross-crate dependency cycle.

pub mod secret_ref;
pub mod tls;

#[cfg(any(test, feature = "test-utils"))]
pub use secret_ref::InlineSecret;
pub use secret_ref::{SecretRef, SecretRefError};
pub use tls::TlsConfig;
