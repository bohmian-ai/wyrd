//! `PyO3` bootstrap module for the Python Wyrd package.

use pyo3::prelude::*;
use pyo3::types::PyModule;

mod cli;

/// Native extension entry point mounted as `wyrd._wyrd`.
#[pymodule]
fn _wyrd(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    wyrd_utils::py::register_wyrd_error_exception(m)?;
    cli::register(m)?;

    let agent = PyModule::new(py, "agent")?;
    skald_agent::python_register(&agent)?;
    skald_workflow::python_register(&agent)?;
    m.add_submodule(&agent)?;
    register_submodule(py, "wyrd._wyrd.agent", &agent)?;

    wyrd_cards::register(py, m)?;
    wyrd_sdk::python::register_cards(&m.getattr("cards")?.cast_into()?)?;
    register_submodule(py, "wyrd._wyrd.cards", &m.getattr("cards")?.cast_into()?)?;

    let runtime = PyModule::new(py, "runtime")?;
    wyrd_sdk::python::register_runtime(&runtime)?;
    m.add_submodule(&runtime)?;
    register_submodule(py, "wyrd._wyrd.runtime", &runtime)?;

    wyrd_config::register(py, m)?;
    register_submodule(py, "wyrd._wyrd.config", &m.getattr("config")?.cast_into()?)?;
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
    register_submodule(
        py,
        "wyrd._wyrd.cards.agent",
        &m.getattr("cards")?.getattr("agent")?.cast_into()?,
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

    skald_observer::python::python_register(m)?;

    let bifrost = PyModule::new(py, "bifrost")?;
    vala_sdk::python::register_bifrost(&bifrost)?;
    m.add_submodule(&bifrost)?;
    register_submodule(py, "wyrd._wyrd.bifrost", &bifrost)?;

    let observe = PyModule::new(py, "observe")?;
    vala_sdk::python::register_observe(&observe)?;
    m.add_submodule(&observe)?;
    register_submodule(py, "wyrd._wyrd.observe", &observe)?;

    #[cfg(feature = "testing")]
    {
        let testing = PyModule::new(py, "testing")?;
        wyrd_testing::python::register(&testing)?;
        m.add_submodule(&testing)?;
        register_submodule(py, "wyrd._wyrd.testing", &testing)?;
    }

    Ok(())
}

fn register_submodule(py: Python<'_>, name: &str, module: &Bound<'_, PyModule>) -> PyResult<()> {
    let sys = py.import("sys")?;
    sys.getattr("modules")?.set_item(name, module)
}
