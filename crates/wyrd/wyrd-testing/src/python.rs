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
use wyrd_utils::py::{WyrdPyError, WyrdPyResult};

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

    fn restore(self, py: Python<'_>) -> WyrdPyResult<()> {
        for (key, value) in [
            ("WYRD_SERVER_URL", self.server_url),
            ("WYRD_GRPC_URL", self.grpc_url),
            ("WYRD_API_KEY", self.api_key),
        ] {
            publish_env(py, key, value.as_deref())?;
        }
        Ok(())
    }
}

/// Publishes one harness endpoint variable to both environment views.
///
/// The process environment is what the Rust client tier resolves from, but
/// Python builds `os.environ` once at interpreter start and never re-reads
/// `environ`, so a `setenv` alone is invisible to the Python side of a test.
/// Both are written here so a Python caller and a Rust client agree on what
/// the harness published.
///
/// # Errors
///
/// Returns a Python error when `os.environ` cannot be reached; deleting an
/// absent key is not an error.
fn publish_env(py: Python<'_>, key: &str, value: Option<&str>) -> WyrdPyResult<()> {
    match value {
        Some(value) => {
            // SAFETY: every harness mutation holds `env_mutex`, and the
            // published values are owned `String`s with no interior nul.
            unsafe { std::env::set_var(key, value) };
            Ok(py.import("os")?.getattr("environ")?.set_item(key, value)?)
        }
        None => {
            unsafe { std::env::remove_var(key) };
            let environ = py.import("os")?.getattr("environ")?;
            if environ.call_method1("__contains__", (key,))?.is_truthy()? {
                environ.del_item(key)?;
            }
            Ok(())
        }
    }
}

/// Python context manager for the Wyrd in-process test server.
#[pyclass(module = "wyrd._wyrd.testing")]
pub struct WyrdTestServer {
    // Retained from the Python constructor signature; teardown wiring is not yet read here.
    cleanup: bool,
    mutate_env: bool,
    /// Whether the server's audit publisher retires staged audit rows.
    ///
    /// Off for a journey that counts staged decisions, such as
    /// [`WyrdTestServer::table_describe_count`], so the count cannot shrink.
    audit_publication: bool,
    /// Whether the server composes the verification runtime.
    ///
    /// On for a journey whose Verifier baselines must fit and whose runs must
    /// execute, off by default so queue-driving journeys are not raced.
    verification_runtime: bool,
    base_url: Option<String>,
    api_key: Option<String>,
    tenant_id: Option<String>,
    server: Option<crate::server::WyrdTestServer>,
    env_snapshot: Option<EnvSnapshot>,
}

#[pymethods]
impl WyrdTestServer {
    #[new]
    #[pyo3(signature = (
        cleanup = true,
        mutate_env = true,
        audit_publication = true,
        verification_runtime = false
    ))]
    fn __new__(
        cleanup: bool,
        mutate_env: bool,
        audit_publication: bool,
        verification_runtime: bool,
    ) -> Self {
        Self {
            cleanup,
            mutate_env,
            audit_publication,
            verification_runtime,
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
    fn __enter__(mut slf: PyRefMut<'_, Self>) -> WyrdPyResult<PyRefMut<'_, Self>> {
        let mutate_env = slf.mutate_env;
        let audit_publication = slf.audit_publication;
        let verification_runtime = slf.verification_runtime;

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
            let builder = crate::server::WyrdTestServer::builder();
            let builder = if audit_publication {
                builder
            } else {
                builder.without_audit_publication_for_test()
            };
            let builder = if verification_runtime {
                builder.with_verification_runtime_for_test()
            } else {
                builder
            };
            let srv = builder
                .start_bound()
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

        let (srv, base_url, grpc_url, api_key, tenant_id) = result.map_err(WyrdPyError::from)?;

        if mutate_env {
            let _guard = env_mutex().lock().unwrap_or_else(|p| p.into_inner());
            let snapshot = EnvSnapshot::capture();
            let py = slf.py();
            publish_env(py, "WYRD_SERVER_URL", Some(&base_url))?;
            publish_env(py, "WYRD_GRPC_URL", Some(&grpc_url))?;
            publish_env(py, "WYRD_API_KEY", Some(&api_key))?;
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
        py: Python<'_>,
        _exc_type: Option<Bound<'_, PyAny>>,
        _exc_value: Option<Bound<'_, PyAny>>,
        _traceback: Option<Bound<'_, PyAny>>,
    ) -> WyrdPyResult<bool> {
        if let Some(snapshot) = self.env_snapshot.take() {
            let _guard = env_mutex().lock().unwrap_or_else(|p| p.into_inner());
            snapshot.restore(py)?;
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
    fn base_url(&self) -> WyrdPyResult<String> {
        self.base_url.clone().ok_or_else(not_started)
    }

    #[getter]
    fn api_key(&self) -> WyrdPyResult<String> {
        self.api_key.clone().ok_or_else(not_started)
    }

    #[getter]
    fn tenant_id(&self) -> WyrdPyResult<String> {
        self.tenant_id.clone().ok_or_else(not_started)
    }

    /// Exchanges the harness's retained API key for a bearer access token.
    ///
    /// Wraps [`crate::server::WyrdTestServer::exchange_api_key`] so a Python
    /// test can hand an unmodified OTLP exporter the `x-wyrd-access-token`
    /// header it needs. The key itself never leaves the harness.
    ///
    /// # Errors
    ///
    /// Raises a Python runtime error when the context manager is inactive, and
    /// a Wyrd Python error when the exchange route refuses the key.
    fn access_token(&self) -> WyrdPyResult<String> {
        let srv = self.server.as_ref().ok_or_else(not_started)?;
        let api_key = self.api_key.as_ref().ok_or_else(not_started)?;
        let key = secrecy::SecretString::from(api_key.clone());
        let result: Result<String, wyrd_spec::error::WyrdError> =
            wyrd_runtime::runtime().block_on(async {
                srv.exchange_api_key(&key)
                    .await
                    .map_err(wyrd_spec::error::WyrdError::from)
            });
        result.map_err(WyrdPyError::from)
    }

    /// Bootstrap a service principal, returning its scoped API key string.
    ///
    /// Wraps [`crate::server::WyrdTestServer::bootstrap_service`]. `roles` is a
    /// list of Wyrd built-in role names; an empty list creates a principal with no
    /// grants (useful for negative RBAC journeys). Must be called inside the context
    /// manager.
    #[pyo3(signature = (roles, name = "svc"))]
    fn bootstrap_service(&self, roles: Vec<String>, name: &str) -> WyrdPyResult<String> {
        let srv = self.server.as_ref().ok_or_else(not_started)?;
        let roles: Vec<&str> = roles.iter().map(String::as_str).collect();
        let result: Result<crate::server::Bootstrap, wyrd_spec::error::WyrdError> =
            wyrd_runtime::runtime().block_on(async {
                srv.bootstrap_service(name, &roles)
                    .await
                    .map_err(wyrd_spec::error::WyrdError::from)
            });
        let bootstrap = result.map_err(WyrdPyError::from)?;
        match bootstrap {
            crate::server::Bootstrap::Machine { api_key, .. } => {
                Ok(api_key.expose_secret().to_owned())
            }
            crate::server::Bootstrap::User { .. } => Err(WyrdPyError::from(harness_error(
                "expected Machine bootstrap from bootstrap_service",
            ))),
        }
    }

    /// Issue an API key for the principal a registered Service Card projects.
    ///
    /// Wraps [`crate::server::WyrdTestServer::credential_registered_service`].
    /// `card_ref` is the canonical `space/Kind/name@version` identity string a
    /// registration receipt returns; the Card must already be registered, since
    /// this credentials the service account registration projected for it
    /// rather than minting a new one. A scoped-observation journey needs this
    /// key: only a writer carrying the registered Service's card-ref scope may
    /// stamp a component Card as an observation subject.
    ///
    /// # Errors
    ///
    /// Raises a Wyrd Python error when the context manager is inactive, the
    /// identity string is malformed, or the Card has no projected principal.
    fn credential_registered_service(
        &self,
        card_ref: &str,
        roles: Vec<String>,
    ) -> WyrdPyResult<String> {
        let srv = self.server.as_ref().ok_or_else(not_started)?;
        let parsed: wyrd_spec::reference::CardRef = card_ref.parse().map_err(|error| {
            WyrdPyError::from(harness_error(format!(
                "`{card_ref}` is not a card identity string: {error}"
            )))
        })?;
        let roles: Vec<&str> = roles.iter().map(String::as_str).collect();
        let bootstrap = wyrd_runtime::runtime()
            .block_on(srv.credential_registered_service(&parsed, &roles))
            .map_err(wyrd_spec::error::WyrdError::from)
            .map_err(WyrdPyError::from)?;
        match bootstrap {
            crate::server::Bootstrap::Machine { api_key, .. } => {
                Ok(api_key.expose_secret().to_owned())
            }
            crate::server::Bootstrap::User { .. } => Err(WyrdPyError::from(harness_error(
                "expected Machine bootstrap from credential_registered_service",
            ))),
        }
    }

    /// Materialize one canonical built-in table for the fixture tenant.
    ///
    /// A canonical signal table is created on first use. An OTLP export
    /// provisions it on ingest, but a journey that writes it through the public
    /// Arrow batch door must ask for it first.
    ///
    /// # Errors
    ///
    /// Raises a Wyrd Python error when the context manager is inactive, no
    /// built-in owns `namespace.name`, or the catalog cannot materialize it.
    fn ensure_builtin_table(&self, namespace: &str, name: &str) -> WyrdPyResult<()> {
        let srv = self.server.as_ref().ok_or_else(not_started)?;
        wyrd_runtime::runtime()
            .block_on(srv.ensure_builtin_table_for_test(srv.data_tenant_id(), namespace, name))
            .map_err(WyrdPyError::from)
    }

    /// Mint an API key for a principal holding exactly `permissions`.
    ///
    /// `permissions` are `resource:action` strings. This is the door a journey
    /// uses to prove an access gate from the caller's side: it seeds one role
    /// carrying only those grants and bootstraps a service onto it.
    ///
    /// # Errors
    ///
    /// Raises `ValueError` for an unparsable permission, and a Wyrd Python
    /// error when the context manager is inactive or role seeding or
    /// bootstrapping fails.
    fn scoped_api_key(&self, role: &str, permissions: Vec<String>) -> WyrdPyResult<String> {
        let srv = self.server.as_ref().ok_or_else(not_started)?;
        let parsed = permissions
            .iter()
            .map(|value| {
                value.parse::<wyrd_runtime::Permission>().map_err(|_| {
                    WyrdPyError::from(harness_error(format!(
                        "`{value}` is not a resource:action permission"
                    )))
                })
            })
            .collect::<WyrdPyResult<Vec<_>>>()?;
        let result: Result<crate::server::Bootstrap, wyrd_spec::error::WyrdError> =
            wyrd_runtime::runtime().block_on(async {
                srv.seed_role(role, &parsed)
                    .await
                    .map_err(wyrd_spec::error::WyrdError::from)?;
                srv.bootstrap_service(role, &[role])
                    .await
                    .map_err(wyrd_spec::error::WyrdError::from)
            });
        match result.map_err(WyrdPyError::from)? {
            crate::server::Bootstrap::Machine { api_key, .. } => {
                Ok(api_key.expose_secret().to_owned())
            }
            crate::server::Bootstrap::User { .. } => Err(WyrdPyError::from(harness_error(
                "expected a machine bootstrap",
            ))),
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
    fn prepare_oracle_query_fixture(&self, fused: bool) -> WyrdPyResult<(String, String)> {
        let srv = self.server.as_ref().ok_or_else(not_started)?;
        wyrd_runtime::runtime()
            .block_on(prepare_oracle_query_fixture(srv, fused))
            .map_err(WyrdPyError::from)
    }

    /// Provision a second tenant so a journey can prove cross-tenant isolation.
    ///
    /// The tenant is seeded through the same operator path the Rust harness
    /// uses, so a Python journey observes the production tenancy boundary
    /// rather than a fixture-only one.
    ///
    /// # Errors
    ///
    /// Raises a Wyrd Python error when the context manager is inactive or the
    /// tenant cannot be seeded.
    #[pyo3(signature = (slug))]
    fn seed_tenant(&self, slug: &str) -> WyrdPyResult<String> {
        let srv = self.server.as_ref().ok_or_else(not_started)?;
        let tenant_id = wyrd_runtime::runtime()
            .block_on(srv.seed_tenant(slug))
            .map_err(WyrdPyError::from)?;
        Ok(tenant_id.to_string())
    }

    /// Bootstrap a service principal under an explicit tenant.
    ///
    /// Pairs with [`seed_tenant`](Self::seed_tenant): the returned API key is
    /// the caller identity a cross-tenant journey uses to prove that the other
    /// tenant's Cards are unreachable.
    ///
    /// # Errors
    ///
    /// Raises a Wyrd Python error when the context manager is inactive, the
    /// tenant id does not parse, bootstrapping fails, or the bootstrap is not a
    /// machine principal.
    #[pyo3(signature = (tenant_id, roles, name = "svc"))]
    fn bootstrap_service_in_tenant(
        &self,
        tenant_id: &str,
        roles: Vec<String>,
        name: &str,
    ) -> WyrdPyResult<String> {
        let srv = self.server.as_ref().ok_or_else(not_started)?;
        let tenant_id = tenant_id
            .parse::<wyrd_spec::DataTenantId>()
            .map_err(|error| {
                WyrdPyError::from(harness_error(format!("invalid tenant id: {error}")))
            })?;
        let roles: Vec<&str> = roles.iter().map(String::as_str).collect();
        let bootstrap = wyrd_runtime::runtime()
            .block_on(srv.bootstrap_service_in_tenant(tenant_id, name, &roles))
            .map_err(WyrdPyError::from)?;
        match bootstrap {
            crate::server::Bootstrap::Machine { api_key, .. } => {
                Ok(api_key.expose_secret().to_owned())
            }
            crate::server::Bootstrap::User { .. } => Err(WyrdPyError::from(harness_error(
                "expected a machine bootstrap from bootstrap_service_in_tenant",
            ))),
        }
    }

    /// Flush the server-owned Scribe after a public client drain.
    ///
    /// # Errors
    /// Raises a Wyrd Python error when the context manager is inactive or the
    /// production Scribe seal path cannot commit its buffered rows.
    fn flush_bifrost(&self) -> WyrdPyResult<()> {
        let server = self.server.as_ref().ok_or_else(not_started)?;
        wyrd_runtime::runtime()
            .block_on(server.flush_bifrost())
            .map_err(WyrdPyError::from)
    }

    /// Bring binding `binding_id`'s schedule cursor to database time, so the
    /// verification runtime schedules its next occurrence now.
    ///
    /// # Errors
    /// Raises a Wyrd Python error when the context manager is inactive,
    /// `binding_id` is not a binding ID, or the update fails.
    fn make_binding_due(&self, binding_id: &str) -> WyrdPyResult<()> {
        let server = self.server.as_ref().ok_or_else(not_started)?;
        let binding = binding_id.parse().map_err(harness_error)?;
        wyrd_runtime::runtime()
            .block_on(async {
                server
                    .verification_fixture()
                    .await?
                    .make_binding_due(binding)
                    .await
            })
            .map_err(|error| WyrdPyError::from(harness_error(error)))
    }

    /// Every verification run ID of the fixture tenant, oldest first.
    ///
    /// # Errors
    /// Raises a Wyrd Python error when the context manager is inactive or the
    /// runs cannot be read.
    fn verification_runs(&self) -> WyrdPyResult<Vec<String>> {
        let server = self.server.as_ref().ok_or_else(not_started)?;
        let runs = wyrd_runtime::runtime()
            .block_on(async { server.verification_fixture().await?.runs().await })
            .map_err(|error| WyrdPyError::from(harness_error(error)))?;
        Ok(runs.iter().map(ToString::to_string).collect())
    }

    /// Strip the fitted-profile format from Verifier `verifier_uid`'s ready
    /// baseline, as a baseline fitted under earlier semantics is stored.
    ///
    /// # Errors
    /// Raises a Wyrd Python error when the context manager is inactive,
    /// `verifier_uid` is not a Card UID, or no ready baseline exists.
    fn retire_fitted_format(&self, verifier_uid: &str) -> WyrdPyResult<()> {
        let server = self.server.as_ref().ok_or_else(not_started)?;
        let verifier = verifier_uid.parse().map_err(harness_error)?;
        wyrd_runtime::runtime()
            .block_on(async {
                server
                    .verification_fixture()
                    .await?
                    .retire_fitted_format(&verifier)
                    .await
            })
            .map_err(|error| WyrdPyError::from(harness_error(error)))
    }

    /// Truncate the next query after its schema frame in the real server.
    ///
    /// # Errors
    ///
    /// Raises `RuntimeError` when the context manager is inactive.
    fn fail_next_query_after_schema(&self) -> WyrdPyResult<()> {
        self.server
            .as_ref()
            .ok_or_else(not_started)?
            .fail_next_query_after_schema();
        Ok(())
    }

    /// Truncate the next query after its first batch frame in the real server.
    ///
    /// # Errors
    ///
    /// Raises `RuntimeError` when the context manager is inactive.
    fn fail_next_query_after_batch(&self) -> WyrdPyResult<()> {
        self.server
            .as_ref()
            .ok_or_else(not_started)?
            .fail_next_query_after_batch();
        Ok(())
    }

    /// Stall the next query after its schema for deterministic cancellation.
    ///
    /// # Errors
    ///
    /// Raises `RuntimeError` when the context manager is inactive.
    fn stall_next_query_after_schema(&self) -> WyrdPyResult<()> {
        self.server
            .as_ref()
            .ok_or_else(not_started)?
            .stall_next_query_after_schema();
        Ok(())
    }

    /// Wait until the real response body reaches its notification-backed stall.
    ///
    /// # Errors
    ///
    /// Raises a Wyrd Python error when no stall is scheduled or the configured
    /// server drain deadline expires.
    fn wait_query_schema_stall(&self, py: Python<'_>) -> WyrdPyResult<String> {
        let server = self.server.as_ref().ok_or_else(not_started)?;
        py.detach(|| wyrd_runtime::runtime().block_on(server.wait_query_schema_stall()))
            .map_err(WyrdPyError::from)
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
    ) -> WyrdPyResult<std::collections::BTreeMap<String, u64>> {
        let server = self.server.as_ref().ok_or_else(not_started)?;
        let snapshot = server
            .bifrost_query_resource_snapshot(query_id)
            .map_err(WyrdPyError::from)?;
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
    ) -> WyrdPyResult<std::collections::BTreeMap<String, u64>> {
        let server = self.server.as_ref().ok_or_else(not_started)?;
        let baseline = query_resource_snapshot_from_map(&baseline)?;
        let snapshot = py
            .detach(|| {
                wyrd_runtime::runtime()
                    .block_on(server.wait_bifrost_query_resources_released(query_id, baseline))
            })
            .map_err(WyrdPyError::from)?;
        Ok(query_resource_snapshot_to_map(snapshot))
    }

    /// Mint an authenticated token lacking `bifrost_query:read`.
    ///
    /// # Errors
    ///
    /// Raises a Wyrd error when the context manager is inactive or token
    /// issuance fails.
    fn query_denied_token(&self) -> WyrdPyResult<String> {
        let server = self.server.as_ref().ok_or_else(not_started)?;
        wyrd_runtime::runtime()
            .block_on(server.query_denied_token())
            .map_err(WyrdPyError::from)
    }

    /// Return the server-observed describe count for `fqn` in the fixture tenant.
    ///
    /// Construct the server with `audit_publication=False`, or the publisher
    /// retires the staged decisions this counts.
    ///
    /// # Errors
    ///
    /// Raises a Wyrd error when the context manager is inactive or the audit
    /// query fails.
    fn table_describe_count(&self, fqn: &str) -> WyrdPyResult<i64> {
        let server = self.server.as_ref().ok_or_else(not_started)?;
        wyrd_runtime::runtime()
            .block_on(server.table_describe_count(fqn))
            .map_err(WyrdPyError::from)
    }

    /// Make every fixture-tenant describe of `fqn` fail until restored.
    ///
    /// # Errors
    ///
    /// Raises a Wyrd error when the context manager is inactive, `fqn` is not a
    /// plain table name, or the fault cannot be installed.
    fn fail_table_describe(&self, fqn: &str) -> WyrdPyResult<()> {
        let server = self.server.as_ref().ok_or_else(not_started)?;
        wyrd_runtime::runtime()
            .block_on(server.fail_table_describe(fqn))
            .map_err(WyrdPyError::from)
    }

    /// Remove the fault `fail_table_describe` installed, if any.
    ///
    /// # Errors
    ///
    /// Raises a Wyrd error when the context manager is inactive or the fault
    /// cannot be removed.
    fn restore_table_describe(&self) -> WyrdPyResult<()> {
        let server = self.server.as_ref().ok_or_else(not_started)?;
        wyrd_runtime::runtime()
            .block_on(server.restore_table_describe())
            .map_err(WyrdPyError::from)
    }

    /// Return the fixture tenant's durable Oracle read-decision count.
    ///
    /// # Errors
    ///
    /// Raises a Wyrd error when the context manager is inactive or the audit
    /// query fails.
    fn bifrost_read_decision_count(&self) -> WyrdPyResult<i64> {
        let server = self.server.as_ref().ok_or_else(not_started)?;
        wyrd_runtime::runtime()
            .block_on(server.bifrost_read_decision_count())
            .map_err(WyrdPyError::from)
    }

    /// Wait until no Oracle audit outbox commit is still in flight.
    ///
    /// Returns the residual pending count, which is `0` once every read
    /// decision is staged. A journey asserting on read-decision staging calls
    /// this first because Oracle stages decisions from a background task.
    ///
    /// # Errors
    ///
    /// Raises a Wyrd error when the context manager is inactive or this server
    /// does not host an Oracle role.
    #[pyo3(signature = (budget_ms=5000))]
    fn wait_oracle_audit_staged(&self, py: Python<'_>, budget_ms: u64) -> WyrdPyResult<u64> {
        let server = self.server.as_ref().ok_or_else(not_started)?;
        py.detach(|| {
            wyrd_runtime::runtime().block_on(
                server.wait_oracle_audit_staged(std::time::Duration::from_millis(budget_ms)),
            )
        })
        .map_err(WyrdPyError::from)
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
/// Raises `WYRD_TESTING_500_HARNESS_START` when a required counter is absent or
/// does not fit the platform's native count width.
fn query_resource_snapshot_from_map(
    baseline: &std::collections::BTreeMap<String, u64>,
) -> WyrdPyResult<crate::server::BifrostQueryResourceSnapshot> {
    let value = |name: &str| {
        baseline.get(name).copied().ok_or_else(|| {
            WyrdPyError::from(harness_error(format!(
                "query resource baseline is missing {name}"
            )))
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
            credential: Some(api_key.clone()),
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
impl From<crate::server::WyrdTestServerError> for WyrdPyError {
    /// Project a harness failure onto the shared Wyrd boundary adapter.
    fn from(error: crate::server::WyrdTestServerError) -> Self {
        Self::from(wyrd_spec::error::WyrdError::from(error))
    }
}

fn harness_error(error: impl std::fmt::Display) -> wyrd_spec::error::WyrdError {
    wyrd_spec::error::WyrdError::HarnessStart {
        message: error.to_string(),
        details: serde_json::json!({}),
    }
}

/// Register the Python-visible test-server classes on `module`.
///
/// # Errors
/// Returns the PyO3 error raised when a class cannot be added.
pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<WyrdTestServer>()
}

/// Build the harness failure raised when the server has not been started.
///
/// Every accessor on [`WyrdTestServerPy`] requires the context manager to be
/// entered first, so they share one catalog-backed failure instead of raising a
/// bare Python exception.
fn not_started() -> WyrdPyError {
    WyrdPyError::from(harness_error(
        "WyrdTestServer not started (use as context manager)",
    ))
}
