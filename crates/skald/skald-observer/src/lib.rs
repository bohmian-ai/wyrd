//! Observer trait and Wyrd-side observation primitives for Skald agent runs.

#![deny(missing_docs)]

mod composite;
mod current;
mod error_map;
pub mod observer;
#[cfg(feature = "otel")]
mod otel;
#[cfg(feature = "python")]
pub mod python;
mod redaction;
mod run_id;
mod sampling;
mod scoped;

pub use composite::CompositeObserver;
pub use current::{current, set_global};
pub use error_map::{WyrdErrorVariant, map_skald_code};
pub use observer::{NoopObserver, Observer};
#[cfg(feature = "otel")]
pub use otel::OtelObserver;
pub use redaction::{REDACTED_PLACEHOLDER, RedactionPolicy};
pub use run_id::{ObservationId, RunId, RunIdSource};
pub use sampling::SamplingPolicy;
pub use scoped::with_observer;

/// Initialize the Wyrd observer bridge.
///
/// This is intentionally a no-op because observer registration is handled by
/// the current/global observer setup.
pub fn init() {}
