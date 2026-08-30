//! Shared fixtures and helpers for the Oracle journey modules.
//!
//! Every item here is used by more than one module of the `oracle` binary.
//! A helper used by exactly one module lives in that module instead, so that
//! reading a journey does not mean reading this file first. Contains no
//! tests.

use arrow::array::{
    ArrayRef, FixedSizeBinaryBuilder, Int32Array, Int64Array, StringArray,
    TimestampMicrosecondArray,
};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use chrono::Utc;
use opendal::Buffer;
use parquet::arrow::ArrowWriter;
use std::sync::Arc;
use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef, TenantTableBinding};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::parquet::writer_properties::bifrost_writer_properties;
use vala_bifrost_redux::schema::with_managed_columns;
use vala_sdk::QueryClient;
use wyrd_client::WyrdClient;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    AuditDecision, AuditEvent, AuditResult, AuthMethod, BifrostQueryRequest, FreshnessPolicy,
    VisibilityMode,
};
use wyrd_testing::Bootstrap;
use wyrd_testing::bifrost::WyrdTestCluster;

/// Boxed error carried by every journey helper that can fail.
pub(crate) type JourneyError = Box<dyn std::error::Error + Send + Sync>;

/// Sums every metric sample matching one production family across all labels.
pub(crate) fn sum_metric(
    delta: &wyrd_testing::bifrost::telemetry::BifrostTelemetryDelta,
    family: &str,
) -> f64 {
    delta
        .metrics
        .iter()
        .filter(|sample| sample.family == family)
        .map(|sample| sample.value)
        .sum()
}
/// Register one tenant-owned Redux table through the server-owned catalog.
pub(crate) async fn register_table(
    server: &wyrd_testing::WyrdTestServer,
    tenant: DataTenantId,
    table: &str,
) -> Result<(), JourneyError> {
    server
        .state()
        .bifrost_catalog()
        .expect("Scribe composition retains the shared catalog")
        .create_table(CreateTableRequest {
            table: TableRef::new(BifrostNamespace::Bifrost, table),
            user_fields: vec![
                Field::new("id", DataType::Int64, false),
                Field::new("filter_key", DataType::Utf8, false),
                Field::new("unused_payload", DataType::Utf8, false),
            ],
            tenant,
            physical_layout: None,
            audit: None,
        })
        .await?;
    Ok(())
}
/// Build one authenticated public client for the fixture tenant.
pub(crate) async fn client(
    server: &wyrd_testing::WyrdTestServer,
    name: &str,
) -> Result<WyrdClient, JourneyError> {
    client_for_tenant(server, server.data_tenant_id(), name).await
}

/// Build one authenticated public client for an explicit tenant.
pub(crate) async fn client_for_tenant(
    server: &wyrd_testing::WyrdTestServer,
    tenant: DataTenantId,
    name: &str,
) -> Result<WyrdClient, JourneyError> {
    let bootstrap = server
        .bootstrap_service_in_tenant(tenant, name, &["admin"])
        .await?;
    client_from_bootstrap(server, bootstrap).await
}

/// Build a client while retaining the bootstrap principal for exact audit correlation.
pub(crate) async fn client_from_bootstrap(
    server: &wyrd_testing::WyrdTestServer,
    bootstrap: Bootstrap,
) -> Result<WyrdClient, JourneyError> {
    let api_key = match bootstrap {
        Bootstrap::Machine { api_key, .. } => api_key,
        Bootstrap::User { .. } => return Err("machine bootstrap returned user".into()),
    };
    Ok(WyrdClient::with_config(ClientConfig {
        grpc: GrpcConfig {
            endpoint: server.grpc_url().ok_or("missing gRPC URL")?,
            connect_retries: 0,
            max_message_bytes: 32 * 1024 * 1024,
            ..GrpcConfig::default()
        },
        http: HttpConfig {
            base_url: server.base_url().ok_or("missing HTTP URL")?.to_owned(),
            ..HttpConfig::default()
        },
        api_key: Some(api_key),
        ..ClientConfig::default()
    })?)
}
/// Persist one foreign-tenant physical row beneath the production provider union.
///
/// # Errors
///
/// Returns an Arrow, Parquet, storage, tenant-SQL, or manifest persistence error.
pub(crate) async fn seed_foreign_hot_row(
    cluster: &WyrdTestCluster,
    owner: DataTenantId,
    table: &str,
    foreign: DataTenantId,
    path_tag: &str,
    node_id: uuid::Uuid,
) -> Result<(), JourneyError> {
    let table_ref = TableRef::new(BifrostNamespace::Bifrost, table);
    let binding = TenantTableBinding::resolve((owner, table_ref))?;
    let schema = Arc::new(Schema::new(with_managed_columns(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("filter_key", DataType::Utf8, false),
        Field::new("unused_payload", DataType::Utf8, false),
    ])));
    let mut batch_ids = FixedSizeBinaryBuilder::with_capacity(1, 16);
    batch_ids.append_value(uuid::Uuid::now_v7().as_bytes())?;
    let batch = RecordBatch::try_new(
        Arc::clone(&schema),
        vec![
            Arc::new(Int64Array::from(vec![999_i64])) as ArrayRef,
            Arc::new(StringArray::from(vec!["foreign"])),
            Arc::new(StringArray::from(vec![unused_payload(999)])),
            Arc::new(StringArray::from(vec![None::<String>])),
            Arc::new(StringArray::from(vec![None::<String>])),
            Arc::new(StringArray::from(vec![uuid::Uuid::now_v7().to_string()])),
            Arc::new(StringArray::from(vec![RequestId::now_v7().to_string()])),
            Arc::new(TimestampMicrosecondArray::from(vec![1_000_000_i64]).with_timezone("UTC")),
            Arc::new(TimestampMicrosecondArray::from(vec![1_000_001_i64]).with_timezone("UTC")),
            Arc::new(batch_ids.finish()),
            Arc::new(Int32Array::from(vec![0])),
            Arc::new(StringArray::from(vec![foreign.to_string()])),
        ],
    )?;
    let mut parquet = Vec::new();
    let properties = bifrost_writer_properties(batch.num_rows(), &[]);
    let mut writer = ArrowWriter::try_new(&mut parquet, schema, Some(properties))?;
    writer.write(&batch)?;
    writer.close()?;
    let path = format!("{}/{path_tag}.parquet", binding.object_prefix);
    cluster
        .storage_operator()
        .write(&path, Buffer::from(parquet.clone()))
        .await?;
    let event = AuditEvent::new(
        RequestId::now_v7(),
        None,
        "oracle.journey.foreign_row".to_owned(),
        "bifrost.oracle.journey".to_owned(),
        None,
        PrincipalId::new(uuid::Uuid::now_v7()),
        PrincipalKindTag::User,
        AuthMethod::Internal,
        "bifrost_query:read".to_owned(),
        AuditDecision::Allow,
        AuditResult::Success,
        "foreign tripwire fixture".to_owned(),
    );
    let mut conn = cluster.pg_fixture().tenant_conn_for(owner).await?;
    sqlx::query(
        "INSERT INTO vala.file_list \
         (id,data_tenant_id,namespace,table_name,file_path,file_size,row_count,min_event_time,max_event_time,partition_granularity,partition_start,node_id,writer_epoch,wal_lsn_min,wal_lsn_max,promotion_record) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,'day',$10,$11,1,9001,9001,'{\"fixture\": \"oracle-foreign-tripwire\"}'::jsonb)",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(owner.as_uuid())
    .bind(&binding.logical_namespace)
    .bind(&binding.table_name)
    .bind(&path)
    .bind(i64::try_from(parquet.len())?)
    .bind(1_i64)
    .bind(Utc::now())
    .bind(Utc::now())
    .bind(chrono::DateTime::<Utc>::UNIX_EPOCH)
    .bind(node_id)
    .execute(&mut **conn.transaction())
    .await?;
    vala_sql::queries::audit_outbox::append_audit(&mut conn, &event).await?;
    conn.commit().await?;
    Ok(())
}
/// Drain a public query stream and require its terminal row count to match frames.
pub(crate) async fn query_rows(
    client: &WyrdClient,
    table: &str,
    visibility: VisibilityMode,
) -> Result<u64, JourneyError> {
    let mut stream = QueryClient::new(client)
        .query(&BifrostQueryRequest {
            sql: format!(
                "SELECT id, filter_key, unused_payload FROM vala.bifrost.{table} ORDER BY id"
            ),
            visibility,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: None,
        })
        .await?;
    let mut rows = 0_u64;
    while let Some(batch) = stream.next_batch().await? {
        rows = rows.saturating_add(u64::try_from(batch.num_rows())?);
    }
    let terminal = stream.terminal().ok_or("query terminal missing")?;
    if terminal.row_count != rows {
        return Err("terminal row count differs from Arrow frames".into());
    }
    Ok(rows)
}
/// Return a collision-free SQL identifier for one serialized journey.
pub(crate) fn unique_table(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::now_v7().simple())
}

/// Number of hex characters in one row's `unused_payload` value.
///
/// Large enough that the column's Parquet chunk dominates a single-row file,
/// so a query that does not request it measurably scans fewer bytes than one
/// that does. Small enough that a journey's fixture stays cheap to write.
const UNUSED_PAYLOAD_CHARS: usize = 4096;

/// Builds one row's deterministic, high-entropy `unused_payload` value.
///
/// The value is derived from `id` alone, so a fixture is reproducible across
/// runs and across nodes, and it is generated by a splitmix64 sequence rather
/// than a repeated literal so Parquet's dictionary and compression codecs
/// cannot collapse it to near-zero bytes. A payload that compressed away would
/// make a projection proof measure nothing.
pub(crate) fn unused_payload(id: i64) -> String {
    #[allow(clippy::cast_sign_loss)]
    let mut state = 0x9E37_79B9_7F4A_7C15_u64
        .wrapping_mul(id as u64)
        .wrapping_add(1);
    let mut out = String::with_capacity(UNUSED_PAYLOAD_CHARS);
    while out.len() < UNUSED_PAYLOAD_CHARS {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        out.push_str(&format!("{z:016x}"));
    }
    out.truncate(UNUSED_PAYLOAD_CHARS);
    out
}
