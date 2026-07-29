//! Optional `PyO3` adapter for the shared CLI parser and dispatcher.

use pyo3::prelude::*;
use pyo3::types::PyModule;

/// Run the same argument parser and dispatcher as the `wyrd` binary.
///
/// Python owns `sys.argv`, so the adapter reads it at this boundary and releases
/// the GIL while the shared Rust dispatcher performs any asynchronous client IO.
///
/// # Errors
///
/// Returns a Python error when `sys.argv` is unavailable or contains a
/// non-string value.
#[pyfunction]
#[pyo3(signature = ())]
fn run_wyrd_cli(py: Python<'_>) -> PyResult<u8> {
    let args = py
        .import("sys")?
        .getattr("argv")?
        .extract::<Vec<String>>()?;
    Ok(py.detach(|| wyrd_runtime::runtime().block_on(crate::run_cli_code(args))))
}

/// Register the CLI entry point on the native extension root.
///
/// # Errors
///
/// Returns a Python error when the function cannot be installed on `module`.
pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(run_wyrd_cli, module)?)
}
