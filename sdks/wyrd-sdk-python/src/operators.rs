//! `PyO3` boundary for the shared Operator connection handle.
//!
//! Each method decodes its Python arguments into the typed wire contract at
//! the edge, calls [`wyrd_client::operator_connections::OperatorConnections`]
//! with the GIL released, and returns the server's redacted view as plain
//! Python values. Secrets travel only inside request dicts and are never
//! echoed in a decode error. No authorization, encryption, or storage logic
//! lives here.

use pyo3::prelude::*;
use pyo3::types::PyModule;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use wyrd_client::bifrost::client_from_options;
use wyrd_client::operator_connections::{
    CreateOperatorConnectionRequest, OperatorConnectionId,
    OperatorConnections as NativeOperatorConnections, UpdateOperatorConnectionRequest,
};
use wyrd_spec::error::WyrdError;
use wyrd_utils::py::{WyrdPyError, WyrdPyResult, json_to_pyobject, pyobject_to_json};

/// Decode one Python argument into a wire type, naming only the argument and
/// the decode category on failure so a secret value is never echoed; shared by
/// the Operator connection and Verification handles.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` with `details.field` set to `field` when
/// `value` does not match the wire contract.
pub(crate) fn decode<T: DeserializeOwned>(field: &str, value: Value) -> WyrdPyResult<T> {
    serde_json::from_value(value).map_err(|error| {
        WyrdPyError::from(WyrdError::Validation {
            message: format!("{field} does not match the wire contract"),
            details: serde_json::json!({ "field": field, "category": format!("{:?}", error.classify()) }),
        })
    })
}

/// Project one wire response into plain Python values; shared by the Operator
/// connection and Verification handles.
///
/// # Errors
/// Returns a Python error when the response cannot be serialized or converted.
pub(crate) fn to_python<T: Serialize>(py: Python<'_>, value: &T) -> WyrdPyResult<Py<PyAny>> {
    Ok(json_to_pyobject(py, &serde_json::to_value(value)?)?)
}

/// Python-facing Operator connection handle: create, list, get, update, disable.
///
/// Every call blocks the calling thread with the GIL released and returns the
/// server's redacted connection view; no call ever returns a secret.
#[pyclass(module = "wyrd._wyrd.operators", name = "OperatorConnections")]
pub struct OperatorConnections {
    /// The shared native handle every call delegates to.
    inner: NativeOperatorConnections,
}

#[pymethods]
impl OperatorConnections {
    /// Build a handle; omitted arguments fall through the client configuration.
    ///
    /// No network call happens here.
    ///
    /// # Errors
    /// Raises `WyrdError` when the server URL or credential cannot be resolved.
    #[new]
    #[pyo3(signature = (server_url=None, credential=None))]
    fn __new__(server_url: Option<&str>, credential: Option<&str>) -> WyrdPyResult<Self> {
        let client = client_from_options(server_url, credential, None)
            .map_err(|error| WyrdPyError::from(WyrdError::from(error)))?;
        Ok(Self {
            inner: NativeOperatorConnections::with_client(client),
        })
    }

    /// Create one connection from a `CreateOperatorConnectionRequest` dict.
    ///
    /// # Errors
    /// Raises `WyrdError` when `request` does not match the wire contract, the
    /// caller lacks `operators:write`, the name is taken, no key is
    /// available, or the request fails.
    fn create(&self, py: Python<'_>, request: &Bound<'_, PyAny>) -> WyrdPyResult<Py<PyAny>> {
        let request: CreateOperatorConnectionRequest =
            decode("request", pyobject_to_json(request)?)?;
        let view = py.detach(|| wyrd_runtime::runtime().block_on(self.inner.create(&request)))?;
        to_python(py, &view)
    }

    /// List the caller tenant's connections as redacted dicts.
    ///
    /// # Errors
    /// Raises `WyrdError` when the caller lacks `operators:read` or the
    /// request fails.
    fn list(&self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
        let views = py.detach(|| wyrd_runtime::runtime().block_on(self.inner.list()))?;
        to_python(py, &views)
    }

    /// Read one connection's redacted view.
    ///
    /// # Errors
    /// Raises `WyrdError` when `connection_id` is not a `UUIDv7`, the caller
    /// lacks `operators:read`, the connection is unknown in the caller's
    /// tenant, or the request fails.
    fn get(&self, py: Python<'_>, connection_id: &str) -> WyrdPyResult<Py<PyAny>> {
        let id: OperatorConnectionId = decode("connection_id", Value::from(connection_id))?;
        let view = py.detach(|| wyrd_runtime::runtime().block_on(self.inner.get(&id)))?;
        to_python(py, &view)
    }

    /// Update or rotate one connection from an `UpdateOperatorConnectionRequest` dict.
    ///
    /// # Errors
    /// Raises `WyrdError` when an argument does not match the wire contract,
    /// the caller lacks `operators:write`, the connection is unknown, the
    /// provider differs, no key is available, or the request fails.
    fn update(
        &self,
        py: Python<'_>,
        connection_id: &str,
        request: &Bound<'_, PyAny>,
    ) -> WyrdPyResult<Py<PyAny>> {
        let id: OperatorConnectionId = decode("connection_id", Value::from(connection_id))?;
        let request: UpdateOperatorConnectionRequest =
            decode("request", pyobject_to_json(request)?)?;
        let view =
            py.detach(|| wyrd_runtime::runtime().block_on(self.inner.update(&id, &request)))?;
        to_python(py, &view)
    }

    /// Disable one connection; Operators naming it fail closed until re-enabled.
    ///
    /// # Errors
    /// Raises `WyrdError` when `connection_id` is not a `UUIDv7`, the caller
    /// lacks `operators:write`, the connection is unknown, or the request
    /// fails.
    fn disable(&self, py: Python<'_>, connection_id: &str) -> WyrdPyResult<Py<PyAny>> {
        let id: OperatorConnectionId = decode("connection_id", Value::from(connection_id))?;
        let view = py.detach(|| wyrd_runtime::runtime().block_on(self.inner.disable(&id)))?;
        to_python(py, &view)
    }
}

/// Register the native `operators` submodule's classes.
///
/// # Errors
/// Returns a Python error when the class cannot be added to the module.
pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<OperatorConnections>()
}
