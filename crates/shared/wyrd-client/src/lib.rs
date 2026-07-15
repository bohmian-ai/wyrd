//! External Wyrd client runtime surfaces.
//!
//! This crate owns the external client transport configuration and credential
//! resolution. Durable shared security refs live in `wyrd-spec`.

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

pub mod auth;
pub mod client;
pub mod config;
pub mod error;
pub mod transport;

pub use client::WyrdClient;

/// Serializes tests that read or mutate process-global `WYRD_*`/`HOME`
/// environment variables. `ClientConfig::from_env` and
/// `CredentialChain::from_env` read ambient env, so env-touching tests across
/// modules in this lib binary must hold this lock for their full duration —
/// otherwise they race under the parallel workspace test runner. Recover from
/// poisoning so one failing test does not cascade.
#[cfg(test)]
pub(crate) static ENV_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());
