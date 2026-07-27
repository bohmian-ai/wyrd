//! Postgres/object-store coverage for ordered and retryable Scribe persistence.

use std::sync::Arc;
use std::time::{Duration, Instant};

use arrow::array::{
    FixedSizeBinaryBuilder, Int64Array, RecordBatch, StringArray, TimestampMicrosecondArray,
};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::ipc::writer::StreamWriter;
use opendal::services::Memory;
use secrecy::ExposeSecret;
use tempfile::TempDir;
use vala_bifrost_redux::catalog::{BifrostCatalog, CreateTableRequest, TableRef};
use vala_bifrost_redux::contracts::{Scribe, ScribeAppend};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::schema::fingerprint::SchemaFingerprint;
use vala_bifrost_redux::scribe::audit_envelope::encode_audit_event;
use vala_bifrost_redux::scribe::memory::BifrostMemoryGovernor;
use vala_bifrost_redux::scribe::persistence::PersistenceFaults;
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
    faults: PersistenceFaults,
    wal_root: TempDir,
    _warehouse: Option<TempDir>,
    tenant: DataTenantId,
}

impl PersistenceFixture {
    async fn start() -> Self {
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
        let persistence =
            ScribePersistenceConfig::new(Arc::new(database.vala_postgres().clone()), 16, 2)
                .with_test_faults(faults.clone());
        let memory = BifrostMemoryGovernor::new(8 * 1024 * 1024 * 1024).expect("memory governor");
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
        let scribe = Arc::new(ScribeImpl::new_with_execution_pools(
            Arc::clone(&operator),
            wal,
            node_id.to_string(),
            1,
            ScribeBuildConfig {
                admission,
                coordination_runtime: tokio::runtime::Handle::current(),
                execution_pools: pools,
                persistence: Some(persistence),
                memory_budget: Some(memory.scribe_budget()),
            },
        ));
        scribe.replay_wal_async().await.expect("empty WAL replay");
        Self {
            database,
            operator,
            scribe,
            faults,
            wal_root,
            _warehouse: None,
            tenant,
        }
    }

    async fn stop(self) {
        self.scribe.shutdown().await;
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
    }

    async fn start_after_wal_restart_with_keys(
        fail_replay_write: bool,
        table_names: &[&str],
        generations: i64,
        object_write_delays: &[Duration],
    ) -> Self {
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
        };
        let first = first_replay_scribe(
            operator.clone(),
            wal.clone(),
            node_id,
            admission.clone(),
            memory.clone(),
        );
        first.replay_wal_async().await.expect("empty WAL replay");
        write_replay_records(&wal, table_names, generations, tenant);
        first.shutdown().await;
        drop(wal);

        let faults = PersistenceFaults::default();
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
        let scribe = Arc::new(ScribeImpl::new_with_execution_pools(
            Arc::clone(&operator),
            wal,
            node_id.to_string(),
            2,
            ScribeBuildConfig {
                admission,
                coordination_runtime: tokio::runtime::Handle::current(),
                execution_pools: pools,
                persistence: Some(persistence),
                memory_budget: Some(memory.scribe_budget()),
            },
        ));
        scribe.replay_wal_async().await.expect("replay");
        Self {
            database,
            operator,
            scribe,
            faults,
            wal_root,
            _warehouse: Some(warehouse),
            tenant,
        }
    }
}

fn first_replay_scribe(
    operator: Arc<opendal::Operator>,
    wal: Arc<WalWriter>,
    node_id: uuid::Uuid,
    admission: vala_bifrost_redux::scribe::admission::AdmissionConfig,
    memory: BifrostMemoryGovernor,
) -> Arc<ScribeImpl> {
    let pools = ScribeExecutionPools::new(
        ScribeIngressCpuPool::new_with_capacity(1, 256),
        ScribePersistenceCpuPool::new_with_capacity(2, 64),
        ScribeWalIoPool::new_with_capacity(2, 256),
    );
    Arc::new(ScribeImpl::new_with_execution_pools(
        operator,
        wal,
        node_id.to_string(),
        1,
        ScribeBuildConfig {
            admission,
            coordination_runtime: tokio::runtime::Handle::current(),
            execution_pools: pools,
            persistence: None,
            memory_budget: Some(memory.scribe_budget()),
        },
    ))
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
            DataType::Timestamp(TimeUnit::Microsecond, None),
            false,
        ),
        Field::new("value", DataType::Int64, false),
    ]));
    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(TimestampMicrosecondArray::from(vec![
                chrono::Utc::now().timestamp_micros(),
            ])),
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
    let rows = batch(value);
    fixture
        .scribe
        .append(ScribeAppend {
            principal: principal(fixture.tenant),
            table: table(table_name),
            schema_fingerprint: SchemaFingerprint::from_arrow_schema(rows.schema().as_ref()),
            request_id: RequestId::now_v7(),
            batch_id: uuid::Uuid::now_v7(),
            measured_wire_bytes: 0,
            rows,
        })
        .await
        .expect("append to Scribe");
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

async fn publication_order(fixture: &PersistenceFixture, table_name: &str) -> Vec<i64> {
    let mut conn = fixture
        .database
        .tenant_conn_for(fixture.tenant)
        .await
        .expect("tenant connection");
    sqlx::query_scalar(
        "SELECT wal_lsn_min FROM vala.file_list
          WHERE data_tenant_id = $1 AND table_name = $2 ORDER BY created_at",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(table_name)
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("publication order")
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
    append_one(&fixture, "failed_front_events", 1).await;
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("failed front flush");
    tokio::time::sleep(Duration::from_millis(200)).await;
    append_one(&fixture, "failed_front_events", 2).await;
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
    assert_eq!(
        publication_order(&fixture, "restart_publish_events").await,
        vec![0, 1, 2]
    );
    assert_eq!(audit_count(&fixture).await, 3);
    assert_eq!(object_paths(&fixture).await.len(), 3);
    fixture.stop().await;
}

#[tokio::test]
async fn replayed_generation_failure_retries_to_durable_publication() {
    let fixture = PersistenceFixture::start_after_wal_restart(true).await;
    let failure_deadline = Instant::now() + Duration::from_secs(10);
    while !fixture
        .faults
        .last_error_for_test()
        .is_some_and(|error| error.contains("test object-store write failure"))
    {
        assert!(
            Instant::now() < failure_deadline,
            "injected replay failure did not fire"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(rows(&fixture).await.is_empty());
    fixture.scribe.check_age(Instant::now());
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if fixture.scribe.persistence_queue_depth_for_test() == 0 && rows(&fixture).await.len() == 3
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "replay retry stalled: {:?}",
            fixture.faults.last_error_for_test()
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(
        publication_order(&fixture, "restart_publish_events").await,
        vec![0, 1, 2]
    );
    assert_eq!(audit_count(&fixture).await, 3);
    fixture.stop().await;
}

#[tokio::test]
async fn replayed_distinct_keys_publish_concurrently_and_fifo() {
    let fixture = PersistenceFixture::start_after_wal_restart_with_keys(
        false,
        &["restart_key_a", "restart_key_b"],
        2,
        &[Duration::from_millis(100)],
    )
    .await;
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
    for table_name in ["restart_key_a", "restart_key_b"] {
        assert_eq!(publication_order(&fixture, table_name).await.len(), 2);
        let persisted = publication_order(&fixture, table_name).await;
        assert!(persisted.windows(2).all(|pair| pair[0] < pair[1]));
    }
    assert_eq!(audit_count(&fixture).await, 4);
    assert_eq!(object_paths(&fixture).await.len(), 4);
    fixture.stop().await;
}
