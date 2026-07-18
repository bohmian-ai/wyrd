//! Shared infrastructure for SLO-gated Criterion benches.

pub mod slo;

pub use slo::{SloError, SloGate, SloMeasurement, SloResult, SloThreshold};
