//! Card implementation crate.

#![deny(missing_docs)]

/// ArtifactCard implementation module.
pub mod artifact;
/// DataCard implementation module.
pub mod card;

#[cfg(feature = "python")]
use {pyo3::prelude::*, pyo3::types::PyModule};

/// Register card Python objects under `wyrd._native.cards`.
#[cfg(feature = "python")]
pub fn register(py: Python<'_>, parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let cards = PyModule::new(py, "cards")?;
    let data = PyModule::new(py, "data")?;

    wyrd_interfaces::register(py, &data)?;
    data.add_class::<artifact::ArtifactCard>()?;
    data.add_class::<card::DataCard>()?;
    data.add_class::<card::DataCardMetadata>()?;

    cards.add_submodule(&data)?;
    parent.add_submodule(&cards)?;
    register_submodule(py, "wyrd._native.cards", &cards)?;
    register_submodule(py, "wyrd._native.cards.data", &data)?;
    Ok(())
}

#[cfg(feature = "python")]
fn register_submodule(py: Python<'_>, name: &str, module: &Bound<'_, PyModule>) -> PyResult<()> {
    let sys = py.import("sys")?;
    let modules = sys.getattr("modules")?;
    modules.set_item(name, module)?;
    Ok(())
}
