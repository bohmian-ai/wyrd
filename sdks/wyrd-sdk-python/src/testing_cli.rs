//! Test-only `PyO3` projection of the in-process `wyrd` CLI commands.
//!
//! Present only when the extension is built with its `testing` feature, so the
//! production wheel exposes the `wyrd` executable and no in-process command.
//! Every `wyrd.testing.cli.<command>(...)` function runs the same
//! [`wyrd_cli::commands`] implementation as the executable, releases the GIL
//! while it runs, returns the typed result the command prints with
//! `--format json`, and raises `WyrdError` instead of returning an exit code.
//! A networked command takes an optional `client` and runs as its principal;
//! omitted, the server and credential resolve from the ambient chain exactly
//! as the executable resolves them.

use std::future::Future;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use pyo3::prelude::*;
use pyo3::types::PyModule;
use wyrd_cards::card_ref::CardRefPy;
use wyrd_cli::commands::{self, LoadOutput, PlanCard, PlanReport, SelectorArgs};
use wyrd_cli::{Diagnostic, Severity};
use wyrd_client::WyrdClient;
use wyrd_spec::auth::IssueKeyResponse;
use wyrd_spec::error::WyrdError;
use wyrd_spec::gateway::ProviderCredentialWrite;
use wyrd_utils::py::WyrdPyResult;

use crate::client::PyWyrdClient;
use crate::gateway::{PyGateway, decode};
use crate::state::{PyHydrationSummary, PyRegistrationReceipt};

/// Run one shared command future on the Wyrd runtime with the GIL released.
///
/// # Errors
///
/// Returns the command's own `WyrdError`.
fn block_on<T: Send>(
    py: Python<'_>,
    command: impl Future<Output = Result<T, WyrdError>> + Send,
) -> Result<T, WyrdError> {
    py.detach(|| wyrd_runtime::runtime().block_on(command))
}

/// Clone the Rust client out of an optional Python `WyrdClient`.
fn rust_client(client: Option<&Bound<'_, PyWyrdClient>>) -> Option<WyrdClient> {
    client.map(|client| client.get().inner().clone())
}

/// One Card a [`PyPlanReport`] would register, identified as authored.
#[pyclass(module = "wyrd.testing.cli", name = "PlanCard", frozen)]
pub struct PyPlanCard {
    /// Authored identity from the shared planner.
    inner: PlanCard,
}

#[pymethods]
impl PyPlanCard {
    /// Card kind wire name, such as `Service`.
    #[getter]
    fn kind(&self) -> &str {
        &self.inner.kind
    }

    /// Authored space, or `None` for the default space.
    #[getter]
    fn space(&self) -> Option<&str> {
        self.inner.space.as_deref()
    }

    /// Card name.
    #[getter]
    fn name(&self) -> &str {
        &self.inner.name
    }

    /// Authored version, or `None` when the server assigns one.
    #[getter]
    fn version(&self) -> Option<&str> {
        self.inner.version.as_deref()
    }
}

/// One non-fatal loader diagnostic in a [`PyPlanReport`].
#[pyclass(module = "wyrd.testing.cli", name = "PlanDiagnostic", frozen)]
pub struct PyPlanDiagnostic {
    /// Diagnostic from the shared loader.
    inner: Diagnostic,
}

#[pymethods]
impl PyPlanDiagnostic {
    /// Stable catalog code.
    #[getter]
    const fn code(&self) -> &'static str {
        self.inner.code
    }

    /// `"error"` or `"warning"`.
    #[getter]
    const fn severity(&self) -> &'static str {
        match self.inner.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }

    /// Authored file the diagnostic concerns.
    #[getter]
    fn path(&self) -> PathBuf {
        self.inner.path.clone()
    }

    /// Human-readable problem detail.
    #[getter]
    fn message(&self) -> &str {
        &self.inner.message
    }
}

/// Deterministic local registration plan returned by `plan`.
#[pyclass(module = "wyrd.testing.cli", name = "PlanReport")]
pub struct PyPlanReport {
    /// Whether the tree loaded and resolved; always `True` when returned.
    #[pyo3(get)]
    ok: bool,
    /// Cards the tree would register, in registration order.
    #[pyo3(get)]
    cards: Vec<Py<PyPlanCard>>,
    /// Non-fatal loader diagnostics.
    #[pyo3(get)]
    diagnostics: Vec<Py<PyPlanDiagnostic>>,
}

impl PyPlanReport {
    /// Move the shared report into Python-owned items.
    ///
    /// # Errors
    ///
    /// Returns a Python error when an item cannot be allocated.
    fn new(py: Python<'_>, report: PlanReport) -> PyResult<Self> {
        Ok(Self {
            ok: report.ok,
            cards: report
                .cards
                .into_iter()
                .map(|inner| Py::new(py, PyPlanCard { inner }))
                .collect::<PyResult<_>>()?,
            diagnostics: report
                .diagnostics
                .into_iter()
                .map(|inner| Py::new(py, PyPlanDiagnostic { inner }))
                .collect::<PyResult<_>>()?,
        })
    }
}

/// Result of `load`: the exact Card the selector resolved to.
#[pyclass(module = "wyrd.testing.cli", name = "LoadOutput", frozen)]
pub struct PyLoadOutput {
    /// Output from the shared load command.
    inner: LoadOutput,
}

#[pymethods]
impl PyLoadOutput {
    /// Exact reference of the loaded Card.
    #[getter]
    fn card_ref(&self) -> CardRefPy {
        CardRefPy(self.inner.card_ref.clone())
    }

    /// Whether the artifacts were materialized; always `True` when returned.
    #[getter]
    const fn materialized(&self) -> bool {
        self.inner.materialized
    }
}

/// A Card-scoped API key returned by `issue_key`, exactly once.
#[pyclass(module = "wyrd.testing.cli", name = "IssueKeyResponse", frozen)]
pub struct PyIssueKeyResponse {
    /// Response from the shared issue-key command.
    inner: IssueKeyResponse,
}

#[pymethods]
impl PyIssueKeyResponse {
    /// Credential id of the issued key.
    #[getter]
    fn key_id(&self) -> String {
        self.inner.key_id.to_string()
    }

    /// Principal the key authenticates: the Card's Service or Agent
    /// principal, the id its Role assignments are addressed by.
    #[getter]
    fn principal_id(&self) -> String {
        self.inner.principal_id.to_string()
    }

    /// Plaintext API key.
    #[getter]
    fn key(&self) -> &str {
        self.inner.key.expose()
    }

    /// Log-safe key prefix.
    #[getter]
    fn prefix(&self) -> &str {
        &self.inner.prefix
    }

    /// The Card the key is bound to.
    #[getter]
    fn card_ref(&self) -> CardRefPy {
        CardRefPy(self.inner.card_ref.clone())
    }

    /// When the key was issued.
    #[getter]
    const fn created_at(&self) -> DateTime<Utc> {
        self.inner.created_at
    }

    /// When the key expires.
    #[getter]
    const fn expires_at(&self) -> DateTime<Utc> {
        self.inner.expires_at
    }
}

/// Validate a local Card tree without contacting a server (`wyrd plan`).
///
/// # Errors
///
/// Raises `WyrdError` with `WYRD_LOADER_400_INVALID_ENVELOPE`, diagnostics in
/// `details`, when the tree cannot be loaded or resolved.
#[pyfunction]
#[pyo3(signature = (path))]
fn plan(py: Python<'_>, path: PathBuf) -> WyrdPyResult<PyPlanReport> {
    let report = py.detach(|| commands::plan(&path))?;
    Ok(PyPlanReport::new(py, report)?)
}

/// Register a local Card tree and return its receipt (`wyrd apply`).
///
/// # Errors
///
/// Raises `WyrdError` for invalid local input, a missing credential or bad
/// endpoint, or the server's refusal.
#[pyfunction]
#[pyo3(signature = (path, *, client=None))]
fn apply(
    py: Python<'_>,
    path: PathBuf,
    client: Option<&Bound<'_, PyWyrdClient>>,
) -> WyrdPyResult<PyRegistrationReceipt> {
    let client = rust_client(client);
    Ok(block_on(py, commands::apply(&path, client))?.into())
}

/// Card selector fields in `(kind, space, name, version, uid)` order.
///
/// The public `wyrd.testing.cli` wrappers pass their selector keywords as
/// this tuple; the shared CLI selector parser validates them.
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
#[pyo3(signature = (output_dir, fields, metadata_only, client))]
fn get(
    py: Python<'_>,
    output_dir: PathBuf,
    fields: SelectorFields,
    metadata_only: bool,
    client: Option<&Bound<'_, PyWyrdClient>>,
) -> WyrdPyResult<PyHydrationSummary> {
    let selector = selector(fields);
    let client = rust_client(client);
    let inner = block_on(
        py,
        commands::get(&selector, &output_dir, metadata_only, client),
    )?;
    Ok(PyHydrationSummary::from(inner))
}

/// Load one Card and materialize its artifacts (`wyrd load`).
///
/// # Errors
///
/// Raises `WyrdError` for an invalid selector, a missing credential or bad
/// endpoint, or a failed load or artifact transfer.
#[pyfunction]
#[pyo3(signature = (fields, path, client))]
fn load(
    py: Python<'_>,
    fields: SelectorFields,
    path: Option<PathBuf>,
    client: Option<&Bound<'_, PyWyrdClient>>,
) -> WyrdPyResult<PyLoadOutput> {
    let selector = selector(fields);
    let client = rust_client(client);
    let inner = block_on(py, commands::load(&selector, path.as_deref(), client))?;
    Ok(PyLoadOutput { inner })
}

/// Issue an API key bound to one exact Card (`wyrd auth issue-key`).
///
/// `card` is `(kind, name, version, space)`.
///
/// # Errors
///
/// Raises `WyrdError` for invalid coordinates, a missing credential or bad
/// endpoint, or the server's refusal.
#[pyfunction]
#[pyo3(signature = (card, label, expires_in_seconds, client))]
fn issue_key(
    py: Python<'_>,
    card: (String, String, String, String),
    label: Option<String>,
    expires_in_seconds: Option<u32>,
    client: Option<&Bound<'_, PyWyrdClient>>,
) -> WyrdPyResult<PyIssueKeyResponse> {
    let (kind, name, version, space) = card;
    let client = rust_client(client);
    let inner = block_on(
        py,
        commands::issue_key(
            &kind,
            &name,
            &version,
            &space,
            label.as_deref(),
            expires_in_seconds,
            client,
        ),
    )?;
    Ok(PyIssueKeyResponse { inner })
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
#[pyo3(signature = (write, *, client=None))]
fn put_provider_credential(
    py: Python<'_>,
    write: &Bound<'_, PyAny>,
    client: Option<&Bound<'_, PyWyrdClient>>,
) -> WyrdPyResult<Py<PyAny>> {
    let write: ProviderCredentialWrite = decode("write", write)?;
    let client = rust_client(client);
    PyGateway::run(py, commands::put_provider_credential(&write, client))
}

/// Terminally revoke one provider credential (`wyrd gateway credential revoke`).
///
/// # Errors
///
/// Raises `WyrdError` for an invalid name, or the client or server error.
#[pyfunction]
#[pyo3(signature = (name, *, client=None))]
fn revoke_provider_credential(
    py: Python<'_>,
    name: &str,
    client: Option<&Bound<'_, PyWyrdClient>>,
) -> WyrdPyResult<Py<PyAny>> {
    let name = PyGateway::credential_name(name)?;
    let client = rust_client(client);
    PyGateway::run(py, commands::revoke_provider_credential(&name, client))
}

/// Delete one unreferenced provider credential; an absent name succeeds
/// (`wyrd gateway credential delete`).
///
/// # Errors
///
/// Raises `WyrdError` for an invalid name, or the client or server error.
#[pyfunction]
#[pyo3(signature = (name, *, client=None))]
fn delete_provider_credential(
    py: Python<'_>,
    name: &str,
    client: Option<&Bound<'_, PyWyrdClient>>,
) -> WyrdPyResult<()> {
    let name = PyGateway::credential_name(name)?;
    let client = rust_client(client);
    Ok(block_on(
        py,
        commands::delete_provider_credential(&name, client),
    )?)
}

/// Register every in-process command and result type on
/// `wyrd._wyrd.testing.cli`.
///
/// # Errors
///
/// Returns a Python error when an item cannot be installed on `module`.
pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyPlanCard>()?;
    module.add_class::<PyPlanDiagnostic>()?;
    module.add_class::<PyPlanReport>()?;
    module.add_class::<PyLoadOutput>()?;
    module.add_class::<PyIssueKeyResponse>()?;
    module.add_function(wrap_pyfunction!(plan, module)?)?;
    module.add_function(wrap_pyfunction!(apply, module)?)?;
    module.add_function(wrap_pyfunction!(get, module)?)?;
    module.add_function(wrap_pyfunction!(load, module)?)?;
    module.add_function(wrap_pyfunction!(issue_key, module)?)?;
    module.add_function(wrap_pyfunction!(put_provider_credential, module)?)?;
    module.add_function(wrap_pyfunction!(revoke_provider_credential, module)?)?;
    module.add_function(wrap_pyfunction!(delete_provider_credential, module)?)
}
