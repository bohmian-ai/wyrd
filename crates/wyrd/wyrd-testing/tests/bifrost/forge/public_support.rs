//! Public-route helpers for the Forge journey group.
//!
//! Contains no tests. Every item here reaches the server the way a real caller
//! does — registration through the real catalog route, ingest over public gRPC,
//! reads over the public strict fused query route — so a journey that uses them
//! is exercising the shipped surface rather than an in-process engine.

use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use wyrd_spec::DataTenantId;
use wyrd_testing::WyrdTestServer;

use arrow::datatypes::{DataType, Field};

/// Registers one single-column table for `tenant` through the real catalog.
///
/// Returns the qualified name a public query addresses it by.
///
/// # Panics
///
/// Panics when the catalog refuses the registration.
pub(crate) async fn register_table(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    name: &str,
) -> String {
    server
        .create_bifrost_table_for_test(CreateTableRequest {
            table: TableRef::new(BifrostNamespace::Datasets, name),
            user_fields: vec![Field::new("value", DataType::Int64, false)],
            tenant,
            physical_layout: None,
            audit: None,
        })
        .await
        .expect("the catalog registers the journey table");
    format!("{}.{name}", BifrostNamespace::Datasets.as_str())
}

/// Builds a unique table name for one journey.
pub(crate) fn unique_table(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::now_v7().simple())
}

/// Builds one authenticated public SDK client bound to `tenant`.
///
/// Each tenant gets its own service principal and API key, which is what makes
/// a cross-tenant read a genuine authorization decision rather than a filter
/// applied to a shared credential.
///
/// # Panics
///
/// Panics when the tenant cannot be bootstrapped or the server is not bound.
pub(crate) async fn tenant_client(
    server: &WyrdTestServer,
    tenant: DataTenantId,
) -> wyrd_client::WyrdClient {
    let bootstrap = server
        .bootstrap_service_in_tenant(tenant, &unique_table("forge_client"), &["admin"])
        .await
        .expect("tenant service bootstrap");
    let api_key = bootstrap.api_key().expect("service API key").clone();
    wyrd_client::WyrdClient::with_config(wyrd_client::config::ClientConfig {
        grpc: wyrd_client::transport::GrpcConfig {
            endpoint: server.grpc_url().expect("bound gRPC URL"),
            connect_retries: 0,
            ..wyrd_client::transport::GrpcConfig::default()
        },
        http: wyrd_client::transport::HttpConfig {
            base_url: server.base_url().expect("bound HTTP URL").to_owned(),
            ..wyrd_client::transport::HttpConfig::default()
        },
        api_key: Some(api_key),
        ..wyrd_client::config::ClientConfig::default()
    })
    .expect("tenant SDK client")
}

/// Encodes one batch as the native Arrow IPC stream public ingest accepts.
///
/// # Panics
///
/// Panics when the batch cannot be encoded, which is a fixture fault.
fn encode_ipc(batch: &arrow::record_batch::RecordBatch) -> Vec<u8> {
    let mut ipc = Vec::new();
    let mut writer = arrow::ipc::writer::StreamWriter::try_new(&mut ipc, batch.schema().as_ref())
        .expect("IPC writer");
    writer.write(batch).expect("IPC batch");
    writer.finish().expect("IPC terminal");
    ipc
}

/// Appends one batch of values through public authenticated gRPC ingest.
///
/// # Panics
///
/// Panics when the transport cannot connect or the append is refused; a
/// journey's ingest is expected to be accepted, so a refusal is a failure.
pub(crate) async fn append_values(client: &wyrd_client::WyrdClient, table: &str, rows: &[i64]) {
    let schema = std::sync::Arc::new(arrow::datatypes::Schema::new(vec![Field::new(
        "value",
        DataType::Int64,
        false,
    )]));
    let batch = arrow::record_batch::RecordBatch::try_new(
        std::sync::Arc::clone(&schema),
        vec![std::sync::Arc::new(arrow::array::Int64Array::from(
            rows.to_vec(),
        ))],
    )
    .expect("value batch");
    vala_sdk::grpc::BifrostGrpcTransport::connect(client)
        .await
        .expect("public ingest transport connects")
        .insert_batch(table, uuid::Uuid::now_v7().into_bytes(), encode_ipc(&batch))
        .await
        .expect("the public append is acknowledged");
}

/// Runs one strict fused public query and returns its sorted `value` column.
///
/// Strict freshness and fused visibility are what make the read an authority
/// check: the answer must come from whichever tier currently owns the rows, so
/// a promotion or a rewrite that lost, duplicated, or stranded a row shows up
/// here rather than only in the catalog.
///
/// # Panics
///
/// Panics when the query cannot start or stream, or does not carry the column.
pub(crate) async fn read_sorted_values(client: &wyrd_client::WyrdClient, table: &str) -> Vec<i64> {
    let sql = format!("SELECT value FROM {table}");
    let mut stream = vala_sdk::query::QueryClient::new(client)
        .query(&wyrd_spec::vala::api::BifrostQueryRequest {
            sql: sql.clone(),
            visibility: wyrd_spec::vala::api::VisibilityMode::Fused,
            freshness: wyrd_spec::vala::api::FreshnessPolicy::Strict,
            deadline_ms: Some(120_000),
        })
        .await
        .unwrap_or_else(|error| panic!("public query `{sql}` starts: {}", error.detail()));
    let mut values = Vec::new();
    while let Some(batch) = stream
        .next_batch()
        .await
        .unwrap_or_else(|error| panic!("public query `{sql}` streams: {}", error.detail()))
    {
        let column = batch
            .column_by_name("value")
            .expect("the query result carries the user column")
            .as_any()
            .downcast_ref::<arrow::array::Int64Array>()
            .expect("the user column stays Int64");
        values.extend(column.values().iter().copied());
    }
    values.sort_unstable();
    values
}

/// Attempts one public query and returns the stable error when it is refused.
///
/// Used only for the cross-tenant probe, where the interesting outcome is the
/// refusal itself: a tenant that can name a neighbour's table at all is a
/// tenancy defect, so the journey must see an error rather than empty rows.
pub(crate) async fn try_read(
    client: &wyrd_client::WyrdClient,
    table: &str,
) -> Result<Vec<i64>, vala_sdk::ValaSdkError> {
    let sql = format!("SELECT value FROM {table}");
    let mut stream = vala_sdk::query::QueryClient::new(client)
        .query(&wyrd_spec::vala::api::BifrostQueryRequest {
            sql,
            visibility: wyrd_spec::vala::api::VisibilityMode::Fused,
            freshness: wyrd_spec::vala::api::FreshnessPolicy::Strict,
            deadline_ms: Some(120_000),
        })
        .await?;
    let mut values = Vec::new();
    while let Some(batch) = stream.next_batch().await? {
        if let Some(column) = batch
            .column_by_name("value")
            .and_then(|column| column.as_any().downcast_ref::<arrow::array::Int64Array>())
        {
            values.extend(column.values().iter().copied());
        }
    }
    values.sort_unstable();
    Ok(values)
}
