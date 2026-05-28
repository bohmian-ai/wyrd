//! Model local IO dispatch shells.

#[cfg(feature = "python")]
use {
    crate::error::{CardPyResult, WyrdPyError},
    pyo3::prelude::*,
    pyo3::types::{PyAny, PyDict},
    std::path::{Path, PathBuf},
};

/// Dispatch model save through the Python interface object.
///
/// # Errors
/// Returns any Python-boundary error raised by the concrete interface `save`.
#[cfg(feature = "python")]
pub fn save_model(
    interface: &Bound<'_, PyAny>,
    path: &Path,
    save_kwargs: Option<&Bound<'_, PyDict>>,
) -> CardPyResult<()> {
    interface.call_method("save", (path.to_path_buf(), save_kwargs), None)?;
    Ok(())
}

/// Dispatch model load through the Python interface object.
///
/// # Errors
/// Returns a validation error when the local path is absent, or any
/// Python-boundary error raised by the concrete interface `load`.
#[cfg(feature = "python")]
pub fn load_model(
    interface: &Bound<'_, PyAny>,
    path: Option<PathBuf>,
    load_kwargs: Option<&Bound<'_, PyDict>>,
) -> CardPyResult<()> {
    let path =
        path.ok_or_else(|| WyrdPyError::model_validation("local model artifact path is required"))?;
    interface.call_method("load", (path, load_kwargs), None)?;
    Ok(())
}
