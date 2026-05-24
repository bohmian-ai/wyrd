//! Data interface implementation crate.

#![deny(missing_docs)]

/// Data interface module family.
pub mod data;
/// Data interface error boundary.
pub mod error;

#[cfg(feature = "python")]
use pyo3::prelude::*;
#[cfg(feature = "python")]
use pyo3::types::PyModule;

/// Register interface Python objects under a parent module.
#[cfg(feature = "python")]
pub fn register(_py: Python<'_>, parent: &Bound<'_, PyModule>) -> PyResult<()> {
    error::register_exceptions(parent)?;
    data::register(parent)?;
    Ok(())
}
