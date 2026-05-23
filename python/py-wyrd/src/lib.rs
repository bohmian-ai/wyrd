//! `PyO3` bootstrap module for the Python Wyrd package.

pub mod cards;
pub mod dtype;

use pyo3::prelude::*;
use pyo3::types::PyModule;

/// Return true when the native extension imported successfully.
#[pyfunction]
fn native_module_ready() -> bool {
    true
}

/// Native extension entry point mounted as `wyrd._native`.
#[pymodule]
fn _native(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(native_module_ready, m)?)?;
    cards::register(py, m)?;
    Ok(())
}
