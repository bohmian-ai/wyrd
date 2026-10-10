//! Test-tier napi projection of the in-process Wyrd server harness.

#![deny(missing_docs)]

use std::fmt::Display;
use std::path::Path;
use std::result::Result as StdResult;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use arrow::datatypes::{DataType, Field};
use napi::{Env, Error, JsValue, Result};
use napi_derive::napi;
use serde_json::Value;
use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use wyrd_spec::error::WyrdError;
use wyrd_testing::Bootstrap;
use wyrd_testing::human_login::{HUMAN_PUBLIC_ORIGIN, HumanSso};
use wyrd_testing::server::{WyrdTestServer, WyrdTestServerError};

pub mod cli;

/// In-process server handle used only by TypeScript integration tests.
///
/// Journeys steer the server through exactly three test controls, each backed
/// by the production code path: `flushBifrost`, `waitForBaseline`, and
/// `makeBindingDue`. The remaining members start, address, or credential the
/// server.
#[napi]
pub struct NativeWyrdTestServer {
    /// Owned Rust harness dropped only after explicit asynchronous shutdown.
    server: Arc<Mutex<Option<WyrdTestServer>>>,
    /// Bound HTTP base URL.
    base_url: String,
    /// Bound gRPC ingest URL used by public TypeScript write journeys.
    grpc_url: String,
    /// Admin access token minted through the real auth route.
    token: String,
    /// Registered table reached by the public query journey.
    table_fqn: String,
    /// API key of the bootstrapped writer principal, for the write journey.
    api_key: String,
    /// Card the writer principal is scoped to, which its rows correlate to.
    card_ref: String,
}

#[napi]
impl NativeWyrdTestServer {
    /// Returns the bound HTTP base URL.
    #[napi(getter)]
    pub fn base_url(&self) -> String {
        self.base_url.clone()
    }

    /// Returns the bound gRPC ingest URL.
    #[napi(getter)]
    pub fn grpc_url(&self) -> String {
        self.grpc_url.clone()
    }

    /// Returns the integration-only admin bearer.
    #[napi(getter)]
    pub fn token(&self) -> String {
        self.token.clone()
    }

    /// Returns the registered table used by the TypeScript Oracle journey.
    #[napi(getter)]
    pub fn table_fqn(&self) -> String {
        self.table_fqn.clone()
    }

    /// Returns the writer principal's API key, which `Bifrost` authenticates with.
    ///
    /// The query journeys exchange this for a bearer through
    /// [`Self::token`]; the write journey needs the key itself because the
    /// write handle resolves its own credential.
    #[napi(getter)]
    pub fn api_key(&self) -> String {
        self.api_key.clone()
    }

    /// Returns the Card the writer principal is scoped to.
    ///
    /// Every row written with [`Self::api_key`] must correlate to this Card, so
    /// the harness publishes it rather than making each test restate it.
    #[napi(getter)]
    pub fn card_ref(&self) -> String {
        self.card_ref.clone()
    }

    /// Flush Scribe so rows written through the public SDK become queryable.
    ///
    /// A client `flush` only proves the server accepted the batch; the rows
    /// reach a readable source after Scribe drains, which a test must wait for
    /// rather than sleep on.
    ///
    /// # Errors
    ///
    /// Throws a `WyrdError`-shaped error (`code`, `status`, `title`, `detail`,
    /// `remediation`, `details`) when the harness is closed or the flush
    /// fails.
    #[napi]
    pub fn flush_bifrost(&self, env: Env) -> Result<()> {
        self.control(|server| wyrd_runtime::runtime().block_on(server.flush_bifrost()))
            .map_err(|error| thrown(env, &error))
    }

    /// Bring binding `binding_id`'s schedule cursor to database time, so the
    /// verification runtime schedules its next occurrence now.
    ///
    /// # Errors
    ///
    /// Throws a `WyrdError`-shaped error when the harness is closed,
    /// `binding_id` is not a binding ID, or the update fails.
    #[napi]
    pub fn make_binding_due(&self, env: Env, binding_id: String) -> Result<()> {
        let parsed = binding_id.parse::<wyrd_spec::ids::BindingId>();
        drop(binding_id);
        parsed
            .map_err(harness)
            .and_then(|binding| {
                self.control(|server| {
                    wyrd_runtime::runtime().block_on(server.make_binding_due(binding))
                })
            })
            .map_err(|error| thrown(env, &error))
    }

    /// Wait until Drift Verifier `verifier`'s fitted baseline is ready.
    ///
    /// `verifier` is the Verifier Card UID. The harness polls the baseline
    /// status a Card read serves for at most `timeoutMs` milliseconds.
    ///
    /// # Errors
    ///
    /// Throws a `WyrdError`-shaped error when the harness is closed or
    /// `verifier` is not a Card UID, and one with code
    /// `WYRD_VERIFICATION_409_BASELINE_NOT_READY` whose `details.baseline` is
    /// the last observed baseline status when `timeoutMs` elapses first.
    #[napi]
    pub fn wait_for_baseline(&self, env: Env, verifier: String, timeout_ms: u32) -> Result<()> {
        let parsed = verifier.parse::<wyrd_spec::ids::CardUid>();
        drop(verifier);
        let timeout = Duration::from_millis(u64::from(timeout_ms));
        parsed
            .map_err(harness)
            .and_then(|verifier| {
                self.control(|server| {
                    wyrd_runtime::runtime().block_on(server.wait_for_baseline(&verifier, timeout))
                })
            })
            .map_err(|error| thrown(env, &error))
    }

    /// Run one test control against the open harness, projecting every
    /// failure onto its catalogued [`WyrdError`].
    ///
    /// # Errors
    ///
    /// Returns `WYRD_TESTING_500_HARNESS_START` when the harness lock is
    /// poisoned or the server is shut down, and the control's own catalogued
    /// error when `call` fails.
    fn control(
        &self,
        call: impl FnOnce(&WyrdTestServer) -> StdResult<(), WyrdTestServerError>,
    ) -> StdResult<(), WyrdError> {
        let guard = self
            .server
            .lock()
            .map_err(|_| harness("test server lock poisoned"))?;
        let server = guard
            .as_ref()
            .ok_or_else(|| harness("test server is shut down"))?;
        call(server).map_err(WyrdError::from)
    }

    /// Mint an API key for a principal holding exactly `permissions`.
    ///
    /// Each permission is either a `resource:action` string, which grants every
    /// object of that operation, or an object in the persisted typed
    /// permission projection (`resource`, `action`, `scope`), which expresses
    /// object scope such as one Verifier. This is the door a journey uses to
    /// prove an access gate from the caller's side: it seeds one role carrying
    /// only those grants and bootstraps a service onto it.
    ///
    /// # Errors
    ///
    /// Returns a napi error for an unparsable permission, or when the harness
    /// is closed or role seeding or bootstrapping fails.
    #[napi]
    pub fn scoped_api_key(
        &self,
        role: String,
        permissions: Vec<serde_json::Value>,
    ) -> Result<String> {
        let result = self.scoped_api_key_borrowed(&role, &permissions);
        drop(role);
        drop(permissions);
        result
    }

    /// Delegates the N-API-owned role and permissions without extending their
    /// ownership into the Rust harness call.
    ///
    /// # Errors
    ///
    /// Returns a napi error for an unparsable permission, or when the harness
    /// lock is poisoned, the server is closed, or seeding or bootstrapping
    /// fails.
    fn scoped_api_key_borrowed(
        &self,
        role: &str,
        permissions: &[serde_json::Value],
    ) -> Result<String> {
        let guard = self
            .server
            .lock()
            .map_err(|_| napi::Error::from_reason("test server lock poisoned".to_owned()))?;
        let server = guard
            .as_ref()
            .ok_or_else(|| napi::Error::from_reason("test server is shut down".to_owned()))?;
        let parsed = permissions
            .iter()
            .map(|value| match value {
                serde_json::Value::String(token) => {
                    token.parse::<wyrd_runtime::Permission>().map_err(|_| {
                        napi::Error::from_reason(format!(
                            "`{token}` is not a resource:action permission"
                        ))
                    })
                }
                typed => serde_json::from_value(typed.clone()).map_err(|error| {
                    napi::Error::from_reason(format!("invalid permission {typed}: {error}"))
                }),
            })
            .collect::<Result<Vec<_>>>()?;
        let bootstrap = wyrd_runtime::runtime()
            .block_on(async {
                server.seed_role(role, &parsed).await?;
                server.bootstrap_service(role, &[role]).await
            })
            .map_err(|error| napi::Error::from_reason(error.to_string()))?;
        match bootstrap {
            Bootstrap::Machine { api_key, .. } => {
                Ok(secrecy::ExposeSecret::expose_secret(&api_key).to_owned())
            }
            Bootstrap::User { .. } => Err(napi::Error::from_reason(
                "expected a machine bootstrap".to_owned(),
            )),
        }
    }

    /// Provision a second active tenant so a journey can prove cross-tenant
    /// isolation, returning its tenant ID.
    ///
    /// The tenant is seeded through the same operator path the Rust harness
    /// uses, so the journey observes the production tenancy boundary.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the harness is closed or seeding fails.
    #[napi]
    pub fn seed_tenant(&self, slug: String) -> Result<String> {
        let result = self.with_server(|server| {
            wyrd_runtime::runtime()
                .block_on(server.seed_tenant(&slug))
                .map(|tenant| tenant.to_string())
                .map_err(reason)
        });
        drop(slug);
        result
    }

    /// Bootstrap a service principal holding exactly the built-in or seeded
    /// `roles` in the fixture tenant and return its API key.
    ///
    /// The door a journey uses for a machine caller with a known role set,
    /// such as one holding only `workload` to prove a gate from the caller's
    /// side.
    ///
    /// # Arguments
    ///
    /// * `roles` - The role names the principal holds.
    /// * `name` - The principal's fixture Service name, unique per harness.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the harness is closed or bootstrapping fails.
    #[napi]
    pub fn bootstrap_service(&self, roles: Vec<String>, name: String) -> Result<String> {
        let result = self.with_server(|server| {
            let roles: Vec<&str> = roles.iter().map(String::as_str).collect();
            match wyrd_runtime::runtime()
                .block_on(server.bootstrap_service(&name, &roles))
                .map_err(reason)?
            {
                Bootstrap::Machine { api_key, .. } => {
                    Ok(secrecy::ExposeSecret::expose_secret(&api_key).to_owned())
                }
                Bootstrap::User { .. } => Err(napi::Error::from_reason(
                    "expected a machine bootstrap".to_owned(),
                )),
            }
        });
        drop((roles, name));
        result
    }

    /// Sign in a fixture-tenant user holding identity-provider `roles` and
    /// return the user's principal id.
    ///
    /// The roles are recorded as a login would record them, so a journey can
    /// prove a direct grant coexists with them.
    ///
    /// # Arguments
    ///
    /// * `roles` - The role names the identity provider grants.
    /// * `name` - The user's name; the email is `<name>@test.wyrd`.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the harness is closed or bootstrapping fails.
    #[napi]
    pub fn bootstrap_user(&self, roles: Vec<String>, name: String) -> Result<String> {
        let result = self.with_server(|server| {
            let roles: Vec<&str> = roles.iter().map(String::as_str).collect();
            let bootstrap = wyrd_runtime::runtime()
                .block_on(server.bootstrap_user(&name, &roles))
                .map_err(reason)?;
            Ok(bootstrap.id().to_string())
        });
        drop((roles, name));
        result
    }

    /// Bootstrap a service principal holding `roles` in tenant `tenant_id` and
    /// return its API key.
    ///
    /// Pairs with [`Self::seed_tenant`]: the key is the foreign caller a
    /// cross-tenant journey uses to prove the fixture tenant is unreachable.
    ///
    /// # Errors
    ///
    /// Returns a napi error when `tenant_id` is not a tenant ID, the harness
    /// is closed, or bootstrapping fails.
    #[napi]
    pub fn bootstrap_service_in_tenant(
        &self,
        tenant_id: String,
        roles: Vec<String>,
        name: String,
    ) -> Result<String> {
        let tenant = tenant_id.parse::<wyrd_spec::DataTenantId>().map_err(reason);
        let result = tenant.and_then(|tenant| {
            self.with_server(|server| {
                let roles: Vec<&str> = roles.iter().map(String::as_str).collect();
                let bootstrap = wyrd_runtime::runtime()
                    .block_on(server.bootstrap_service_in_tenant(tenant, &name, &roles))
                    .map_err(reason)?;
                match bootstrap {
                    Bootstrap::Machine { api_key, .. } => {
                        Ok(secrecy::ExposeSecret::expose_secret(&api_key).to_owned())
                    }
                    Bootstrap::User { .. } => Err(napi::Error::from_reason(
                        "expected a machine bootstrap".to_owned(),
                    )),
                }
            })
        });
        drop((tenant_id, roles, name));
        result
    }

    /// Issue a key for the fixture tenant's unbound administrator: the key
    /// `wyrd setup` prints, bound to no Card.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the harness is closed or issuing fails.
    #[napi]
    pub fn tenant_admin_key(&self) -> Result<String> {
        self.with_server(|server| {
            let key = wyrd_runtime::runtime()
                .block_on(server.tenant_admin_key())
                .map_err(reason)?;
            Ok(secrecy::ExposeSecret::expose_secret(&key).to_owned())
        })
    }

    /// Run `call` against the open harness while holding its lock.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the lock is poisoned, the server is shut
    /// down, or `call` fails.
    fn with_server<T>(&self, call: impl FnOnce(&WyrdTestServer) -> Result<T>) -> Result<T> {
        let guard = self
            .server
            .lock()
            .map_err(|_| napi::Error::from_reason("test server lock poisoned".to_owned()))?;
        let server = guard
            .as_ref()
            .ok_or_else(|| napi::Error::from_reason("test server is shut down".to_owned()))?;
        call(server)
    }

    /// Activate the identity lane's Keycloak sign-in for one tenant and
    /// return its id: the fixture tenant when `tenantSlug` is absent, else a
    /// newly seeded tenant of that slug with its own administrator.
    ///
    /// Needs `startTestServer({ humanSso: true })` and the identity lane's
    /// Keycloak.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the harness is closed, seeding fails, or any
    /// served activation step panics.
    #[napi(catch_unwind)]
    pub fn activate_human_sso(&self, tenant_slug: Option<String>) -> Result<String> {
        let guard = self
            .server
            .lock()
            .map_err(|_| napi::Error::from_reason("test server lock poisoned".to_owned()))?;
        let server = guard
            .as_ref()
            .ok_or_else(|| napi::Error::from_reason("test server is shut down".to_owned()))?;
        let sso = HumanSso::new(&self.base_url);
        wyrd_runtime::runtime().block_on(async {
            let Some(slug) = tenant_slug else {
                sso.activate_keycloak(&self.api_key).await;
                return Ok(server.data_tenant_id().to_string());
            };
            let tenant = server.seed_tenant(&slug).await.map_err(reason)?;
            let admin = server
                .bootstrap_service_in_tenant(tenant, &format!("{slug}-admin"), &["admin"])
                .await
                .map_err(reason)?;
            let Bootstrap::Machine { api_key, .. } = admin else {
                return Err(reason("service bootstrap returned a user principal"));
            };
            sso.activate_keycloak(secrecy::ExposeSecret::expose_secret(&api_key))
                .await;
            Ok(tenant.to_string())
        })
    }

    /// Log `username` in to `tenant` through the RFC 8628 device login and
    /// save the credential under the Wyrd configuration directory `configHome`,
    /// exactly as `wyrd auth login` does.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the login or the save panics.
    #[napi(catch_unwind)]
    pub fn save_human_login(
        &self,
        config_home: String,
        tenant: String,
        username: String,
        password: String,
    ) -> Result<()> {
        wyrd_runtime::runtime().block_on(HumanSso::new(&self.base_url).save_login(
            Path::new(&config_home),
            &tenant,
            &username,
            &password,
        ));
        drop((config_home, tenant, username, password));
        Ok(())
    }

    /// Make the saved login for `tenant` under `configHome` stale, so the
    /// next client renews it.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the login is missing or cannot be saved.
    #[napi(catch_unwind)]
    pub fn expire_saved_login(&self, config_home: String, tenant: String) -> Result<()> {
        HumanSso::new(&self.base_url).expire_saved(Path::new(&config_home), &tenant);
        drop((config_home, tenant));
        Ok(())
    }

    /// Whether the saved login for `tenant` under `configHome` holds an
    /// expired access token, so a journey can prove a renewal was saved.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the login is missing.
    #[napi(catch_unwind)]
    pub fn saved_login_is_stale(&self, config_home: String, tenant: String) -> Result<bool> {
        let stale = HumanSso::new(&self.base_url).saved_is_stale(Path::new(&config_home), &tenant);
        drop((config_home, tenant));
        Ok(stale)
    }

    /// Revoke the server-side refresh chain of the saved login for `tenant`
    /// under `configHome` without touching the record, as another device's
    /// logout would.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the login is not ready or the server refuses
    /// the revocation.
    #[napi(catch_unwind)]
    pub fn revoke_saved_login(&self, config_home: String, tenant: String) -> Result<()> {
        wyrd_runtime::runtime()
            .block_on(HumanSso::new(&self.base_url).revoke_saved(Path::new(&config_home), &tenant));
        drop((config_home, tenant));
        Ok(())
    }

    /// Gracefully shuts down the in-process server once.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the Rust harness cannot complete shutdown.
    #[napi]
    pub fn shutdown(&self) -> Result<()> {
        let server = self
            .server
            .lock()
            .map_err(|_| napi::Error::from_reason("test server lock poisoned".to_owned()))?
            .take();
        if let Some(server) = server {
            wyrd_runtime::runtime()
                .block_on(server.shutdown())
                .map_err(|error| napi::Error::from_reason(error.to_string()))?;
        }
        Ok(())
    }
}

/// Test-only capabilities a story's server starts with; every field is
/// optional and defaults to off.
#[napi(object)]
pub struct NativeTestServerOptions {
    /// Roots built-in gateway adapters at a local mock upstream; its `/v1`
    /// segment also serves the verification runtime's `OpenAI` Eval judge,
    /// which calls `<providerBaseUrl>/v1/chat/completions`.
    pub provider_base_url: Option<String>,
    /// `true` runs Drift baseline fitting and Verifier runs.
    pub verification_runtime: Option<bool>,
    /// `true` serves the public origin the identity lane's Keycloak clients
    /// register, for saved user login journeys.
    pub human_sso: Option<bool>,
    /// Access-token lifetime in seconds, verified with no clock-skew
    /// allowance, so a journey can outlive one token; omitted keeps the
    /// production lifetime.
    pub access_ttl_seconds: Option<u32>,
}

/// Starts a real bound Wyrd test server and mints an admin access token.
///
/// `options` selects the test-only capabilities; omitted, the server keeps
/// its default upstreams and runs no verification runtime.
///
/// # Errors
///
/// Returns a napi error for an invalid provider URL or server setup failure.
#[napi]
pub fn start_test_server(
    options: Option<NativeTestServerOptions>,
) -> napi::Result<NativeWyrdTestServer> {
    let options = options.unwrap_or(NativeTestServerOptions {
        provider_base_url: None,
        verification_runtime: None,
        human_sso: None,
        access_ttl_seconds: None,
    });
    let provider_root = options
        .provider_base_url
        .as_deref()
        .map(|base| {
            url::Url::parse(base).map_err(|error| {
                napi::Error::from_reason(format!("invalid providerBaseUrl: {error}"))
            })
        })
        .transpose()?;
    wyrd_runtime::runtime().block_on(Box::pin(start_test_server_async(
        provider_root,
        options.verification_runtime.unwrap_or(false),
        options.human_sso.unwrap_or(false),
        options.access_ttl_seconds,
    )))
}

/// Starts the bound harness with the requested test-only capabilities.
///
/// # Errors
///
/// Returns a napi error when any server setup step fails.
async fn start_test_server_async(
    provider_root: Option<url::Url>,
    verification_runtime: bool,
    human_sso: bool,
    access_ttl_seconds: Option<u32>,
) -> napi::Result<NativeWyrdTestServer> {
    let mut builder = WyrdTestServer::builder();
    if let Some(seconds) = access_ttl_seconds {
        builder = builder
            .with_access_ttl(chrono::Duration::seconds(i64::from(seconds)))
            .with_auth_verify_settings(wyrd_auth_verify::WyrdAuthVerifySettings {
                allowed_clock_skew: Duration::ZERO,
            });
    }
    if human_sso {
        builder = builder.with_public_origin(url::Url::parse(HUMAN_PUBLIC_ORIGIN).map_err(reason)?);
    }
    if verification_runtime {
        builder = builder.with_verification_runtime_for_test();
    }
    if let Some(root) = provider_root {
        builder = builder.with_gateway_provider_root_for_test(root);
    }
    let server = Box::pin(builder.start_bound())
        .await
        .map_err(|error| napi::Error::from_reason(error.to_string()))?;
    let table_name = "typescript_oracle_query";
    server
        .state()
        .bifrost_catalog()
        .ok_or_else(|| napi::Error::from_reason("Redux catalog is unavailable".to_owned()))?
        .create_table(CreateTableRequest {
            table: TableRef::new(BifrostNamespace::Bifrost, table_name),
            user_fields: vec![Field::new("value", DataType::Int64, false)],
            tenant: server.data_tenant_id(),
            physical_layout: None,
        })
        .await
        .map_err(|error| napi::Error::from_reason(error.to_string()))?;
    let bootstrap = server
        .bootstrap_service("typescript-oracle-query", &["admin"])
        .await
        .map_err(|error| napi::Error::from_reason(error.to_string()))?;
    let card_ref = bootstrap
        .card_ref()
        .ok_or_else(|| {
            napi::Error::from_reason("service bootstrap returned a user principal".to_owned())
        })?
        .to_string();
    let api_key = match bootstrap {
        Bootstrap::Machine { api_key, .. } => api_key,
        Bootstrap::User { .. } => {
            return Err(napi::Error::from_reason(
                "service bootstrap returned a user principal".to_owned(),
            ));
        }
    };
    let token = server
        .exchange_api_key(&api_key)
        .await
        .map_err(|error| napi::Error::from_reason(error.to_string()))?;
    let base_url = server
        .base_url()
        .ok_or_else(|| napi::Error::from_reason("test server has no HTTP URL".to_owned()))?
        .to_owned();
    let grpc_url = server
        .grpc_url()
        .ok_or_else(|| napi::Error::from_reason("test server has no gRPC URL".to_owned()))?
        .clone();
    Ok(NativeWyrdTestServer {
        server: Arc::new(Mutex::new(Some(server))),
        base_url,
        grpc_url,
        token,
        table_fqn: format!("vala.bifrost.{table_name}"),
        api_key: secrecy::ExposeSecret::expose_secret(&api_key).to_owned(),
        card_ref,
    })
}

/// Convert a harness failure into a napi error carrying its message.
fn reason(error: impl Display) -> Error {
    Error::from_reason(error.to_string())
}

/// Build the catalogued harness failure for a refusal raised at this boundary.
fn harness(error: impl Display) -> WyrdError {
    WyrdError::HarnessStart {
        message: error.to_string(),
        details: serde_json::json!({}),
    }
}

/// Project a catalogued [`WyrdError`] onto a thrown JavaScript error.
///
/// The thrown object carries the same `name`, `code`, `status`, `title`,
/// `detail`, `remediation`, and `details` fields as the public SDK's
/// `WyrdError`, with public text taken from the problem document, so a test
/// asserts one exact catalog code the way it does for an SDK refusal. When the
/// JavaScript error itself cannot be built, the napi failure that prevented it
/// is thrown instead.
fn thrown(env: Env, error: &WyrdError) -> Error {
    let problem = error.as_problem_json();
    let text = |key: &str, fallback: String| {
        problem
            .get(key)
            .and_then(Value::as_str)
            .map_or(fallback, str::to_owned)
    };
    let detail = text("detail", error.to_string());
    env.create_error(Error::from_reason(detail.clone()))
        .and_then(|mut object| {
            object.set("name", "WyrdError")?;
            object.set("code", error.code())?;
            object.set("status", u32::from(error.status()))?;
            object.set("title", text("title", error.title().to_owned()))?;
            object.set("detail", detail)?;
            object.set("remediation", error.remediation())?;
            object.set(
                "details",
                problem.get("details").cloned().unwrap_or(Value::Null),
            )?;
            Ok(Error::from(object.to_unknown()))
        })
        .unwrap_or_else(|failure| failure)
}
