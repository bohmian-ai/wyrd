//! `PyO3` bootstrap module for the Python Wyrd package.

use pyo3::prelude::*;
use pyo3::types::PyModule;

/// Native extension entry point mounted as `wyrd._wyrd`.
#[pymodule]
fn _wyrd(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    wyrd_utils::py::register_wyrd_error_exception(m)?;

    let agent = PyModule::new(py, "agent")?;
    skald_agent::python_register(&agent)?;
    m.add_submodule(&agent)?;
    register_submodule(py, "wyrd._wyrd.agent", &agent)?;

    wyrd_cards::register(py, m)?;
    register_submodule(py, "wyrd._wyrd.cards", &m.getattr("cards")?.cast_into()?)?;
    register_submodule(
        py,
        "wyrd._wyrd.cards.data",
        &m.getattr("cards")?.getattr("data")?.cast_into()?,
    )?;
    register_submodule(
        py,
        "wyrd._wyrd.cards.model",
        &m.getattr("cards")?.getattr("model")?.cast_into()?,
    )?;
    register_submodule(
        py,
        "wyrd._wyrd.cards.prompt",
        &m.getattr("cards")?.getattr("prompt")?.cast_into()?,
    )?;

    let tool = PyModule::new(py, "tool")?;
    skald_tool::python_register(&tool)?;
    m.add_submodule(&tool)?;
    register_submodule(py, "wyrd._wyrd.tool", &tool)?;

    let prompt = PyModule::new(py, "prompt")?;
    skald_prompt::register_prompt(&prompt)?;
    m.add_submodule(&prompt)?;
    register_submodule(py, "wyrd._wyrd.prompt", &prompt)?;

    let providers = PyModule::new(py, "providers")?;
    skald_runtime::python_register(&providers)?;
    m.add_submodule(&providers)?;
    register_submodule(py, "wyrd._wyrd.providers", &providers)?;

    wyrd_observe_impl::python_register(m)?;
    Ok(())
}

fn register_submodule(py: Python<'_>, name: &str, module: &Bound<'_, PyModule>) -> PyResult<()> {
    let sys = py.import("sys")?;
    sys.getattr("modules")?.set_item(name, module)
}
