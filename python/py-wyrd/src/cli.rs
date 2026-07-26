//! Python entry point for the shared Rust Wyrd CLI dispatcher.

use pyo3::prelude::*;
use pyo3::types::PyModule;

/// Run the same argument parser and dispatcher as the `wyrd` binary.
#[pyfunction]
#[pyo3(signature = ())]
fn run_wyrd_cli(py: Python<'_>) -> PyResult<u8> {
    let exit_code =
        py.detach(|| wyrd_runtime::runtime().block_on(wyrd_cli::run_cli_code(std::env::args_os())));
    Ok(exit_code)
}

/// Register the CLI function on the native extension root.
pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(run_wyrd_cli, module)?)
}
