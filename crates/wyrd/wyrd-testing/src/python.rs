//! Python context manager surface for [`WyrdTestServer`].

use std::sync::{Mutex, OnceLock};

use pyo3::prelude::*;
use pyo3::types::PyAny;
use secrecy::ExposeSecret;
use wyrd_utils::py::wyrd_error_to_py_err;

static ENV_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

fn env_mutex() -> &'static Mutex<()> {
    ENV_MUTEX.get_or_init(|| Mutex::new(()))
}

struct EnvSnapshot {
    server_url: Option<String>,
    grpc_url: Option<String>,
    api_key: Option<String>,
}

impl EnvSnapshot {
    fn capture() -> Self {
        Self {
            server_url: std::env::var("WYRD_SERVER_URL").ok(),
            grpc_url: std::env::var("WYRD_GRPC_URL").ok(),
            api_key: std::env::var("WYRD_API_KEY").ok(),
        }
    }

    fn restore(self) {
        match self.server_url {
            Some(v) => unsafe { std::env::set_var("WYRD_SERVER_URL", &v) },
            None => unsafe { std::env::remove_var("WYRD_SERVER_URL") },
        }
        match self.grpc_url {
            Some(v) => unsafe { std::env::set_var("WYRD_GRPC_URL", &v) },
            None => unsafe { std::env::remove_var("WYRD_GRPC_URL") },
        }
        match self.api_key {
            Some(v) => unsafe { std::env::set_var("WYRD_API_KEY", &v) },
            None => unsafe { std::env::remove_var("WYRD_API_KEY") },
        }
    }
}

/// Python context manager for the Wyrd in-process test server.
#[pyclass(module = "wyrd._wyrd.testing")]
pub struct WyrdTestServer {
    // Retained from the Python constructor signature; teardown wiring is not yet read here.
    #[allow(dead_code)]
    cleanup: bool,
    mutate_env: bool,
    base_url: Option<String>,
    api_key: Option<String>,
    tenant_id: Option<String>,
    server: Option<crate::server::WyrdTestServer>,
    env_snapshot: Option<EnvSnapshot>,
}

#[pymethods]
impl WyrdTestServer {
    #[new]
    #[pyo3(signature = (cleanup = true, mutate_env = true))]
    fn __new__(cleanup: bool, mutate_env: bool) -> Self {
        Self {
            cleanup,
            mutate_env,
            base_url: None,
            api_key: None,
            tenant_id: None,
            server: None,
            env_snapshot: None,
        }
    }

    fn __enter__(mut slf: PyRefMut<'_, Self>) -> PyResult<PyRefMut<'_, Self>> {
        let mutate_env = slf.mutate_env;

        let result: Result<
            (
                crate::server::WyrdTestServer,
                String,
                String,
                String,
                String,
            ),
            wyrd_spec::error::WyrdError,
        > = wyrd_runtime::runtime().block_on(async {
            let srv = crate::server::WyrdTestServer::start_bound()
                .await
                .map_err(wyrd_spec::error::WyrdError::from)?;
            let base_url = srv.base_url().unwrap_or("").to_owned();
            let grpc_url = srv.grpc_url().unwrap_or_default();
            let api_key = srv.api_key().expose_secret().to_owned();
            let tenant_id = srv.data_tenant_id().to_string();
            Ok((srv, base_url, grpc_url, api_key, tenant_id))
        });

        let (srv, base_url, grpc_url, api_key, tenant_id) = result.map_err(wyrd_error_to_py_err)?;

        if mutate_env {
            let _guard = env_mutex().lock().unwrap_or_else(|p| p.into_inner());
            let snapshot = EnvSnapshot::capture();
            unsafe {
                std::env::set_var("WYRD_SERVER_URL", &base_url);
                std::env::set_var("WYRD_GRPC_URL", &grpc_url);
                std::env::set_var("WYRD_API_KEY", &api_key);
            }
            slf.env_snapshot = Some(snapshot);
        }

        slf.base_url = Some(base_url);
        slf.api_key = Some(api_key);
        slf.tenant_id = Some(tenant_id);
        slf.server = Some(srv);
        Ok(slf)
    }

    fn __exit__(
        &mut self,
        _exc_type: Option<Bound<'_, PyAny>>,
        _exc_value: Option<Bound<'_, PyAny>>,
        _traceback: Option<Bound<'_, PyAny>>,
    ) -> PyResult<bool> {
        if let Some(snapshot) = self.env_snapshot.take() {
            let _guard = env_mutex().lock().unwrap_or_else(|p| p.into_inner());
            snapshot.restore();
        }
        if let Some(srv) = self.server.take() {
            wyrd_runtime::runtime().block_on(async {
                let _ = srv.shutdown().await;
            });
        }
        Ok(false)
    }

    #[getter]
    fn base_url(&self) -> PyResult<String> {
        self.base_url.clone().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err(
                "WyrdTestServer not started (use as context manager)",
            )
        })
    }

    #[getter]
    fn api_key(&self) -> PyResult<String> {
        self.api_key
            .clone()
            .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("WyrdTestServer not started"))
    }

    #[getter]
    fn tenant_id(&self) -> PyResult<String> {
        self.tenant_id
            .clone()
            .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("WyrdTestServer not started"))
    }

    /// Bootstrap a service principal, returning its scoped API key string.
    ///
    /// Wraps [`crate::server::WyrdTestServer::bootstrap_service`]. `permissions`
    /// maps to the Rust `roles` slice at the boundary; an empty list creates a
    /// principal with no grants (useful for negative RBAC journeys). Must be
    /// called inside the context manager.
    #[pyo3(signature = (permissions, name = "svc"))]
    fn bootstrap_service(&self, permissions: Vec<String>, name: &str) -> PyResult<String> {
        let srv = self.server.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err(
                "WyrdTestServer not started (use as context manager)",
            )
        })?;
        let roles: Vec<&str> = permissions.iter().map(String::as_str).collect();
        let result: Result<crate::server::Bootstrap, wyrd_spec::error::WyrdError> =
            wyrd_runtime::runtime().block_on(async {
                srv.bootstrap_service(name, &roles)
                    .await
                    .map_err(wyrd_spec::error::WyrdError::from)
            });
        let bootstrap = result.map_err(wyrd_error_to_py_err)?;
        match bootstrap {
            crate::server::Bootstrap::Machine { api_key, .. } => {
                Ok(api_key.expose_secret().to_owned())
            }
            crate::server::Bootstrap::User { .. } => {
                Err(pyo3::exceptions::PyRuntimeError::new_err(
                    "expected Machine bootstrap from bootstrap_service",
                ))
            }
        }
    }
}

pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<WyrdTestServer>()
}
