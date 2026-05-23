//! Native `DataCard` module registration.

pub mod card;
pub mod error;
pub mod interfaces;
pub mod io;
pub mod schema;
pub mod split;
pub mod stats;

use pyo3::prelude::*;
use pyo3::types::PyModule;

/// Register the native `DataCard` module scaffold under `wyrd._native.cards`.
pub fn register(py: Python<'_>, parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let data = PyModule::new(py, "data")?;
    data.add("native_datacard_scaffold", true)?;
    parent.add_submodule(&data)?;
    Ok(())
}
