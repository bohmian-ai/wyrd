//! Test-tier napi projection of the in-process Wyrd server harness.

#![deny(missing_docs)]

use std::sync::{Arc, Mutex};

use arrow::datatypes::{DataType, Field};
use napi::Result;
use napi_derive::napi;
use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use wyrd_testing::Bootstrap;
use wyrd_testing::server::WyrdTestServer;

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
/// `providerBaseUrl` roots every built-in gateway adapter at one local mock
/// upstream (`OpenAI` under `/v1`), so a TypeScript gateway journey dispatches
/// over HTTP with the harness's operator credential bindings; without it the
/// gateway admits and accounts calls but reaches no provider.
///
/// # Errors
///
/// Returns a napi error when `providerBaseUrl` is not an absolute URL, or when
/// server startup, service bootstrap, API-key exchange, or URL discovery
/// fails.
#[napi]
pub fn start_test_server(provider_base_url: Option<String>) -> napi::Result<NativeWyrdTestServer> {
    let result = start_test_server_borrowed(provider_base_url.as_deref());
    drop(provider_base_url);
    result
}

/// Parses the N-API-owned provider root without extending its ownership into
/// the Rust harness call, then starts the harness.
///
/// # Errors
///
/// Returns a napi error when `provider_base_url` is not an absolute URL or any
/// server setup step fails.
fn start_test_server_borrowed(
    provider_base_url: Option<&str>,
) -> napi::Result<NativeWyrdTestServer> {
    let root = provider_base_url
        .map(|base| {
            url::Url::parse(base).map_err(|error| {
                napi::Error::from_reason(format!("invalid providerBaseUrl: {error}"))
            })
        })
        .transpose()?;
    wyrd_runtime::runtime().block_on(Box::pin(start_test_server_async(root)))
}

/// Starts the bound harness inside Wyrd's shared runtime, rooting the gateway
/// adapters at `provider_root` when a journey supplied one.
///
/// # Errors
///
/// Returns a napi error when any server setup step fails.
async fn start_test_server_async(
    provider_root: Option<url::Url>,
) -> napi::Result<NativeWyrdTestServer> {
    let mut builder = WyrdTestServer::builder();
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
