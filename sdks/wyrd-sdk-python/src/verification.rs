//! `PyO3` boundary for the shared Verification control-plane handle.
//!
//! Each method converts its Python arguments to the typed wire contract at the
//! edge, calls [`wyrd_client::verification::Verification`] with the GIL
//! released, and returns the server's JSON projection as plain Python values.
//! No readiness, authorization, idempotency, or run logic lives here.

use pyo3::prelude::*;
use pyo3::types::PyModule;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use wyrd_client::bifrost::client_from_options;
use wyrd_client::verification::{
    BindingId, StartVerificationRunRequest, Verification as NativeVerification, VerificationRunId,
};
use wyrd_spec::error::WyrdError;
use wyrd_utils::py::{WyrdPyError, WyrdPyResult, json_to_pyobject, pyobject_to_json};

/// Decode one Python argument into a wire type, naming the argument on failure.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` with `details.field` set to `field` when
/// `value` does not match the wire contract.
fn decode<T: DeserializeOwned>(field: &str, value: Value) -> WyrdPyResult<T> {
    serde_json::from_value(value).map_err(|error| {
        WyrdPyError::from(WyrdError::Validation {
            message: format!("{field} is invalid: {error}"),
            details: serde_json::json!({ "field": field, "reason": error.to_string() }),
        })
    })
}

/// Project one wire response into plain Python values.
///
/// # Errors
/// Returns a Python error when the response cannot be serialized or converted.
fn to_python<T: Serialize>(py: Python<'_>, value: &T) -> WyrdPyResult<Py<PyAny>> {
    Ok(json_to_pyobject(py, &serde_json::to_value(value)?)?)
}

/// Python-facing Verification handle: binding status, manual runs, run status.
///
/// Every call blocks the calling thread with the GIL released and returns once
/// the server answers; a started run is enqueued, not finished.
#[pyclass(module = "wyrd._wyrd.verification", name = "Verification")]
pub struct Verification {
    /// The shared native handle every call delegates to.
    inner: NativeVerification,
}

#[pymethods]
impl Verification {
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
            inner: NativeVerification::with_client(client),
        })
    }

    /// Read one binding's identities, activity, readiness, and cursor as a dict.
    ///
    /// # Errors
    /// Raises `WyrdError` when `binding_id` is not a UUID, the caller lacks
    /// `cards:read`, the binding is unknown in the caller's tenant, or the
    /// request fails.
    fn get_binding(&self, py: Python<'_>, binding_id: &str) -> WyrdPyResult<Py<PyAny>> {
        let binding_id: BindingId = decode("binding_id", Value::from(binding_id))?;
        let status =
            py.detach(|| wyrd_runtime::runtime().block_on(self.inner.get_binding(&binding_id)))?;
        to_python(py, &status)
    }

    /// Durably enqueue one manual Drift run and return its run ID.
    ///
    /// `request` is the `StartVerificationRunRequest` wire shape. A retry with
    /// the same `idempotency_key` and request returns the same run ID.
    ///
    /// # Errors
    /// Raises `WyrdError` when `request` does not match the wire contract, the
    /// window or target is invalid, the caller lacks `evals:run` or subject
    /// scope, the Verifier is not ready, the key was used for a different
    /// request, or the request fails.
    #[pyo3(signature = (request, idempotency_key=None))]
    fn start_run(
        &self,
        py: Python<'_>,
        request: &Bound<'_, PyAny>,
        idempotency_key: Option<&str>,
    ) -> WyrdPyResult<String> {
        let request: StartVerificationRunRequest = decode("request", pyobject_to_json(request)?)?;
        let run_id = py.detach(|| {
            wyrd_runtime::runtime().block_on(self.inner.start_run(&request, idempotency_key))
        })?;
        Ok(run_id.to_string())
    }

    /// Read one run's status, requester, result pointer, and dispatches as a dict.
    ///
    /// # Errors
    /// Raises `WyrdError` when `run_id` is not a UUID, the caller lacks
    /// `cards:read`, the run is unknown in the caller's tenant, or the request
    /// fails.
    fn get_run(&self, py: Python<'_>, run_id: &str) -> WyrdPyResult<Py<PyAny>> {
        let run_id: VerificationRunId = decode("run_id", Value::from(run_id))?;
        let status = py.detach(|| wyrd_runtime::runtime().block_on(self.inner.get_run(&run_id)))?;
        to_python(py, &status)
    }
}

/// Register the native `verification` submodule's classes.
///
/// # Errors
/// Returns a Python error when the class cannot be added to the module.
pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<Verification>()
}
