//! Card implementation crate.

#![deny(missing_docs)]

/// ArtifactCard implementation module.
pub mod artifact {}
/// DataCard implementation module.
pub mod card {}

#[cfg(feature = "python")]
use pyo3::prelude::*;
#[cfg(feature = "python")]
use pyo3::types::PyModule;

/// Register card Python objects under a parent module.
#[cfg(feature = "python")]
pub fn register(_py: Python<'_>, _parent: &Bound<'_, PyModule>) -> PyResult<()> {
    Ok(())
}
