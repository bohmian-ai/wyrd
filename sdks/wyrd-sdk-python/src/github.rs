//! `PyO3` boundary for the shared tenant GitHub connection handle.
//!
//! Each method calls [`wyrd_client::GithubConnection`] with the GIL released
//! and returns the server's redacted view or connect receipt as plain Python
//! values. Association, authorization, and audit stay on the server; nothing
//! here holds a GitHub token, code, or key.

use pyo3::prelude::*;
use pyo3::types::PyModule;
use wyrd_client::GithubConnection as NativeGithubConnection;
use wyrd_utils::py::WyrdPyResult;

use crate::operators::to_python;

/// Python-facing GitHub connection handle: begin, get, disconnect.
///
/// Every call blocks the calling thread with the GIL released and returns the
/// server's redacted connection view or connect receipt; no call ever returns
/// a GitHub token, code, or key.
#[pyclass(module = "wyrd._wyrd.github", name = "GithubConnection")]
pub struct GithubConnection {
    /// The one native connection every call delegates to, connected on the
    /// first server operation (or by `connect`).
    inner: crate::client::LazyConnection<NativeGithubConnection>,
}

impl GithubConnection {
    /// Return the connected native handle, connecting it on first use.
    ///
    /// # Errors
    /// Returns the stable configuration, credential, or refused-exchange
    /// error of the shared Rust `connect`.
    fn handle(&self, py: Python<'_>) -> WyrdPyResult<&NativeGithubConnection> {
        Ok(self.inner.get(py, |client| async move {
            match client {
                Some(client) => NativeGithubConnection::connect_with(&client).await,
                None => NativeGithubConnection::connect().await,
            }
        })?)
    }
}

#[pymethods]
impl GithubConnection {
    /// Build a GitHub connection handle acting as `client`, performing no IO.
    ///
    /// The client, or the ambient chain when `client` is omitted, resolves
    /// and connects once on the first server operation, exactly as `connect`
    /// does.
    ///
    /// # Arguments
    /// * `client` - The `WyrdClient` every call is sent as, or `None` for the
    ///   ambient chain.
    #[new]
    #[pyo3(signature = (client=None))]
    fn __new__(client: Option<PyRef<'_, crate::client::PyWyrdClient>>) -> Self {
        Self {
            inner: crate::client::LazyConnection::new(client.as_deref()),
        }
    }

    /// Connect a GitHub connection handle acting as `client` now.
    ///
    /// # Arguments
    /// * `client` - The `WyrdClient` every call is sent as, or `None` for the
    ///   ambient chain.
    ///
    /// # Errors
    /// Raises `WyrdError` with the stable configuration, credential, or
    /// refused-exchange error.
    #[staticmethod]
    #[pyo3(signature = (client=None))]
    fn connect(
        py: Python<'_>,
        client: Option<PyRef<'_, crate::client::PyWyrdClient>>,
    ) -> WyrdPyResult<Self> {
        let handle = Self::__new__(client);
        handle.handle(py)?;
        Ok(handle)
    }

    /// Begin a connection and return GitHub's install URL with its expiry;
    /// the connection completes only in a browser signed in to Wyrd as the
    /// same principal.
    ///
    /// # Errors
    /// Raises `WyrdError` when GitHub is not configured, the caller lacks
    /// permission, or the request fails.
    fn begin(&self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
        let receipt = crate::trace::block_on(py, self.handle(py)?.begin())?;
        to_python(py, &receipt)
    }

    /// Read the caller tenant's redacted connection view.
    ///
    /// # Errors
    /// Raises `WyrdError` when GitHub is not configured, the caller lacks
    /// permission, or the request fails.
    fn get(&self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
        let view = crate::trace::block_on(py, self.handle(py)?.get())?;
        to_python(py, &view)
    }

    /// Release the caller tenant's installation and return the resulting view.
    ///
    /// # Errors
    /// Raises `WyrdError` when GitHub is not configured, the caller lacks
    /// permission, nothing is connected, or the request fails.
    fn disconnect(&self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
        let view = crate::trace::block_on(py, self.handle(py)?.disconnect())?;
        to_python(py, &view)
    }
}

/// Register the native `github` submodule's classes.
///
/// # Errors
/// Returns a Python error when the class cannot be added to the module.
pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<GithubConnection>()
}
