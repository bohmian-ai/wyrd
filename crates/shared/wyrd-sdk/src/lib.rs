//! Rust-owned client SDK behavior for local Wyrd state and Python projections.

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

mod state;

pub use state::{StateCard, WyrdState};

#[cfg(feature = "python")]
pub mod python;
