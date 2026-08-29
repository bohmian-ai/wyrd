//! Shared production-harness setup for the Scribe suite.
//!
//! Every test in this binary uses the S9 harness and public routes: a real
//! server, real Postgres, the real object store, the production Scribe service
//! and the production Oracle read path. Nothing here builds a parallel topology
//! or writes a durable row the production path did not produce.

use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::scribe::geometry::ScribeGeometry;
use wyrd_spec::DataTenantId;
use wyrd_testing::WyrdTestServer;

use arrow::datatypes::{DataType, Field};

/// Starts one bound production server with the default Scribe geometry.
///
/// Bound rather than in-process because every Scribe case here drives public
/// gRPC ingest and the public query route, which need real endpoints.
pub(super) async fn start_scribe_server() -> WyrdTestServer {
    WyrdTestServer::start_bound()
        .await
        .expect("the Scribe production harness starts")
}

/// Starts one bound production server with an explicit Scribe geometry.
///
/// The geometry is the only production control a scaled case may move: it lets
/// a test reach a rotation, a target roll or a residue boundary without writing
/// production-sized data, while every other control stays exactly what
/// production uses.
pub(super) async fn start_scribe_server_with_geometry(geometry: ScribeGeometry) -> WyrdTestServer {
    WyrdTestServer::builder()
        .with_scribe_geometry_for_test(geometry)
        .start_bound()
        .await
        .expect("the Scribe production harness starts with the requested geometry")
}

/// Registers one single-column table for a tenant through the real catalog.
pub(super) async fn register_table(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    namespace: BifrostNamespace,
    name: &str,
) -> String {
    server
        .create_bifrost_table_for_test(CreateTableRequest {
            table: TableRef::new(namespace, name),
            user_fields: vec![Field::new("value", DataType::Int64, false)],
            tenant,
            physical_layout: None,
            audit: None,
        })
        .await
        .expect("the catalog registers the table");
    format!("{}.{name}", namespace.as_str())
}

/// Builds a unique table name for one case.
pub(super) fn unique_table(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::now_v7().simple())
}

/// Builds one authenticated public SDK client bound to `tenant`.
///
/// Every Scribe case drives ingest and query through the same public routes a
/// real caller uses, so each tenant in a case needs its own service principal
/// and API key rather than a shared fixture credential.
pub(super) async fn tenant_client(
    server: &WyrdTestServer,
    tenant: DataTenantId,
) -> wyrd_client::WyrdClient {
    let bootstrap = server
        .bootstrap_service_in_tenant(tenant, &unique_table("scribe_client"), &["admin"])
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

/// Returns the single-column ingress schema `register_table` declares.
pub(super) fn value_schema() -> std::sync::Arc<arrow::datatypes::Schema> {
    std::sync::Arc::new(arrow::datatypes::Schema::new(vec![Field::new(
        "value",
        DataType::Int64,
        false,
    )]))
}

/// Returns the ingress schema that also carries the client's own event time.
///
/// `wyrd_event_time` is a managed column: a caller may supply it and Scribe
/// lifts the value verbatim into the managed slot, which is how a case selects
/// the physical partition its rows land in rather than accepting wall clock.
pub(super) fn event_time_schema() -> std::sync::Arc<arrow::datatypes::Schema> {
    std::sync::Arc::new(arrow::datatypes::Schema::new(vec![
        Field::new("value", DataType::Int64, false),
        Field::new(
            wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME,
            DataType::Timestamp(arrow::datatypes::TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
    ]))
}

/// Encodes one batch as the native Arrow IPC stream public ingest accepts.
pub(super) fn encode_ipc(batch: &arrow::record_batch::RecordBatch) -> Vec<u8> {
    let mut ipc = Vec::new();
    let mut writer = arrow::ipc::writer::StreamWriter::try_new(&mut ipc, batch.schema().as_ref())
        .expect("IPC writer");
    writer.write(batch).expect("IPC batch");
    writer.finish().expect("IPC terminal");
    ipc
}

/// Appends one fixed-identity batch of values through public gRPC ingest.
///
/// Returns the ingest error rather than panicking so pressure, fencing and
/// replay cases can assert on the exact typed refusal a real client receives.
/// Failing to reach the route at all is a harness fault, not a refusal, so
/// connect failures still panic.
///
/// # Errors
///
/// Returns the stable Wyrd error the public ingest route produced.
pub(super) async fn append_values(
    client: &wyrd_client::WyrdClient,
    table: &str,
    batch_id: uuid::Uuid,
    rows: &[i64],
) -> Result<(), wyrd_spec::error::WyrdError> {
    let schema = value_schema();
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
        .insert_batch(table, batch_id.into_bytes(), encode_ipc(&batch))
        .await
        .map(|_| ())
}

/// Appends one batch whose rows all carry the caller's chosen event time.
///
/// # Errors
///
/// Returns the stable Wyrd error the public ingest route produced.
pub(super) async fn append_values_at(
    client: &wyrd_client::WyrdClient,
    table: &str,
    batch_id: uuid::Uuid,
    rows: &[i64],
    event_time: chrono::DateTime<chrono::Utc>,
) -> Result<(), wyrd_spec::error::WyrdError> {
    let schema = event_time_schema();
    let batch = arrow::record_batch::RecordBatch::try_new(
        std::sync::Arc::clone(&schema),
        vec![
            std::sync::Arc::new(arrow::array::Int64Array::from(rows.to_vec())),
            std::sync::Arc::new(
                arrow::array::TimestampMicrosecondArray::from(vec![
                    event_time.timestamp_micros();
                    rows.len()
                ])
                .with_timezone("UTC"),
            ),
        ],
    )
    .expect("event-time batch");
    vala_sdk::grpc::BifrostGrpcTransport::connect(client)
        .await
        .expect("public ingest transport connects")
        .insert_batch(table, batch_id.into_bytes(), encode_ipc(&batch))
        .await
        .map(|_| ())
}

/// Reads one table's values back through the public strict fused query route.
///
/// Strict freshness and fused visibility are what make the read an authority
/// check: the answer must come from whichever source currently owns the rows,
/// not from whatever happens to be cheapest.
pub(super) async fn read_values(client: &wyrd_client::WyrdClient, table: &str) -> Vec<i64> {
    read_sql(client, &format!("SELECT value FROM {table}")).await
}

/// Runs one strict fused public query and collects its `value` column.
pub(super) async fn read_sql(client: &wyrd_client::WyrdClient, sql: &str) -> Vec<i64> {
    let mut stream = vala_sdk::query::QueryClient::new(client)
        .query(&wyrd_spec::vala::api::BifrostQueryRequest {
            sql: sql.to_owned(),
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
            .expect("query result has a value column")
            .as_any()
            .downcast_ref::<arrow::array::Int64Array>()
            .expect("value column is Int64");
        values.extend(column.values().iter().copied());
    }
    values
}

/// Returns the sorted values a table currently reads back.
pub(super) async fn sorted_values(client: &wyrd_client::WyrdClient, table: &str) -> Vec<i64> {
    let mut values = read_values(client, table).await;
    values.sort_unstable();
    values
}

/// Counts every published hot object one tenant owns for one table.
pub(super) async fn published_object_count(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    namespace: BifrostNamespace,
    table_name: &str,
) -> usize {
    server
        .published_hot_files_for_test(tenant, namespace.as_str(), table_name)
        .await
        .expect("published hot files are inspectable")
        .len()
}
