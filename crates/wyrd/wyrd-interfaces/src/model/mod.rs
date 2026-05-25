//! Model interface modules and Python registration.

/// Model interface detection hooks.
pub mod detect;
/// Model interface Python/Rust adapter types.
pub mod interfaces;
/// Model interface local IO dispatch helpers.
pub mod io;
/// Model sample-input Python/Rust wrappers.
pub mod sample;
/// Model signature Python/Rust wrappers.
pub mod signature;

#[cfg(feature = "python")]
use {pyo3::prelude::*, pyo3::types::PyModule};

/// Register model interface Python objects under a parent module.
#[cfg(feature = "python")]
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    interfaces::register(parent)?;
    signature::register(parent)?;
    sample::register(parent)?;
    Ok(())
}
