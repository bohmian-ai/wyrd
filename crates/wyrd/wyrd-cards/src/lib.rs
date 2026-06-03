//! Card implementation crate.

#![deny(missing_docs)]

/// AgentCard implementation module.
pub mod agent;
/// ArtifactCard implementation module.
pub mod artifact;
/// Compatibility re-exports for card holder types.
pub mod card;
/// DataCard implementation module.
pub mod data;
/// Typed envelope holder modules.
pub mod envelope;
/// Card boundary errors.
pub mod error;
/// ModelCard implementation module.
pub mod model;
/// PromptCard implementation module.
pub mod prompt;
#[cfg(feature = "python")]
/// Python wrappers owned by this crate.
pub mod python;

#[cfg(feature = "python")]
use {pyo3::prelude::*, pyo3::types::PyModule};

/// Register card Python objects under `wyrd._native.cards`.
#[cfg(feature = "python")]
pub fn register(py: Python<'_>, parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let cards = PyModule::new(py, "cards")?;
    let data = PyModule::new(py, "data")?;
    let model = PyModule::new(py, "model")?;
    let prompt = PyModule::new(py, "prompt")?;
    let agent = PyModule::new(py, "agent")?;

    wyrd_interfaces::error::register_exceptions(&data)?;
    wyrd_interfaces::data::register(&data)?;
    data.add_class::<artifact::ArtifactCard>()?;
    data.add_class::<data::DataCard>()?;
    data.add_class::<data::DataCardMetadata>()?;

    wyrd_interfaces::error::register_exceptions(&model)?;
    wyrd_interfaces::model::register(&model)?;
    model.add_class::<model::ModelCard>()?;
    model.add_class::<model::ModelCardMetadata>()?;

    wyrd_interfaces::error::register_exceptions(&prompt)?;
    skald_prompt::register_prompt(&prompt)?;
    prompt.add_class::<prompt::PromptRef>()?;
    prompt.add_class::<prompt::PromptCard>()?;
    prompt.add_class::<prompt::PromptCardMetadata>()?;

    wyrd_interfaces::error::register_exceptions(&agent)?;
    python::register(&agent)?;

    cards.add_submodule(&data)?;
    cards.add_submodule(&model)?;
    cards.add_submodule(&prompt)?;
    cards.add_submodule(&agent)?;
    parent.add_submodule(&cards)?;
    register_submodule(py, "wyrd._native.cards", &cards)?;
    register_submodule(py, "wyrd._native.cards.data", &data)?;
    register_submodule(py, "wyrd._native.cards.model", &model)?;
    register_submodule(py, "wyrd._native.cards.prompt", &prompt)?;
    register_submodule(py, "wyrd._native.cards.agent", &agent)?;
    Ok(())
}

#[cfg(feature = "python")]
fn register_submodule(py: Python<'_>, name: &str, module: &Bound<'_, PyModule>) -> PyResult<()> {
    let sys = py.import("sys")?;
    let modules = sys.getattr("modules")?;
    modules.set_item(name, module)?;
    Ok(())
}
