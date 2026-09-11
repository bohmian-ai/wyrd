//! Small real-server Bifrost query fixture shared by agent-surface journeys.

use std::sync::Arc;

use crate::bifrost::write::BifrostWriter;
use crate::server::{WyrdTestServer, WyrdTestServerError};
use arrow::datatypes::{DataType, Field, Schema};
use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};

/// Deterministic two-row query fixture returned by [`seed_query_fixture`].
#[derive(Debug, Clone)]
pub struct SeededBifrostQuery {
    /// Bound Wyrd HTTP endpoint for CLI or MCP clients.
    pub endpoint: String,
    /// Short-lived bearer exchanged through the real auth route.
    pub token: String,
    /// Fully qualified Bifrost table name.
    pub table: String,
    /// Exact values selected by the fixture query.
    pub expected_rows: Vec<serde_json::Value>,
}

/// Register, ingest, flush, and authenticate a deterministic two-row table.
///
/// The helper intentionally uses the production catalog, gRPC ingest path, and
/// Scribe flush so CLI and MCP tests exercise the same server seams as callers.
///
/// # Errors
///
/// Returns a [`WyrdTestServerError`] when bootstrap, table registration, Arrow
/// encoding, ingest, flush, endpoint discovery, or token exchange fails.
pub async fn seed_query_fixture(
    server: &WyrdTestServer,
    name: &str,
) -> Result<SeededBifrostQuery, WyrdTestServerError> {
    let bootstrap = server.bootstrap_service(name, &["admin"]).await?;
    let api_key = bootstrap
        .api_key()
        .ok_or_else(|| WyrdTestServerError::Auth("query fixture requires a service key".into()))?
        .clone();
    let safe_name = name.replace('-', "_");
    let table_name = format!(
        "agent_query_{}_{}",
        safe_name,
        uuid::Uuid::now_v7().simple()
    );
    let table = format!("vala.bifrost.{table_name}");
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("value", DataType::Utf8, false),
    ]));
    server
        .state()
        .bifrost_catalog()
        .expect("production server has Bifrost catalog")
        .create_table(CreateTableRequest {
            table: TableRef::new(BifrostNamespace::Bifrost, &table_name),
            user_fields: schema
                .fields()
                .iter()
                .map(|field| field.as_ref().clone())
                .collect(),
            tenant: server.data_tenant_id(),
            physical_layout: None,
            audit: None,
        })
        .await
        .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;

    let writer = BifrostWriter::connect(
        ClientConfig {
            grpc: GrpcConfig {
                endpoint: server
                    .grpc_url()
                    .ok_or_else(|| WyrdTestServerError::Start("missing gRPC URL".into()))?,
                connect_retries: 0,
                ..GrpcConfig::default()
            },
            http: HttpConfig {
                base_url: server
                    .base_url()
                    .ok_or_else(|| WyrdTestServerError::Start("missing HTTP URL".into()))?
                    .to_owned(),
                ..HttpConfig::default()
            },
            credential: Some(api_key.clone()),
            ..ClientConfig::default()
        },
        bootstrap
            .card_ref()
            .ok_or_else(|| {
                WyrdTestServerError::Auth("query fixture requires a machine principal".into())
            })?
            .clone(),
    )
    .await
    .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
    writer
        .write(
            &table,
            &schema,
            [
                br#"{"id": 1, "value": "first"}"#.to_vec(),
                br#"{"id": 2, "value": "second"}"#.to_vec(),
            ],
        )
        .await
        .map_err(|error| WyrdTestServerError::Start(error.to_string()))?;
    server.flush_bifrost().await?;
    let token = server.exchange_api_key(&api_key).await?;
    let endpoint = server
        .base_url()
        .ok_or_else(|| WyrdTestServerError::Start("missing HTTP URL".into()))?
        .to_owned();
    Ok(SeededBifrostQuery {
        endpoint,
        token,
        table,
        expected_rows: vec![
            serde_json::json!({"id": 1, "value": "first"}),
            serde_json::json!({"id": 2, "value": "second"}),
        ],
    })
}
