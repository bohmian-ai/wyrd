//! Native card module registration.

pub mod data;

use pyo3::prelude::*;
use pyo3::types::PyModule;

/// Register native card submodules under `wyrd._native`.
pub fn register(py: Python<'_>, parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let cards = PyModule::new(py, "cards")?;
    data::register(py, &cards)?;
    parent.add_submodule(&cards)?;
    Ok(())
}
