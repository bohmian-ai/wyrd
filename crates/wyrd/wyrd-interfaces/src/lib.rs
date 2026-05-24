//! Data interface implementation crate.

#![deny(missing_docs)]

/// Data interface dtype helpers.
pub mod dtype;
/// Data interface error boundary.
pub mod error;
/// Data interface Python/Rust adapter types.
pub mod interfaces;
/// Data interface local IO helpers.
pub mod io {}
/// Data schema Python/Rust wrappers.
pub mod schema;
/// Data split Python/Rust wrappers.
pub mod split;
/// Data stats Python/Rust wrappers.
pub mod stats;

#[cfg(feature = "python")]
use pyo3::prelude::*;
#[cfg(feature = "python")]
use pyo3::types::PyModule;

/// Register interface Python objects under a parent module.
#[cfg(feature = "python")]
pub fn register(_py: Python<'_>, parent: &Bound<'_, PyModule>) -> PyResult<()> {
    error::register_exceptions(parent)?;
    interfaces::register(parent)?;
    schema::register(parent)?;
    split::register(parent)?;
    stats::register(parent)?;
    Ok(())
}
