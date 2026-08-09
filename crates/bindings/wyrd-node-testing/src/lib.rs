//! Test-tier napi projection of the in-process Wyrd server harness.

#![deny(missing_docs)]

use std::sync::{Arc, Mutex};

use arrow::datatypes::{DataType, Field};
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

    /// Seed rows through the real gRPC ingest and Scribe flush paths.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the harness is closed or registration,
    /// encoding, ingest, or flushing fails.
    #[napi]
    pub fn seed_bifrost_rows(&self, table: String, rows: Vec<i64>) -> napi::Result<Vec<i64>> {
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
    #[napi]
    pub fn wait_for_bifrost_publication(&self) -> napi::Result<()> {
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
    fn seed_bifrost_rows_borrowed(&self, table: &str, rows: &[i64]) -> napi::Result<Vec<i64>> {
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

    /// Mint an authenticated token without `bifrost_query:read`.
    ///
    /// # Errors
    ///
    /// Returns a napi error when the harness is closed or token issuance fails.
    #[napi]
    pub fn query_denied_token(&self) -> napi::Result<String> {
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
    pub fn bifrost_read_decision_count(&self) -> napi::Result<i64> {
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
    #[napi]
    pub fn fail_next_query_after_schema(&self) -> napi::Result<()> {
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
    #[napi]
    pub fn fail_next_query_after_batch(&self) -> napi::Result<()> {
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
    pub fn shutdown(&self) -> napi::Result<()> {
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
/// # Errors
///
/// Returns a napi error when server startup, service bootstrap, API-key
/// exchange, or URL discovery fails.
#[napi]
pub fn start_test_server() -> napi::Result<NativeWyrdTestServer> {
    wyrd_runtime::runtime().block_on(Box::pin(start_test_server_async()))
}

/// Starts the bound harness inside Wyrd's shared runtime.
///
/// # Errors
///
/// Returns a napi error when any server setup step fails.
async fn start_test_server_async() -> napi::Result<NativeWyrdTestServer> {
    let server = Box::pin(WyrdTestServer::start_bound())
        .await
        .map_err(|error| napi::Error::from_reason(error.to_string()))?;
    let table_name = "typescript_oracle_query";
    server
        .state()
        .bifrost_redux
        .as_ref()
        .ok_or_else(|| napi::Error::from_reason("Redux catalog is unavailable".to_owned()))?
        .create_table(CreateTableRequest {
            table: TableRef::new(BifrostNamespace::Bifrost, table_name),
            user_fields: vec![Field::new("value", DataType::Int64, false)],
            tenant: server.data_tenant_id(),
            audit: None,
        })
        .await
        .map_err(|error| napi::Error::from_reason(error.to_string()))?;
    let bootstrap = server
        .bootstrap_service("typescript-oracle-query", &["admin"])
        .await
        .map_err(|error| napi::Error::from_reason(error.to_string()))?;
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
    })
}
