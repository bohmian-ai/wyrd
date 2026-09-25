//! Test-tier napi projection of the in-process Wyrd server harness.

#![deny(missing_docs)]

use std::fmt::Display;
use std::sync::{Arc, Mutex};

use arrow::datatypes::{DataType, Field};
use napi::{Error, Result};
use napi_derive::napi;
use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use wyrd_testing::Bootstrap;
use wyrd_testing::server::WyrdTestServer;
use wyrd_testing::verification::VerificationFixture;

/// In-process server handle used only by TypeScript integration tests.
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

    /// Seed rows through the real gRPC ingest and Scribe flush paths.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the harness is closed or registration,
    /// encoding, ingest, or flushing fails.
    #[napi]
    pub fn seed_bifrost_rows(&self, table: String, rows: Vec<i64>) -> Result<Vec<i64>> {
        let result = self.seed_bifrost_rows_borrowed(&table, &rows);
        drop(table);
        drop(rows);
        result
    }

    /// Wait for the test-tier Scribe publication barrier after a public write.
    ///
    /// This does not expose a production flush API; it only lets a journey
    /// await the existing server-owned publication lifecycle before Oracle
    /// reads the acknowledged batch.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the server lock is poisoned, the server is
    /// shut down, or the publication barrier fails.
    #[napi]
    pub fn wait_for_bifrost_publication(&self) -> Result<()> {
        let guard = self
            .server
            .lock()
            .map_err(|_| napi::Error::from_reason("test server lock poisoned".to_owned()))?;
        let server = guard
            .as_ref()
            .ok_or_else(|| napi::Error::from_reason("test server is shut down".to_owned()))?;
        wyrd_runtime::runtime()
            .block_on(server.flush_bifrost())
            .map_err(|error| napi::Error::from_reason(error.to_string()))
    }

    /// Delegates the N-API-owned inputs without extending their ownership into
    /// the Rust harness call.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the harness lock is poisoned, the server is
    /// already closed, or registration, encoding, ingest, or flushing fails.
    fn seed_bifrost_rows_borrowed(&self, table: &str, rows: &[i64]) -> Result<Vec<i64>> {
        let guard = self
            .server
            .lock()
            .map_err(|_| napi::Error::from_reason("test server lock poisoned".to_owned()))?;
        let server = guard
            .as_ref()
            .ok_or_else(|| napi::Error::from_reason("test server is shut down".to_owned()))?;
        wyrd_runtime::runtime()
            .block_on(server.seed_bifrost_rows(table, rows))
            .map_err(|error| napi::Error::from_reason(error.to_string()))
    }

    /// Flush Scribe so rows written through the public SDK become queryable.
    ///
    /// A client `flush` only proves the server accepted the batch; the rows
    /// reach a readable source after Scribe drains, which a test must wait for
    /// rather than sleep on.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the harness is closed or the flush fails.
    #[napi]
    pub fn flush_bifrost(&self) -> Result<()> {
        let guard = self
            .server
            .lock()
            .map_err(|_| napi::Error::from_reason("test server lock poisoned".to_owned()))?;
        let server = guard
            .as_ref()
            .ok_or_else(|| napi::Error::from_reason("test server is shut down".to_owned()))?;
        wyrd_runtime::runtime()
            .block_on(server.flush_bifrost())
            .map_err(|error| napi::Error::from_reason(error.to_string()))
    }

    /// Bring binding `binding_id`'s schedule cursor to database time, so the
    /// verification runtime schedules its next occurrence now.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the harness is closed, `binding_id` is not a
    /// binding ID, or the update fails.
    #[napi]
    pub fn make_binding_due(&self, binding_id: String) -> Result<()> {
        let binding = binding_id.parse::<wyrd_spec::ids::BindingId>();
        drop(binding_id);
        let binding = binding.map_err(reason)?;
        let fixture = self.verification_fixture()?;
        wyrd_runtime::runtime()
            .block_on(fixture.make_binding_due(binding))
            .map_err(reason)
    }

    /// Every verification run ID of the fixture tenant, oldest first.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the harness is closed or the runs cannot be
    /// read.
    #[napi]
    pub fn verification_runs(&self) -> Result<Vec<String>> {
        let fixture = self.verification_fixture()?;
        let runs = wyrd_runtime::runtime()
            .block_on(fixture.runs())
            .map_err(reason)?;
        Ok(runs.iter().map(ToString::to_string).collect())
    }

    /// Strip the fitted-profile format from Verifier `verifier_uid`'s ready
    /// baseline, as a baseline fitted under earlier semantics is stored.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the harness is closed, `verifier_uid` is not
    /// a Card UID, or no ready baseline exists.
    #[napi]
    pub fn retire_fitted_format(&self, verifier_uid: String) -> Result<()> {
        let verifier = verifier_uid.parse::<wyrd_spec::ids::CardUid>();
        drop(verifier_uid);
        let verifier = verifier.map_err(reason)?;
        let fixture = self.verification_fixture()?;
        wyrd_runtime::runtime()
            .block_on(fixture.retire_fitted_format(&verifier))
            .map_err(reason)
    }

    /// Open the open server's verification fixture, which owns its own
    /// Postgres handle and so outlives the harness lock.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the harness lock is poisoned, the server is
    /// shut down, or the fixture tenant cannot be provisioned.
    fn verification_fixture(&self) -> Result<VerificationFixture> {
        let guard = self
            .server
            .lock()
            .map_err(|_| napi::Error::from_reason("test server lock poisoned".to_owned()))?;
        let server = guard
            .as_ref()
            .ok_or_else(|| napi::Error::from_reason("test server is shut down".to_owned()))?;
        wyrd_runtime::runtime()
            .block_on(server.verification_fixture())
            .map_err(reason)
    }

    /// Provision one canonical built-in table for the fixture tenant.
    ///
    /// A canonical signal ledger is server-owned, so a journey cannot register
    /// it through the public write path. This is the harness door that makes
    /// `vala.traces.spans`, `vala.logs.records` and `vala.metrics.points`
    /// exist before a public Arrow write reaches them.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the harness is closed or provisioning fails.
    #[napi]
    pub fn ensure_builtin_table(&self, namespace: String, name: String) -> Result<()> {
        let result = self.ensure_builtin_table_borrowed(&namespace, &name);
        drop(namespace);
        drop(name);
        result
    }

    /// Delegates the N-API-owned names without extending their ownership into
    /// the Rust harness call.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the harness lock is poisoned, the server is
    /// already closed, or provisioning fails.
    fn ensure_builtin_table_borrowed(&self, namespace: &str, name: &str) -> Result<()> {
        let guard = self
            .server
            .lock()
            .map_err(|_| napi::Error::from_reason("test server lock poisoned".to_owned()))?;
        let server = guard
            .as_ref()
            .ok_or_else(|| napi::Error::from_reason("test server is shut down".to_owned()))?;
        wyrd_runtime::runtime()
            .block_on(server.ensure_builtin_table_for_test(
                server.data_tenant_id(),
                namespace,
                name,
            ))
            .map_err(|error| napi::Error::from_reason(error.to_string()))
    }

    /// Mint an API key for a principal holding exactly `permissions`.
    ///
    /// `permissions` are `resource:action` strings. This is the door a journey
    /// uses to prove an access gate from the caller's side: it seeds one role
    /// carrying only those grants and bootstraps a service onto it.
    ///
    /// # Errors
    ///
    /// Returns a napi error for an unparsable permission, or when the harness
    /// is closed or role seeding or bootstrapping fails.
    #[napi]
    pub fn scoped_api_key(&self, role: String, permissions: Vec<String>) -> Result<String> {
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
    fn scoped_api_key_borrowed(&self, role: &str, permissions: &[String]) -> Result<String> {
        let guard = self
            .server
            .lock()
            .map_err(|_| napi::Error::from_reason("test server lock poisoned".to_owned()))?;
        let server = guard
            .as_ref()
            .ok_or_else(|| napi::Error::from_reason("test server is shut down".to_owned()))?;
        let parsed = permissions
            .iter()
            .map(|value| {
                value.parse::<wyrd_runtime::Permission>().map_err(|_| {
                    napi::Error::from_reason(format!(
                        "`{value}` is not a resource:action permission"
                    ))
                })
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

    /// Issue an API key for the principal a registered Service Card projects.
    ///
    /// `card_ref` is the canonical `space/Kind/name@version` identity a
    /// registration receipt returns. The Card must already be registered: this
    /// credentials the service account registration projected for it rather
    /// than minting a new one, so the key carries the registered Service's real
    /// card-ref scope and a run may observe the component Cards its spec
    /// references.
    ///
    /// # Errors
    ///
    /// Returns a napi error for a malformed identity string, or when the
    /// harness is closed or the Card has no projected principal.
    #[napi]
    pub fn credential_registered_service(
        &self,
        card_ref: String,
        roles: Vec<String>,
    ) -> Result<String> {
        let result = self.credential_registered_service_borrowed(&card_ref, &roles);
        drop(card_ref);
        drop(roles);
        result
    }

    /// Delegates the N-API-owned identity and roles without extending their
    /// ownership into the Rust harness call.
    ///
    /// # Errors
    ///
    /// Returns a napi error for a malformed identity string, or when the
    /// harness lock is poisoned, the server is closed, or the Card has no
    /// projected principal.
    fn credential_registered_service_borrowed(
        &self,
        card_ref: &str,
        roles: &[String],
    ) -> Result<String> {
        let guard = self
            .server
            .lock()
            .map_err(|_| napi::Error::from_reason("test server lock poisoned".to_owned()))?;
        let server = guard
            .as_ref()
            .ok_or_else(|| napi::Error::from_reason("test server is shut down".to_owned()))?;
        let parsed: wyrd_spec::reference::CardRef = card_ref.parse().map_err(|error| {
            napi::Error::from_reason(format!(
                "`{card_ref}` is not a card identity string: {error}"
            ))
        })?;
        let roles: Vec<&str> = roles.iter().map(String::as_str).collect();
        let bootstrap = wyrd_runtime::runtime()
            .block_on(server.credential_registered_service(&parsed, &roles))
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

    /// Mint an authenticated token without `bifrost_query:read`.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the harness is closed or token issuance fails.
    #[napi]
    pub fn query_denied_token(&self) -> Result<String> {
        let guard = self
            .server
            .lock()
            .map_err(|_| napi::Error::from_reason("test server lock poisoned".to_owned()))?;
        let server = guard
            .as_ref()
            .ok_or_else(|| napi::Error::from_reason("test server is shut down".to_owned()))?;
        wyrd_runtime::runtime()
            .block_on(server.query_denied_token())
            .map_err(|error| napi::Error::from_reason(error.to_string()))
    }

    /// Return the durable read-decision audit count for the fixture tenant.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the harness is closed or the audit query fails.
    #[napi]
    pub fn bifrost_read_decision_count(&self) -> Result<i64> {
        let guard = self
            .server
            .lock()
            .map_err(|_| napi::Error::from_reason("test server lock poisoned".to_owned()))?;
        let server = guard
            .as_ref()
            .ok_or_else(|| napi::Error::from_reason("test server is shut down".to_owned()))?;
        wyrd_runtime::runtime()
            .block_on(server.bifrost_read_decision_count())
            .map_err(|error| napi::Error::from_reason(error.to_string()))
    }

    /// Return the staged allowed describe decisions for one table FQN.
    ///
    /// Every server describe stages exactly one decision, so a journey started
    /// with audit publication disabled reads how many schema describes a table
    /// has received.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the harness is closed or the audit query fails.
    #[napi]
    pub fn table_describe_count(&self, fqn: String) -> Result<i64> {
        let guard = self
            .server
            .lock()
            .map_err(|_| napi::Error::from_reason("test server lock poisoned".to_owned()))?;
        let server = guard
            .as_ref()
            .ok_or_else(|| napi::Error::from_reason("test server is shut down".to_owned()))?;
        let result = wyrd_runtime::runtime()
            .block_on(server.table_describe_count(&fqn))
            .map_err(|error| napi::Error::from_reason(error.to_string()));
        drop(fqn);
        result
    }

    /// Make every describe of one table FQN fail until restored.
    ///
    /// The server answers the failed describe with
    /// `WYRD_VALA_500_AUDIT_UNAVAILABLE`.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the harness is closed, the FQN is not a
    /// dotted identifier, or installing the fault fails.
    #[napi]
    pub fn fail_table_describe(&self, fqn: String) -> Result<()> {
        let guard = self
            .server
            .lock()
            .map_err(|_| napi::Error::from_reason("test server lock poisoned".to_owned()))?;
        let server = guard
            .as_ref()
            .ok_or_else(|| napi::Error::from_reason("test server is shut down".to_owned()))?;
        let result = wyrd_runtime::runtime()
            .block_on(server.fail_table_describe(&fqn))
            .map_err(|error| napi::Error::from_reason(error.to_string()));
        drop(fqn);
        result
    }

    /// Remove the describe fault installed by `fail_table_describe`.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the harness is closed or removing the fault
    /// fails.
    #[napi]
    pub fn restore_table_describe(&self) -> Result<()> {
        let guard = self
            .server
            .lock()
            .map_err(|_| napi::Error::from_reason("test server lock poisoned".to_owned()))?;
        let server = guard
            .as_ref()
            .ok_or_else(|| napi::Error::from_reason("test server is shut down".to_owned()))?;
        wyrd_runtime::runtime()
            .block_on(server.restore_table_describe())
            .map_err(|error| napi::Error::from_reason(error.to_string()))
    }

    /// Truncate the next query after its schema frame.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the server lock is poisoned or the server is
    /// shut down.
    #[napi]
    pub fn fail_next_query_after_schema(&self) -> Result<()> {
        let guard = self
            .server
            .lock()
            .map_err(|_| napi::Error::from_reason("test server lock poisoned".to_owned()))?;
        let server = guard
            .as_ref()
            .ok_or_else(|| napi::Error::from_reason("test server is shut down".to_owned()))?;
        server.fail_next_query_after_schema();
        Ok(())
    }

    /// Truncate the next query after its first batch frame.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the server lock is poisoned or the server is
    /// shut down.
    #[napi]
    pub fn fail_next_query_after_batch(&self) -> Result<()> {
        let guard = self
            .server
            .lock()
            .map_err(|_| napi::Error::from_reason("test server lock poisoned".to_owned()))?;
        let server = guard
            .as_ref()
            .ok_or_else(|| napi::Error::from_reason("test server is shut down".to_owned()))?;
        server.fail_next_query_after_batch();
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

/// Starts a real bound Wyrd test server and mints an admin access token.
///
/// `auditPublication: false` keeps staged audit rows in place so a journey can
/// count describe decisions; omitted, publication runs as in production.
/// `verificationRuntime: true` composes the verification runtime, so Drift
/// Verifier baselines fit and manual runs execute and persist results; omitted,
/// it stays off so queue-driving journeys are not raced.
///
/// # Errors
///
/// Returns a napi error when server startup, service bootstrap, API-key
/// exchange, or URL discovery fails.
#[napi]
pub fn start_test_server(
    audit_publication: Option<bool>,
    verification_runtime: Option<bool>,
) -> Result<NativeWyrdTestServer> {
    wyrd_runtime::runtime().block_on(Box::pin(start_test_server_async(
        audit_publication.unwrap_or(true),
        verification_runtime.unwrap_or(false),
    )))
}

/// Starts the bound harness inside Wyrd's shared runtime, disabling audit
/// publication when `audit_publication` is false and composing the
/// verification runtime when `verification_runtime` is true.
///
/// # Errors
///
/// Returns a napi error when any server setup step fails.
async fn start_test_server_async(
    audit_publication: bool,
    verification_runtime: bool,
) -> Result<NativeWyrdTestServer> {
    let mut builder = WyrdTestServer::builder();
    if !audit_publication {
        builder = builder.without_audit_publication_for_test();
    }
    if verification_runtime {
        builder = builder.with_verification_runtime_for_test();
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
            audit: None,
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
