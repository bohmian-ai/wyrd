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

use pyo3::prelude::*;
use pyo3::types::PyModule;
use wyrd_cards::card_ref::CardRefPy;
use wyrd_cli::commands::{
    self, BumpLevel, CheckCard, CheckReport, McpHostReport, McpInstallReport, SelectorArgs,
};
use wyrd_cli::{Diagnostic, Severity};
use wyrd_client::WyrdClient;
use wyrd_spec::error::WyrdError;
use wyrd_spec::gateway::ProviderCredentialWrite;
use wyrd_utils::py::{WyrdPyResult, json_to_pyobject};

use crate::client::PyWyrdClient;
use crate::gateway_admin::{PyGateway, decode};
use crate::state::{PyCardEnvelope, PyDeclarationReceipt, PyPullSummary};

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

/// One Card a [`PyCheckReport`] would declare, identified as authored.
#[pyclass(module = "wyrd.testing.cli", name = "CheckCard", frozen)]
pub struct PyCheckCard {
    /// Authored identity from the shared local check.
    inner: CheckCard,
}

#[pymethods]
impl PyCheckCard {
    /// Card kind wire name, such as `Service`.
    #[getter]
    fn kind(&self) -> &str {
        &self.inner.kind
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

/// One non-fatal loader diagnostic in a [`PyCheckReport`].
#[pyclass(module = "wyrd.testing.cli", name = "CheckDiagnostic", frozen)]
pub struct PyCheckDiagnostic {
    /// Diagnostic from the shared loader.
    inner: Diagnostic,
}

#[pymethods]
impl PyCheckDiagnostic {
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

/// Deterministic local check report returned by `declare_check`.
#[pyclass(module = "wyrd.testing.cli", name = "CheckReport")]
pub struct PyCheckReport {
    /// Whether the tree loaded and resolved; always `True` when returned.
    #[pyo3(get)]
    ok: bool,
    /// Cards the tree would declare, in declaration order.
    #[pyo3(get)]
    cards: Vec<Py<PyCheckCard>>,
    /// Non-fatal loader diagnostics.
    #[pyo3(get)]
    diagnostics: Vec<Py<PyCheckDiagnostic>>,
}

impl PyCheckReport {
    /// Move the shared report into Python-owned items.
    ///
    /// # Errors
    ///
    /// Returns a Python error when an item cannot be allocated.
    fn new(py: Python<'_>, report: CheckReport) -> PyResult<Self> {
        Ok(Self {
            ok: report.ok,
            cards: report
                .cards
                .into_iter()
                .map(|inner| Py::new(py, PyCheckCard { inner }))
                .collect::<PyResult<_>>()?,
            diagnostics: report
                .diagnostics
                .into_iter()
                .map(|inner| Py::new(py, PyCheckDiagnostic { inner }))
                .collect::<PyResult<_>>()?,
        })
    }
}

/// What `mcp_install` did to one host's configuration file.
#[pyclass(module = "wyrd.testing.cli", name = "McpHostReport", frozen)]
pub struct PyMcpHostReport {
    /// Outcome from the shared installer.
    inner: McpHostReport,
}

#[pymethods]
impl PyMcpHostReport {
    /// The `--host` value naming the host, such as `claude-code`.
    #[getter]
    fn host(&self) -> &'static str {
        self.inner.host.name()
    }

    /// `added`, `updated`, or `unchanged` when the host is connected;
    /// otherwise `not_detected`, `conflict`, `unreadable`, or `unwritable`.
    #[getter]
    fn status(&self) -> String {
        self.inner.status.to_string()
    }

    /// The host's configuration file, or `None` when it was not detected.
    #[getter]
    fn path(&self) -> Option<PathBuf> {
        self.inner.path.clone()
    }

    /// Why the host was not connected and what to do next, or `None`.
    #[getter]
    fn detail(&self) -> Option<&str> {
        self.inner.detail.as_deref()
    }
}

/// Per-host outcomes returned by `mcp_install`, in selection order.
#[pyclass(module = "wyrd.testing.cli", name = "McpInstallReport")]
pub struct PyMcpInstallReport {
    /// One outcome per distinct selected host.
    #[pyo3(get)]
    hosts: Vec<Py<PyMcpHostReport>>,
}

impl PyMcpInstallReport {
    /// Move the shared report into Python-owned items.
    ///
    /// # Errors
    ///
    /// Returns a Python error when an item cannot be allocated.
    fn new(py: Python<'_>, report: McpInstallReport) -> PyResult<Self> {
        Ok(Self {
            hosts: report
                .hosts
                .into_iter()
                .map(|inner| Py::new(py, PyMcpHostReport { inner }))
                .collect::<PyResult<_>>()?,
        })
    }
}

/// Validate a local Card tree without contacting a server (`wyrd declare --check`).
///
/// # Errors
///
/// Raises `WyrdError` with `WYRD_LOADER_400_INVALID_ENVELOPE`, diagnostics in
/// `details`, when the tree cannot be loaded or resolved.
#[pyfunction]
#[pyo3(signature = (path))]
fn declare_check(py: Python<'_>, path: PathBuf) -> WyrdPyResult<PyCheckReport> {
    let report = py.detach(|| commands::declare_check(&path))?;
    Ok(PyCheckReport::new(py, report)?)
}

/// Declare a local Card tree and return its receipt (`wyrd declare`).
///
/// # Errors
///
/// Raises `WyrdError` for invalid local input, a missing credential or bad
/// endpoint, or the server's refusal.
#[pyfunction]
#[pyo3(signature = (path, *, client=None))]
fn declare(
    py: Python<'_>,
    path: PathBuf,
    client: Option<&Bound<'_, PyWyrdClient>>,
) -> WyrdPyResult<PyDeclarationReceipt> {
    let client = rust_client(client);
    Ok(block_on(py, commands::declare(&path, client))?.into())
}

/// Card selector fields in `(kind, name, version, uid)` order.
///
/// The public `wyrd.testing.cli` wrappers pass their selector keywords as
/// this tuple; the shared CLI selector parser validates them.
type SelectorFields = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// Build the shared CLI selector from its Python fields.
fn selector((kind, name, version, uid): SelectorFields) -> SelectorArgs {
    SelectorArgs {
        kind,
        name,
        version,
        uid,
    }
}

/// Read one Card envelope without downloading artifacts (`wyrd get`).
///
/// # Errors
///
/// Raises `WyrdError` for an invalid selector, a missing credential or bad
/// endpoint, or a failed read.
#[pyfunction]
#[pyo3(signature = (fields, client))]
fn get(
    py: Python<'_>,
    fields: SelectorFields,
    client: Option<&Bound<'_, PyWyrdClient>>,
) -> WyrdPyResult<PyCardEnvelope> {
    let selector = selector(fields);
    let client = rust_client(client);
    let card = block_on(py, commands::get(&selector, client))?;
    Ok(PyCardEnvelope::from_registry_card(card)?)
}

/// Pull a Card's reachable graph into `output_dir` (`wyrd pull`).
///
/// # Errors
///
/// Raises `WyrdError` for an invalid selector, a missing credential or bad
/// endpoint, or a failed read, download, or bundle publication.
#[pyfunction]
#[pyo3(signature = (output_dir, fields, metadata_only, client))]
fn pull(
    py: Python<'_>,
    output_dir: PathBuf,
    fields: SelectorFields,
    metadata_only: bool,
    client: Option<&Bound<'_, PyWyrdClient>>,
) -> WyrdPyResult<PyPullSummary> {
    let selector = selector(fields);
    let client = rust_client(client);
    let inner = block_on(
        py,
        commands::pull(&selector, &output_dir, metadata_only, client),
    )?;
    Ok(PyPullSummary::from(inner))
}

/// Artifact upload identity fields in `(name, version, bump)` order.
///
/// The public `wyrd.testing.cli.artifact_upload` wrapper passes its identity
/// keywords as this tuple; `bump` is one of the executable's `--bump` levels.
type UploadFields = (String, Option<String>, Option<String>);

/// Publish one file or directory as an Artifact Card (`wyrd artifact upload`).
///
/// # Errors
///
/// Raises `WyrdError` with `WYRD_SPEC_400_VALIDATION` for a `bump` other than
/// `major`, `minor`, or `patch`, an invalid name or version, the upload
/// preparation's refusals, a missing credential or bad endpoint, or the
/// server's refusal.
#[pyfunction]
#[pyo3(signature = (source, fields, artifact_kind, extract, client))]
fn artifact_upload(
    py: Python<'_>,
    source: PathBuf,
    fields: UploadFields,
    artifact_kind: String,
    extract: bool,
    client: Option<&Bound<'_, PyWyrdClient>>,
) -> WyrdPyResult<CardRefPy> {
    let (name, version, bump) = fields;
    let bump = bump
        .map(|level| match level.as_str() {
            "major" => Ok(BumpLevel::Major),
            "minor" => Ok(BumpLevel::Minor),
            "patch" => Ok(BumpLevel::Patch),
            _ => Err(WyrdError::Validation {
                message: format!("bump: {level} is not major, minor, or patch"),
                details: serde_json::json!({ "field": "bump", "value": level }),
            }),
        })
        .transpose()?;
    let client = rust_client(client);
    let root = block_on(
        py,
        commands::artifact_upload(
            &source,
            &name,
            version.as_deref(),
            bump,
            &artifact_kind,
            extract,
            client,
        ),
    )?;
    Ok(CardRefPy(root))
}

/// List the files one Card holds (`wyrd artifact files`).
///
/// # Errors
///
/// Raises `WyrdError` for an invalid selector, a missing credential or bad
/// endpoint, or a failed read.
#[pyfunction]
#[pyo3(signature = (fields, client))]
fn artifact_files(
    py: Python<'_>,
    fields: SelectorFields,
    client: Option<&Bound<'_, PyWyrdClient>>,
) -> WyrdPyResult<Py<PyAny>> {
    let selector = selector(fields);
    let client = rust_client(client);
    let inventory = block_on(py, commands::artifact_files(&selector, client))?;
    Ok(json_to_pyobject(py, &serde_json::to_value(inventory)?)?)
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

/// Connect the named MCP hosts to Wyrd (`wyrd mcp install --host ...`).
///
/// Hosts are located from the process environment; a host that cannot be
/// connected is reported in the result, not raised.
///
/// # Errors
///
/// Raises `WyrdError` with `WYRD_SPEC_400_VALIDATION` for an unknown or empty
/// host selection, and `WYRD_CLIENT_400_CONFIG_INVALID` for an invalid
/// `server`.
#[pyfunction]
#[pyo3(signature = (hosts, *, server=None))]
fn mcp_install(
    py: Python<'_>,
    hosts: Vec<String>,
    server: Option<String>,
) -> WyrdPyResult<PyMcpInstallReport> {
    let report = py.detach(|| commands::mcp_install(&hosts, server.as_deref()))?;
    Ok(PyMcpInstallReport::new(py, report)?)
}

/// Register every in-process command and result type on
/// `wyrd._wyrd.testing.cli`.
///
/// # Errors
///
/// Returns a Python error when an item cannot be installed on `module`.
pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyCheckCard>()?;
    module.add_class::<PyCheckDiagnostic>()?;
    module.add_class::<PyCheckReport>()?;
    module.add_class::<PyMcpHostReport>()?;
    module.add_class::<PyMcpInstallReport>()?;
    module.add_function(wrap_pyfunction!(declare_check, module)?)?;
    module.add_function(wrap_pyfunction!(declare, module)?)?;
    module.add_function(wrap_pyfunction!(get, module)?)?;
    module.add_function(wrap_pyfunction!(pull, module)?)?;
    module.add_function(wrap_pyfunction!(artifact_upload, module)?)?;
    module.add_function(wrap_pyfunction!(artifact_files, module)?)?;
    module.add_function(wrap_pyfunction!(mcp_install, module)?)?;
    module.add_function(wrap_pyfunction!(put_provider_credential, module)?)?;
    module.add_function(wrap_pyfunction!(revoke_provider_credential, module)?)?;
    module.add_function(wrap_pyfunction!(delete_provider_credential, module)?)
}
