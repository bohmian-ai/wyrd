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
use vala_bifrost_redux::oracle::analytical::{
    AnalyticalAttemptContext, AnalyticalLiveInspection, DataFusionQueryId, PublicQueryId,
};
use vala_bifrost_redux::parquet::writer_properties::bifrost_writer_properties;
use vala_bifrost_redux::schema::with_managed_columns;
use vala_sdk::QueryClient;
use wyrd_client::WyrdClient;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_runtime::permission::PermissionSet;
use wyrd_runtime::{Permission, Principal, PrincipalKind};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    AuditDecision, AuditEvent, AuditResult, AuthMethod, BifrostQueryRequest, FreshnessPolicy,
    VisibilityMode,
};
use wyrd_testing::Bootstrap;
use wyrd_testing::bifrost::WyrdTestCluster;
use wyrd_testing::bifrost::write::BifrostWriter;

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
        credential: Some(api_key),
        ..ClientConfig::default()
    })?)
}
/// The three-column user schema every Oracle journey table registers.
///
/// `unused_payload` is the wide column no narrow query requests; it exists so
/// a projection that reaches the physical reader is visible in scanned bytes.
pub(crate) fn journey_schema() -> arrow::datatypes::SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("filter_key", DataType::Utf8, false),
        Field::new("unused_payload", DataType::Utf8, false),
    ]))
}

/// Encodes one deterministic journey row as the JSON the write door buffers.
pub(crate) fn journey_row(id: i64, filter_key: &str) -> Vec<u8> {
    serde_json::json!({
        "id": id,
        "filter_key": filter_key,
        "unused_payload": unused_payload(id),
    })
    .to_string()
    .into_bytes()
}

/// Build one authenticated write door for the fixture tenant.
pub(crate) async fn writer(
    server: &wyrd_testing::WyrdTestServer,
    name: &str,
) -> Result<BifrostWriter, JourneyError> {
    writer_for_tenant(server, server.data_tenant_id(), name).await
}

/// Build one authenticated write door for an explicit tenant.
///
/// The returned handle owns the client the journey also reads with, so the
/// write and the read provably share one credential.
pub(crate) async fn writer_for_tenant(
    server: &wyrd_testing::WyrdTestServer,
    tenant: DataTenantId,
    name: &str,
) -> Result<BifrostWriter, JourneyError> {
    writer_from_bootstrap(
        server,
        server
            .bootstrap_service_in_tenant(tenant, name, &["admin"])
            .await?,
    )
    .await
}

/// Build a write door while retaining the bootstrap principal's Card scope.
///
/// # Errors
///
/// Returns a journey error when the bootstrap is not a machine principal or
/// the authenticated ingest transport cannot connect.
pub(crate) async fn writer_from_bootstrap(
    server: &wyrd_testing::WyrdTestServer,
    bootstrap: Bootstrap,
) -> Result<BifrostWriter, JourneyError> {
    let card_ref = bootstrap
        .card_ref()
        .ok_or("machine bootstrap returned no Card scope")?
        .clone();
    let Bootstrap::Machine { api_key, .. } = bootstrap else {
        return Err("machine bootstrap returned user".into());
    };
    Ok(BifrostWriter::connect(
        ClientConfig {
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
            credential: Some(api_key),
            ..ClientConfig::default()
        },
        card_ref,
    )
    .await?)
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
/// Builds one authenticated in-process query context for the fixture tenant.
///
/// The public SDK cannot reach the inactive path, so the journey authenticates
/// the same way the public query service does — a tenant-bound principal
/// holding exactly `bifrost:query:read` — and hands Oracle the identical
/// context its own gRPC surface would have built.
pub(crate) fn query_context(
    tenant: DataTenantId,
) -> Result<vala_bifrost_redux::oracle::AuthorizedQueryContext, JourneyError> {
    let permission = Permission::bifrost_query_read();
    let principal = Principal::new(
        PrincipalId::new(uuid::Uuid::now_v7()),
        PrincipalKind::User,
        tenant,
        Vec::new(),
        PermissionSet::from_iter([permission.clone()]),
    );
    Ok(vala_bifrost_redux::oracle::AuthorizedQueryContext::try_new(
        principal,
        tenant,
        RequestId::now_v7(),
        None,
        AuthMethod::Internal,
        permission,
    )?)
}

/// Allocates the per-query identities one inactive attempt is leased under.
///
/// The two query identities are allocated independently on purpose: a leaked
/// public identity into the distributed graph, or the reverse, is exactly what
/// the stage authority's identity isolation exists to refuse.
pub(crate) fn attempt_context() -> AnalyticalAttemptContext {
    AnalyticalAttemptContext {
        public_query_id: PublicQueryId::from_uuid(uuid::Uuid::now_v7()),
        datafusion_query_id: DataFusionQueryId::from_uuid(uuid::Uuid::now_v7()),
        snapshot_digest: format!("snapshot-{}", uuid::Uuid::now_v7().simple()),
        permission_digest: format!("permission-{}", uuid::Uuid::now_v7().simple()),
    }
}

/// Returns every Oracle node's live Analytical ownership, leader and follower
/// halves.
///
/// A node that composed no Oracle is skipped rather than refused: a Scribe-only
/// node has no Analytical ownership to inspect, so demanding one from it would
/// report a correct topology as a failure.
///
/// # Errors
///
/// Returns an error when an Oracle node composed no Analytical handle or its
/// ownership lock is poisoned.
pub(crate) fn live_ownership(
    cluster: &WyrdTestCluster,
) -> Result<Vec<AnalyticalLiveInspection>, JourneyError> {
    cluster
        .servers()
        .filter_map(|server| server.state().bifrost_query())
        .map(|query| {
            let engine = query.engine();
            let handle = engine
                .analytical_execution()
                .ok_or("Oracle composed no Analytical handle")?;
            Ok(handle.live()?)
        })
        .collect()
}

/// Waits, under a bound, for every node to retain no Analytical ownership.
///
/// A follower settles on its own stage-operation path rather than with the
/// leader's stream, so the assertion is a bounded convergence rather than an
/// instantaneous read. It is bounded because a node that never converges is a
/// leak, and reporting it as a timeout is the point.
///
/// # Errors
///
/// Returns the first inspection error, or a description of what a node still
/// retained when the bound expired.
pub(crate) async fn await_clean_nodes(cluster: &WyrdTestCluster) -> Result<(), JourneyError> {
    for _ in 0..CLEAN_NODE_POLLS {
        let live = live_ownership(cluster)?;
        if live.iter().all(AnalyticalLiveInspection::is_clean) {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Err(format!(
        "nodes still retain Analytical ownership: {:?}",
        live_ownership(cluster)?
    )
    .into())
}

/// Bound on how long terminal cleanup may take before it is called a leak.
pub(crate) const CLEAN_NODE_POLLS: usize = 50;

/// Left-table rows in the qualified Analytical baseline workload.
///
/// Wider than the right table so the join is not an identity.
pub(crate) const ANALYTICAL_LEFT_ROWS: i64 = 400_000;

/// Right-table rows in the qualified Analytical baseline workload.
///
/// The join key range that actually matches, and therefore the result's row
/// count. Sized so the output sort's input exceeds one Analytical grant.
pub(crate) const ANALYTICAL_RIGHT_ROWS: i64 = 300_000;

/// Digits the baseline query left-pads each id to.
pub(crate) const ANALYTICAL_KEY_DIGITS: usize = 6;

/// Filler characters appended to each key, making every key exactly 1 KiB.
pub(crate) const ANALYTICAL_KEY_FILLER: usize = 1018;

/// Builds the qualified Analytical baseline statement over two fixture tables.
///
/// One equi-join, one fixed-width grouped aggregate, and one output sort over a
/// key wide enough that the sort's input cannot fit an Analytical grant. Shared
/// by the physical baseline and by the contention qualification that reuses the
/// same admitted workload, so both are provably running one statement.
pub(crate) fn analytical_baseline_sql(left: &str, right: &str) -> String {
    format!(
        "SELECT LPAD(CAST(l.id AS VARCHAR), {ANALYTICAL_KEY_DIGITS}, '0') ||          REPEAT('x', {ANALYTICAL_KEY_FILLER}) AS filter_key, COUNT(*) AS matched          FROM vala.bifrost.{left} AS l          JOIN vala.bifrost.{right} AS r ON l.id = r.id          GROUP BY l.id ORDER BY filter_key"
    )
}

/// Recomputes the exact result [`analytical_baseline_sql`] must produce.
///
/// Generated from the fixture's own definition rather than from anything the
/// cluster returned, so a query that silently dropped, duplicated, or reordered
/// rows cannot agree with it.
pub(crate) fn expected_analytical_digest() -> String {
    let mut digest = Sha256::new();
    for id in 0..ANALYTICAL_RIGHT_ROWS {
        let key = format!(
            "{id:0ANALYTICAL_KEY_DIGITS$}{filler}",
            filler = "x".repeat(ANALYTICAL_KEY_FILLER)
        );
        digest.update(u32::try_from(key.len()).unwrap_or(u32::MAX).to_le_bytes());
        digest.update(key.as_bytes());
        digest.update(1_i64.to_le_bytes());
    }
    format!("{:x}", digest.finalize())
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

/// Bounded polls the journey waits for a pod to return to its baseline.
pub(crate) const BASELINE_POLLS: usize = 50;

/// Waits, bounded, until one pod's ownership returns to its recorded baseline.
///
/// # Errors
///
/// Returns the control-protocol error, or a description of what the pod still
/// retained when the bound expired.
pub(crate) async fn await_baseline(
    cluster: &mut wyrd_testing::bifrost::process_cluster::BifrostProcessCluster,
    index: usize,
    before: wyrd_testing::bifrost::process_cluster::OracleOwnershipSnapshot,
) -> Result<(), JourneyError> {
    for _ in 0..BASELINE_POLLS {
        if cluster.nodes_mut()[index].ownership_snapshot()? == before {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let after = cluster.nodes_mut()[index].ownership_snapshot()?;
    Err(format!("pod {index} did not return to {before:?}, holds {after:?}").into())
}
