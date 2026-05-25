//! Model autodetection placeholders.

#[cfg(feature = "python")]
use pyo3::prelude::*;

use crate::error::{CardPyResult, WyrdPyError};

/// Return the current Python runtime version string.
///
/// # Errors
/// Returns a Python-boundary error when `sys.version_info` cannot be read.
#[cfg(feature = "python")]
pub(crate) fn python_version_string(py: Python<'_>) -> CardPyResult<String> {
    let sys = py.import("sys")?;
    let version_info = sys.getattr("version_info")?;
    let major: u8 = version_info.getattr("major")?.extract()?;
    let minor: u8 = version_info.getattr("minor")?.extract()?;
    let micro: u8 = version_info.getattr("micro")?.extract()?;
    Ok(format!("{major}.{minor}.{micro}"))
}

/// Report that model autodetection is intentionally unavailable in this stage.
///
/// # Errors
/// Always returns a validation error because autodetection is implemented by a
/// later `ModelCard` stage.
pub fn autodetect_not_available() -> CardPyResult<()> {
    Err(WyrdPyError::validation(
        "model interface autodetection is not available in this stage",
    ))
}
