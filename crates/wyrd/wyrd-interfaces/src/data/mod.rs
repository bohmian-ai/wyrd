//! Data interface modules and Python registration.

/// Data interface dtype helpers.
pub mod dtype;
/// Data interface Python/Rust adapter types.
pub mod interfaces;
/// Data interface local IO helpers.
pub mod io;
/// Data interface local artifact layout helpers.
pub mod layout;
/// Data schema Python/Rust wrappers.
pub mod schema;
/// Data split Python/Rust wrappers.
pub mod split;
/// Data stats Python/Rust wrappers.
pub mod stats;

#[cfg(feature = "python")]
use {pyo3::prelude::*, pyo3::types::PyModule};

/// Register data interface Python objects under a parent module.
#[cfg(feature = "python")]
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    interfaces::register(parent)?;
    schema::register(parent)?;
    split::register(parent)?;
    stats::register(parent)?;
    Ok(())
}
