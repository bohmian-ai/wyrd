//! `PyO3` boundary for the shared deferred evidence publisher.
//!
//! [`PyEvidence::publish`] calls [`wyrd_client::Evidence::publish`] with the
//! GIL released; source resolution, upload, matching, and the receipt stay
//! in the shared client and the server.

use pyo3::prelude::*;
use wyrd_client::Evidence as NativeEvidence;
use wyrd_utils::py::WyrdPyResult;

use crate::operators::to_python;

/// Python-facing deferred evidence publisher.
#[pyclass(module = "wyrd._wyrd.verify", name = "Evidence")]
pub struct PyEvidence {
    /// The one native publisher every call delegates to, connected on the
    /// first server operation (or by `connect`).
    inner: crate::client::LazyConnection<NativeEvidence>,
}

impl PyEvidence {
    /// Return the connected native publisher, connecting it on first use.
    ///
    /// # Errors
    /// Returns the stable configuration, credential, or refused-exchange
    /// error of the shared Rust `connect`.
    fn handle(&self, py: Python<'_>) -> WyrdPyResult<&NativeEvidence> {
        Ok(self.inner.get(py, |client| async move {
            match client {
                Some(client) => NativeEvidence::connect_with(&client).await,
                None => NativeEvidence::connect().await,
            }
        })?)
    }
}

#[pymethods]
impl PyEvidence {
    /// Build a publisher acting as `client`, performing no IO.
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

    /// Connect a publisher acting as `client` now.
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

    /// Publish `value` as the deferred input `name` of `repository`
    /// (`owner/name`) at `commit` and return the receipt.
    ///
    /// # Errors
    /// Raises `WyrdError` for an unconvertible value, an unresolvable
    /// source, or the server's refusal.
    #[pyo3(signature = (name, value, repository=None, commit=None))]
    fn publish(
        &self,
        py: Python<'_>,
        name: &str,
        value: &Bound<'_, PyAny>,
        repository: Option<&str>,
        commit: Option<&str>,
    ) -> WyrdPyResult<Py<PyAny>> {
        let value = crate::observe::input(py, "value", value)?;
        let handle = self.handle(py)?;
        let receipt = crate::trace::block_on(py, handle.publish(name, value, repository, commit))?;
        to_python(py, &receipt)
    }
}
