//! `PyO3` bootstrap module for the Python Wyrd package.
//!
//! Every item is gated on the crate's `python` feature: selecting the crate
//! without it compiles an empty library and never activates `PyO3` or an
//! owner crate's Python boundary.

#[cfg(feature = "python")]
mod bifrost;
#[cfg(feature = "python")]
mod state;

#[cfg(feature = "python")]
use pyo3::prelude::*;
#[cfg(feature = "python")]
use pyo3::types::PyModule;

/// Native extension entry point mounted as `wyrd._wyrd`.
///
/// The aggregator creates each public submodule and delegates registration to
/// its owning Rust crate; it contains no client behavior or duplicate Card
/// logic. The state submodule is the native owner for offline bundle loading.
///
/// # Errors
///
/// Returns a Python error when a native submodule cannot be allocated,
/// registered, or inserted into `sys.modules`, including failures reported by
/// an owning crate's registration function.
#[cfg(feature = "python")]
#[pymodule]
fn _wyrd(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    wyrd_utils::py::register_wyrd_error_exception(m)?;
    wyrd_cli::python::register(m)?;

    let agent = PyModule::new(py, "agent")?;
    skald_agent::python_register(&agent)?;
    skald_workflow::python_register(&agent)?;
    m.add_submodule(&agent)?;
    register_submodule(py, "wyrd._wyrd.agent", &agent)?;

    wyrd_cards::register(py, m)?;
    state::register_cards(&m.getattr("cards")?.cast_into()?)?;
    register_submodule(py, "wyrd._wyrd.cards", &m.getattr("cards")?.cast_into()?)?;

    let state = PyModule::new(py, "state")?;
    state.setattr(
        "__doc__",
        "Offline, fully hydrated Wyrd Card graph. Load a complete local Service bundle without a registry or network; typed holders are shared across aliases, and artifact descriptors expose confined local paths without reading payload bytes.",
    )?;
    state::register_state(&state)?;
    m.add_submodule(&state)?;
    register_submodule(py, "wyrd._wyrd.state", &state)?;

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
    bifrost::register_bifrost(&bifrost)?;
    m.add_submodule(&bifrost)?;
    register_submodule(py, "wyrd._wyrd.bifrost", &bifrost)?;

    let observe = PyModule::new(py, "observe")?;
    bifrost::register_observe(&observe)?;
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

/// Publishes one already-populated native submodule under its dotted import name.
///
/// Inserting into `sys.modules` lets `import wyrd._wyrd.<name>` resolve the
/// submodule the aggregator created instead of searching for a separate
/// extension file. It replaces any existing entry with the same name.
///
/// # Errors
///
/// Returns a Python error when `sys` cannot be imported or `sys.modules`
/// rejects the insertion.
#[cfg(feature = "python")]
fn register_submodule(py: Python<'_>, name: &str, module: &Bound<'_, PyModule>) -> PyResult<()> {
    let sys = py.import("sys")?;
    sys.getattr("modules")?.set_item(name, module)
}
