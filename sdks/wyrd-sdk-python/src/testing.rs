//! `PyO3` boundary for the in-process Wyrd test server, `wyrd.testing`.
//!
//! [`WyrdTestServer`] is a Python context manager over the Rust-native
//! [`TestServer`] harness. Each method converts Python arguments, blocks on the
//! shared Wyrd runtime for the owning harness operation, and projects the
//! result back to plain Python values; server composition, fixtures, and
//! lifecycle stay in `wyrd-testing`. The module exists only with the SDK's
//! `testing` feature, so the production wheel never exposes it.

use std::collections::BTreeMap;
use std::fmt::Display;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use pyo3::prelude::*;
use pyo3::types::{PyAny, PyModule};
use secrecy::{ExposeSecret, SecretString};
use url::Url;
use wyrd_runtime::Permission;
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;
use wyrd_testing::human_login::{HUMAN_PUBLIC_ORIGIN, HumanSso};
use wyrd_testing::{Bootstrap, WyrdTestServer as TestServer, WyrdTestServerError};
use wyrd_utils::py::{WyrdPyError, WyrdPyResult};

/// Serializes every harness mutation of the `WYRD_*` endpoint variables.
static ENV_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

/// Returns the process-wide lock guarding harness environment mutations.
fn env_mutex() -> &'static Mutex<()> {
    ENV_MUTEX.get_or_init(|| Mutex::new(()))
}

/// Endpoint variables captured before a context manager publishes its own,
/// restored on exit.
struct EnvSnapshot {
    /// Prior `WYRD_SERVER_URL`.
    server_url: Option<String>,
    /// Prior `WYRD_GRPC_URL`.
    grpc_url: Option<String>,
    /// Prior `WYRD_API_KEY`.
    api_key: Option<String>,
}

impl EnvSnapshot {
    /// Reads the current endpoint variables from the process environment.
    fn capture() -> Self {
        Self {
            server_url: std::env::var("WYRD_SERVER_URL").ok(),
            grpc_url: std::env::var("WYRD_GRPC_URL").ok(),
            api_key: std::env::var("WYRD_API_KEY").ok(),
        }
    }

    /// Republishes every captured value, removing variables that were unset.
    ///
    /// # Errors
    /// Returns a Python error when `os.environ` cannot be updated.
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
/// The Rust client tier resolves from the process environment, while Python
/// code reads `os.environ`, which the interpreter builds once at start.
/// Assigning or deleting an `os.environ` item also calls `putenv` or
/// `unsetenv`, so writing through it keeps a Python caller and a Rust client
/// in agreement on what the harness published.
///
/// # Errors
///
/// Returns a Python error when `os.environ` cannot be reached; deleting an
/// absent key is not an error.
fn publish_env(py: Python<'_>, key: &str, value: Option<&str>) -> WyrdPyResult<()> {
    let environ = py.import("os")?.getattr("environ")?;
    match value {
        Some(value) => environ.set_item(key, value)?,
        None => {
            if environ.contains(key)? {
                environ.del_item(key)?;
            }
        }
    }
    Ok(())
}

/// Python context manager for the Wyrd in-process test server.
#[pyclass(module = "wyrd._wyrd.testing")]
#[allow(
    clippy::struct_excessive_bools,
    reason = "preserve the established Python test-harness constructor flags"
)]
pub struct WyrdTestServer {
    /// Retained from the Python constructor signature; teardown always runs.
    cleanup: bool,
    /// Whether entering publishes the `WYRD_*` endpoint variables.
    mutate_env: bool,
    /// Whether the audit publisher runs during the fixture.
    audit_publication: bool,
    /// Whether the verification runtime is composed during the fixture.
    verification_runtime: bool,
    /// Root every built-in gateway adapter targets; `None` keeps a gateway
    /// that dispatches nothing unless `live_providers` is set.
    provider_base_url: Option<Url>,
    /// Whether every built-in gateway adapter targets its real provider
    /// endpoint, for the opt-in live smoke lane.
    live_providers: bool,
    /// Whether the server answers at the public origin the Keycloak fixture
    /// clients register, so human sign-ins can complete against it.
    human_sso: bool,
    /// Bound HTTP base URL while entered.
    base_url: Option<String>,
    /// Writer service API key bootstrapped on entry.
    api_key: Option<String>,
    /// Fixture data tenant id while entered.
    tenant_id: Option<String>,
    /// Running harness while entered.
    server: Option<TestServer>,
    /// Endpoint variables to restore on exit when `mutate_env` published them.
    env_snapshot: Option<EnvSnapshot>,
    /// Principal `scoped_api_key` bootstrapped for each seeded role, so
    /// `revoke_scoped_role` can revoke that exact grant.
    scoped_principals: Mutex<BTreeMap<String, Bootstrap>>,
}

#[pymethods]
impl WyrdTestServer {
    /// Creates an unstarted server; `__enter__` boots it.
    ///
    /// `provider_base_url` roots every built-in gateway adapter at one local
    /// mock upstream (`OpenAI` under `/v1`), so gateway journeys dispatch over
    /// HTTP with the harness's operator credential bindings; without it the
    /// gateway reaches no provider. The same `/v1` upstream also serves the
    /// `OpenAI` LLM judge of the verification runtime, so a continuous Eval
    /// journey's judge calls `<root>/v1/chat/completions`. `live_providers` instead points every
    /// built-in adapter at its real provider endpoint, which only the opt-in
    /// live smoke lane asks for. `human_sso` serves the public origin the
    /// identity lane's Keycloak clients register, for saved user login
    /// journeys.
    ///
    /// # Errors
    /// Raises the harness error when `provider_base_url` is not an absolute
    /// URL, or when it is combined with `live_providers`.
    #[new]
    #[pyo3(signature = (cleanup = true, mutate_env = true, audit_publication = true, verification_runtime = false, provider_base_url = None, live_providers = false, human_sso = false))]
    #[allow(
        clippy::fn_params_excessive_bools,
        reason = "PyO3 projects the existing Python test-harness flags directly"
    )]
    fn __new__(
        cleanup: bool,
        mutate_env: bool,
        audit_publication: bool,
        verification_runtime: bool,
        provider_base_url: Option<&str>,
        live_providers: bool,
        human_sso: bool,
    ) -> WyrdPyResult<Self> {
        if live_providers && provider_base_url.is_some() {
            return Err(harness_error(
                "provider_base_url and live_providers are mutually exclusive",
            ));
        }
        let provider_base_url = provider_base_url
            .map(|base| {
                Url::parse(base)
                    .map_err(|error| harness_error(format!("invalid provider_base_url: {error}")))
            })
            .transpose()?;
        Ok(Self {
            cleanup,
            mutate_env,
            audit_publication,
            verification_runtime,
            provider_base_url,
            live_providers,
            human_sso,
            base_url: None,
            api_key: None,
            tenant_id: None,
            server: None,
            env_snapshot: None,
            scoped_principals: Mutex::new(BTreeMap::new()),
        })
    }

    /// Start a bound server, bootstrap its writer service key, and optionally
    /// publish the endpoints through the documented `WYRD_*` environment vars.
    ///
    /// # Errors
    /// Returns a Python error when server startup, service bootstrap, or
    /// environment setup cannot complete.
    fn __enter__(mut slf: PyRefMut<'_, Self>) -> WyrdPyResult<PyRefMut<'_, Self>> {
        let root = slf.provider_base_url.clone();
        let live_providers = slf.live_providers;
        let audit_publication = slf.audit_publication;
        let verification_runtime = slf.verification_runtime;
        let human_sso = slf.human_sso;
        let (server, api_key) = wyrd_runtime::runtime()
            .block_on(async {
                let mut builder = TestServer::builder();
                if !audit_publication {
                    builder = builder.without_audit_publication_for_test();
                }
                if verification_runtime {
                    builder = builder.with_verification_runtime_for_test();
                }
                if human_sso {
                    builder = builder.with_public_origin(
                        Url::parse(HUMAN_PUBLIC_ORIGIN).expect("the fixture origin is a URL"),
                    );
                }
                if let Some(root) = root {
                    builder = builder.with_gateway_provider_root_for_test(root);
                } else if live_providers {
                    builder = builder.with_live_gateway_providers_for_test();
                }
                let server = Box::pin(builder.start_bound()).await?;
                let bootstrap = server
                    .bootstrap_service("python-integration-writer", &["admin"])
                    .await?;
                let Bootstrap::Machine { api_key, .. } = bootstrap else {
                    return Err(WyrdError::HarnessStart {
                        message: "Python test server writer bootstrap returned a user".to_owned(),
                        details: serde_json::json!({}),
                    });
                };
                Ok((server, api_key.expose_secret().to_owned()))
            })
            .map_err(WyrdPyError::from)?;
        let base_url = server.base_url().unwrap_or("").to_owned();
        let grpc_url = server.grpc_url().unwrap_or_default();

        if slf.mutate_env {
            let _guard = env_mutex()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let snapshot = EnvSnapshot::capture();
            let py = slf.py();
            publish_env(py, "WYRD_SERVER_URL", Some(&base_url))?;
            publish_env(py, "WYRD_GRPC_URL", Some(&grpc_url))?;
            publish_env(py, "WYRD_API_KEY", Some(&api_key))?;
            slf.env_snapshot = Some(snapshot);
        }

        slf.tenant_id = Some(server.data_tenant_id().to_string());
        slf.base_url = Some(base_url);
        slf.api_key = Some(api_key);
        slf.server = Some(server);
        Ok(slf)
    }

    /// Restores published endpoint variables and shuts the server down.
    ///
    /// Never suppresses an exception raised inside the `with` block.
    ///
    /// # Errors
    /// Returns a Python error when `os.environ` cannot be restored.
    fn __exit__(
        &mut self,
        py: Python<'_>,
        _exc_type: Option<Bound<'_, PyAny>>,
        _exc_value: Option<Bound<'_, PyAny>>,
        _traceback: Option<Bound<'_, PyAny>>,
    ) -> WyrdPyResult<bool> {
        if let Some(snapshot) = self.env_snapshot.take() {
            let _guard = env_mutex()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            snapshot.restore(py)?;
        }
        if !self.cleanup {
            tracing::warn!(
                "WyrdTestServer: cleanup=false is not yet implemented; \
                 the server and embedded Postgres will still be cleaned up"
            );
        }
        if let Some(server) = self.server.take() {
            wyrd_runtime::runtime().block_on(async {
                let _ = server.shutdown().await;
            });
        }
        Ok(false)
    }

    /// Bound HTTP base URL.
    ///
    /// # Errors
    /// Raises the harness error outside the context manager.
    #[getter]
    fn base_url(&self) -> WyrdPyResult<String> {
        self.base_url.clone().ok_or_else(not_started)
    }

    /// Writer service API key bootstrapped on entry.
    ///
    /// # Errors
    /// Raises the harness error outside the context manager.
    #[getter]
    fn api_key(&self) -> WyrdPyResult<String> {
        self.api_key.clone().ok_or_else(not_started)
    }

    /// Fixture data tenant id.
    ///
    /// # Errors
    /// Raises the harness error outside the context manager.
    #[getter]
    fn tenant_id(&self) -> WyrdPyResult<String> {
        self.tenant_id.clone().ok_or_else(not_started)
    }

    /// Exchanges the harness's retained API key for a bearer access token.
    ///
    /// Lets a Python test hand an unmodified OTLP exporter or provider SDK the
    /// Wyrd access token it needs. The key itself never leaves the harness.
    ///
    /// # Errors
    ///
    /// Raises the harness error when the context manager is inactive, and a
    /// Wyrd Python error when the exchange route refuses the key.
    fn access_token(&self) -> WyrdPyResult<String> {
        let server = self.started()?;
        let api_key = self.api_key.as_ref().ok_or_else(not_started)?;
        let key = SecretString::from(api_key.clone());
        wyrd_runtime::runtime()
            .block_on(server.exchange_api_key(&key))
            .map_err(py_error)
    }

    /// Bootstrap a service principal, returning its scoped API key string.
    ///
    /// `roles` is a list of Wyrd built-in role names; an empty list creates a
    /// principal with no grants (useful for negative RBAC journeys). Must be
    /// called inside the context manager.
    ///
    /// # Errors
    /// Raises a Wyrd Python error when the context manager is inactive or
    /// bootstrap fails or yields a user.
    #[pyo3(signature = (roles, name = "svc"))]
    fn bootstrap_service(&self, roles: Vec<String>, name: &str) -> WyrdPyResult<String> {
        let server = self.started()?;
        let roles: Vec<&str> = roles.iter().map(String::as_str).collect();
        let bootstrap = wyrd_runtime::runtime()
            .block_on(server.bootstrap_service(name, &roles))
            .map_err(py_error)?;
        machine_key(
            bootstrap,
            "expected Machine bootstrap from bootstrap_service",
        )
    }

    /// Issue an API key for the principal projected by a registered Service Card.
    ///
    /// # Errors
    /// Raises a harness error for an invalid Card identity or a missing principal.
    fn credential_registered_service(
        &self,
        card_ref: &str,
        roles: Vec<String>,
    ) -> WyrdPyResult<String> {
        let server = self.started()?;
        let card_ref = card_ref
            .parse::<wyrd_spec::reference::CardRef>()
            .map_err(|error| harness_error(format!("invalid card identity: {error}")))?;
        let roles: Vec<&str> = roles.iter().map(String::as_str).collect();
        let bootstrap = wyrd_runtime::runtime()
            .block_on(server.credential_registered_service(&card_ref, &roles))
            .map_err(py_error)?;
        machine_key(bootstrap, "expected a machine bootstrap")
    }

    /// Mint an API key for a principal holding exactly `permissions`.
    ///
    /// Each permission is either a `resource:action` string, which grants every
    /// object of that operation, or a dict in the persisted typed permission
    /// projection (`resource`, `action`, `scope`), which expresses object
    /// scope such as one gateway model. This is the door a journey uses to
    /// prove an access gate from the caller's side: it seeds one role carrying
    /// only those grants and bootstraps a service named `role` onto it.
    ///
    /// # Errors
    ///
    /// Raises the harness error for an unparsable permission, and a Wyrd
    /// Python error when the context manager is inactive or role seeding or
    /// bootstrapping fails.
    fn scoped_api_key(
        &self,
        py: Python<'_>,
        role: &str,
        permissions: Vec<Bound<'_, PyAny>>,
    ) -> WyrdPyResult<String> {
        let server = self.started()?;
        let dumps = py
            .import("json")
            .and_then(|json| json.getattr("dumps"))
            .map_err(harness_error)?;
        let parsed = permissions
            .iter()
            .map(|value| {
                if let Ok(token) = value.extract::<String>() {
                    return token.parse::<Permission>().map_err(|_| {
                        harness_error(format!("`{token}` is not a resource:action permission"))
                    });
                }
                let json: String = dumps
                    .call1((value,))
                    .and_then(|dumped| dumped.extract())
                    .map_err(harness_error)?;
                serde_json::from_str::<Permission>(&json)
                    .map_err(|error| harness_error(format!("invalid permission {json}: {error}")))
            })
            .collect::<WyrdPyResult<Vec<_>>>()?;
        let bootstrap = wyrd_runtime::runtime()
            .block_on(async {
                server.seed_role(role, &parsed).await?;
                server.bootstrap_service(role, &[role]).await
            })
            .map_err(py_error)?;
        self.scoped_principals
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(role.to_owned(), bootstrap.clone());
        machine_key(bootstrap, "expected a machine bootstrap")
    }

    /// Revoke `role` from the service `scoped_api_key` bootstrapped for it.
    ///
    /// Removes the durable role grant through the harness's tenant role setup.
    /// Access tokens exchanged afterwards resolve the revoked grant set.
    ///
    /// # Errors
    ///
    /// Raises the harness error when `scoped_api_key` never seeded `role`, and
    /// a Wyrd Python error when the context manager is inactive or the
    /// revocation write fails.
    fn revoke_scoped_role(&self, role: &str) -> WyrdPyResult<()> {
        let server = self.started()?;
        let principal = self
            .scoped_principals
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(role)
            .cloned()
            .ok_or_else(|| harness_error(format!("no scoped principal holds role `{role}`")))?;
        wyrd_runtime::runtime()
            .block_on(server.revoke_role(&principal, role))
            .map_err(py_error)
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
        let server = self.started()?;
        let tenant_id = wyrd_runtime::runtime()
            .block_on(server.seed_tenant(slug))
            .map_err(py_error)?;
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
        let server = self.started()?;
        let tenant_id = tenant_id
            .parse::<DataTenantId>()
            .map_err(|error| harness_error(format!("invalid tenant id: {error}")))?;
        let roles: Vec<&str> = roles.iter().map(String::as_str).collect();
        let bootstrap = wyrd_runtime::runtime()
            .block_on(server.bootstrap_service_in_tenant(tenant_id, name, &roles))
            .map_err(py_error)?;
        machine_key(
            bootstrap,
            "expected a machine bootstrap from bootstrap_service_in_tenant",
        )
    }

    /// Flush the server-owned Scribe after a public client drain.
    ///
    /// The harness bounds the flush, so a Scribe that cannot settle its
    /// staged rows fails the calling journey instead of blocking the
    /// interpreter thread forever.
    ///
    /// # Errors
    /// Raises a Wyrd Python error when the context manager is inactive, the
    /// production Scribe seal path cannot commit its buffered rows, or the
    /// flush does not settle within the harness bound.
    fn flush_bifrost(&self) -> WyrdPyResult<()> {
        let server = self.started()?;
        wyrd_runtime::runtime()
            .block_on(server.flush_bifrost())
            .map_err(py_error)
    }

    /// Bring a binding's schedule cursor to database time, so the
    /// verification runtime schedules its next occurrence now.
    ///
    /// # Errors
    /// Raises a harness error for an invalid binding ID, and a Wyrd Python
    /// error when the context manager is inactive or the update fails.
    fn make_binding_due(&self, binding_id: &str) -> WyrdPyResult<()> {
        let server = self.started()?;
        let binding = binding_id.parse().map_err(harness_error)?;
        wyrd_runtime::runtime()
            .block_on(server.make_binding_due(binding))
            .map_err(py_error)
    }

    /// Wait until Drift Verifier `verifier`'s fitted baseline is ready.
    ///
    /// `verifier` is the Verifier Card UID and `timeout` is in seconds. The
    /// GIL is released while the harness polls the baseline status a Card read
    /// serves.
    ///
    /// # Errors
    /// Raises a harness error for an invalid UID or a negative or non-finite
    /// `timeout`, `WYRD_VERIFICATION_409_BASELINE_NOT_READY` carrying the last
    /// observed baseline state when `timeout` elapses first, and a Wyrd Python
    /// error when the context manager is inactive or the status read fails.
    fn wait_for_baseline(&self, py: Python<'_>, verifier: &str, timeout: f64) -> WyrdPyResult<()> {
        let server = self.started()?;
        let verifier = verifier.parse().map_err(harness_error)?;
        let timeout = Duration::try_from_secs_f64(timeout).map_err(harness_error)?;
        py.detach(|| wyrd_runtime::runtime().block_on(server.wait_for_baseline(&verifier, timeout)))
            .map_err(py_error)
    }

    /// Stage, test, and activate the identity lane's Keycloak sign-in for the
    /// tenant `admin_key` administers, mapping `wyrd-admins` to `admin` and
    /// `wyrd-viewers` to `reader`.
    ///
    /// Needs `human_sso=True` and the identity lane's Keycloak.
    ///
    /// # Errors
    /// Raises the harness error outside the context manager.
    ///
    /// # Panics
    /// Panics (a Python `PanicException`) when any served step fails.
    fn activate_human_sso(&self, py: Python<'_>, admin_key: &str) -> WyrdPyResult<()> {
        let sso = HumanSso::new(&self.base_url()?);
        py.detach(|| wyrd_runtime::runtime().block_on(sso.activate_keycloak(admin_key)));
        Ok(())
    }

    /// Log `username` in to `tenant` through the RFC 8628 device login and
    /// save the credential under the Wyrd configuration directory `config_home`,
    /// exactly as `wyrd auth login` does.
    ///
    /// # Errors
    /// Raises the harness error outside the context manager.
    ///
    /// # Panics
    /// Panics (a Python `PanicException`) when the login or the save fails.
    fn save_human_login(
        &self,
        py: Python<'_>,
        config_home: PathBuf,
        tenant: &str,
        username: &str,
        password: &str,
    ) -> WyrdPyResult<()> {
        let sso = HumanSso::new(&self.base_url()?);
        py.detach(move || {
            wyrd_runtime::runtime().block_on(sso.save_login(
                &config_home,
                tenant,
                username,
                password,
            ))
        });
        Ok(())
    }

    /// Make the saved login for `tenant` under `config_home` stale, so the
    /// next client renews it.
    ///
    /// # Errors
    /// Raises the harness error outside the context manager.
    ///
    /// # Panics
    /// Panics (a Python `PanicException`) when the login is missing or cannot
    /// be saved.
    fn expire_saved_login(
        &self,
        py: Python<'_>,
        config_home: PathBuf,
        tenant: &str,
    ) -> WyrdPyResult<()> {
        let sso = HumanSso::new(&self.base_url()?);
        py.detach(move || sso.expire_saved(&config_home, tenant));
        Ok(())
    }

    /// Whether the saved login for `tenant` under `config_home` holds an
    /// expired access token, so a journey can prove a renewal was saved.
    ///
    /// # Errors
    /// Raises the harness error outside the context manager.
    ///
    /// # Panics
    /// Panics (a Python `PanicException`) when the login is missing.
    fn saved_login_is_stale(
        &self,
        py: Python<'_>,
        config_home: PathBuf,
        tenant: &str,
    ) -> WyrdPyResult<bool> {
        let sso = HumanSso::new(&self.base_url()?);
        Ok(py.detach(move || sso.saved_is_stale(&config_home, tenant)))
    }

    /// Revoke the server-side refresh chain of the saved login for `tenant`
    /// under `config_home` without touching the record, as another device's
    /// logout would.
    ///
    /// # Errors
    /// Raises the harness error outside the context manager.
    ///
    /// # Panics
    /// Panics (a Python `PanicException`) when the login is not ready or the
    /// server refuses the revocation.
    fn revoke_saved_login(
        &self,
        py: Python<'_>,
        config_home: PathBuf,
        tenant: &str,
    ) -> WyrdPyResult<()> {
        let sso = HumanSso::new(&self.base_url()?);
        py.detach(move || {
            wyrd_runtime::runtime().block_on(sso.revoke_saved(&config_home, tenant));
        });
        Ok(())
    }
}

impl WyrdTestServer {
    /// The running harness.
    ///
    /// # Errors
    /// Returns the harness error when the context manager is not entered.
    fn started(&self) -> WyrdPyResult<&TestServer> {
        self.server.as_ref().ok_or_else(not_started)
    }
}

/// API key of a machine `bootstrap`.
///
/// # Errors
/// Returns the harness error carrying `unexpected` for a user bootstrap.
fn machine_key(bootstrap: Bootstrap, unexpected: &str) -> WyrdPyResult<String> {
    match bootstrap {
        Bootstrap::Machine { api_key, .. } => Ok(api_key.expose_secret().to_owned()),
        Bootstrap::User { .. } => Err(harness_error(unexpected)),
    }
}

/// Projects one harness failure onto the shared Wyrd boundary adapter.
fn py_error(error: WyrdTestServerError) -> WyrdPyError {
    WyrdPyError::from(WyrdError::from(error))
}

/// Builds the harness-start failure raised for a boundary-side refusal.
fn harness_error(error: impl Display) -> WyrdPyError {
    WyrdPyError::from(WyrdError::HarnessStart {
        message: error.to_string(),
        details: serde_json::json!({}),
    })
}

/// Build the harness failure raised when the server has not been started.
///
/// Every accessor requires the context manager to be entered first, so they
/// share one catalog-backed failure instead of raising a bare Python exception.
fn not_started() -> WyrdPyError {
    harness_error("WyrdTestServer not started (use as context manager)")
}

/// Registers [`WyrdTestServer`] on the `wyrd._wyrd.testing` submodule.
///
/// # Errors
/// Returns a Python error when the class cannot be added.
pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<WyrdTestServer>()
}
