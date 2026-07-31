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
    /// Admin access token minted through the real auth route.
    token: String,
    /// Registered empty table reached by the public query journey.
    table_fqn: String,
}

#[napi]
impl NativeWyrdTestServer {
    /// Returns the bound HTTP base URL.
    #[napi(getter)]
    pub fn base_url(&self) -> String {
        self.base_url.clone()
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
    wyrd_runtime::runtime().block_on(start_test_server_async())
}

/// Starts the bound harness inside Wyrd's shared runtime.
///
/// # Errors
///
/// Returns a napi error when any server setup step fails.
async fn start_test_server_async() -> napi::Result<NativeWyrdTestServer> {
    let server = WyrdTestServer::start_bound()
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
    Ok(NativeWyrdTestServer {
        server: Arc::new(Mutex::new(Some(server))),
        base_url,
        token,
        table_fqn: format!("vala.bifrost.{table_name}"),
    })
}
