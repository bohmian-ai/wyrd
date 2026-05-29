//! `PyO3` bootstrap module for the Python Wyrd package.

use pyo3::prelude::*;
use pyo3::types::PyModule;

/// Native extension entry point mounted as `wyrd._native`.
#[pymodule]
fn _native(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    wyrd_cards::register(py, m)?;
    let prompt = PyModule::new(py, "prompt")?;
    skald_prompt::register_prompt(&prompt)?;
    m.add_submodule(&prompt)?;
    register_submodule(py, "wyrd._native.prompt", &prompt)?;
    Ok(())
}

fn register_submodule(py: Python<'_>, name: &str, module: &Bound<'_, PyModule>) -> PyResult<()> {
    let sys = py.import("sys")?;
    let modules = sys.getattr("modules")?;
    modules.set_item(name, module)?;
    Ok(())
}
