//! External Wyrd client runtime surfaces.
//!
//! This crate owns the external client transport configuration and credential
//! resolution. Durable shared security refs live in `wyrd-spec`.

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]
#![allow(clippy::module_name_repetitions)]

pub mod auth;
pub mod config;
pub mod error;
pub mod transport;
