//! The shared Wyrd client: the sole SDK-facing Rust client surface.
//!
//! This crate owns client transport configuration, credential resolution,
//! authentication, and the composed client capabilities — [`cards::Cards`],
//! [`storage::WyrdStorageClient`], [`state::WyrdState`], and [`Bifrost`] —
//! that the Rust, Python, and TypeScript SDKs project. Durable
//! shared security refs live in `wyrd-spec`.

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

pub mod auth;
pub mod bifrost;
pub mod cards;
pub mod client;
pub mod config;
pub mod error;
pub mod eval;
pub mod global_config;
pub mod platform;
pub mod principals;
pub mod state;
pub mod storage;
pub mod transport;

pub use bifrost::Bifrost;
pub use client::WyrdClient;
pub use eval::EvalProtocol;
pub use global_config::GlobalConfig;
pub use platform::Platform;
pub use principals::Principals;

/// Serializes tests that read or mutate process-global `WYRD_*`/`HOME`
/// environment variables. `ClientConfig::from_env` and
/// `CredentialChain::from_env` read ambient env, so env-touching tests across
/// modules in this lib binary must hold this lock for their full duration —
/// otherwise they race under the parallel workspace test runner. Recover from
/// poisoning so one failing test does not cascade.
#[cfg(test)]
pub(crate) static ENV_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());
