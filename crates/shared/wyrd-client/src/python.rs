//! Python boundary for the tenant-admin SDK surface.
//!
//! [`PyWyrdClient`] is the native, flat projection of [`AdminClient`]: each
//! method decodes a Python mapping into the `wyrd-spec` request DTO, drives the
//! async admin call on a held Tokio runtime with the GIL released, and converts
//! the redacted view back into a Python object. Errors cross the boundary
//! through [`wyrd_error_to_py_err`] so the server's structured codes (including
//! `WYRD_AUTH_409_ADMIN_CONFLICT` / `WYRD_AUTH_404_ADMIN_NOT_FOUND`) reach
//! Python as the registered `WyrdError`, never a string.
//!
//! The ergonomic `client.admin.trusted_issuers.*` namespace lives in the
//! `wyrd.client` Python package over this surface; PyO3 stays out of `wyrd-spec`
//! and `wyrd-auth-oidc` — the spec DTOs are converted here via serde, not by
//! deriving `pyclass` on them.

#![cfg(feature = "python")]

use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyModule;
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::runtime::Runtime;
use wyrd_spec::auth::{CreateTrustedIssuerRequest, CreateWorkloadBindingRequest};
use wyrd_spec::error::WyrdError;
use wyrd_utils::py::{
    json_to_pyobject, pyobject_to_json, register_wyrd_error_exception, wyrd_error_to_py_err,
};

use crate::admin::AdminClient;
use crate::client::WyrdClient;
use crate::config::ClientConfig;
use crate::error::WyrdClientError;

/// Native tenant-admin client. The flat surface the `wyrd.client` package wraps
/// into the `client.admin.{trusted_issuers,workload_bindings}` namespace.
#[pyclass(module = "wyrd._wyrd.client", name = "_WyrdClient")]
pub struct PyWyrdClient {
    admin: AdminClient,
    runtime: Arc<Runtime>,
}

#[pymethods]
impl PyWyrdClient {
    /// Assemble a client.
    ///
    /// `base_url` overrides `WYRD_SERVER_URL`; `api_key` outranks ambient
    /// credentials. With neither, resolution falls back to the `WYRD_*`
    /// environment exactly like `WyrdClient::from_env`.
    #[new]
    #[pyo3(signature = (base_url=None, api_key=None))]
    fn __new__(base_url: Option<String>, api_key: Option<String>) -> PyResult<Self> {
        let mut config = ClientConfig::from_env();
        if let Some(base_url) = base_url {
            config.http.base_url = base_url;
        }
        if let Some(api_key) = api_key {
            config.api_key = Some(api_key.into());
        }
        let client = WyrdClient::with_config(config).map_err(client_error_to_py)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| {
                wyrd_error_to_py_err(WyrdError::Internal {
                    message: format!("failed to build admin client runtime: {err}"),
                    details: serde_json::json!({}),
                })
            })?;
        Ok(Self {
            admin: AdminClient::new(client),
            runtime: Arc::new(runtime),
        })
    }

    /// Register a trusted OIDC issuer; returns the redacted view.
    fn create_trusted_issuer(
        &self,
        py: Python<'_>,
        request: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        let request: CreateTrustedIssuerRequest =
            decode_request(request, "CreateTrustedIssuerRequest")?;
        let view = py
            .detach(|| {
                self.runtime
                    .block_on(self.admin.create_trusted_issuer(&request))
            })
            .map_err(wyrd_error_to_py_err)?;
        encode_view(py, &view)
    }

    /// List every trusted OIDC issuer for the caller's tenant.
    fn list_trusted_issuers(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let views = py
            .detach(|| self.runtime.block_on(self.admin.list_trusted_issuers()))
            .map_err(wyrd_error_to_py_err)?;
        encode_view(py, &views)
    }

    /// Remove a trusted OIDC issuer addressed by query string.
    #[pyo3(signature = (issuer, cascade=false))]
    fn delete_trusted_issuer(&self, py: Python<'_>, issuer: &str, cascade: bool) -> PyResult<()> {
        py.detach(|| {
            self.runtime
                .block_on(self.admin.delete_trusted_issuer(issuer, cascade))
        })
        .map_err(wyrd_error_to_py_err)
    }

    /// Register a workload binding; returns the binding view.
    fn create_workload_binding(
        &self,
        py: Python<'_>,
        request: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        let request: CreateWorkloadBindingRequest =
            decode_request(request, "CreateWorkloadBindingRequest")?;
        let view = py
            .detach(|| {
                self.runtime
                    .block_on(self.admin.create_workload_binding(&request))
            })
            .map_err(wyrd_error_to_py_err)?;
        encode_view(py, &view)
    }

    /// List workload bindings, optionally filtered by exact issuer and subject.
    #[pyo3(signature = (issuer=None, subject=None))]
    fn list_workload_bindings(
        &self,
        py: Python<'_>,
        issuer: Option<&str>,
        subject: Option<&str>,
    ) -> PyResult<Py<PyAny>> {
        let views = py
            .detach(|| {
                self.runtime
                    .block_on(self.admin.list_workload_bindings(issuer, subject))
            })
            .map_err(wyrd_error_to_py_err)?;
        encode_view(py, &views)
    }

    /// Remove a workload binding addressed by query string.
    fn delete_workload_binding(&self, py: Python<'_>, issuer: &str, subject: &str) -> PyResult<()> {
        py.detach(|| {
            self.runtime
                .block_on(self.admin.delete_workload_binding(issuer, subject))
        })
        .map_err(wyrd_error_to_py_err)
    }
}

/// Register the `wyrd._wyrd.client` submodule.
///
/// # Errors
/// Returns PyO3 registration errors.
pub fn python_register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    register_wyrd_error_exception(module)?;
    module.add_class::<PyWyrdClient>()?;
    Ok(())
}

/// Decode a Python mapping into a request DTO, mapping a bad shape to a
/// structured validation error.
fn decode_request<T: DeserializeOwned>(request: &Bound<'_, PyAny>, label: &str) -> PyResult<T> {
    let value = pyobject_to_json(request)?;
    serde_json::from_value(value).map_err(|err| {
        wyrd_error_to_py_err(WyrdError::Validation {
            message: format!("invalid {label}: {err}"),
            details: serde_json::json!({ "dto": label }),
        })
    })
}

/// Convert a serializable view into a Python object.
fn encode_view<T: Serialize>(py: Python<'_>, view: &T) -> PyResult<Py<PyAny>> {
    let value = serde_json::to_value(view).map_err(|err| {
        wyrd_error_to_py_err(WyrdError::Internal {
            message: format!("admin view serialization failed: {err}"),
            details: serde_json::json!({}),
        })
    })?;
    json_to_pyobject(py, &value)
}

/// Map a client-assembly failure into a structured Python error.
fn client_error_to_py(error: WyrdClientError) -> PyErr {
    wyrd_error_to_py_err(WyrdError::Internal {
        message: format!("failed to assemble Wyrd client: {error}"),
        details: serde_json::json!({}),
    })
}
