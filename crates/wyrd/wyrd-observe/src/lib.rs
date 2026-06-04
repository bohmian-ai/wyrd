//! Wyrd-side observation primitives.

#![deny(missing_docs)]
#![allow(clippy::module_name_repetitions)]

mod composite;
mod current;
mod error_map;
#[cfg(feature = "otel")]
mod otel;
mod redaction;
mod run_id;
mod sampling;
mod scoped;

pub use composite::CompositeObserver;
pub use current::{current, set_global};
pub use error_map::{WyrdErrorVariant, map_skald_code};
#[cfg(feature = "otel")]
pub use otel::OtelObserver;
pub use redaction::{REDACTED_PLACEHOLDER, RedactionPolicy};
pub use run_id::{ObservationId, RunId, RunIdSource};
pub use sampling::SamplingPolicy;
pub use scoped::with_observer;

pub use skald_agent::observer::Observer;
