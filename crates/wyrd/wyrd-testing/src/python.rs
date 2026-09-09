//! Python context manager surface for [`WyrdTestServer`].

use std::sync::{Mutex, OnceLock};

use arrow::datatypes::{DataType, Field, Schema};
use pyo3::prelude::*;
use pyo3::types::PyAny;
use secrecy::ExposeSecret;
use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::scribe::tail_rpc::{LocalTailReadTransport, TailReadTransport};
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
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

    /// Start a bound server, bootstrap its writer service key, and optionally
    /// publish the endpoints through the documented `WYRD_*` environment vars.
    ///
    /// # Errors
    /// Returns a Python error when server startup, service bootstrap, or
    /// environment setup cannot complete.
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
            let bootstrap = srv
                .bootstrap_service("python-integration-writer", &["admin"])
                .await
                .map_err(wyrd_spec::error::WyrdError::from)?;
            let api_key = match bootstrap {
                crate::server::Bootstrap::Machine { api_key, .. } => {
                    api_key.expose_secret().to_owned()
                }
                crate::server::Bootstrap::User { .. } => {
                    return Err(wyrd_spec::error::WyrdError::HarnessStart {
                        message: "Python test server writer bootstrap returned a user".to_owned(),
                        details: serde_json::json!({}),
                    });
                }
            };
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
        if !self.cleanup {
            tracing::warn!(
                "WyrdTestServer: cleanup=false is not yet implemented; \
                 the server and embedded Postgres will still be cleaned up"
            );
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
            pyo3::exceptions::PyRuntimeError::new_err(
                "WyrdTestServer not started (use as context manager)",
            )
        })
    }

    #[getter]
    fn tenant_id(&self) -> PyResult<String> {
        self.tenant_id.clone().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err(
                "WyrdTestServer not started (use as context manager)",
            )
        })
    }

    /// Bootstrap a service principal, returning its scoped API key string.
    ///
    /// Wraps [`crate::server::WyrdTestServer::bootstrap_service`]. `roles` is a
    /// list of Wyrd built-in role names; an empty list creates a principal with no
    /// grants (useful for negative RBAC journeys). Must be called inside the context
    /// manager.
    #[pyo3(signature = (roles, name = "svc"))]
    fn bootstrap_service(&self, roles: Vec<String>, name: &str) -> PyResult<String> {
        let srv = self.server.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err(
                "WyrdTestServer not started (use as context manager)",
            )
        })?;
        let roles: Vec<&str> = roles.iter().map(String::as_str).collect();
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

    /// Creates a real sealed Oracle fixture and returns its table and access token.
    ///
    /// The setup uses the public gRPC ingest transport, flushes Scribe, and
    /// exchanges the harness admin API key through the real auth route. The
    /// returned values are intended for public language-client integration tests.
    ///
    /// # Errors
    ///
    /// Raises a Wyrd Python error when the context manager is inactive or table
    /// registration, ingest, flush, token exchange, or Arrow encoding fails.
    #[pyo3(signature = (fused = false))]
    fn prepare_oracle_query_fixture(&self, fused: bool) -> PyResult<(String, String)> {
        let srv = self.server.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err(
                "WyrdTestServer not started (use as context manager)",
            )
        })?;
        wyrd_runtime::runtime()
            .block_on(prepare_oracle_query_fixture(srv, fused))
            .map_err(wyrd_error_to_py_err)
    }

    /// Flush the server-owned Scribe after a public client drain.
    ///
    /// # Errors
    /// Raises a Wyrd Python error when the context manager is inactive or the
    /// production Scribe seal path cannot commit its buffered rows.
    fn flush_bifrost(&self) -> PyResult<()> {
        let server = self.server.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("WyrdTestServer not started")
        })?;
        wyrd_runtime::runtime()
            .block_on(server.flush_bifrost())
            .map_err(|error| wyrd_error_to_py_err(error.into()))
    }

    /// Truncate the next query after its schema frame in the real server.
    ///
    /// # Errors
    ///
    /// Raises `RuntimeError` when the context manager is inactive.
    fn fail_next_query_after_schema(&self) -> PyResult<()> {
        self.server
            .as_ref()
            .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("WyrdTestServer not started"))?
            .fail_next_query_after_schema();
        Ok(())
    }

    /// Truncate the next query after its first batch frame in the real server.
    ///
    /// # Errors
    ///
    /// Raises `RuntimeError` when the context manager is inactive.
    fn fail_next_query_after_batch(&self) -> PyResult<()> {
        self.server
            .as_ref()
            .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("WyrdTestServer not started"))?
            .fail_next_query_after_batch();
        Ok(())
    }

    /// Stall the next query after its schema for deterministic cancellation.
    ///
    /// # Errors
    ///
    /// Raises `RuntimeError` when the context manager is inactive.
    fn stall_next_query_after_schema(&self) -> PyResult<()> {
        self.server
            .as_ref()
            .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("WyrdTestServer not started"))?
            .stall_next_query_after_schema();
        Ok(())
    }

    /// Wait until the real response body reaches its notification-backed stall.
    ///
    /// # Errors
    ///
    /// Raises a Wyrd Python error when no stall is scheduled or the configured
    /// server drain deadline expires.
    fn wait_query_schema_stall(&self, py: Python<'_>) -> PyResult<String> {
        let server = self.server.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("WyrdTestServer not started")
        })?;
        py.detach(|| wyrd_runtime::runtime().block_on(server.wait_query_schema_stall()))
            .map_err(|error| wyrd_error_to_py_err(error.into()))
    }

    /// Return exact admission, memory, peer-slot, and tail-fence counts.
    ///
    /// # Errors
    ///
    /// Raises a Wyrd Python error when production-shaped resource owners cannot
    /// provide an exact snapshot.
    fn bifrost_query_resource_snapshot(
        &self,
        query_id: &str,
    ) -> PyResult<std::collections::BTreeMap<String, u64>> {
        let server = self.server.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("WyrdTestServer not started")
        })?;
        let snapshot = server
            .bifrost_query_resource_snapshot(query_id)
            .map_err(|error| wyrd_error_to_py_err(error.into()))?;
        Ok(query_resource_snapshot_to_map(snapshot))
    }

    /// Wait until all exact query resources equal the supplied baseline.
    ///
    /// # Errors
    ///
    /// Raises `ValueError` for a malformed baseline and a Wyrd Python error
    /// when notification-backed release exceeds `shutdown.drain_ms`.
    fn wait_bifrost_query_resources_released(
        &self,
        py: Python<'_>,
        query_id: &str,
        baseline: std::collections::BTreeMap<String, u64>,
    ) -> PyResult<std::collections::BTreeMap<String, u64>> {
        let server = self.server.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("WyrdTestServer not started")
        })?;
        let baseline = query_resource_snapshot_from_map(&baseline)?;
        let snapshot = py
            .detach(|| {
                wyrd_runtime::runtime()
                    .block_on(server.wait_bifrost_query_resources_released(query_id, baseline))
            })
            .map_err(|error| wyrd_error_to_py_err(error.into()))?;
        Ok(query_resource_snapshot_to_map(snapshot))
    }

    /// Mint an authenticated token lacking `bifrost_query:read`.
    ///
    /// # Errors
    ///
    /// Raises a Wyrd error when the context manager is inactive or token
    /// issuance fails.
    fn query_denied_token(&self) -> PyResult<String> {
        let server = self.server.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("WyrdTestServer not started")
        })?;
        wyrd_runtime::runtime()
            .block_on(server.query_denied_token())
            .map_err(|error| wyrd_error_to_py_err(error.into()))
    }

    /// Return the fixture tenant's durable Oracle read-decision count.
    ///
    /// # Errors
    ///
    /// Raises a Wyrd error when the context manager is inactive or the audit
    /// query fails.
    fn bifrost_read_decision_count(&self) -> PyResult<i64> {
        let server = self.server.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("WyrdTestServer not started")
        })?;
        wyrd_runtime::runtime()
            .block_on(server.bifrost_read_decision_count())
            .map_err(|error| wyrd_error_to_py_err(error.into()))
    }
}

/// Projects one exact Rust resource snapshot into a Python dictionary.
fn query_resource_snapshot_to_map(
    snapshot: crate::server::BifrostQueryResourceSnapshot,
) -> std::collections::BTreeMap<String, u64> {
    std::collections::BTreeMap::from([
        ("admission_slots".to_owned(), snapshot.admission_slots),
        ("memory_bytes".to_owned(), snapshot.memory_bytes),
        ("peer_slots".to_owned(), snapshot.peer_slots),
        ("tail_fences".to_owned(), snapshot.tail_fences),
    ])
}

/// Parses one Python baseline dictionary into exact Rust resource counts.
///
/// # Errors
///
/// Raises `ValueError` when a required counter is absent or does not fit the
/// platform's native count width.
fn query_resource_snapshot_from_map(
    baseline: &std::collections::BTreeMap<String, u64>,
) -> PyResult<crate::server::BifrostQueryResourceSnapshot> {
    let value = |name: &str| {
        baseline.get(name).copied().ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!(
                "query resource baseline is missing {name}"
            ))
        })
    };
    Ok(crate::server::BifrostQueryResourceSnapshot {
        admission_slots: value("admission_slots")?,
        memory_bytes: value("memory_bytes")?,
        peer_slots: value("peer_slots")?,
        tail_fences: value("tail_fences")?,
    })
}

/// Builds the Python query journey's real ingest-to-sealed prerequisite.
///
/// # Errors
///
/// Returns a Wyrd error when table registration, Arrow encoding, client
/// construction, gRPC ingest, Scribe flush, or token exchange fails.
async fn prepare_oracle_query_fixture(
    srv: &crate::server::WyrdTestServer,
    fused: bool,
) -> Result<(String, String), wyrd_spec::error::WyrdError> {
    let bootstrap = srv
        .bootstrap_service(
            &format!("python-oracle-query-{}", uuid::Uuid::now_v7().simple()),
            &["admin"],
        )
        .await
        .map_err(wyrd_spec::error::WyrdError::from)?;
    let api_key = bootstrap
        .api_key()
        .ok_or_else(|| harness_error("Oracle fixture bootstrap did not return an API key"))?;
    let table_name = format!("python_oracle_{}", uuid::Uuid::now_v7().simple());
    let table_fqn = format!("vala.bifrost.{table_name}");
    let schema = std::sync::Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("value", DataType::Utf8, false),
    ]));
    srv.state()
        .bifrost_catalog()
        .expect("test server exposes its Bifrost catalog")
        .create_table(CreateTableRequest {
            table: TableRef::new(BifrostNamespace::Bifrost, &table_name),
            user_fields: schema
                .fields()
                .iter()
                .map(|field| field.as_ref().clone())
                .collect(),
            tenant: srv.data_tenant_id(),
            physical_layout: None,
            audit: None,
        })
        .await
        .map_err(harness_error)?;
    let writer = crate::bifrost::write::BifrostWriter::connect(
        ClientConfig {
            grpc: GrpcConfig {
                endpoint: srv.grpc_url().unwrap_or_default(),
                connect_retries: 0,
                ..GrpcConfig::default()
            },
            http: HttpConfig {
                base_url: srv.base_url().unwrap_or_default().to_owned(),
                ..HttpConfig::default()
            },
            api_key: Some(api_key.clone()),
            ..ClientConfig::default()
        },
        bootstrap
            .card_ref()
            .ok_or_else(|| harness_error("Oracle fixture bootstrap is not a machine principal"))?
            .clone(),
    )
    .await?;
    writer
        .write(
            &table_fqn,
            &schema,
            [
                br#"{"id": 1, "value": "first"}"#.to_vec(),
                br#"{"id": 2, "value": "second"}"#.to_vec(),
            ],
        )
        .await?;
    let ingest = srv
        .state()
        .bifrost_ingest()
        .ok_or_else(|| harness_error("Scribe runtime is unavailable"))?;
    let stream = ingest
        .scribe()
        .tail_service()
        .map_err(harness_error)?
        .stream();
    let writer_epoch = u64::try_from(stream.writer_epoch.as_i64()).map_err(harness_error)?;
    let time_partition = vala_bifrost_redux::catalog::TimeGranularity::Hour
        .bucket(chrono::Utc::now())
        .map_err(harness_error)?
        .to_wire();
    let tail_transport: std::sync::Arc<dyn TailReadTransport> =
        std::sync::Arc::new(LocalTailReadTransport::new(ingest.tail_reader()));
    let oracle = srv
        .state()
        .bifrost_query()
        .ok_or_else(|| harness_error("Oracle runtime is unavailable"))?
        .oracle();
    if fused {
        oracle.prefer_local_tail_routes_for_test();
    }
    oracle.tail_transports().insert_live_stream_for_tenant(
        srv.data_tenant_id(),
        &table_fqn,
        wyrd_spec::vala::api::NodeId::new(stream.node_id.as_uuid()),
        writer_epoch,
        time_partition,
        tail_transport,
    );
    if !fused {
        srv.flush_bifrost()
            .await
            .map_err(wyrd_spec::error::WyrdError::from)?;
    }
    writer
        .write(
            &table_fqn,
            &schema,
            [br#"{"id": 3, "value": "live"}"#.to_vec()],
        )
        .await?;
    let token = srv
        .exchange_api_key(api_key)
        .await
        .map_err(wyrd_spec::error::WyrdError::from)?;
    Ok((table_fqn, token))
}

/// Converts one fixture setup failure into the stable test-harness catalog.
fn harness_error(error: impl std::fmt::Display) -> wyrd_spec::error::WyrdError {
    wyrd_spec::error::WyrdError::HarnessStart {
        message: error.to_string(),
        details: serde_json::json!({}),
    }
}

pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<WyrdTestServer>()
}
