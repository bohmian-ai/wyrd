//! Wyrd-side observation primitives.

#![deny(missing_docs)]
#![allow(clippy::module_name_repetitions)]

mod current;
mod error_map;
mod redaction;
mod run_id;
mod sampling;
mod scoped;

pub use current::{current, set_global};
pub use error_map::{WyrdErrorVariant, map_skald_code};
pub use redaction::{REDACTED_PLACEHOLDER, RedactionPolicy};
pub use run_id::{ObservationId, RunId, RunIdSource};
pub use sampling::SamplingPolicy;
pub use scoped::with_observer;

pub use skald_agent::observer::Observer;
