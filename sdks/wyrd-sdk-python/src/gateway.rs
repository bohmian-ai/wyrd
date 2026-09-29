//! `PyO3` boundary for tenant gateway administration.
//!
//! Each method converts its Python dict or name argument into the typed
//! `wyrd-spec` gateway contract, calls the shared [`wyrd_client::Gateway`]
//! handle, and projects the typed response back into plain Python data. Serde
//! decoding of the contract is the only local validation; lifecycle, conflict,
//! redaction, and authorization rules stay on the server.
//!
//! A decode failure is reported by kind, position, and argument name only.
//! `serde_json`'s own message quotes the offending input, and a gateway body
//! can carry a provider key, so that text never reaches Python.

use std::future::Future;

use pyo3::prelude::*;
use pyo3::types::PyModule;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Error as JsonError;
use serde_json::error::Category;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{ProviderCredentialName, ProviderDeploymentName};
use wyrd_utils::py::{WyrdPyError, WyrdPyResult, json_to_pyobject, pyobject_to_json};

/// Builds the catalog validation failure for one rejected Python argument.
///
/// Callers branch on `WYRD_SPEC_400_VALIDATION` and read `details["field"]`
/// rather than parsing a message. `reason` must describe the failure without
/// quoting the argument: a gateway write body can hold a provider key, so no
/// part of it may reach a message or a detail.
fn invalid_argument(field: &str, reason: &str) -> WyrdPyError {
    WyrdPyError::from(WyrdError::Validation {
        message: format!("{field} is invalid: {reason}"),
        details: serde_json::json!({ "field": field, "reason": reason }),
    })
}

/// Describes a JSON decode failure by kind and position, quoting no input.
fn decode_failure(error: &JsonError) -> String {
    let kind = match error.classify() {
        Category::Syntax => "well-formed JSON",
        Category::Data => "a value matching this argument's contract",
        Category::Eof => "a complete JSON value",
        Category::Io => "a readable value",
    };
    format!(
        "expected {kind} (decode failed at line {}, column {})",
        error.line(),
        error.column()
    )
}

/// Decodes one Python mapping into its typed gateway contract.
///
/// # Errors
///
/// Returns `WYRD_SPEC_400_VALIDATION` naming `field` when the value is not
/// JSON-compatible or does not match the contract's serde shape. Neither the
/// message nor the details repeat any part of the rejected value.
fn decode<T: DeserializeOwned>(field: &str, value: &Bound<'_, PyAny>) -> WyrdPyResult<T> {
    let json = pyobject_to_json(value)
        .map_err(|_| invalid_argument(field, "expected a JSON-compatible Python value"))?;
    serde_json::from_value(json).map_err(|error| invalid_argument(field, &decode_failure(&error)))
}

/// Python-facing tenant gateway administration client.
///
/// Wraps one [`wyrd_client::Gateway`] bound to a client resolved from explicit
/// options or the standard Wyrd environment/credential chain. Provider
/// credential mutation is absent by construction: submitting, rotating,
/// revoking, or deleting a credential lives on
/// `wyrd_client::gateway_credential`, which this binding never constructs, so
/// no Python value — however it is built at runtime — can reach a managed
/// secret write from here. Reads, deployments, policies, and invocation stay.
#[pyclass(module = "wyrd._wyrd.gateway", name = "Gateway")]
pub struct PyGateway {
    /// Shared Rust administration handle every method delegates to.
    inner: wyrd_client::Gateway,
}

impl PyGateway {
    /// Runs one handle call with the GIL released and projects its result.
    ///
    /// A unit result projects to `None`.
    ///
    /// # Errors
    ///
    /// Returns the server's stable `WyrdError` from `call`, or an internal
    /// error when the typed response cannot be projected to Python.
    fn run<T, F>(py: Python<'_>, call: F) -> WyrdPyResult<Py<PyAny>>
    where
        T: Serialize + Send,
        F: Future<Output = Result<T, WyrdError>> + Send,
    {
        let value = py.detach(|| wyrd_runtime::runtime().block_on(call))?;
        let json = serde_json::to_value(value)?;
        Ok(json_to_pyobject(py, &json)?)
    }

    /// Parses a credential name argument.
    ///
    /// # Errors
    ///
    /// Returns `WYRD_SPEC_400_VALIDATION` when `name` is not a valid token.
    fn credential_name(name: &str) -> WyrdPyResult<ProviderCredentialName> {
        ProviderCredentialName::new(name)
            .map_err(|_| invalid_argument("name", "expected a provider credential name"))
    }

    /// Parses a deployment name argument.
    ///
    /// # Errors
    ///
    /// Returns `WYRD_SPEC_400_VALIDATION` when `name` is not a valid token.
    fn deployment_name(name: &str) -> WyrdPyResult<ProviderDeploymentName> {
        ProviderDeploymentName::new(name)
            .map_err(|_| invalid_argument("name", "expected a provider deployment name"))
    }
}

#[pymethods]
impl PyGateway {
    /// Connects to a Wyrd server; omitted options resolve from the environment.
    ///
    /// # Errors
    ///
    /// Raises `WyrdError` with `WYRD_CLIENT_401_NO_CREDENTIALS` when no
    /// credential resolves, or a transport error when the client cannot build.
    #[new]
    #[pyo3(signature = (server_url=None, credential=None))]
    fn __new__(server_url: Option<&str>, credential: Option<&str>) -> WyrdPyResult<Self> {
        let client = wyrd_client::bifrost::client_from_options(server_url, credential, None)
            .map_err(WyrdError::from)?;
        Ok(Self {
            inner: wyrd_client::Gateway::new(client),
        })
    }

    /// Reads one redacted provider credential.
    ///
    /// # Errors
    ///
    /// Raises `WyrdError` for an invalid name or the server's permission,
    /// not-found, or availability error.
    fn credential(&self, py: Python<'_>, name: &str) -> WyrdPyResult<Py<PyAny>> {
        let name = Self::credential_name(name)?;
        Self::run(py, self.inner.credential(&name))
    }

    /// Lists redacted provider credentials ordered by name.
    ///
    /// # Errors
    ///
    /// Raises `WyrdError` for the server's permission or availability error.
    fn credentials(&self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
        Self::run(py, self.inner.credentials())
    }

    /// Creates or replaces a provider deployment.
    ///
    /// # Errors
    ///
    /// Raises `WyrdError` for an invalid body or the server's permission,
    /// invalid-configuration, or availability error.
    fn put_deployment(
        &self,
        py: Python<'_>,
        deployment: &Bound<'_, PyAny>,
    ) -> WyrdPyResult<Py<PyAny>> {
        let deployment = decode("deployment", deployment)?;
        Self::run(py, self.inner.put_deployment(&deployment))
    }

    /// Reads one provider deployment.
    ///
    /// # Errors
    ///
    /// Raises `WyrdError` for an invalid name or the server's permission,
    /// not-found, or availability error.
    fn deployment(&self, py: Python<'_>, name: &str) -> WyrdPyResult<Py<PyAny>> {
        let name = Self::deployment_name(name)?;
        Self::run(py, self.inner.deployment(&name))
    }

    /// Lists provider deployments ordered by name.
    ///
    /// # Errors
    ///
    /// Raises `WyrdError` for the server's permission or availability error.
    fn deployments(&self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
        Self::run(py, self.inner.deployments())
    }

    /// Deletes a provider deployment; an absent name succeeds.
    ///
    /// # Errors
    ///
    /// Raises `WyrdError` for an invalid name or the server's permission,
    /// conflict, or availability error.
    fn delete_deployment(&self, py: Python<'_>, name: &str) -> WyrdPyResult<Py<PyAny>> {
        let name = Self::deployment_name(name)?;
        Self::run(py, self.inner.delete_deployment(&name))
    }

    /// Replaces the tenant fallback policy.
    ///
    /// # Errors
    ///
    /// Raises `WyrdError` for an invalid body or the server's permission,
    /// invalid-configuration, or availability error.
    fn put_fallback_policy(
        &self,
        py: Python<'_>,
        policy: &Bound<'_, PyAny>,
    ) -> WyrdPyResult<Py<PyAny>> {
        let policy = decode("policy", policy)?;
        Self::run(py, self.inner.put_fallback_policy(&policy))
    }

    /// Reads the tenant fallback policy.
    ///
    /// # Errors
    ///
    /// Raises `WyrdError` for the server's permission or availability error.
    fn fallback_policy(&self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
        Self::run(py, self.inner.fallback_policy())
    }

    /// Restores the default fallback policy.
    ///
    /// # Errors
    ///
    /// Raises `WyrdError` for the server's permission or availability error.
    fn delete_fallback_policy(&self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
        Self::run(py, self.inner.delete_fallback_policy())
    }

    /// Replaces the tenant governance policy.
    ///
    /// # Errors
    ///
    /// Raises `WyrdError` for an invalid body or the server's permission,
    /// invalid-configuration, conflict, or availability error.
    fn put_governance_policy(
        &self,
        py: Python<'_>,
        policy: &Bound<'_, PyAny>,
    ) -> WyrdPyResult<Py<PyAny>> {
        let policy = decode("policy", policy)?;
        Self::run(py, self.inner.put_governance_policy(&policy))
    }

    /// Reads the tenant governance policy.
    ///
    /// # Errors
    ///
    /// Raises `WyrdError` for the server's permission or availability error.
    fn governance_policy(&self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
        Self::run(py, self.inner.governance_policy())
    }

    /// Restores the default governance policy.
    ///
    /// # Errors
    ///
    /// Raises `WyrdError` for the server's permission or availability error.
    fn delete_governance_policy(&self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
        Self::run(py, self.inner.delete_governance_policy())
    }

    /// Replaces the tenant capture policy and returns its versioned view.
    ///
    /// # Errors
    ///
    /// Raises `WyrdError` for an invalid body or the server's permission,
    /// invalid-configuration, or availability error.
    fn put_capture_policy(
        &self,
        py: Python<'_>,
        policy: &Bound<'_, PyAny>,
    ) -> WyrdPyResult<Py<PyAny>> {
        let policy = decode("policy", policy)?;
        Self::run(py, self.inner.put_capture_policy(&policy))
    }

    /// Reads the tenant capture policy.
    ///
    /// # Errors
    ///
    /// Raises `WyrdError` for the server's permission or availability error.
    fn capture_policy(&self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
        Self::run(py, self.inner.capture_policy())
    }
}

/// Registers the gateway administration client on `module`.
///
/// # Errors
///
/// Returns `PyO3` registration errors.
pub fn register_gateway(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyGateway>()
}
