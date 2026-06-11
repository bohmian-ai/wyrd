//! Durable shared security primitives.
//!
//! This module owns the schema-bearing security refs that both the external
//! client (`wyrd-client`) and the server-internal alert router
//! (`vala-core::alert_router`) depend on. It is PyO3-free, async-free,
//! WASM-safe — the same constraints as the rest of `wyrd-spec`.
//!
//! Module map:
//! - `tls` — `TlsConfig` (CA cert, client cert, SNI override).
//! - `secret_ref` — `SecretRef` (typed pointer into the runtime secret
//!   store) and the test-only `InlineSecret` newtype.
//!
//! Re-exports below are populated in commit 2.

pub mod secret_ref;
pub mod tls;
