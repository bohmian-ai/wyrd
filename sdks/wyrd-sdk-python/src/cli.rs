//! `PyO3` projection of the shared `wyrd` CLI.
//!
//! The installed `wyrd` executable and every `wyrd.cli.<command>(...)`
//! function run the Rust command implementation in [`wyrd_cli`]. Each command
//! releases the GIL while the shared command runs and returns the plain Python
//! value the executable prints with `--format json`; a failure raises
//! `WyrdError`. The public `wyrd.cli` module gives each command its keyword
//! signature and passes grouped selector fields here. No credential is an
//! argument: networked commands read the ambient chain, and `server` re-points
//! only the endpoint.

use std::path::PathBuf;

use pyo3::prelude::*;
use pyo3::types::PyModule;
use wyrd_cli::commands::{self, SelectorArgs};
use wyrd_spec::gateway::ProviderCredentialWrite;
use wyrd_utils::py::{WyrdPyResult, json_to_pyobject};

use crate::gateway::{PyGateway, decode};

/// Run the same argument parser and dispatcher as the `wyrd` binary.
///
/// Python owns `sys.argv`, so the adapter reads it at this boundary and
/// releases the GIL while the shared Rust dispatcher performs any client IO.
/// This is the installed `wyrd` console script.
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
    Ok(py.detach(|| wyrd_runtime::runtime().block_on(wyrd_cli::run_cli_code(args))))
}

/// Validate a local Card tree without contacting a server (`wyrd plan`).
///
/// # Errors
///
/// Raises `WyrdError` with `WYRD_LOADER_400_INVALID_ENVELOPE`, diagnostics in
/// `details`, when the tree cannot be loaded or resolved.
#[pyfunction]
#[pyo3(signature = (path))]
fn plan(py: Python<'_>, path: PathBuf) -> WyrdPyResult<Py<PyAny>> {
    let report = py.detach(|| commands::plan(&path))?;
    Ok(json_to_pyobject(py, &serde_json::to_value(report)?)?)
}

/// Register a local Card tree and return its receipt (`wyrd apply`).
///
/// # Errors
///
/// Raises `WyrdError` for invalid local input, a missing credential or bad
/// endpoint, or the server's refusal.
#[pyfunction]
#[pyo3(signature = (path, *, server=None))]
fn apply(py: Python<'_>, path: PathBuf, server: Option<String>) -> WyrdPyResult<Py<PyAny>> {
    PyGateway::run(py, commands::apply(&path, server.as_deref()))
}

/// Card selector fields in `(kind, space, name, version, uid)` order.
///
/// The public `wyrd.cli` wrappers pass their selector keywords as this tuple;
/// the shared CLI selector parser validates them.
type SelectorFields = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// Build the shared CLI selector from its Python fields.
fn selector((kind, space, name, version, uid): SelectorFields) -> SelectorArgs {
    SelectorArgs {
        kind,
        space,
        name,
        version,
        uid,
    }
}

/// Hydrate a Card's reachable graph into `output_dir` (`wyrd get`).
///
/// # Errors
///
/// Raises `WyrdError` for an invalid selector, a missing credential or bad
/// endpoint, or a failed read or hydration.
#[pyfunction]
#[pyo3(signature = (output_dir, fields, metadata_only, server))]
fn get(
    py: Python<'_>,
    output_dir: PathBuf,
    fields: SelectorFields,
    metadata_only: bool,
    server: Option<String>,
) -> WyrdPyResult<Py<PyAny>> {
    let selector = selector(fields);
    PyGateway::run(
        py,
        commands::get(&selector, &output_dir, metadata_only, server.as_deref()),
    )
}

/// Load one Card and materialize its artifacts (`wyrd load`).
///
/// # Errors
///
/// Raises `WyrdError` for an invalid selector, a missing credential or bad
/// endpoint, or a failed load or artifact transfer.
#[pyfunction]
#[pyo3(signature = (fields, path, server))]
fn load(
    py: Python<'_>,
    fields: SelectorFields,
    path: Option<PathBuf>,
    server: Option<String>,
) -> WyrdPyResult<Py<PyAny>> {
    let selector = selector(fields);
    PyGateway::run(
        py,
        commands::load(&selector, path.as_deref(), server.as_deref()),
    )
}

/// Issue an API key bound to one exact Card (`wyrd auth issue-key`).
///
/// `card` is `(kind, name, version, space)`. The returned mapping holds the
/// plaintext `key` exactly once.
///
/// # Errors
///
/// Raises `WyrdError` for invalid coordinates, a missing credential or bad
/// endpoint, or the server's refusal.
#[pyfunction]
#[pyo3(signature = (card, label, expires_in_seconds, server))]
fn issue_key(
    py: Python<'_>,
    card: (String, String, String, String),
    label: Option<String>,
    expires_in_seconds: Option<u32>,
    server: Option<String>,
) -> WyrdPyResult<Py<PyAny>> {
    let (kind, name, version, space) = card;
    PyGateway::run(
        py,
        commands::issue_key(
            &kind,
            &name,
            &version,
            &space,
            label.as_deref(),
            expires_in_seconds,
            server.as_deref(),
        ),
    )
}

/// Create or rotate one provider credential (`wyrd gateway credential put`).
///
/// `write` may carry a provider key; a decode failure never quotes it, and
/// the returned view is redacted.
///
/// # Errors
///
/// Raises `WyrdError` with `WYRD_SPEC_400_VALIDATION` when `write` does not
/// match the credential contract, or the client or server error.
#[pyfunction]
#[pyo3(signature = (write, *, server=None))]
fn put_provider_credential(
    py: Python<'_>,
    write: &Bound<'_, PyAny>,
    server: Option<String>,
) -> WyrdPyResult<Py<PyAny>> {
    let write: ProviderCredentialWrite = decode("write", write)?;
    PyGateway::run(
        py,
        commands::put_provider_credential(&write, server.as_deref()),
    )
}

/// Terminally revoke one provider credential (`wyrd gateway credential revoke`).
///
/// # Errors
///
/// Raises `WyrdError` for an invalid name, or the client or server error.
#[pyfunction]
#[pyo3(signature = (name, *, server=None))]
fn revoke_provider_credential(
    py: Python<'_>,
    name: &str,
    server: Option<String>,
) -> WyrdPyResult<Py<PyAny>> {
    let name = PyGateway::credential_name(name)?;
    PyGateway::run(
        py,
        commands::revoke_provider_credential(&name, server.as_deref()),
    )
}

/// Delete one unreferenced provider credential; an absent name succeeds
/// (`wyrd gateway credential delete`).
///
/// # Errors
///
/// Raises `WyrdError` for an invalid name, or the client or server error.
#[pyfunction]
#[pyo3(signature = (name, *, server=None))]
fn delete_provider_credential(
    py: Python<'_>,
    name: &str,
    server: Option<String>,
) -> WyrdPyResult<Py<PyAny>> {
    let name = PyGateway::credential_name(name)?;
    PyGateway::run(
        py,
        commands::delete_provider_credential(&name, server.as_deref()),
    )
}

/// Register the installed console-script entrypoint on the extension root.
///
/// # Errors
///
/// Returns a Python error when the function cannot be installed on `module`.
pub fn register_entrypoint(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(run_wyrd_cli, module)?)
}

/// Register the entrypoint and every in-process command on `wyrd._wyrd.cli`.
///
/// # Errors
///
/// Returns a Python error when a function cannot be installed on `module`.
pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    register_entrypoint(module)?;
    module.add_function(wrap_pyfunction!(plan, module)?)?;
    module.add_function(wrap_pyfunction!(apply, module)?)?;
    module.add_function(wrap_pyfunction!(get, module)?)?;
    module.add_function(wrap_pyfunction!(load, module)?)?;
    module.add_function(wrap_pyfunction!(issue_key, module)?)?;
    module.add_function(wrap_pyfunction!(put_provider_credential, module)?)?;
    module.add_function(wrap_pyfunction!(revoke_provider_credential, module)?)?;
    module.add_function(wrap_pyfunction!(delete_provider_credential, module)?)
}
