//! Python entry point for the shared Rust Wyrd CLI dispatcher.

use pyo3::prelude::*;
use pyo3::types::PyModule;

/// Run the same argument parser and dispatcher as the `wyrd` binary.
///
/// The embedded extension cannot use the process-level [`std::env::args_os`]
/// because Python owns `sys.argv` when this function is called from a Python
/// process. Reading `sys.argv` here preserves both direct Python invocation and
/// the console-script entry point while keeping parsing and dispatch in the
/// shared Rust CLI.
///
/// # Errors
/// Returns a Python extraction error when `sys.argv` is unavailable or contains
/// a non-string value.
#[pyfunction]
#[pyo3(signature = ())]
fn run_wyrd_cli(py: Python<'_>) -> PyResult<u8> {
    let args = py
        .import("sys")?
        .getattr("argv")?
        .extract::<Vec<String>>()?;
    Ok(py.detach(|| wyrd_runtime::runtime().block_on(wyrd_cli::run_cli_code(args))))
}

/// Register the CLI function on the native extension root.
pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(run_wyrd_cli, module)?)
}
