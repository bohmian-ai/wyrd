//! Wyrd-side observer adapter for Skald agent runs.
//!
//! Skald owns the agent-loop hook. This crate implements that hook on the
//! Wyrd side and emits Vala records through [`vala_client::ValaClient`].

#![deny(missing_docs)]
#![allow(clippy::module_name_repetitions)]

mod error_map;
mod observer;
mod redaction;
mod run_id;
mod sampling;

pub use error_map::{WyrdErrorVariant, map_skald_code};
pub use observer::WyrdObserver;
pub use redaction::{REDACTED_PLACEHOLDER, RedactionPolicy};
pub use run_id::{ObservationId, RunId, RunIdSource};
pub use sampling::SamplingPolicy;
