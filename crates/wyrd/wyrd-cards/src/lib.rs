//! Card implementation crate.

#![deny(missing_docs)]

/// ArtifactCard implementation module.
pub mod artifact;
/// Compatibility re-exports for card holder types.
pub mod card;
/// DataCard implementation module.
pub mod data;
/// ModelCard implementation module.
pub mod model;

#[cfg(feature = "python")]
use {pyo3::prelude::*, pyo3::types::PyModule};

/// Register card Python objects under `wyrd._native.cards`.
#[cfg(feature = "python")]
pub fn register(py: Python<'_>, parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let cards = PyModule::new(py, "cards")?;
    let data = PyModule::new(py, "data")?;
    let model = PyModule::new(py, "model")?;

    wyrd_interfaces::error::register_exceptions(&data)?;
    wyrd_interfaces::data::register(&data)?;
    data.add_class::<artifact::ArtifactCard>()?;
    data.add_class::<data::DataCard>()?;
    data.add_class::<data::DataCardMetadata>()?;

    wyrd_interfaces::error::register_exceptions(&model)?;
    wyrd_interfaces::model::register(&model)?;
    model.add_class::<model::ModelCard>()?;
    model.add_class::<model::ModelCardMetadata>()?;

    cards.add_submodule(&data)?;
    cards.add_submodule(&model)?;
    parent.add_submodule(&cards)?;
    register_submodule(py, "wyrd._native.cards", &cards)?;
    register_submodule(py, "wyrd._native.cards.data", &data)?;
    register_submodule(py, "wyrd._native.cards.model", &model)?;
    Ok(())
}

#[cfg(feature = "python")]
fn register_submodule(py: Python<'_>, name: &str, module: &Bound<'_, PyModule>) -> PyResult<()> {
    let sys = py.import("sys")?;
    let modules = sys.getattr("modules")?;
    modules.set_item(name, module)?;
    Ok(())
}
