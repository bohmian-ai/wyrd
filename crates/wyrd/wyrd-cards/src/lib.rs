//! Card implementation crate.

#![deny(missing_docs)]

/// ArtifactCard implementation module.
pub mod artifact;
/// DataCard implementation module.
pub mod card;

#[cfg(feature = "python")]
use {pyo3::prelude::*, pyo3::types::PyModule};

/// Register card Python objects under a parent module.
#[cfg(feature = "python")]
pub fn register(py: Python<'_>, parent: &Bound<'_, PyModule>) -> PyResult<()> {
    wyrd_interfaces::register(py, parent)?;
    parent.add_class::<artifact::ArtifactCard>()?;
    parent.add_class::<card::DataCard>()?;
    parent.add_class::<card::DataCardMetadata>()?;
    Ok(())
}
