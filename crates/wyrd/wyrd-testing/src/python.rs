//! Python context manager surface for [`WyrdTestServer`].

use std::sync::{Mutex, OnceLock};

use pyo3::prelude::*;
use pyo3::types::PyAny;
use wyrd_utils::py::wyrd_error_to_py_err;

static ENV_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

fn env_mutex() -> &'static Mutex<()> {
    ENV_MUTEX.get_or_init(|| Mutex::new(()))
}

struct EnvSnapshot {
    api_url: Option<String>,
    api_key: Option<String>,
}

impl EnvSnapshot {
    fn capture() -> Self {
        Self {
            api_url: std::env::var("WYRD_API_URL").ok(),
            api_key: std::env::var("WYRD_API_KEY").ok(),
        }
    }

    fn restore(self) {
        match self.api_url {
            Some(v) => unsafe { std::env::set_var("WYRD_API_URL", &v) },
            None => unsafe { std::env::remove_var("WYRD_API_URL") },
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
            (crate::server::WyrdTestServer, String, String),
            wyrd_spec::error::WyrdError,
        > = wyrd_runtime::runtime().block_on(async {
            let srv = crate::server::WyrdTestServer::start_bound()
                .await
                .map_err(wyrd_spec::error::WyrdError::from)?;
            let base_url = srv.base_url().unwrap_or("").to_owned();
            let tenant_id = srv.data_tenant_id().to_string();
            Ok((srv, base_url, tenant_id))
        });

        let (srv, base_url, tenant_id) = result.map_err(wyrd_error_to_py_err)?;

        if mutate_env {
            let _guard = env_mutex().lock().unwrap_or_else(|p| p.into_inner());
            let snapshot = EnvSnapshot::capture();
            unsafe {
                std::env::set_var("WYRD_API_URL", &base_url);
            }
            slf.env_snapshot = Some(snapshot);
        }

        slf.base_url = Some(base_url);
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
        self.api_key.clone().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("WyrdTestServer not started")
        })
    }

    #[getter]
    fn tenant_id(&self) -> PyResult<String> {
        self.tenant_id.clone().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("WyrdTestServer not started")
        })
    }
}

pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<WyrdTestServer>()
}
