//! TODO(commit 07): PyO3 surface (behind `python` feature).

use pyo3::prelude::*;

/// Register the wyrd.config submodule.
pub fn register(_py: Python<'_>, _parent: &Bound<'_, PyModule>) -> PyResult<()> {
    Ok(())
}
