//! `PyO3` projection of the installed `wyrd` executable.
//!
//! `wyrd.cli.run_wyrd_cli` is the console script: it runs the same argument
//! parser and dispatcher as the native `wyrd` binary. The in-process command
//! functions are a test surface and live in `wyrd.testing.cli`, built only
//! with the `testing` feature.

use pyo3::exceptions::PyKeyboardInterrupt;
use pyo3::prelude::*;
use pyo3::types::PyModule;

/// Run the same argument parser and dispatcher as the `wyrd` binary.
///
/// Python owns `sys.argv`, so the adapter reads it at this boundary and
/// releases the GIL while the shared Rust dispatcher performs any client IO.
/// This is the installed `wyrd` console script.
///
/// # Errors
///
/// Returns a Python error when `sys.argv` is unavailable or contains a
/// non-string value, or a pending Python signal raises another exception.
#[pyfunction]
#[pyo3(signature = ())]
fn run_wyrd_cli(py: Python<'_>) -> PyResult<u8> {
    let args = py
        .import("sys")?
        .getattr("argv")?
        .extract::<Vec<String>>()?;
    let code = crate::trace::block_on(py, wyrd_cli::run_cli_code(args));
    // Tokio already settled the interrupted command; preserve its exit code.
    if code == 130
        && let Err(error) = py.check_signals()
        && !error.is_instance_of::<PyKeyboardInterrupt>(py)
    {
        return Err(error);
    }
    Ok(code)
}

/// Register the installed console-script entrypoint on the extension root.
///
/// # Errors
///
/// Returns a Python error when the function cannot be installed on `module`.
pub fn register_entrypoint(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(run_wyrd_cli, module)?)
}

/// Register the console-script entrypoint on `wyrd._wyrd.cli`.
///
/// # Errors
///
/// Returns a Python error when the function cannot be installed on `module`.
pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    register_entrypoint(module)
}
