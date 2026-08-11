//! Postgres/object-store coverage for ordered and retryable Scribe persistence.

use std::sync::Arc;
use std::time::{Duration, Instant};

use arrow::array::{
    FixedSizeBinaryBuilder, Int32Array, Int64Array, RecordBatch, StringArray,
    TimestampMicrosecondArray,
};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::ipc::writer::StreamWriter;
use opendal::services::Memory;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use secrecy::ExposeSecret;
use tempfile::TempDir;
use vala_bifrost_redux::catalog::{
    BifrostCatalog, CreateTableRequest, TableRef, TenantTableBinding,
};
use vala_bifrost_redux::contracts::{
    IngressPayload, Scribe, ScribeAppend, ScribeError, ScribeIngressFrame,
};
use vala_bifrost_redux::maintenance::staging_file_channel;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::schema::fingerprint::SchemaFingerprint;
use vala_bifrost_redux::scribe::audit_envelope::encode_audit_event;
use vala_bifrost_redux::scribe::memory::BifrostMemoryGovernor;
use vala_bifrost_redux::scribe::persistence::PersistenceFaults;
use vala_bifrost_redux::scribe::stream_identity::{NodeId, StreamIdentity, WriterEpoch};
use vala_bifrost_redux::scribe::wal::{WalConfig, WalWriter};
use vala_bifrost_redux::scribe::{
    ScribeBuildConfig, ScribeExecutionPools, ScribeImpl, ScribeIngressCpuPool, ScribeLaneConfig,
    ScribePersistenceConfig, ScribePersistenceCpuPool, ScribeWalIoPool,
};
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_runtime::{Principal, PrincipalKind, permission::PermissionSet};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::PrincipalId;
use wyrd_spec::auth::PrincipalKindTag;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};
use wyrd_storage::BackendConfig;

struct PersistenceFixture {
    database: PgFixture,
    operator: Arc<opendal::Operator>,
    scribe: Arc<ScribeImpl>,
    /// Shared production governor used to overlap Oracle range ownership.
    memory: BifrostMemoryGovernor,
    faults: PersistenceFaults,
    /// Receiver retained so post-commit hints remain observable until assertions finish.
    hint_inbox: vala_bifrost_redux::maintenance::StagingFileInbox,
    wal_root: TempDir,
    _warehouse: Option<TempDir>,
    tenant: DataTenantId,
}

impl PersistenceFixture {
    /// Starts the standard persistence fixture with the production-derived governor.
    ///
    /// # Panics
    ///
    /// Panics when the governor or any Postgres, object-store, WAL, execution-lane,
    /// persistence, or Scribe fixture dependency cannot be constructed.
    async fn start() -> Self {
        Self::start_with_memory(
            BifrostMemoryGovernor::new(8 * 1024 * 1024 * 1024).expect("memory governor"),
        )
        .await
    }

    /// Starts a persistence fixture with one caller-selected shared governor.
    ///
    /// The supplied governor is shared by Scribe and any test-owned overlapping
    /// Oracle reservation so the fixture exercises the production accounting tree.
    ///
    /// # Panics
    ///
    /// Panics when any Postgres, object-store, WAL, execution-lane, persistence,
    /// channel, or Scribe fixture dependency cannot be constructed.
    async fn start_with_memory(memory: BifrostMemoryGovernor) -> Self {
        let database = PgFixture::start().await.expect("Postgres fixture");
        let tenant = database.data_tenant_id();
        let operator = Arc::new(
            opendal::Operator::new(Memory::default())
                .expect("memory object store")
                .finish(),
        );
        let wal_root = tempfile::tempdir().expect("WAL directory");
        let node_id = uuid::Uuid::now_v7();
        let wal = Arc::new(
            WalWriter::new(
                wal_root.path(),
                *node_id.as_bytes(),
                1,
                WalConfig::default(),
            )
            .expect("WAL writer"),
        );
        let faults = PersistenceFaults::default();
        let (staging_file_publisher, hint_inbox) = staging_file_channel(16).expect("hint channel");
        let persistence =
            ScribePersistenceConfig::new(Arc::new(database.vala_postgres().clone()), 16, 2)
                .with_test_faults(faults.clone());
        let admission = vala_bifrost_redux::scribe::admission::AdmissionConfig::default();
        let lane_config = ScribeLaneConfig {
            ingress_cpu_threads: 1,
            persistence_cpu_threads: 2,
            wal_io_threads: 2,
        };
        let pools = ScribeExecutionPools::new(
            ScribeIngressCpuPool::new_with_capacity(lane_config.ingress_cpu_threads, 256),
            ScribePersistenceCpuPool::new_with_capacity(lane_config.persistence_cpu_threads, 64),
            ScribeWalIoPool::new_with_capacity(lane_config.wal_io_threads, 256),
        );
        let scribe = Arc::new(ScribeImpl::new_with_execution_pools(ScribeBuildConfig {
            operator: Arc::clone(&operator),
            wal,
            stream: StreamIdentity::new(NodeId::new(node_id), WriterEpoch::new(1)),
            admission,
            coordination_runtime: tokio::runtime::Handle::current(),
            execution_pools: pools,
            persistence: Some(persistence),
            memory_budget: Some(memory.scribe_budget()),
            staging_file_publisher: Some(staging_file_publisher),
        }));
        scribe.replay_wal_async().await.expect("empty WAL replay");
        Self {
            database,
            operator,
            scribe,
            memory,
            faults,
            hint_inbox,
            wal_root,
            _warehouse: None,
            tenant,
        }
    }

    /// Derives the measured-wire remainder that places a projected append at `target`.
    ///
    /// The calibration traverses the production projection path because managed
    /// columns make admitted Arrow ownership larger than the source batch. Its
    /// expected oversize rejection occurs before WAL or memtable mutation.
    ///
    /// # Panics
    ///
    /// Panics when the source batch is already at the target, calibration does
    /// not return the decoded-size ceiling, or projected ownership reaches the
    /// target without room for a measured-wire remainder.
    async fn calibrate_exact_wire_bytes(
        &self,
        rows: &RecordBatch,
        schema: &Schema,
        table_name: &str,
        target: usize,
    ) -> usize {
        let source_bytes = rows.get_array_memory_size();
        assert!(source_bytes < target);
        let calibration_wire_bytes = target - source_bytes;
        let calibration = self
            .scribe
            .append(ScribeAppend {
                principal: principal(self.tenant),
                table: table(table_name),
                schema_fingerprint: SchemaFingerprint::from_arrow_schema(schema),
                request_id: RequestId::now_v7(),
                batch_id: uuid::Uuid::now_v7(),
                measured_wire_bytes: calibration_wire_bytes,
                rows: rows.clone(),
            })
            .await
            .expect_err("managed projection makes the source-sized calibration oversized");
        let ScribeError::DecodedPayloadTooLarge {
            bytes: calibrated_bytes,
            limit,
        } = calibration
        else {
            panic!("calibration must return the decoded-size ceiling");
        };
        assert_eq!(limit, target);
        let decoded_bytes = calibrated_bytes - calibration_wire_bytes;
        assert!(decoded_bytes < target);
        let measured_wire_bytes = target - decoded_bytes;
        assert_eq!(decoded_bytes + measured_wire_bytes, target);
        measured_wire_bytes
    }

    async fn stop(self) {
        self.scribe
            .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(1))
            .await;
        drop(self.wal_root);
    }

    async fn start_after_wal_restart(fail_replay_write: bool) -> Self {
        Self::start_after_wal_restart_with_keys(
            fail_replay_write,
            &["restart_publish_events"],
            3,
            &[
                Duration::from_millis(300),
                Duration::from_millis(1),
                Duration::from_millis(1),
            ],
        )
        .await
        .expect("replay")
    }

    async fn start_after_wal_restart_with_keys(
        fail_replay_write: bool,
        table_names: &[&str],
        generations: i64,
        object_write_delays: &[Duration],
    ) -> Result<Self, ScribeError> {
        let database = PgFixture::start().await.expect("Postgres fixture");
        let tenant = database.data_tenant_id();
        let operator = Arc::new(
            opendal::Operator::new(Memory::default())
                .expect("memory operator")
                .finish(),
        );
        let warehouse = tempfile::tempdir().expect("warehouse");
        let catalog = BifrostCatalog::new(
            database.catalog_dsn().expose_secret(),
            &BackendConfig::Local {
                root: warehouse.path().to_path_buf(),
            },
            database.vala_postgres().clone(),
        )
        .await
        .expect("Redux catalog");
        register_replay_tables(&catalog, table_names, tenant).await;
        let wal_root = tempfile::tempdir().expect("WAL directory");
        let node_id = uuid::Uuid::now_v7();
        let wal = Arc::new(
            WalWriter::new(
                wal_root.path(),
                *node_id.as_bytes(),
                1,
                WalConfig::default(),
            )
            .expect("WAL writer"),
        );
        let memory = BifrostMemoryGovernor::new(8 * 1024 * 1024 * 1024).expect("memory governor");
        let admission = vala_bifrost_redux::scribe::admission::AdmissionConfig {
            memory_limit_bytes: 4 * 1024 * 1024 * 1024,
            scribe_memory_limit_bytes: Some(8 * 1024 * 1024 * 1024),
            event_time_window: vala_bifrost_redux::scribe::admission::EventTimeWindow::default(),
        };
        let first = first_replay_scribe(operator.clone(), wal.clone(), node_id, admission, &memory);
        first.replay_wal_async().await.expect("empty WAL replay");
        write_replay_records(&wal, table_names, generations, tenant);
        first
            .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(1))
            .await;
        drop(wal);

        let faults = PersistenceFaults::default();
        let (staging_file_publisher, hint_inbox) = staging_file_channel(16).expect("hint channel");
        if fail_replay_write {
            faults.fail_next_object_write();
        }
        faults.set_object_write_delays_for_test(object_write_delays);
        let persistence =
            ScribePersistenceConfig::new(Arc::new(database.vala_postgres().clone()), 16, 2)
                .with_test_faults(faults.clone());
        let wal = Arc::new(
            WalWriter::new(
                wal_root.path(),
                *node_id.as_bytes(),
                2,
                WalConfig::default(),
            )
            .expect("restarted WAL writer"),
        );
        let pools = ScribeExecutionPools::new(
            ScribeIngressCpuPool::new_with_capacity(1, 256),
            ScribePersistenceCpuPool::new_with_capacity(2, 64),
            ScribeWalIoPool::new_with_capacity(2, 256),
        );
        let scribe = Arc::new(ScribeImpl::new_with_execution_pools(ScribeBuildConfig {
            operator: Arc::clone(&operator),
            wal,
            stream: StreamIdentity::new(NodeId::new(node_id), WriterEpoch::new(2)),
            admission,
            coordination_runtime: tokio::runtime::Handle::current(),
            execution_pools: pools,
            persistence: Some(persistence),
            memory_budget: Some(memory.scribe_budget()),
            staging_file_publisher: Some(staging_file_publisher),
        }));
        scribe.replay_wal_async().await?;
        Ok(Self {
            database,
            operator,
            scribe,
            memory,
            faults,
            hint_inbox,
            wal_root,
            _warehouse: Some(warehouse),
            tenant,
        })
    }
}

fn first_replay_scribe(
    operator: Arc<opendal::Operator>,
    wal: Arc<WalWriter>,
    node_id: uuid::Uuid,
    admission: vala_bifrost_redux::scribe::admission::AdmissionConfig,
    memory: &BifrostMemoryGovernor,
) -> Arc<ScribeImpl> {
    let pools = ScribeExecutionPools::new(
        ScribeIngressCpuPool::new_with_capacity(1, 256),
        ScribePersistenceCpuPool::new_with_capacity(2, 64),
        ScribeWalIoPool::new_with_capacity(2, 256),
    );
    Arc::new(ScribeImpl::new_with_execution_pools(ScribeBuildConfig {
        operator,
        wal,
        stream: StreamIdentity::new(NodeId::new(node_id), WriterEpoch::new(1)),
        admission,
        coordination_runtime: tokio::runtime::Handle::current(),
        execution_pools: pools,
        persistence: None,
        memory_budget: Some(memory.scribe_budget()),
        staging_file_publisher: None,
    }))
}

async fn register_replay_tables(
    catalog: &BifrostCatalog,
    table_names: &[&str],
    tenant: DataTenantId,
) {
    for table_name in table_names {
        catalog
            .create_table(CreateTableRequest {
                table: table(table_name),
                user_fields: (1..=6)
                    .map(|index| Field::new(format!("value_{index}"), DataType::Int64, false))
                    .collect(),
                tenant,
                audit: None,
            })
            .await
            .expect("catalog table");
    }
}

fn write_replay_records(
    wal: &WalWriter,
    table_names: &[&str],
    generations: i64,
    tenant: DataTenantId,
) {
    for table_name in table_names {
        for value in 1_i64..=generations {
            let batch_id = *uuid::Uuid::now_v7().as_bytes();
            let request_id = RequestId::now_v7();
            let data = managed_batch_bytes(value, tenant, batch_id, &request_id);
            let audit = encode_audit_event(&audit_event("bifrost.append", request_id))
                .expect("audit encoding");
            let key = vala_bifrost_redux::scribe::seal_key::SealKey::new(
                tenant,
                table(table_name),
                vala_bifrost_redux::scribe::seal_key::EventDay::new(
                    chrono::NaiveDate::from_ymd_opt(2026, 7, 24).expect("date"),
                ),
            );
            wal.append_and_fsync_for_test(&key, batch_id, &audit, &data)
                .expect("raw WAL append");
        }
    }
}

fn table(name: &str) -> TableRef {
    TableRef::new(BifrostNamespace::Bifrost, name)
}

fn principal(tenant: DataTenantId) -> Principal {
    Principal {
        id: PrincipalId::new(uuid::Uuid::now_v7()),
        kind: PrincipalKind::User,
        tenant_id: tenant,
        roles: Vec::new(),
        effective_permissions: PermissionSet::new(),
    }
}

fn batch(value: i64) -> RecordBatch {
    let schema = Arc::new(Schema::new(vec![
        Field::new(
            "wyrd_event_time",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
        Field::new("value", DataType::Int64, false),
    ]));
    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(
                TimestampMicrosecondArray::from(vec![chrono::Utc::now().timestamp_micros()])
                    .with_timezone("UTC"),
            ),
            Arc::new(Int64Array::from(vec![value])),
        ],
    )
    .expect("persistence batch")
}

fn audit_event(operation: &str, request_id: RequestId) -> AuditEvent {
    AuditEvent {
        request_id,
        trace_id: None,
        operation: operation.to_owned(),
        resource: "vala.bifrost.replay".to_owned(),
        card_ref: None,
        principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
        principal_kind: PrincipalKindTag::User,
        auth_method: AuthMethod::Jwt,
        permission: "bifrost:write".to_owned(),
        decision: AuditDecision::Allow,
        result: AuditResult::Success,
        payload_summary: "replay rows".to_owned(),
        detail: None,
    }
}

fn managed_batch_bytes(
    value: i64,
    tenant: DataTenantId,
    batch_id: [u8; 16],
    request_id: &RequestId,
) -> Vec<u8> {
    let row_count = 50_000;
    let schema = Arc::new(Schema::new(vec![
        Field::new("value_1", DataType::Int64, false),
        Field::new("value_2", DataType::Int64, false),
        Field::new("value_3", DataType::Int64, false),
        Field::new("value_4", DataType::Int64, false),
        Field::new("value_5", DataType::Int64, false),
        Field::new("value_6", DataType::Int64, false),
        Field::new("run_id", DataType::Utf8, true),
        Field::new("card_uid", DataType::Utf8, true),
        Field::new("principal_id", DataType::Utf8, false),
        Field::new("wyrd_request_id", DataType::Utf8, false),
        Field::new(
            "wyrd_event_time",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
        Field::new(
            "wyrd_ingested_at",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
        Field::new("wyrd_batch_id", DataType::FixedSizeBinary(16), false),
        Field::new("wyrd_row_ordinal", DataType::Int32, false),
        Field::new("data_tenant_id", DataType::Utf8, false),
    ]));
    let timestamp = chrono::Utc::now().timestamp_micros();
    let mut batch_builder = FixedSizeBinaryBuilder::with_capacity(row_count, 16);
    for _ in 0..row_count {
        batch_builder
            .append_value(batch_id)
            .expect("fixed batch id");
    }
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(vec![value; row_count])),
            Arc::new(Int64Array::from(vec![value + 1; row_count])),
            Arc::new(Int64Array::from(vec![value + 2; row_count])),
            Arc::new(Int64Array::from(vec![value + 3; row_count])),
            Arc::new(Int64Array::from(vec![value + 4; row_count])),
            Arc::new(Int64Array::from(vec![value + 5; row_count])),
            Arc::new(StringArray::from(vec![None::<String>; row_count])),
            Arc::new(StringArray::from(vec![None::<String>; row_count])),
            Arc::new(StringArray::from(vec!["replay-principal"; row_count])),
            Arc::new(StringArray::from(vec![request_id.as_str(); row_count])),
            Arc::new(
                TimestampMicrosecondArray::from(vec![timestamp; row_count]).with_timezone("UTC"),
            ),
            Arc::new(
                TimestampMicrosecondArray::from(vec![timestamp; row_count]).with_timezone("UTC"),
            ),
            Arc::new(batch_builder.finish()),
            Arc::new(Int32Array::from_iter_values(
                0..i32::try_from(row_count).expect("test row count fits i32"),
            )),
            Arc::new(StringArray::from(vec![tenant.to_string(); row_count])),
        ],
    )
    .expect("managed persistence batch");
    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, &schema).expect("IPC writer");
    writer.write(&batch).expect("IPC batch");
    writer.finish().expect("IPC finish");
    bytes
}

async fn append_one(fixture: &PersistenceFixture, table_name: &str, value: i64) {
    append_with_batch_id(fixture, table_name, value, uuid::Uuid::now_v7()).await;
}

/// Append one generation carrying a caller-chosen `batch_id`.
///
/// [`append_one`] mints a fresh `now_v7` id per call, which spreads generations
/// across shard lanes via `shard_for`. Tests that must place two generations
/// on ONE shard lane instead choose their `batch_id`s explicitly through this
/// helper so the routing key is deterministic.
async fn append_with_batch_id(
    fixture: &PersistenceFixture,
    table_name: &str,
    value: i64,
    batch_id: uuid::Uuid,
) {
    let rows = batch(value);
    fixture
        .scribe
        .append(ScribeAppend {
            principal: principal(fixture.tenant),
            table: table(table_name),
            schema_fingerprint: SchemaFingerprint::from_arrow_schema(rows.schema().as_ref()),
            request_id: RequestId::now_v7(),
            batch_id,
            measured_wire_bytes: 0,
            rows,
        })
        .await
        .expect("append to Scribe");
}

/// Deterministically choose a second, distinct `batch_id` that Scribe routes to
/// the same shard lane as `first`.
///
/// Scribe selects a shard with `shard_for(tenant, table, batch_id)`
/// (`scribe::routing`, mirrored in `scribe::shards::ShardSet::try_send`). With
/// `(tenant, table)` fixed, the shard depends only on the `batch_id`, so a
/// bounded counter walk finds a co-locating id with no runtime randomness
/// (~1-in-16 candidates match across the 16 lanes). Co-location is what places
/// both generations on ONE FIFO lane; keeping the ids DISTINCT (never a reused
/// id) makes this a genuine same-`SealKey` ordering scenario rather than a
/// `(batch_id, seal_key)` idempotency replay.
fn colocated_distinct_batch_id(
    tenant: DataTenantId,
    table: &TableRef,
    first: uuid::Uuid,
) -> uuid::Uuid {
    let target = vala_bifrost_redux::scribe::routing::shard_for(tenant, table, first);
    (0_u128..1_000_000)
        .map(uuid::Uuid::from_u128)
        .find(|candidate| {
            *candidate != first
                && vala_bifrost_redux::scribe::routing::shard_for(tenant, table, *candidate)
                    == target
        })
        .expect("a co-locating distinct batch id exists within the bounded search")
}

async fn wait_for_state(fixture: &PersistenceFixture, pending: usize) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let stats = fixture.scribe.memtable_stats().expect("memtable stats");
        if fixture.scribe.persistence_queue_depth_for_test() == 0
            && stats.pending_generations == pending
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "persistence state did not converge: pending={}, queue={}",
            stats.pending_generations,
            fixture.scribe.persistence_queue_depth_for_test()
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Poll the fixture's bounded wake-up inbox after persistence settles.
///
/// # Errors
/// Returns the channel's empty or closed status when no advisory signal is available.
fn hint_outcome(
    fixture: &mut PersistenceFixture,
) -> Result<
    vala_bifrost_redux::maintenance::StagingFileCommitted,
    tokio::sync::mpsc::error::TryRecvError,
> {
    fixture.hint_inbox.try_recv_for_test()
}

async fn retry_and_wait(fixture: &PersistenceFixture) {
    fixture.scribe.check_age(Instant::now());
    wait_for_state(fixture, 0).await;
}

async fn rows(fixture: &PersistenceFixture) -> Vec<(i64, i64, String)> {
    let mut conn = fixture
        .database
        .tenant_conn_for(fixture.tenant)
        .await
        .expect("tenant connection");
    sqlx::query_as(
        "SELECT wal_lsn_min, wal_lsn_max, file_path
           FROM vala.file_list
          WHERE data_tenant_id = $1
          ORDER BY wal_lsn_min",
    )
    .bind(fixture.tenant.as_uuid())
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("file-list rows")
}

async fn rows_for_table(fixture: &PersistenceFixture, table_name: &str) -> Vec<(i64, i64, String)> {
    let mut conn = fixture
        .database
        .tenant_conn_for(fixture.tenant)
        .await
        .expect("tenant connection");
    sqlx::query_as(
        "SELECT wal_lsn_min, wal_lsn_max, file_path FROM vala.file_list
          WHERE data_tenant_id = $1 AND table_name = $2 ORDER BY wal_lsn_min",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(table_name)
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("file-list rows")
}

/// Assert the published generations of one stream form a prefix-complete,
/// strictly ordered chain of disjoint WAL-LSN ranges.
///
/// Durable FIFO order for a single seal key is carried by each published file's
/// `[wal_lsn_min, wal_lsn_max]` range, NOT by the `created_at` publication
/// timestamp: replayed same-key generations commit within one clock tick, so
/// `ORDER BY created_at` breaks ties non-deterministically and cannot witness
/// WAL order. Given `ranges` already sorted by `wal_lsn_min` (see
/// [`rows_for_table`]), this checks (a) exactly `expected_len` generations
/// published — the non-vacuity guard against an empty or short chain, (b) every
/// range is well-formed (`min <= max`), and (c) each range ends strictly before
/// the next begins (`max[i] < min[i+1]`), so the ranges are pairwise disjoint
/// and chain in ascending WAL order. Disjointness is a real property that the
/// `wal_lsn_min` sort alone does not imply (sorted ranges may still overlap), so
/// the assertion is not a sort tautology.
fn assert_wal_lsn_chain(ranges: &[(i64, i64, String)], expected_len: usize) {
    assert_eq!(
        ranges.len(),
        expected_len,
        "expected {expected_len} published generations, got {ranges:?}"
    );
    assert!(
        ranges.iter().all(|range| range.0 <= range.1),
        "each generation must carry a well-formed WAL-LSN range: {ranges:?}"
    );
    assert!(
        ranges.windows(2).all(|pair| pair[0].1 < pair[1].0),
        "generations must chain as disjoint, ascending WAL-LSN ranges: {ranges:?}"
    );
}

async fn audit_count(fixture: &PersistenceFixture) -> i64 {
    let mut conn = fixture
        .database
        .tenant_conn_for(fixture.tenant)
        .await
        .expect("tenant connection");
    sqlx::query_scalar(
        "SELECT count(*)::bigint
           FROM vala.audit_outbox
          WHERE data_tenant_id = wyrd.current_tenant()
            AND operation = 'bifrost.append'",
    )
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("audit count")
}

async fn object_paths(fixture: &PersistenceFixture) -> Vec<String> {
    fixture
        .operator
        .list_with("")
        .recursive(true)
        .await
        .expect("object list")
        .into_iter()
        .map(|entry| entry.path().to_owned())
        .collect()
}

#[tokio::test]
async fn same_seal_key_generations_publish_in_fifo_order() {
    let fixture = PersistenceFixture::start().await;
    fixture
        .faults
        .set_object_write_delay_for_test(Duration::from_millis(50));
    append_one(&fixture, "fifo_events", 1).await;
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("first flush");
    append_one(&fixture, "fifo_events", 2).await;
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("second flush");
    wait_for_state(&fixture, 0).await;

    let persisted = rows(&fixture).await;
    assert_eq!(persisted.len(), 2);
    assert!(
        persisted[0].0 < persisted[1].0,
        "same-key FIFO was preserved"
    );
    fixture.stop().await;
}

#[tokio::test]
async fn different_seal_keys_persist_concurrently() {
    let fixture = PersistenceFixture::start().await;
    fixture
        .faults
        .set_object_write_delay_for_test(Duration::from_millis(100));
    append_one(&fixture, "concurrent_events_a", 1).await;
    append_one(&fixture, "concurrent_events_b", 2).await;
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("cross-key flush");
    wait_for_state(&fixture, 0).await;

    assert!(
        fixture.faults.max_concurrent_object_writes_for_test() >= 2,
        "different SealKey generations must overlap across persistence workers"
    );
    fixture.stop().await;
}

#[tokio::test]
async fn failed_front_generation_blocks_later_same_key_generation() {
    let fixture = PersistenceFixture::start().await;
    fixture.faults.fail_next_object_write();
    // The failed front can only block its successor when BOTH generations occupy
    // the same shard FIFO. `shard_for(tenant, table, batch_id)` keys on the
    // `batch_id`, so `append_one`'s per-call `now_v7` ids would otherwise spread
    // these two generations across lanes and let the successor publish
    // independently. Pin both to one lane by choosing a second, distinct
    // `batch_id` that routes to the same shard as the first. Distinct ids keep
    // this a genuine FIFO-ordering scenario, not a `(batch_id, seal_key)` replay.
    let front_batch_id = uuid::Uuid::from_u128(0x00FA_11ED);
    let successor_batch_id = colocated_distinct_batch_id(
        fixture.tenant,
        &table("failed_front_events"),
        front_batch_id,
    );
    append_with_batch_id(&fixture, "failed_front_events", 1, front_batch_id).await;
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("failed front flush");
    tokio::time::sleep(Duration::from_millis(200)).await;
    append_with_batch_id(&fixture, "failed_front_events", 2, successor_batch_id).await;
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("blocked successor flush");
    wait_for_state(&fixture, 2).await;
    assert!(
        rows(&fixture).await.is_empty(),
        "failed front must publish nothing"
    );

    retry_and_wait(&fixture).await;
    assert_eq!(
        rows(&fixture).await.len(),
        2,
        "retry releases the FIFO successor"
    );
    fixture.stop().await;
}

#[tokio::test]
async fn failed_generation_retries_with_fresh_object_id() {
    let fixture = PersistenceFixture::start().await;
    fixture.faults.fail_next_sql_commit();
    append_one(&fixture, "fresh_retry_events", 1).await;
    let wal_before = fixture.scribe.wal_bytes_on_disk();
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("SQL fault flush");
    wait_for_state(&fixture, 1).await;
    assert!(
        rows(&fixture).await.is_empty(),
        "SQL failure must roll back rows"
    );
    assert_eq!(fixture.scribe.wal_bytes_on_disk(), wal_before);
    assert_eq!(object_paths(&fixture).await.len(), 1);

    retry_and_wait(&fixture).await;
    let paths = object_paths(&fixture).await;
    assert_eq!(rows(&fixture).await.len(), 1);
    assert_eq!(paths.len(), 2, "retry must use a fresh object identifier");
    assert_ne!(paths[0], paths[1]);
    fixture.stop().await;
}

#[tokio::test]
async fn sql_failure_keeps_wal_and_file_list_unchanged() {
    let fixture = PersistenceFixture::start().await;
    append_one(&fixture, "sql_failure_events", 1).await;
    let wal_before = fixture.scribe.wal_bytes_on_disk();
    fixture.faults.fail_next_sql_commit();
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("SQL failure flush");
    wait_for_state(&fixture, 1).await;

    assert_eq!(fixture.scribe.wal_bytes_on_disk(), wal_before);
    assert!(rows(&fixture).await.is_empty());
    assert_eq!(audit_count(&fixture).await, 0);
    fixture.stop().await;
}

/// A confirmed file-list transaction publishes exactly one advisory wake-up.
#[tokio::test]
async fn confirmed_commit_publishes_hint() {
    let mut fixture = PersistenceFixture::start().await;
    append_one(&fixture, "confirmed_hint_events", 1).await;
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("confirmed flush");
    wait_for_state(&fixture, 0).await;

    assert!(hint_outcome(&mut fixture).is_ok());
    assert!(matches!(
        hint_outcome(&mut fixture),
        Err(tokio::sync::mpsc::error::TryRecvError::Empty)
    ));
    fixture.stop().await;
}

/// Pre-commit object and SQL failures publish no wake-up signal.
#[tokio::test]
async fn precommit_and_commit_failure_publish_no_hint() {
    let mut fixture = PersistenceFixture::start().await;
    fixture.faults.fail_next_object_write();
    append_one(&fixture, "precommit_hint_events", 1).await;
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("object failure flush");
    wait_for_state(&fixture, 1).await;
    assert!(matches!(
        hint_outcome(&mut fixture),
        Err(tokio::sync::mpsc::error::TryRecvError::Empty)
    ));

    retry_and_wait(&fixture).await;
    let _ = hint_outcome(&mut fixture);
    fixture.faults.fail_next_sql_commit();
    append_one(&fixture, "commit_hint_events", 2).await;
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("SQL failure flush");
    wait_for_state(&fixture, 1).await;
    assert!(matches!(
        hint_outcome(&mut fixture),
        Err(tokio::sync::mpsc::error::TryRecvError::Empty)
    ));
    fixture.stop().await;
}

/// A manifest failure after SQL commit still publishes the durable-row hint.
#[tokio::test]
async fn manifest_failure_after_commit_still_publishes_hint() {
    let mut fixture = PersistenceFixture::start().await;
    fixture.faults.fail_next_manifest_publication();
    append_one(&fixture, "manifest_hint_events", 1).await;
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("manifest failure flush");
    wait_for_state(&fixture, 1).await;
    assert_eq!(
        rows_for_table(&fixture, "manifest_hint_events").await.len(),
        1
    );
    assert!(hint_outcome(&mut fixture).is_ok());
    fixture.stop().await;
}

#[tokio::test]
async fn manifest_failure_retains_retryable_front() {
    let fixture = PersistenceFixture::start().await;
    fixture.faults.fail_next_manifest_publication();
    append_one(&fixture, "manifest_failure_events", 1).await;
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("manifest failure flush");
    wait_for_state(&fixture, 1).await;
    assert_eq!(
        rows(&fixture).await.len(),
        1,
        "SQL commits before manifest failure"
    );
    assert_eq!(audit_count(&fixture).await, 1);

    retry_and_wait(&fixture).await;
    assert_eq!(
        rows(&fixture).await.len(),
        1,
        "manifest retry must dedupe SQL"
    );
    assert_eq!(
        audit_count(&fixture).await,
        1,
        "manifest retry must not duplicate audit"
    );
    fixture.stop().await;
}

#[tokio::test]
async fn file_list_and_audit_commit_atomically() {
    let fixture = PersistenceFixture::start().await;
    fixture.faults.fail_next_sql_commit();
    append_one(&fixture, "atomic_events", 1).await;
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("atomic SQL fault flush");
    wait_for_state(&fixture, 1).await;
    assert!(rows(&fixture).await.is_empty());
    assert_eq!(audit_count(&fixture).await, 0);

    retry_and_wait(&fixture).await;
    assert_eq!(rows(&fixture).await.len(), 1);
    assert_eq!(audit_count(&fixture).await, 1);
    fixture.stop().await;
}

#[tokio::test]
async fn three_generations_same_key_remain_fifo_and_file_paths_are_object_keys() {
    let fixture = PersistenceFixture::start().await;
    fixture
        .faults
        .set_object_write_delay_for_test(Duration::from_millis(50));
    for value in 1..=3 {
        append_one(&fixture, "three_generation_events", value).await;
        fixture
            .scribe
            .flush_writable_for_test()
            .await
            .expect("generation flush");
    }

    wait_for_state(&fixture, 0).await;
    let persisted = rows(&fixture).await;
    let objects = object_paths(&fixture).await;
    assert_eq!(persisted.len(), 3);
    assert_eq!(objects.len(), 3);
    assert!(persisted.windows(2).all(|pair| pair[0].0 < pair[1].0));
    for (_, _, path) in persisted {
        assert_eq!(path.matches("day=").count(), 1);
        assert!(
            objects.contains(&path),
            "file_list path must be an object key"
        );
    }
    fixture.stop().await;
}

#[tokio::test]
async fn replayed_generation_publishes_durably_after_restart() {
    let fixture = PersistenceFixture::start_after_wal_restart(false).await;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if fixture.scribe.persistence_queue_depth_for_test() == 0 && rows(&fixture).await.len() == 3
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "replay persistence stalled: {:?}",
            fixture.faults.last_error_for_test()
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(rows(&fixture).await.len(), 3);
    // The three replayed same-key generations must publish as an ordered chain
    // of disjoint WAL-LSN ranges (durable FIFO order lives in the WAL range, not
    // in `created_at`, which ties across the batched replay commits).
    assert_wal_lsn_chain(&rows_for_table(&fixture, "restart_publish_events").await, 3);
    assert_eq!(audit_count(&fixture).await, 3);
    assert_eq!(object_paths(&fixture).await.len(), 3);
    fixture.stop().await;
}

/// Preserves immutable row identity from a replayed WAL Arrow batch into Parquet publication.
#[tokio::test]
async fn wal_replay_preserves_row_identity() {
    let fixture = PersistenceFixture::start_after_wal_restart(false).await;
    let deadline = Instant::now() + Duration::from_secs(15);
    let path = loop {
        let persisted = rows(&fixture).await;
        if fixture.scribe.persistence_queue_depth_for_test() == 0 && !persisted.is_empty() {
            break persisted[0].2.clone();
        }
        assert!(Instant::now() < deadline, "replay publication stalled");
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    let bytes = fixture
        .operator
        .read(&path)
        .await
        .expect("published parquet reads");
    let builder = ParquetRecordBatchReaderBuilder::try_new(bytes::Bytes::from(bytes.to_vec()))
        .expect("published object is parquet");
    let reader = builder.build().expect("parquet reader builds");
    let ordinals = reader
        .flat_map(|batch| {
            let batch = batch.expect("parquet batch decodes");
            batch
                .column_by_name("wyrd_row_ordinal")
                .expect("row identity column persists")
                .as_any()
                .downcast_ref::<Int32Array>()
                .expect("row identity remains Int32")
                .values()
                .to_vec()
        })
        .collect::<Vec<_>>();
    assert_eq!(ordinals.len(), 50_000);
    assert!(ordinals.iter().enumerate().all(|(index, ordinal)| {
        *ordinal == i32::try_from(index).expect("test ordinal fits i32")
    }));
    fixture.stop().await;
}

#[tokio::test]
async fn replayed_generation_failure_keeps_replacement_unready() {
    let error = match PersistenceFixture::start_after_wal_restart_with_keys(
        true,
        &["restart_publish_events"],
        3,
        &[
            Duration::from_millis(300),
            Duration::from_millis(1),
            Duration::from_millis(1),
        ],
    )
    .await
    {
        Ok(fixture) => {
            fixture.stop().await;
            panic!("replay publication failure must fail replacement startup");
        }
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("test object-store write failure")
    );
}

#[tokio::test]
async fn replayed_distinct_keys_publish_concurrently_and_fifo() {
    let fixture = PersistenceFixture::start_after_wal_restart_with_keys(
        false,
        &["restart_key_a", "restart_key_b"],
        2,
        &[Duration::from_millis(100)],
    )
    .await
    .expect("replay");
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if fixture.scribe.persistence_queue_depth_for_test() == 0
            && rows_for_table(&fixture, "restart_key_a").await.len() == 2
            && rows_for_table(&fixture, "restart_key_b").await.len() == 2
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "cross-key replay stalled: {:?}",
            fixture.faults.last_error_for_test()
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(fixture.faults.max_concurrent_object_writes_for_test() >= 2);
    // Distinct seal keys publish concurrently (asserted above), yet within each
    // key the two generations must still form an ordered chain of disjoint
    // WAL-LSN ranges — per-stream FIFO independent of cross-key publish timing.
    for table_name in ["restart_key_a", "restart_key_b"] {
        assert_wal_lsn_chain(&rows_for_table(&fixture, table_name).await, 2);
    }
    assert_eq!(audit_count(&fixture).await, 4);
    assert_eq!(object_paths(&fixture).await.len(), 4);
    fixture.stop().await;
}

/// Builds a native Arrow IPC frame carrying a caller-supplied `wyrd_event_time`
/// column whose two rows fall on two distinct UTC partition days.
///
/// The `wyrd_event_time` field uses the managed physical type
/// (`Timestamp(Microsecond, UTC)`) so the native ingest contract accepts it as
/// the authoritative event time instead of stamping the server receipt time.
fn native_event_time_frame(
    tenant: DataTenantId,
    table_ref: &TableRef,
    first_day_micros: i64,
    second_day_micros: i64,
) -> ScribeIngressFrame {
    let schema = Arc::new(Schema::new(vec![
        Field::new("value", DataType::Int64, false),
        Field::new(
            "wyrd_event_time",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
    ]));
    let batch = RecordBatch::try_new(
        Arc::clone(&schema),
        vec![
            Arc::new(Int64Array::from(vec![1_i64, 2_i64])),
            Arc::new(
                TimestampMicrosecondArray::from(vec![first_day_micros, second_day_micros])
                    .with_timezone("UTC"),
            ),
        ],
    )
    .expect("native caller event-time batch is valid");
    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, schema.as_ref()).expect("IPC writer");
    writer.write(&batch).expect("IPC batch");
    writer.finish().expect("IPC finish");

    let user_only = Schema::new(vec![Field::new("value", DataType::Int64, false)]);
    let request_id = RequestId::now_v7();
    ScribeIngressFrame {
        principal: principal(tenant),
        binding: TenantTableBinding::resolve((tenant, table_ref.clone())).expect("binding"),
        expected_schema_fingerprint: SchemaFingerprint::from_arrow_schema(&user_only),
        request_id: request_id.clone(),
        batch_id: uuid::Uuid::now_v7(),
        audit_event: audit_event("bifrost.append", request_id),
        measured_wire_bytes: bytes.len(),
        payload: IngressPayload::ArrowIpc(bytes.into()),
    }
}

/// Persistence workspace admission remains available while a governed Oracle
/// range overlaps, and both role counters return to their exact baseline.
#[tokio::test]
async fn persistence_headroom_survives_oracle_range_overlap() {
    let memory =
        BifrostMemoryGovernor::new_with_test_scribe_limit(1024 * 1024 * 1024, 256 * 1024 * 1024)
            .expect("bounded production-formula governor");
    let fixture = PersistenceFixture::start_with_memory(memory).await;
    let baseline = fixture.memory.snapshot();
    let oracle = fixture.memory.oracle_budget();
    let range = oracle
        .try_reserve(oracle.limit_bytes())
        .expect("worst-case Oracle child range occupancy");
    let schema = Arc::new(Schema::new(vec![
        Field::new(
            "wyrd_event_time",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
        Field::new("payload", DataType::Utf8, false),
    ]));
    let rows = RecordBatch::try_new(
        Arc::clone(&schema),
        vec![
            Arc::new(
                TimestampMicrosecondArray::from(vec![chrono::Utc::now().timestamp_micros()])
                    .with_timezone("UTC"),
            ),
            Arc::new(arrow::array::StringArray::from(vec![
                "x".repeat(59 * 1024 * 1024),
            ])),
        ],
    )
    .expect("near-target persistence batch");
    let target_bytes = fixture.memory.scribe_budget().active_bucket_target_bytes();
    let request_id = RequestId::now_v7();
    let measured_wire_bytes = fixture
        .calibrate_exact_wire_bytes(
            &rows,
            schema.as_ref(),
            "persistence_range_overlap",
            target_bytes,
        )
        .await;
    fixture
        .scribe
        .append(ScribeAppend {
            principal: principal(fixture.tenant),
            table: table("persistence_range_overlap"),
            schema_fingerprint: SchemaFingerprint::from_arrow_schema(schema.as_ref()),
            request_id,
            batch_id: uuid::Uuid::now_v7(),
            measured_wire_bytes,
            rows,
        })
        .await
        .expect("near-target generation admits under Oracle overlap");
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("flush while Oracle range is retained");
    wait_for_state(&fixture, 0).await;
    assert_eq!(
        rows_for_table(&fixture, "persistence_range_overlap")
            .await
            .len(),
        1
    );
    fixture
        .scribe
        .retire_committed_for_test()
        .await
        .expect("retire committed generation while Oracle range is retained");
    let overlapped = fixture.memory.snapshot();
    assert_eq!(overlapped.oracle_total_bytes, oracle.limit_bytes());
    assert!(overlapped.bifrost_total_bytes >= overlapped.oracle_total_bytes);
    drop(range);
    let restored = fixture.memory.snapshot();
    assert_eq!(restored.oracle_total_bytes, baseline.oracle_total_bytes);
    assert_eq!(
        restored.scribe_total_bytes, baseline.scribe_total_bytes,
        "scribe ownership did not restore: {restored:?}"
    );
    assert_eq!(
        restored.bifrost_total_bytes, baseline.bifrost_total_bytes,
        "parent ownership did not restore: {restored:?}"
    );
    fixture.stop().await;
}

/// End-to-end proof that native ingest honours a caller-supplied event time:
/// two rows stamped with event times on two distinct UTC days land in two
/// distinct partition-day objects, matching the projected (OTLP) contract.
#[tokio::test]
async fn native_caller_event_time_lands_on_two_partition_days() {
    let fixture = PersistenceFixture::start().await;
    // 2026-07-14T12:00:00Z and 2026-07-16T12:00:00Z: two distinct partition days.
    let first_day_micros = chrono::NaiveDate::from_ymd_opt(2026, 7, 14)
        .expect("valid day")
        .and_hms_opt(12, 0, 0)
        .expect("valid time")
        .and_utc()
        .timestamp_micros();
    let second_day_micros = chrono::NaiveDate::from_ymd_opt(2026, 7, 16)
        .expect("valid day")
        .and_hms_opt(12, 0, 0)
        .expect("valid time")
        .and_utc()
        .timestamp_micros();
    let table_ref = table("native_event_time_events");
    let frame = native_event_time_frame(
        fixture.tenant,
        &table_ref,
        first_day_micros,
        second_day_micros,
    );
    fixture
        .scribe
        .ingest_frame(frame)
        .await
        .expect("native caller event time is admitted");
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("flush native event-time rows");
    wait_for_state(&fixture, 0).await;

    let persisted = rows_for_table(&fixture, "native_event_time_events").await;
    assert_eq!(
        persisted.len(),
        2,
        "two distinct event days must publish two file-list entries"
    );
    let day_paths: Vec<String> = object_paths(&fixture)
        .await
        .into_iter()
        .filter(|path| path.contains("day="))
        .collect();
    assert_eq!(
        day_paths.len(),
        2,
        "two distinct event days must land in two partition-day objects: {day_paths:?}"
    );
    let mut days: Vec<String> = day_paths
        .iter()
        .filter_map(|path| {
            path.split('/')
                .find(|segment| segment.starts_with("day="))
                .map(str::to_owned)
        })
        .collect();
    days.sort();
    days.dedup();
    assert_eq!(days.len(), 2, "partition days must be distinct: {days:?}");
    assert!(days.iter().any(|day| day.contains("2026-07-14")));
    assert!(days.iter().any(|day| day.contains("2026-07-16")));
    fixture.stop().await;
}
