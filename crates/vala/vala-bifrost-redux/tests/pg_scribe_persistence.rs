//! Postgres/object-store coverage for ordered and retryable Scribe persistence.

use std::sync::Arc;
use std::time::{Duration, Instant};

use arrow::array::{Int64Array, RecordBatch, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use opendal::services::Memory;
use tempfile::TempDir;
use vala_bifrost_redux::catalog::TableRef;
use vala_bifrost_redux::contracts::{Scribe, ScribeAppend};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::schema::fingerprint::SchemaFingerprint;
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
use wyrd_spec::request_id::RequestId;

struct PersistenceFixture {
    database: PgFixture,
    operator: Arc<opendal::Operator>,
    scribe: Arc<ScribeImpl>,
    faults: PersistenceFaults,
    wal_root: TempDir,
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
        let memory = BifrostMemoryGovernor::new(1024 * 1024 * 1024).expect("memory governor");
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
            tenant,
        }
    }

    async fn stop(self) {
        self.scribe.shutdown().await;
        drop(self.wal_root);
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
