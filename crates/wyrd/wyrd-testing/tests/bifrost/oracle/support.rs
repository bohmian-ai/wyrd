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
use sha2::{Digest as _, Sha256};
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
/// The row is well formed in every respect except its tenancy, including the
/// durable SHA-256 the Scribe file-list writer always publishes. That matters:
/// the Oracle refuses a hot row whose checksum is not an identity before it
/// signs a descriptor, so a checksumless row would fail closed for the wrong
/// reason and never reach the tenant invariant this fixture exists to trip.
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
         (id,data_tenant_id,namespace,table_name,file_path,file_size,row_count,min_event_time,max_event_time,partition_granularity,partition_start,node_id,file_checksum,writer_epoch,wal_lsn_min,wal_lsn_max,promotion_record) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,'day',$10,$11,$12,1,9001,9001,'{\"fixture\": \"oracle-foreign-tripwire\"}'::jsonb)",
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
    .bind(hex::encode(Sha256::digest(&parquet)))
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
/// Large enough that the column is a real fraction of a published file rather
/// than a rounding error, and small enough that a journey writing dozens of
/// single-row batches through the public ingest path stays quick.
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

/// Bound on how many Forge planning passes the fixture will drive before it
/// gives up on compacting its first batch. Generous, because a pass may claim
/// nothing, retry, or lose a lease race; finite, because a stalled Forge must
/// fail the journey rather than hang it.
const COMPACTION_PASS_BUDGET: usize = 32;

/// Number of rows written before compaction. Every one of them is sealed as
/// its own Parquet file, so the Forge pass has `min_files` worth of real
/// inputs to rewrite into a single published data file.
/// Compacts every sealed file already written for `table`, so that a later
/// query reads them through the Iceberg snapshot rather than the hot manifest.
///
/// Three things have to happen for that, and none of them are automatic:
///
/// * The event-day partition holding the batch has to close. Forge does not
///   rewrite a partition it may still receive writes for, so the test clock is
///   advanced past it first.
/// * A planning pass has to run, and the worker it hands the task to has to
///   finish. The supervisor's own ticker is a minute long, so passes are
///   requested explicitly; the wait is on the worker completion observer,
///   because a planning pass returns as soon as the task is claimed.
/// * A task that lands in `retryable` has to become eligible again. Real
///   backoff is minutes; `release_forge_retries` moves the durable
///   `next_eligible_at` back instead of sleeping, leaving the failure
///   classification untouched.
///
/// The loop is bounded and its exit condition is the durable `compacted` flag,
/// not a pass or completion count: a pass that claimed nothing, and a
/// completion that rewrote some other table, must not be mistaken for this
/// batch having moved tiers.
///
/// # Errors
///
/// Returns an error when the observer is absent, when the clock cannot be
/// advanced, when a Postgres probe fails, or when fewer than `expected` inputs
/// are compacted before the loop's budget runs out.
pub(crate) async fn compact_sealed_batch(
    cluster: &WyrdTestCluster,
    tenant: wyrd_spec::DataTenantId,
    table: &str,
    expected: i64,
) -> Result<(), JourneyError> {
    let observer = cluster
        .forge_completion_observer()
        .ok_or("cluster was started without a Forge completion observer")?;
    for server in cluster.servers() {
        server
            .forge_clock()
            .advance(chrono::Duration::days(1))
            .map_err(|error| format!("close the written partition: {error}"))?;
    }
    for _ in 0..COMPACTION_PASS_BUDGET {
        let (compacted, _) = file_tier_counts(cluster, tenant, table).await?;
        if compacted >= expected {
            return Ok(());
        }
        release_forge_retries(cluster, tenant, table).await?;
        let target = observer.completed().saturating_add(1);
        cluster.request_forge_scheduler_pass_for_test();
        // A lapsed wait is not a failure on its own: the pass may legitimately
        // have found nothing to claim on this iteration. The durable flag
        // checked at the top of the next iteration is the real verdict.
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            observer.wait_for_at_least(target),
        )
        .await;
    }
    let (compacted, hot) = file_tier_counts(cluster, tenant, table).await?;
    Err(format!(
        "Forge compacted {compacted} of {expected} sealed inputs within \
         {COMPACTION_PASS_BUDGET} passes ({hot} still hot)"
    )
    .into())
}
/// Makes every `retryable` Forge task for one table immediately eligible.
///
/// Backoff between attempts is real production time, which a bounded journey
/// cannot wait out. Only `next_eligible_at` and `ready_at` move; the attempt
/// count and failure classification are left alone, so a task that is failing
/// for a real reason still exhausts its attempts and still reports why.
///
/// # Errors
///
/// Returns the SQLx error when the eligibility update cannot be applied.
pub(crate) async fn release_forge_retries(
    cluster: &WyrdTestCluster,
    tenant: wyrd_spec::DataTenantId,
    table: &str,
) -> Result<(), JourneyError> {
    sqlx::query(
        "UPDATE vala.forge_tasks \
         SET ready_at = statement_timestamp(), \
             next_eligible_at = statement_timestamp() - interval '15 minutes' \
         WHERE data_tenant_id = $1 AND table_name = $2 AND state = 'retryable'",
    )
    .bind(tenant.as_uuid())
    .bind(table)
    .execute(cluster.pg_fixture().operator_pool().pool())
    .await?;
    Ok(())
}

/// Returns `(compacted, hot)` durable file counts for one tenant-owned table.
///
/// Reads `vala.file_list` through the operator pool because the split between
/// the two tiers is durable server state the journey has no client-visible
/// projection of; the query's own metrics report scan totals without naming
/// which leaf produced them.
///
/// # Errors
///
/// Returns the SQLx error when either count cannot be read.
pub(crate) async fn file_tier_counts(
    cluster: &WyrdTestCluster,
    tenant: wyrd_spec::DataTenantId,
    table: &str,
) -> Result<(i64, i64), JourneyError> {
    let pool = cluster.pg_fixture().operator_pool().pool();
    let compacted: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.file_list WHERE data_tenant_id = $1 AND table_name = $2 AND compacted",
    )
    .bind(tenant.as_uuid())
    .bind(table)
    .fetch_one(pool)
    .await?;
    let hot: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.file_list WHERE data_tenant_id = $1 AND table_name = $2 AND NOT compacted",
    )
    .bind(tenant.as_uuid())
    .bind(table)
    .fetch_one(pool)
    .await?;
    Ok((compacted, hot))
}
