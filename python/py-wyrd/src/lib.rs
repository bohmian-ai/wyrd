//! `PyO3` bootstrap module for the Python Wyrd package.

use pyo3::prelude::*;
use pyo3::types::PyModule;

/// Native extension entry point mounted as `wyrd._native`.
#[pymodule]
fn _native(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    wyrd_cards::register(py, m)?;
    Ok(())
}
