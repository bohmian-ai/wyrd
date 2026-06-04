//! Python initialization hook for observer auto-attach.

#![cfg(feature = "python")]

use pyo3::prelude::*;
use pyo3::types::PyModule;

/// Initialize the Rust observer bridge.
#[pyfunction]
pub fn _init() {
    crate::init();
}

/// Register the top-level `_init` function on `wyrd._wyrd`.
///
/// # Errors
/// Returns PyO3 registration errors.
pub fn python_register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(_init, module)?)?;
    Ok(())
}
