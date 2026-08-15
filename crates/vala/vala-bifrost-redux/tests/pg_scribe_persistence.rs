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
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use vala_bifrost_redux::catalog::{
    BifrostCatalog, CreateTableRequest, TableRef, TenantTableBinding,
};
use vala_bifrost_redux::contracts::{
    IngressPayload, Scribe, ScribeAppend, ScribeError, ScribeIngressFrame,
};
use vala_bifrost_redux::maintenance::staging_file_channel;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::resources::{
    BifrostResourcePolicy, BifrostRole, BifrostRoleResources, BifrostRuntimeResources,
    OracleWorkerClass, ResourceSource, SystemResourceSnapshot,
};
use vala_bifrost_redux::schema::fingerprint::SchemaFingerprint;
use vala_bifrost_redux::scribe::audit_envelope::encode_audit_event;
use vala_bifrost_redux::scribe::memory::MemoryCategory;
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
    memory: BifrostRoleResources,
    faults: PersistenceFaults,
    /// Receiver retained so post-commit hints remain observable until assertions finish.
    hint_inbox: vala_bifrost_redux::maintenance::StagingFileInbox,
    wal_root: TempDir,
    /// Physical root retained for generation-owned Scribe output scratch.
    scratch_root: TempDir,
    _warehouse: Option<TempDir>,
    tenant: DataTenantId,
    /// Aggregate decoded Arrow ownership represented by the seeded replay WAL.
    replay_decoded_bytes: usize,
}

/// Composes production-equivalent Scribe and Oracle capabilities for persistence fixtures.
fn persistence_test_roles(memory_limit_bytes: usize) -> BifrostRoleResources {
    BifrostRuntimeResources::from_snapshot(
        SystemResourceSnapshot {
            memory_limit_bytes,
            effective_cpu: 4,
            scratch_capacity_bytes: 2 * 1024 * 1024 * 1024,
            scratch_available_bytes: 2 * 1024 * 1024 * 1024,
            memory_source: ResourceSource::Injected,
            cpu_source: ResourceSource::Injected,
        },
        BifrostResourcePolicy {
            roles: [BifrostRole::Scribe, BifrostRole::Oracle]
                .into_iter()
                .collect(),
            memory_limit_bytes: None,
            unmanaged_reserve_bytes: None,
            scratch_limit_bytes: None,
            effective_cpu: None,
            scratch_root: std::path::PathBuf::new(),
            volume_roots: None,
        },
    )
    .expect("test Bifrost runtime resources")
    .compose_roles()
    .expect("test Bifrost role resources")
}

impl PersistenceFixture {
    /// Registers the exact Scribe stream fence consumed by writer-v2 publication.
    ///
    /// # Panics
    ///
    /// Panics when the repository-managed Postgres fixture cannot expose its
    /// operator connection or persist the requested stream epoch.
    async fn register_scribe_fence(database: &PgFixture, node_id: uuid::Uuid, epoch: i64) {
        let pool = database.superuser_pool().await.expect("superuser pool");
        sqlx::query(
            "INSERT INTO vala.cluster_nodes (data_tenant_id,node_id,role,advertise_addr,fencing_token,started_at,heartbeat_at) VALUES ($1,$2,'scribe','127.0.0.1:1',$3,now(),now()) ON CONFLICT (data_tenant_id,node_id,role) DO UPDATE SET fencing_token=EXCLUDED.fencing_token,heartbeat_at=now()",
        )
        .bind(DataTenantId::SYSTEM_OWNER.as_uuid())
        .bind(node_id)
        .bind(epoch)
        .execute(&pool)
        .await
        .expect("register Scribe publication fence");
    }

    /// Starts the standard persistence fixture with the production-derived governor.
    ///
    /// # Panics
    ///
    /// Panics when the governor or any Postgres, object-store, WAL, execution-lane,
    /// persistence, or Scribe fixture dependency cannot be constructed.
    async fn start() -> Self {
        Self::start_with_memory(persistence_test_roles(8 * 1024 * 1024 * 1024)).await
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
    async fn start_with_memory(requested_memory: BifrostRoleResources) -> Self {
        let database = PgFixture::start().await.expect("Postgres fixture");
        let tenant = database.data_tenant_id();
        let operator = Arc::new(
            opendal::Operator::new(Memory::default())
                .expect("memory object store")
                .finish(),
        );
        let wal_root = tempfile::tempdir().expect("WAL directory");
        let scratch_root = tempfile::tempdir().expect("scratch directory");
        let scribe_output = scratch_root.path().join("scribe-output");
        let forge_scratch = scratch_root.path().join("forge");
        let oracle_scratch = scratch_root.path().join("oracle");
        for root in [&scribe_output, &forge_scratch, &oracle_scratch] {
            std::fs::create_dir(root).expect("test volume root");
        }
        let requested_snapshot = requested_memory.snapshot().expect("root snapshot");
        let runtime_resources =
            vala_bifrost_redux::resources::BifrostRuntimeResources::from_snapshot(
                vala_bifrost_redux::resources::SystemResourceSnapshot {
                    memory_limit_bytes: requested_snapshot.plan.memory_limit_bytes,
                    effective_cpu: 4,
                    scratch_capacity_bytes: 2 * 1024 * 1024 * 1024,
                    scratch_available_bytes: 2 * 1024 * 1024 * 1024,
                    memory_source: vala_bifrost_redux::resources::ResourceSource::Injected,
                    cpu_source: vala_bifrost_redux::resources::ResourceSource::Injected,
                },
                vala_bifrost_redux::resources::BifrostResourcePolicy {
                    roles: std::collections::BTreeSet::from([
                        vala_bifrost_redux::resources::BifrostRole::Scribe,
                        vala_bifrost_redux::resources::BifrostRole::Oracle,
                    ]),
                    memory_limit_bytes: None,
                    unmanaged_reserve_bytes: None,
                    scratch_limit_bytes: None,
                    effective_cpu: None,
                    scratch_root: scratch_root.path().to_owned(),
                    volume_roots: Some(vala_bifrost_redux::resources::BifrostVolumeRoots {
                        wal: wal_root.path().to_owned(),
                        scribe_output_scratch: scribe_output,
                        forge_scratch,
                        oracle_scratch,
                    }),
                },
            )
            .expect("test Bifrost resources");
        let resources = runtime_resources
            .compose_roles()
            .expect("test role resources");
        let scribe_resources = resources.scribe().expect("test Scribe resources");
        let memory = resources.clone();
        let (_, output_scratch) = scribe_resources
            .volume_capabilities()
            .expect("test Scribe volumes");
        let node_id = uuid::Uuid::now_v7();
        Self::register_scribe_fence(&database, node_id, 1).await;
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
                .with_operator_pool(database.operator_pool().clone())
                .with_output_scratch(output_scratch)
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
            resources: scribe_resources,
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
            scratch_root,
            _warehouse: None,
            tenant,
            replay_decoded_bytes: 0,
        }
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
            persistence_test_roles(9 * 1024 * 1024 * 1024),
            2,
            50_000,
        )
        .await
        .expect("replay")
    }

    async fn start_after_wal_restart_with_keys<T: AsRef<str>>(
        fail_replay_write: bool,
        table_names: &[T],
        generations: i64,
        object_write_delays: &[Duration],
        memory: BifrostRoleResources,
        wal_io_threads: usize,
        rows_per_generation: usize,
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
        let scratch_root = tempfile::tempdir().expect("scratch directory");
        let scribe_output = scratch_root.path().join("scribe-output");
        let forge_scratch = scratch_root.path().join("forge");
        let oracle_scratch = scratch_root.path().join("oracle");
        for root in [&scribe_output, &forge_scratch, &oracle_scratch] {
            std::fs::create_dir(root).expect("test volume root");
        }
        let requested_snapshot = memory.snapshot().expect("root snapshot");
        let runtime_resources =
            vala_bifrost_redux::resources::BifrostRuntimeResources::from_snapshot(
                vala_bifrost_redux::resources::SystemResourceSnapshot {
                    memory_limit_bytes: requested_snapshot.plan.memory_limit_bytes,
                    effective_cpu: 4,
                    scratch_capacity_bytes: 2 * 1024 * 1024 * 1024,
                    scratch_available_bytes: 2 * 1024 * 1024 * 1024,
                    memory_source: vala_bifrost_redux::resources::ResourceSource::Injected,
                    cpu_source: vala_bifrost_redux::resources::ResourceSource::Injected,
                },
                vala_bifrost_redux::resources::BifrostResourcePolicy {
                    roles: std::collections::BTreeSet::from([
                        vala_bifrost_redux::resources::BifrostRole::Scribe,
                        vala_bifrost_redux::resources::BifrostRole::Oracle,
                    ]),
                    memory_limit_bytes: None,
                    unmanaged_reserve_bytes: None,
                    scratch_limit_bytes: None,
                    effective_cpu: None,
                    scratch_root: scratch_root.path().to_owned(),
                    volume_roots: Some(vala_bifrost_redux::resources::BifrostVolumeRoots {
                        wal: wal_root.path().to_owned(),
                        scribe_output_scratch: scribe_output,
                        forge_scratch,
                        oracle_scratch,
                    }),
                },
            )
            .expect("test Bifrost resources");
        let resources = runtime_resources
            .compose_roles()
            .expect("test role resources");
        let scribe_resources = resources.scribe().expect("test Scribe resources");
        let memory = resources.clone();
        let (_, output_scratch) = scribe_resources
            .volume_capabilities()
            .expect("test Scribe volumes");
        let node_id = uuid::Uuid::now_v7();
        Self::register_scribe_fence(&database, node_id, 1).await;
        let wal = Arc::new(
            WalWriter::new(
                wal_root.path(),
                *node_id.as_bytes(),
                1,
                WalConfig::default(),
            )
            .expect("WAL writer"),
        );
        let admission = vala_bifrost_redux::scribe::admission::AdmissionConfig {
            memory_limit_bytes: 4 * 1024 * 1024 * 1024,
            scribe_memory_limit_bytes: Some(8 * 1024 * 1024 * 1024),
            event_time_window: vala_bifrost_redux::scribe::admission::EventTimeWindow::default(),
        };
        let first = first_replay_scribe(operator.clone(), wal.clone(), node_id, admission, &memory);
        first.replay_wal_async().await.expect("empty WAL replay");
        let replay_decoded_bytes =
            write_replay_records(&wal, table_names, generations, tenant, rows_per_generation);
        first
            .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(1))
            .await;
        drop(wal);
        Self::register_scribe_fence(&database, node_id, 2).await;

        let faults = PersistenceFaults::default();
        let (staging_file_publisher, hint_inbox) = staging_file_channel(16).expect("hint channel");
        if fail_replay_write {
            faults.fail_next_object_write();
        }
        faults.set_object_write_delays_for_test(object_write_delays);
        let persistence =
            ScribePersistenceConfig::new(Arc::new(database.vala_postgres().clone()), 16, 2)
                .with_operator_pool(database.operator_pool().clone())
                .with_output_scratch(output_scratch)
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
            ScribeWalIoPool::new_with_capacity(wal_io_threads, 256),
        );
        let scribe = Arc::new(ScribeImpl::new_with_execution_pools(ScribeBuildConfig {
            operator: Arc::clone(&operator),
            wal,
            stream: StreamIdentity::new(NodeId::new(node_id), WriterEpoch::new(2)),
            admission,
            coordination_runtime: tokio::runtime::Handle::current(),
            execution_pools: pools,
            persistence: Some(persistence),
            resources: scribe_resources,
            staging_file_publisher: Some(staging_file_publisher),
        }));
        if let Err(error) = scribe.replay_wal_async().await {
            return Err(Self::assert_failed_replay(&database, tenant, &scribe, error).await);
        }
        Ok(Self {
            database,
            operator,
            scribe,
            memory,
            faults,
            hint_inbox,
            wal_root,
            scratch_root,
            _warehouse: Some(warehouse),
            tenant,
            replay_decoded_bytes,
        })
    }

    /// Verifies the fail-closed startup state before returning its exact error.
    ///
    /// The helper shuts down the failed owner, then proves no replay generation
    /// became visible after the terminal failure.
    ///
    /// # Panics
    ///
    /// Panics when failed replay became ready, Postgres inspection fails, or
    /// any file-list publication survived the failed startup.
    async fn assert_failed_replay(
        database: &PgFixture,
        tenant: DataTenantId,
        scribe: &Arc<ScribeImpl>,
        error: ScribeError,
    ) -> ScribeError {
        assert!(!scribe.is_ready(), "failed replay must remain unready");
        scribe
            .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(1))
            .await;
        let mut conn = database
            .tenant_conn_for(tenant)
            .await
            .expect("failed-replay tenant connection");
        let published: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM vala.file_list WHERE data_tenant_id = $1")
                .bind(tenant.as_uuid())
                .fetch_one(&mut **conn.transaction())
                .await
                .expect("failed-replay publication count");
        assert_eq!(published, 0, "failed replay must not publish later state");
        error
    }
}

fn first_replay_scribe(
    operator: Arc<opendal::Operator>,
    wal: Arc<WalWriter>,
    node_id: uuid::Uuid,
    admission: vala_bifrost_redux::scribe::admission::AdmissionConfig,
    memory: &BifrostRoleResources,
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
        resources: memory.scribe().expect("composed Scribe capability"),
        staging_file_publisher: None,
    }))
}

async fn register_replay_tables<T: AsRef<str>>(
    catalog: &BifrostCatalog,
    table_names: &[T],
    tenant: DataTenantId,
) {
    for table_name in table_names {
        let table_name = table_name.as_ref();
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

fn write_replay_records<T: AsRef<str>>(
    wal: &WalWriter,
    table_names: &[T],
    generations: i64,
    tenant: DataTenantId,
    rows_per_generation: usize,
) -> usize {
    let mut decoded_bytes = 0_usize;
    for table_name in table_names {
        let table_name = table_name.as_ref();
        for value in 1_i64..=generations {
            let batch_id = *uuid::Uuid::now_v7().as_bytes();
            let request_id = RequestId::now_v7();
            let (data, batch_bytes) =
                managed_batch_bytes(value, tenant, batch_id, &request_id, rows_per_generation);
            decoded_bytes = decoded_bytes.saturating_add(batch_bytes);
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
    decoded_bytes
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
    row_count: usize,
) -> (Vec<u8>, usize) {
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
    let decoded_bytes = batch.get_array_memory_size();
    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, &schema).expect("IPC writer");
    writer.write(&batch).expect("IPC batch");
    writer.finish().expect("IPC finish");
    (bytes, decoded_bytes)
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
            "persistence state did not converge: pending={}, queue={}, last_error={:?}",
            stats.pending_generations,
            fixture.scribe.persistence_queue_depth_for_test(),
            fixture.faults.last_error_for_test()
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

/// Fetches the ordered catalog identity needed to prove object parity.
async fn artifact_rows_for_table(
    fixture: &PersistenceFixture,
    table_name: &str,
) -> Vec<(i64, i64, i64, String, i64, String)> {
    let mut conn = fixture
        .database
        .tenant_conn_for(fixture.tenant)
        .await
        .expect("tenant connection");
    sqlx::query_as(
        "SELECT writer_epoch, wal_lsn_min, wal_lsn_max, file_path, file_size, file_checksum
           FROM vala.file_list
          WHERE data_tenant_id = $1 AND table_name = $2
          ORDER BY writer_epoch, wal_lsn_min, file_ordinal",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(table_name)
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("artifact catalog rows")
}

/// Proves every catalog length and SHA-256 digest matches its fetched object.
async fn assert_catalog_object_parity(
    fixture: &PersistenceFixture,
    rows: &[(i64, i64, i64, String, i64, String)],
) {
    for (_, _, _, path, catalog_size, catalog_checksum) in rows {
        let bytes = fixture
            .operator
            .read(path)
            .await
            .expect("catalog object remains readable")
            .to_bytes();
        assert_eq!(
            i64::try_from(bytes.len()).expect("test object length fits i64"),
            *catalog_size,
            "catalog length must match fetched object at {path}"
        );
        assert_eq!(
            hex::encode(Sha256::digest(&bytes)),
            *catalog_checksum,
            "catalog checksum must match fetched object at {path}"
        );
    }
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
async fn failed_generation_reuses_deterministic_object_id() {
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
    assert_eq!(object_paths(&fixture).await.len(), 0);

    retry_and_wait(&fixture).await;
    let paths = object_paths(&fixture).await;
    assert_eq!(rows(&fixture).await.len(), 1);
    assert_eq!(paths.len(), 1, "retry reuses the exact generation identity");
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

/// A lost automatic COMMIT response transfers the exact generation into the
/// runtime reconciler and completes without duplicate rows, audit, or objects.
#[tokio::test]
async fn automatic_commit_ambiguity_reconciles_exactly_once() {
    let mut fixture = PersistenceFixture::start().await;
    fixture.faults.fail_next_post_commit_response();
    append_one(&fixture, "automatic_ambiguous_commit_events", 1).await;
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("ambiguous automatic flush");
    wait_for_state(&fixture, 0).await;

    assert_eq!(
        rows_for_table(&fixture, "automatic_ambiguous_commit_events")
            .await
            .len(),
        1,
        "fenced retry validates the already-committed row"
    );
    let artifacts = artifact_rows_for_table(&fixture, "automatic_ambiguous_commit_events").await;
    assert_eq!(artifacts.len(), 1, "retry preserves one artifact set");
    assert_catalog_object_parity(&fixture, &artifacts).await;
    assert_eq!(audit_count(&fixture).await, 1, "retry preserves one audit");
    assert_eq!(object_paths(&fixture).await.len(), 1);
    assert!(hint_outcome(&mut fixture).is_ok());
    assert!(matches!(
        hint_outcome(&mut fixture),
        Err(tokio::sync::mpsc::error::TryRecvError::Empty)
    ));
    fixture.stop().await;
}

/// Proves the automatic persistence producer emits writer-v2 at the 832 MiB floor.
#[tokio::test]
async fn scribe_persistence_writer_v2_is_bounded_at_exact_floor() {
    let fixture =
        PersistenceFixture::start_with_memory(persistence_test_roles(832 * 1024 * 1024)).await;
    append_one(&fixture, "exact_floor_events", 1).await;
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("exact-floor flush");
    wait_for_state(&fixture, 0).await;

    let mut conn = fixture
        .database
        .tenant_conn_for(fixture.tenant)
        .await
        .expect("exact-floor tenant connection");
    let (path, file_size, ordinal, checksum): (String, i64, i16, Option<String>) =
        sqlx::query_as(
            "SELECT file_path,file_size,file_ordinal,file_checksum FROM vala.file_list WHERE data_tenant_id=wyrd.current_tenant() AND table_name='exact_floor_events'",
        )
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("exact-floor file-list row");
    assert_eq!(ordinal, 0);
    let checksum = checksum.expect("writer-v2 checksum");
    assert_eq!(checksum.len(), 64);
    assert!(checksum.bytes().all(|byte| byte.is_ascii_hexdigit()));
    let bytes = fixture
        .operator
        .read(&path)
        .await
        .expect("exact-floor writer-v2 object")
        .to_bytes();
    assert_eq!(
        i64::try_from(bytes.len()).expect("object size fits i64"),
        file_size
    );
    let metadata = parquet::file::metadata::ParquetMetaDataReader::new()
        .parse_and_finish(&bytes)
        .expect("standard Parquet decoder accepts persistence output");
    vala_bifrost_redux::parquet::memory::validate_writer_v2_structure(&metadata)
        .expect("persistence output respects structural caps");
    let writer_fields = metadata
        .file_metadata()
        .key_value_metadata()
        .into_iter()
        .flatten()
        .filter(|entry| entry.key.starts_with("wyrd.bifrost."))
        .count();
    assert_eq!(
        writer_fields, 9,
        "writer-v2 publishes the closed metadata set"
    );
    let scratch = fixture.scratch_root.path().join("scribe-output");
    assert_eq!(
        std::fs::read_dir(scratch)
            .expect("Scribe scratch root")
            .count(),
        0,
        "known committed publication cleans its exact generation scratch"
    );
    assert_eq!(
        fixture.scribe.memory_snapshot().categories[MemoryCategory::Persistence as usize],
        0
    );
    drop(conn);
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

/// Automatic persistence keeps old- and new-epoch objects distinct and exact.
#[tokio::test]
async fn automatic_scribe_artifact_identity_survives_cross_epoch_restart() {
    let fixture = PersistenceFixture::start_after_wal_restart(false).await;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if fixture.scribe.persistence_queue_depth_for_test() == 0
            && artifact_rows_for_table(&fixture, "restart_publish_events")
                .await
                .len()
                == 3
        {
            break;
        }
        assert!(Instant::now() < deadline, "epoch-one replay stalled");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    append_one(&fixture, "restart_publish_events", 99).await;
    fixture
        .scribe
        .flush_writable_for_test()
        .await
        .expect("epoch-two generation flush");
    wait_for_state(&fixture, 0).await;

    let rows = artifact_rows_for_table(&fixture, "restart_publish_events").await;
    assert_eq!(rows.len(), 4);
    assert_eq!(rows.iter().filter(|row| row.0 == 1).count(), 3);
    assert_eq!(rows.iter().filter(|row| row.0 == 2).count(), 1);
    assert_eq!(
        rows.iter()
            .map(|row| row.3.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        rows.len(),
        "cross-epoch durable paths must be distinct: {rows:?}"
    );
    assert_catalog_object_parity(&fixture, &rows).await;
    assert_eq!(audit_count(&fixture).await, 4);
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
        persistence_test_roles(9 * 1024 * 1024 * 1024),
        2,
        25_000,
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

/// Replay applies publication-driven backpressure when aggregate WAL ownership exceeds memory.
#[tokio::test]
async fn replay_total_above_ceiling_is_bounded_by_persistence_retirement() {
    let memory = persistence_test_roles(1024 * 1024 * 1024);
    let baseline = memory.snapshot().expect("baseline root snapshot");
    let tables = (1..=160)
        .map(|index| format!("bounded_replay_{index}"))
        .collect::<Vec<_>>();
    let fixture = PersistenceFixture::start_after_wal_restart_with_keys(
        false,
        &tables,
        1,
        &[Duration::from_millis(1)],
        memory,
        1,
        10_000,
    )
    .await
    .expect("bounded replay completes with one WAL worker");
    assert!(
        fixture.replay_decoded_bytes > baseline.plan.scribe_floor_bytes,
        "aggregate decoded WAL ownership {} must exceed the Scribe ceiling {}",
        fixture.replay_decoded_bytes,
        baseline.plan.scribe_floor_bytes,
    );
    assert!(fixture.scribe.is_ready());
    for table_name in &tables {
        assert_eq!(rows_for_table(&fixture, table_name).await.len(), 1);
    }
    let restored = fixture.memory.snapshot().expect("restored root snapshot");
    assert_eq!(
        restored.scribe_memory_used_bytes,
        baseline.scribe_memory_used_bytes
    );
    assert_eq!(
        restored.oracle_memory_used_bytes,
        baseline.oracle_memory_used_bytes
    );
    assert_eq!(
        restored.forge_memory_used_bytes,
        baseline.forge_memory_used_bytes
    );
    fixture.stop().await;
}

/// One indivisible replay generation fails closed without advancing readiness.
#[tokio::test]
async fn replay_indivisible_generation_over_ceiling_stays_unready() {
    let memory = persistence_test_roles(1024 * 1024 * 1024);
    let baseline = memory.snapshot().expect("baseline root snapshot");
    let error = match PersistenceFixture::start_after_wal_restart_with_keys(
        false,
        &["oversized_replay", "later_replay"],
        1,
        &[Duration::from_millis(1)],
        memory.clone(),
        1,
        100_000,
    )
    .await
    {
        Ok(fixture) => {
            fixture.stop().await;
            panic!("an indivisible generation above the remaining ceiling must fail");
        }
        Err(error) => error,
    };
    match &error {
        ScribeError::IngestBusy { table } => assert_eq!(table, "memory"),
        ScribeError::Internal { detail } => assert_eq!(detail, "ingest busy for table: memory"),
        _ => panic!("replay refusal must retain its structural capacity error: {error:?}"),
    }
    assert_eq!(
        memory
            .snapshot()
            .expect("restored root snapshot")
            .scribe_memory_used_bytes,
        baseline.scribe_memory_used_bytes
    );
    assert_eq!(
        memory
            .snapshot()
            .expect("restored root snapshot")
            .elastic_memory_used_bytes,
        baseline.elastic_memory_used_bytes
    );
}

#[tokio::test]
async fn replayed_distinct_keys_publish_in_replay_order_and_fifo() {
    let fixture = PersistenceFixture::start_after_wal_restart_with_keys(
        false,
        &["restart_key_a", "restart_key_b"],
        2,
        &[Duration::from_millis(100)],
        persistence_test_roles(9 * 1024 * 1024 * 1024),
        2,
        50_000,
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
            "cross-key replay stalled: error={:?} rows_a={} rows_b={} objects={:?}",
            fixture.faults.last_error_for_test(),
            rows_for_table(&fixture, "restart_key_a").await.len(),
            rows_for_table(&fixture, "restart_key_b").await.len(),
            object_paths(&fixture).await,
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(fixture.faults.max_concurrent_object_writes_for_test(), 1);
    // Replay deliberately waits for each exact generation's publication and
    // retirement before advancing. Within each key the generations therefore
    // form an ordered chain of disjoint WAL-LSN ranges.
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
    let memory = persistence_test_roles(1024 * 1024 * 1024);
    let fixture = PersistenceFixture::start_with_memory(memory).await;
    let baseline = fixture.memory.snapshot().expect("baseline root snapshot");
    let oracle = fixture.memory.oracle().expect("composed Oracle capability");
    let range = oracle
        .try_acquire_worker(OracleWorkerClass::Analytical)
        .expect("analytical Oracle worker occupancy");
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
                TimestampMicrosecondArray::from(vec![chrono::Utc::now().timestamp_micros(); 4])
                    .with_timezone("UTC"),
            ),
            Arc::new(arrow::array::StringArray::from(vec![
                "x".repeat(
                    (20 * 1024 * 1024) / 4
                );
                4
            ])),
        ],
    )
    .expect("near-target persistence batch");
    let request_id = RequestId::now_v7();
    let measured_wire_bytes = vala_bifrost_redux::scribe::admission::MAX_REQUEST_BYTES;
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
    let overlapped = fixture.memory.snapshot().expect("overlapped root snapshot");
    assert_eq!(overlapped.oracle_memory_used_bytes, range.memory_bytes());
    assert!(overlapped.oracle_memory_used_bytes <= overlapped.plan.managed_memory_bytes);
    drop(range);
    let restored = fixture.memory.snapshot().expect("restored root snapshot");
    assert_eq!(
        restored.oracle_memory_used_bytes,
        baseline.oracle_memory_used_bytes
    );
    assert_eq!(
        restored.scribe_memory_used_bytes, baseline.scribe_memory_used_bytes,
        "scribe ownership did not restore: {restored:?}"
    );
    assert_eq!(
        restored.elastic_memory_used_bytes, baseline.elastic_memory_used_bytes,
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
    let first_day = chrono::Utc::now().date_naive();
    let second_day = first_day.succ_opt().expect("current date has a successor");
    let first_day_micros = first_day
        .and_hms_opt(12, 0, 0)
        .expect("valid time")
        .and_utc()
        .timestamp_micros();
    let second_day_micros = second_day
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
    assert!(days.iter().any(|day| day.contains(&first_day.to_string())));
    assert!(days.iter().any(|day| day.contains(&second_day.to_string())));
    fixture.stop().await;
}
