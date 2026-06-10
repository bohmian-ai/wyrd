//! Card implementation crate.

#![deny(missing_docs)]

/// Compatibility re-exports for card holder types.
pub mod card;
/// Python-boundary card reference types: CardRef and CardKind.
#[cfg(feature = "python")]
pub mod card_ref;
/// DataCard implementation module.
pub mod data;
/// Typed envelope holder modules.
pub mod envelope;
mod identity;
/// ModelCard implementation module.
pub mod model;
/// PromptCard implementation module.
pub mod prompt;
#[cfg(feature = "python")]
use {pyo3::prelude::*, pyo3::types::PyModule};

/// Register card Python objects under `wyrd.cards`.
#[cfg(feature = "python")]
pub fn register(py: Python<'_>, parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let cards = PyModule::new(py, "cards")?;
    let data = PyModule::new(py, "data")?;
    let model = PyModule::new(py, "model")?;
    let prompt = PyModule::new(py, "prompt")?;

    cards.add_class::<card_ref::CardRefPy>()?;
    cards.add_class::<card_ref::Kind>()?;

    wyrd_interfaces::error::register_exceptions(&data)?;
    wyrd_interfaces::data::register(&data)?;
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

    cards.add_submodule(&data)?;
    cards.add_submodule(&model)?;
    cards.add_submodule(&prompt)?;
    parent.add_submodule(&cards)?;
    register_submodule(py, "wyrd.cards", &cards)?;
    register_submodule(py, "wyrd.cards.data", &data)?;
    register_submodule(py, "wyrd.cards.model", &model)?;
    register_submodule(py, "wyrd.cards.prompt", &prompt)?;
    Ok(())
}

#[cfg(feature = "python")]
fn register_submodule(py: Python<'_>, name: &str, module: &Bound<'_, PyModule>) -> PyResult<()> {
    let sys = py.import("sys")?;
    let modules = sys.getattr("modules")?;
    modules.set_item(name, module)?;
    Ok(())
}
