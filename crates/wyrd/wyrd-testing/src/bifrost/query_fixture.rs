//! Small real-server Bifrost query fixture shared by agent-surface journeys.

use std::fmt::Display;
use std::sync::Arc;

use crate::bifrost::write::BifrostWriter;
use crate::server::{WyrdTestServer, WyrdTestServerError};
use arrow::datatypes::{DataType, Field, Schema};
use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef, TimeGranularity};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::scribe::tail_rpc::{LocalTailReadTransport, TailReadTransport};
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::NodeId;

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

impl WyrdTestServer {
    /// Builds a sealed-plus-live Oracle query table and returns its fully
    /// qualified name with an access token that may query it.
    ///
    /// Two rows travel through the public gRPC ingest path and a third follows
    /// on the live tail, which Oracle reaches through a local tail transport
    /// registered for this server's Scribe stream. Unless `fused`, Scribe seals
    /// the first two rows before the third is written; with `fused`, Oracle
    /// prefers the local tail route and all three rows stay unsealed. The token
    /// comes from exchanging a fresh admin service key through the real auth
    /// route. Language-client journeys use this as their query prerequisite.
    ///
    /// # Errors
    /// Returns `HarnessStart` when bootstrap does not yield a machine key, the
    /// catalog refuses the table, or the Scribe or Oracle runtime is absent,
    /// and the underlying Wyrd error when ingest, flush, or token exchange
    /// fails.
    ///
    /// # Panics
    /// Panics when the server composes no Bifrost catalog, which a test server
    /// always does.
    pub async fn prepare_oracle_query_fixture(
        &self,
        fused: bool,
    ) -> Result<(String, String), WyrdError> {
        let bootstrap = self
            .bootstrap_service(
                &format!("python-oracle-query-{}", uuid::Uuid::now_v7().simple()),
                &["admin"],
            )
            .await
            .map_err(WyrdError::from)?;
        let api_key = bootstrap
            .api_key()
            .ok_or_else(|| harness_error("Oracle fixture bootstrap did not return an API key"))?;
        let table_name = format!("python_oracle_{}", uuid::Uuid::now_v7().simple());
        let table_fqn = format!("vala.bifrost.{table_name}");
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("value", DataType::Utf8, false),
        ]));
        self.state()
            .bifrost_catalog()
            .expect("test server exposes its Bifrost catalog")
            .create_table(CreateTableRequest {
                table: TableRef::new(BifrostNamespace::Bifrost, &table_name),
                user_fields: schema
                    .fields()
                    .iter()
                    .map(|field| field.as_ref().clone())
                    .collect(),
                tenant: self.data_tenant_id(),
                physical_layout: None,
                audit: None,
            })
            .await
            .map_err(harness_error)?;
        let writer = BifrostWriter::connect(
            ClientConfig {
                grpc: GrpcConfig {
                    endpoint: self.grpc_url().unwrap_or_default(),
                    connect_retries: 0,
                    ..GrpcConfig::default()
                },
                http: HttpConfig {
                    base_url: self.base_url().unwrap_or_default().to_owned(),
                    ..HttpConfig::default()
                },
                credential: Some(api_key.clone()),
                ..ClientConfig::default()
            },
            bootstrap
                .card_ref()
                .ok_or_else(|| {
                    harness_error("Oracle fixture bootstrap is not a machine principal")
                })?
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
        let ingest = self
            .state()
            .bifrost_ingest()
            .ok_or_else(|| harness_error("Scribe runtime is unavailable"))?;
        let stream = ingest
            .scribe()
            .tail_service()
            .map_err(harness_error)?
            .stream();
        let writer_epoch = u64::try_from(stream.writer_epoch.as_i64()).map_err(harness_error)?;
        let time_partition = TimeGranularity::Hour
            .bucket(chrono::Utc::now())
            .map_err(harness_error)?
            .to_wire();
        let tail_transport: Arc<dyn TailReadTransport> =
            Arc::new(LocalTailReadTransport::new(ingest.tail_reader()));
        let oracle = self
            .state()
            .bifrost_query()
            .ok_or_else(|| harness_error("Oracle runtime is unavailable"))?
            .oracle();
        if fused {
            oracle.prefer_local_tail_routes_for_test();
        }
        oracle.tail_transports().insert_live_stream_for_tenant(
            self.data_tenant_id(),
            &table_fqn,
            NodeId::new(stream.node_id.as_uuid()),
            writer_epoch,
            time_partition,
            tail_transport,
        );
        if !fused {
            self.flush_bifrost().await.map_err(WyrdError::from)?;
        }
        writer
            .write(
                &table_fqn,
                &schema,
                [br#"{"id": 3, "value": "live"}"#.to_vec()],
            )
            .await?;
        let token = self
            .exchange_api_key(api_key)
            .await
            .map_err(WyrdError::from)?;
        Ok((table_fqn, token))
    }
}

/// Projects one Oracle fixture setup failure onto the harness catalog entry.
fn harness_error(error: impl Display) -> WyrdError {
    WyrdError::HarnessStart {
        message: error.to_string(),
        details: serde_json::json!({}),
    }
}
