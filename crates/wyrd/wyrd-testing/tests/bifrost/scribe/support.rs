//! Shared fixtures for the scribe modules.
//!
//! Every item here is used by more than one sibling module. A helper
//! used by exactly one module lives in that module instead. Contains
//! no tests.

use arrow::array::{FixedSizeBinaryArray, Int32Array, Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use std::sync::Arc;
use std::time::Duration;
use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_sdk::{
    BifrostFrame, BifrostGrpcTransport, CollectedQueryLimits, CollectedQueryResult, QueryClient,
};
use wyrd_client::WyrdClient;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_spec::vala::api::{
    BifrostQueryRequest, FreshnessPolicy, QueryTerminalOutcome, VisibilityMode,
};
use wyrd_testing::Bootstrap;
use wyrd_testing::bifrost::WyrdTestCluster;

pub(crate) const TABLE_NAME: &str = "closeout_events";
pub(crate) const TABLE_FQN: &str = "vala.bifrost.closeout_events";

/// Wait until Forge holds no active or claimable work before a drain assertion.
///
/// `assert_drained_shutdown` requires zero Forge claims and attempts at
/// shutdown. A worker cancelled by shutdown *after* a durable effect correctly
/// retains its claim for lease recovery, so a scheduler-produced maintenance
/// task that is in flight or newly claimable at cancel time trips that
/// assertion even though production behavior is correct. This poll establishes
/// the assertion's stated precondition — a fully quiesced Forge — by waiting,
/// under a generous liveness deadline, until no task is active
/// (`forge_active_claims`, `forge_active_attempts`) and none is claimable
/// (`forge_claimable_tasks`), confirmed stable across one immediate re-poll so a
/// transient trough observed mid scheduler tick is not mistaken for quiescence.
///
/// It masks no defect: every drain assertion stays unmodified, and Forge
/// generates no further work once the journey's writes have stopped and its
/// tables have reached their compaction fixpoint. The scheduler is
/// state-triggered — a tick plans only from live small-file groups, staging
/// folds, or a clock-eligible maintenance candidate — and the Forge clock is
/// test-controlled, never advanced during this wait, so no time-based
/// maintenance becomes newly eligible while it runs. A tick over the drained
/// tenant therefore enqueues nothing, and the confirmed all-zero state holds.
///
/// # Errors
///
/// Returns an inspection error, or a diagnostic naming the surviving active and
/// claimable counts when the Forge does not quiesce before the deadline.
pub(crate) async fn wait_forge_quiesce(
    cluster: &WyrdTestCluster,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    loop {
        let inspection = cluster.oracle_inspection().await?;
        let quiesced = inspection.forge_active_claims == 0
            && inspection.forge_active_attempts == 0
            && inspection.forge_claimable_tasks == 0;
        if quiesced {
            let confirm = cluster.oracle_inspection().await?;
            if confirm.forge_active_claims == 0
                && confirm.forge_active_attempts == 0
                && confirm.forge_claimable_tasks == 0
            {
                return Ok(());
            }
        }
        if std::time::Instant::now() >= deadline {
            let surviving = cluster.oracle_inspection().await?;
            return Err(format!(
                "Forge did not quiesce before shutdown: {} active claims, {} active attempts, {} claimable tasks after 60s",
                surviving.forge_active_claims,
                surviving.forge_active_attempts,
                surviving.forge_claimable_tasks,
            )
            .into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Assert every server, listener, and retained runtime owner drained at shutdown.
pub(crate) fn assert_drained_shutdown(
    inspection: wyrd_testing::bifrost::ClusterShutdownInspection,
) {
    assert!(inspection.servers_stopped);
    assert!(inspection.listeners_stopped);
    assert_eq!(inspection.scribe_queued, 0);
    assert_eq!(inspection.scribe_inflight, 0);
    assert_eq!(inspection.scribe_wal_streams, 0);
    assert_eq!(inspection.forge_active_claims, 0);
    assert_eq!(inspection.forge_active_attempts, 0);
    assert_eq!(inspection.supervised_tasks, 0);
}

/// Exercises the complete multi-pod ingest, rejection, flush, query, and audit journey.
///
/// Every accepted write is flushed before each independently authenticated
/// Oracle entrypoint must return the same exact ordered rows and terminal
/// metadata. Returning early leaves already acknowledged and flushed test data
/// durable until the caller shuts down the cluster.
///
/// # Errors
///
/// Returns an error when table creation, client bootstrap, ingest, flush,
/// query collection, SQL inspection, or tenant connection acquisition fails.
///
/// # Cancellation
///
/// Cancelling this future stops the current client or SQL operation. Accepted
/// writes and completed flushes are not rolled back; the caller retains cluster
/// ownership and is responsible for bounded shutdown.
pub(crate) async fn run_closeout_journey(
    cluster: &WyrdTestCluster,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let tenant = cluster.data_tenant_id();
    let first = cluster.server(0).ok_or("missing first Bifrost pod")?;
    first
        .create_bifrost_table_for_test(CreateTableRequest {
            table: TableRef::new(BifrostNamespace::Bifrost, TABLE_NAME),
            user_fields: vec![
                Field::new("id", DataType::Int64, false),
                Field::new("value", DataType::Utf8, false),
            ],
            tenant,
            physical_layout: None,
            audit: None,
        })
        .await?;

    let admin = bootstrap_transport(first, "closeout-admin", &["admin"]).await?;
    let scribe_count = cluster
        .servers()
        .filter(|server| server.bifrost_scribe().is_some())
        .count();
    let mut expected_ids = vec![1, 2, 3, 4];
    expected_ids.extend((0..scribe_count).map(|pod| 10 + i64::try_from(pod).unwrap_or(i64::MAX)));
    expected_ids.push(99);
    let batch_id = uuid::Uuid::now_v7().into_bytes();
    admin.send_frame(frame(batch_id, &[1, 2])).await?;
    admin
        .send_frame(frame(uuid::Uuid::now_v7().into_bytes(), &[3, 4]))
        .await?;

    // Exercise symmetric traffic: every independently bound server receives a
    // frame through its own public SDK connection and server-owned Gate.
    for (pod, server) in cluster
        .servers()
        .filter(|server| server.bifrost_scribe().is_some())
        .enumerate()
    {
        let transport =
            bootstrap_transport(server, &format!("closeout-pod-{pod}"), &["admin"]).await?;
        let id = uuid::Uuid::now_v7().into_bytes();
        transport
            .insert_batch(TABLE_FQN, id, ipc(&[10 + pod as i64]))
            .await?;
        server.flush_bifrost().await?;
    }

    // A retry of an already admitted frame is idempotent at the durable slice
    // boundary. The duplicate is intentionally flushed after the first frame.
    let retry_id = uuid::Uuid::now_v7().into_bytes();
    let retry_frame = frame(retry_id, &[99]);
    admin
        .insert_batch(
            TABLE_FQN,
            retry_frame.batch_id,
            retry_frame.arrow_ipc.to_vec(),
        )
        .await?;
    admin
        .insert_batch(
            TABLE_FQN,
            retry_frame.batch_id,
            retry_frame.arrow_ipc.to_vec(),
        )
        .await?;
    for server in cluster
        .servers()
        .filter(|server| server.bifrost_scribe().is_some())
    {
        server.flush_bifrost().await?;
    }

    // Negative auth and resolution paths are exercised through the same public
    // SDK transport. Neither request is allowed to create durable file-list
    // state because Gate rejects before Scribe admission.
    let underprivileged = bootstrap_transport(first, "closeout-underprivileged", &[]).await?;
    let denied = underprivileged
        .insert_batch(TABLE_FQN, uuid::Uuid::now_v7().into_bytes(), ipc(&[200]))
        .await
        .expect_err("under-privileged token must be rejected");
    assert_eq!(denied.status(), 403);

    let unknown = admin
        .insert_batch(
            "vala.bifrost.unknown_closeout_table",
            uuid::Uuid::now_v7().into_bytes(),
            ipc(&[201]),
        )
        .await
        .expect_err("unknown table must be rejected before Scribe");
    assert_eq!(unknown.status(), 404);

    let conflict = admin
        .insert_batch(
            TABLE_FQN,
            uuid::Uuid::now_v7().into_bytes(),
            conflicting_ipc(),
        )
        .await
        .expect_err("schema conflict must be rejected before ACK");
    assert_eq!(conflict.status(), 409);

    for (pod, server) in cluster
        .servers()
        .filter(|server| server.base_url().is_some())
        .enumerate()
    {
        let query_client =
            bootstrap_client(server, &format!("closeout-query-{pod}"), &["admin"]).await?;
        let query = QueryClient::new(&query_client)
            .collect_bounded(
                &closeout_query(),
                CollectedQueryLimits {
                    max_rows: 1_024,
                    max_encoded_bytes: 8 * 1024 * 1024,
                },
            )
            .await?;
        assert_query_result(&query, &expected_ids);
    }

    let identity_client = bootstrap_client(first, "closeout-identity-query", &["admin"]).await?;
    let identity_query = QueryClient::new(&identity_client)
        .collect_bounded(
            &BifrostQueryRequest {
                sql: format!(
                    "SELECT id, wyrd_batch_id, wyrd_row_ordinal FROM {TABLE_FQN} ORDER BY id"
                ),
                visibility: VisibilityMode::PublishedOnly,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: None,
            },
            CollectedQueryLimits {
                max_rows: 1_024,
                max_encoded_bytes: 8 * 1024 * 1024,
            },
        )
        .await?;
    let mut retry_identities = Vec::new();
    for batch in &identity_query.batches {
        let ids = batch
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .ok_or("identity query id column must match the authoritative schema")?;
        let batch_ids = batch
            .column(1)
            .as_any()
            .downcast_ref::<FixedSizeBinaryArray>()
            .ok_or("identity query batch column must be fixed-size binary")?;
        let ordinals = batch
            .column(2)
            .as_any()
            .downcast_ref::<Int32Array>()
            .ok_or("identity query ordinal column must be int32")?;
        retry_identities.extend((0..batch.num_rows()).filter_map(|row| {
            (batch_ids.value(row) == retry_id).then_some((ids.value(row), ordinals.value(row)))
        }));
    }
    assert_eq!(retry_identities, vec![(99, 0)]);

    let rows: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(row_count), 0)::bigint
           FROM vala.file_list
          WHERE data_tenant_id = $1 AND namespace = 'vala.bifrost' AND table_name = $2",
    )
    .bind(tenant.as_uuid())
    .bind(TABLE_NAME)
    .fetch_one(cluster.pg_fixture().operator_pool().pool())
    .await?;
    assert_eq!(
        rows,
        i64::try_from(expected_ids.len()).unwrap_or(i64::MAX),
        "durable readback must include every unique frame row"
    );

    let mut conn = wyrd_sql::TenantConn::acquire(cluster.pg_fixture().app_pool(), tenant).await?;
    let audit_principals: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT principal_id)::bigint
           FROM vala.audit_outbox
          WHERE data_tenant_id = wyrd.current_tenant() AND resource = $1",
    )
    .bind(TABLE_FQN)
    .fetch_one(&mut **conn.transaction())
    .await?;
    assert!(
        audit_principals >= i64::try_from(scribe_count.saturating_add(1)).unwrap_or(i64::MAX),
        "audit rows retain every successful public writer identity"
    );
    Ok(())
}

/// Verifies the authoritative schema, decoded rows, and successful terminal.
///
/// # Panics
///
/// Panics when schema, terminal, row or byte counts, batch presence, decoded
/// identifiers, or decoded values differ from the expected query result.
pub(crate) fn assert_query_result(query: &CollectedQueryResult, expected_ids: &[i64]) {
    assert_eq!(query.schema.fields().len(), 2);
    assert_eq!(query.schema.field(0).name(), "id");
    assert_eq!(query.schema.field(0).data_type(), &DataType::Int64);
    assert!(!query.schema.field(0).is_nullable());
    assert_eq!(query.schema.field(1).name(), "value");
    assert_eq!(query.schema.field(1).data_type(), &DataType::Utf8);
    assert!(!query.schema.field(1).is_nullable());
    assert_eq!(query.rows, expected_ids.len());
    assert!(query.encoded_bytes > 0);
    assert_eq!(query.terminal.outcome, QueryTerminalOutcome::Success);
    assert_eq!(
        query.terminal.row_count,
        u64::try_from(expected_ids.len()).unwrap_or(u64::MAX)
    );
    assert!(!query.batches.is_empty(), "Oracle must return Arrow data");

    let mut ids = Vec::new();
    let mut values = Vec::new();
    for batch in &query.batches {
        let batch_ids = batch
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .expect("query id column must match the authoritative schema");
        let batch_values = batch
            .column(1)
            .as_any()
            .downcast_ref::<StringArray>()
            .expect("query value column must match the authoritative schema");
        ids.extend((0..batch.num_rows()).map(|row| batch_ids.value(row)));
        values.extend((0..batch.num_rows()).map(|row| batch_values.value(row)));
    }
    assert_eq!(ids, expected_ids);
    assert_eq!(values, vec!["journey"; expected_ids.len()]);
}

/// Builds the explicit public Oracle policy used by closeout readback.
pub(crate) fn closeout_query() -> BifrostQueryRequest {
    query_for_table(TABLE_FQN)
}

/// Build the strict public Oracle query for one tenant-bound table.
pub(crate) fn query_for_table(table: &str) -> BifrostQueryRequest {
    BifrostQueryRequest {
        sql: format!("SELECT id, value FROM {table} ORDER BY id"),
        visibility: VisibilityMode::PublishedOnly,
        freshness: FreshnessPolicy::Strict,
        deadline_ms: None,
    }
}

pub(crate) async fn bootstrap_transport(
    server: &wyrd_testing::WyrdTestServer,
    name: &str,
    roles: &[&str],
) -> Result<BifrostGrpcTransport, Box<dyn std::error::Error + Send + Sync>> {
    let client = bootstrap_client(server, name, roles).await?;
    Ok(BifrostGrpcTransport::connect(&client).await?)
}

pub(crate) async fn bootstrap_client(
    server: &wyrd_testing::WyrdTestServer,
    name: &str,
    roles: &[&str],
) -> Result<WyrdClient, Box<dyn std::error::Error + Send + Sync>> {
    let bootstrap = server.bootstrap_service(name, roles).await?;
    let key = match bootstrap {
        Bootstrap::Machine { api_key, .. } => api_key,
        Bootstrap::User { .. } => return Err("service bootstrap returned a user".into()),
    };
    let mut config = ClientConfig {
        grpc: GrpcConfig {
            endpoint: server.grpc_url().ok_or("missing gRPC endpoint")?,
            connect_retries: 0,
            ..GrpcConfig::default()
        },
        http: HttpConfig {
            base_url: server.base_url().ok_or("missing HTTP endpoint")?.to_owned(),
            ..HttpConfig::default()
        },
        api_key: Some(key),
        ..ClientConfig::default()
    };
    config.grpc.max_message_bytes = 32 * 1024 * 1024;
    Ok(WyrdClient::with_config(config)?)
}

/// Bootstrap one public client under an explicit data tenant.
pub(crate) async fn bootstrap_client_for_tenant(
    server: &wyrd_testing::WyrdTestServer,
    tenant: wyrd_spec::DataTenantId,
    name: &str,
) -> Result<WyrdClient, Box<dyn std::error::Error + Send + Sync>> {
    let bootstrap = server
        .bootstrap_service_in_tenant(tenant, name, &["admin"])
        .await?;
    let key = match bootstrap {
        Bootstrap::Machine { api_key, .. } => api_key,
        Bootstrap::User { .. } => return Err("tenant bootstrap returned a user".into()),
    };
    let mut config = ClientConfig {
        grpc: GrpcConfig {
            endpoint: server.grpc_url().ok_or("missing gRPC endpoint")?,
            connect_retries: 0,
            ..GrpcConfig::default()
        },
        http: HttpConfig {
            base_url: server.base_url().ok_or("missing HTTP endpoint")?.to_owned(),
            ..HttpConfig::default()
        },
        api_key: Some(key),
        ..ClientConfig::default()
    };
    config.grpc.max_message_bytes = 32 * 1024 * 1024;
    Ok(WyrdClient::with_config(config)?)
}

pub(crate) fn frame(batch_id: [u8; 16], ids: &[i64]) -> BifrostFrame {
    frame_for_table(TABLE_FQN, batch_id, ids)
}

/// Build one exact public ingest frame for a selected table.
pub(crate) fn frame_for_table(table: &str, batch_id: [u8; 16], ids: &[i64]) -> BifrostFrame {
    BifrostFrame {
        table: table.to_owned(),
        batch_id,
        arrow_ipc: ipc(ids).into(),
    }
}

pub(crate) fn ipc(ids: &[i64]) -> Vec<u8> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("value", DataType::Utf8, false),
    ]));
    let rows = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(ids.to_vec())),
            Arc::new(StringArray::from(vec!["journey"; ids.len()])),
        ],
    )
    .expect("valid closeout batch");
    encode(rows, schema)
}

pub(crate) fn conflicting_ipc() -> Vec<u8> {
    let ids = &[1_i64];
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("value", DataType::Utf8, false),
    ]));
    let rows = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(StringArray::from(
                ids.iter().map(ToString::to_string).collect::<Vec<_>>(),
            )),
            Arc::new(StringArray::from(vec!["conflict"; ids.len()])),
        ],
    )
    .expect("valid conflicting batch");
    encode(rows, schema)
}

pub(crate) fn encode(rows: RecordBatch, schema: Arc<Schema>) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, schema.as_ref()).expect("IPC writer");
    writer.write(&rows).expect("IPC batch");
    writer.finish().expect("IPC stream");
    bytes
}
