//! Shared fixtures for the scribe modules.
//!
//! Every item here is used by more than one sibling module. A helper
//! used by exactly one module lives in that module instead. Contains
//! no tests.

use arrow::array::{
    FixedSizeBinaryBuilder, Int32Array, Int64Array, RecordBatch, StringArray,
    TimestampMicrosecondArray,
};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::ipc::writer::StreamWriter;
use opendal::services::Memory;
use secrecy::ExposeSecret;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use vala_bifrost_redux::catalog::{BifrostCatalog, CreateTableRequest, TableRef};
use vala_bifrost_redux::contracts::{
    FrameAdmission, IngressPayload, Scribe, ScribeError, ScribeIngressFrame,
};
use vala_bifrost_redux::maintenance::staging_file_channel;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::resources::{
    BifrostResourcePolicy, BifrostRole, BifrostRoleResources, BifrostRuntimeResources,
    ResourceSource, SystemResourceSnapshot,
};
use vala_bifrost_redux::schema::fingerprint::SchemaFingerprint;
use vala_bifrost_redux::scribe::audit_envelope::encode_audit_event;
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

/// Composes the shared Scribe/Oracle fixture resources and volume layout.
pub(crate) fn compose_persistence_resources(
    requested: &BifrostRoleResources,
    wal_root: &TempDir,
    scratch_root: &TempDir,
) -> BifrostRoleResources {
    let scribe_output = scratch_root.path().join("scribe-output");
    let forge_scratch = scratch_root.path().join("forge");
    let oracle_scratch = scratch_root.path().join("oracle");
    for volume_root in [&scribe_output, &forge_scratch, &oracle_scratch] {
        std::fs::create_dir(volume_root).expect("test volume root");
    }
    let snapshot = requested.snapshot().expect("root snapshot");
    BifrostRuntimeResources::from_snapshot(
        SystemResourceSnapshot {
            memory_limit_bytes: snapshot.plan.memory_limit_bytes,
            effective_cpu: 4,
            scratch_capacity_bytes: 2 * 1024 * 1024 * 1024,
            scratch_available_bytes: 2 * 1024 * 1024 * 1024,
            memory_source: ResourceSource::Injected,
            cpu_source: ResourceSource::Injected,
        },
        BifrostResourcePolicy {
            roles: std::collections::BTreeSet::from([BifrostRole::Scribe, BifrostRole::Oracle]),
            memory_limit_bytes: None,
            unmanaged_reserve_bytes: None,
            scratch_limit_bytes: None,
            effective_cpu: None,
            oracle_query_slot_limit: None,
            scratch_root: scratch_root.path().to_owned(),
            volume_roots: Some(vala_bifrost_redux::resources::BifrostVolumeRoots {
                wal: wal_root.path().to_owned(),
                scribe_output_scratch: scribe_output,
                forge_scratch,
                oracle_scratch,
            }),
        },
    )
    .expect("test Bifrost resources")
    .compose_roles()
    .expect("test role resources")
}

pub(crate) struct PersistenceFixture {
    pub(crate) database: PgFixture,
    pub(crate) operator: Arc<opendal::Operator>,
    pub(crate) scribe: Arc<ScribeImpl>,
    /// Shared production governor used to overlap Oracle range ownership.
    pub(crate) memory: BifrostRoleResources,
    pub(crate) faults: PersistenceFaults,
    /// Receiver retained so post-commit hints remain observable until assertions finish.
    pub(crate) hint_inbox: vala_bifrost_redux::maintenance::StagingFileInbox,
    pub(crate) wal_root: TempDir,
    /// Physical root retained for generation-owned Scribe output scratch.
    pub(crate) scratch_root: TempDir,
    pub(crate) _warehouse: Option<TempDir>,
    pub(crate) tenant: DataTenantId,
    /// Stable source node reused across replacement epochs.
    pub(crate) node_id: uuid::Uuid,
    /// Aggregate decoded Arrow ownership represented by the seeded replay WAL.
    pub(crate) replay_decoded_bytes: usize,
}

/// Dependency bundle for the restarted replay owner.
pub(crate) struct RestartedScribeConfig {
    /// Object store shared with the first owner.
    pub(crate) operator: Arc<opendal::Operator>,
    /// Reopened epoch-two WAL.
    pub(crate) wal: Arc<WalWriter>,
    /// Stable node identity.
    pub(crate) node_id: uuid::Uuid,
    /// Replay admission policy.
    pub(crate) admission: vala_bifrost_redux::scribe::admission::AdmissionConfig,
    /// Durable persistence owner.
    pub(crate) persistence: ScribePersistenceConfig,
    /// Scribe resource capability.
    pub(crate) resources: vala_bifrost_redux::resources::ScribeResources,
    /// Staging publication channel.
    pub(crate) publisher: vala_bifrost_redux::maintenance::StagingFilePublisher,
    /// Replay WAL lane width.
    pub(crate) wal_io_threads: usize,
}

/// Builds the epoch-two Scribe owner used by replay tests.
pub(crate) fn restarted_scribe(config: RestartedScribeConfig) -> Arc<ScribeImpl> {
    let pools = ScribeExecutionPools::new(
        ScribeIngressCpuPool::new_with_capacity(1, 256),
        ScribePersistenceCpuPool::new_with_capacity(2, 64),
        ScribeWalIoPool::new_with_capacity(config.wal_io_threads, 256),
    );
    Arc::new(ScribeImpl::new_with_execution_pools(ScribeBuildConfig {
        catalog: None,
        operator: config.operator,
        wal: config.wal,
        stream: StreamIdentity::new(NodeId::new(config.node_id), WriterEpoch::new(2)),
        admission: config.admission,
        coordination_runtime: tokio::runtime::Handle::current(),
        execution_pools: pools,
        persistence: Some(config.persistence),
        resources: config.resources,
        ingest_limits: vala_bifrost_redux::gate::limits::IngestLimits::default(),
        wal_rotation_bytes: 512 * 1024 * 1024,
        memtable_rotation_bytes: 512 * 1024 * 1024,
        memtable_max_age: Duration::from_mins(10),
        staging_file_publisher: Some(config.publisher),
    }))
}

/// Composes production-equivalent Scribe and Oracle capabilities for persistence fixtures.
pub(crate) fn persistence_test_roles(memory_limit_bytes: usize) -> BifrostRoleResources {
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
            oracle_query_slot_limit: None,
            scratch_root: std::path::PathBuf::new(),
            volume_roots: None,
        },
    )
    .expect("test Bifrost runtime resources")
    .compose_roles()
    .expect("test Bifrost role resources")
}

/// Opens a local-warehouse catalog and registers every replay table in it.
///
/// The warehouse directory is returned rather than dropped because the catalog's
/// data lives under it for the lifetime of the fixture; dropping it early would
/// delete the tables the restart is supposed to find. The catalog itself is not
/// returned — registration is its only purpose here, and the restarted Scribe
/// re-opens the catalog from the same DSN.
///
/// # Panics
///
/// Panics if the warehouse directory or catalog cannot be created, or if table
/// registration fails.
pub(crate) async fn open_replay_catalog<T: AsRef<str>>(
    database: &PgFixture,
    table_names: &[T],
    tenant: DataTenantId,
) -> tempfile::TempDir {
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
    warehouse
}

/// The admission ceiling every replay-restart fixture shares.
///
/// Replay must be bounded by the role resource graph under test, not by
/// admission, so these limits sit far above any generation the suite seeds.
pub(crate) fn persistence_admission_config()
-> vala_bifrost_redux::scribe::admission::AdmissionConfig {
    vala_bifrost_redux::scribe::admission::AdmissionConfig {
        memory_limit_bytes: 4 * 1024 * 1024 * 1024,
        scribe_memory_limit_bytes: Some(8 * 1024 * 1024 * 1024),
        event_time_window: vala_bifrost_redux::scribe::admission::EventTimeWindow::default(),
    }
}

/// Opens the post-restart WAL writer at fencing epoch 2.
///
/// The epoch must advance past the pre-restart writer so replay observes a
/// genuine restart boundary rather than reopening the same generation.
///
/// # Panics
/// Panics when the WAL directory cannot be opened at epoch 2.
pub(crate) fn restarted_wal_writer(
    wal_root: &std::path::Path,
    node_id: uuid::Uuid,
) -> Arc<WalWriter> {
    Arc::new(
        WalWriter::new(wal_root, *node_id.as_bytes(), 2, WalConfig::default())
            .expect("restarted WAL writer"),
    )
}

/// One WAL-restart replay scenario driven by [`PersistenceFixture`].
///
/// Replay behaviour is a product of eight independent knobs — injected write
/// failure, table set, generation count, per-object delays, the role resource
/// ceiling, WAL worker count, generation size, and segment size. Naming them
/// keeps each call site readable about which axis it is actually varying.
pub(crate) struct ReplayRestartSpec<'a> {
    /// Inject a replay write failure to exercise the refusal path.
    pub(crate) fail_replay_write: bool,
    /// Logical tables registered in the replay catalog.
    pub(crate) table_names: &'a [&'a str],
    /// Number of generations seeded into the WAL before restart.
    pub(crate) generations: i64,
    /// Per-object write delays applied in order during replay.
    pub(crate) object_write_delays: &'a [Duration],
    /// Root-derived role resources bounding replay memory.
    pub(crate) memory: BifrostRoleResources,
    /// WAL worker threads available to replay.
    pub(crate) wal_io_threads: usize,
    /// Rows written per seeded generation.
    pub(crate) rows_per_generation: usize,
    /// Optional WAL segment ceiling; `None` keeps the default.
    pub(crate) wal_segment_bytes: Option<u64>,
}

impl PersistenceFixture {
    /// Registers the exact Scribe stream fence consumed by writer-v2 publication.
    ///
    /// # Panics
    ///
    /// Panics when the repository-managed Postgres fixture cannot expose its
    /// operator connection or persist the requested stream epoch.
    pub(crate) async fn register_scribe_fence(
        database: &PgFixture,
        node_id: uuid::Uuid,
        epoch: i64,
    ) {
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
    pub(crate) async fn start() -> Self {
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
    pub(crate) async fn start_with_memory(requested_memory: BifrostRoleResources) -> Self {
        let database = PgFixture::start().await.expect("Postgres fixture");
        let tenant = database.data_tenant_id();
        let operator = Arc::new(
            opendal::Operator::new(Memory::default())
                .expect("memory object store")
                .finish(),
        );
        let wal_root = tempfile::tempdir().expect("WAL directory");
        let scratch_root = tempfile::tempdir().expect("scratch directory");
        let resources = compose_persistence_resources(&requested_memory, &wal_root, &scratch_root);
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
            catalog: None,
            operator: Arc::clone(&operator),
            wal,
            stream: StreamIdentity::new(NodeId::new(node_id), WriterEpoch::new(1)),
            admission,
            coordination_runtime: tokio::runtime::Handle::current(),
            execution_pools: pools,
            persistence: Some(persistence),
            resources: scribe_resources,
            ingest_limits: vala_bifrost_redux::gate::limits::IngestLimits::default(),
            wal_rotation_bytes: 512 * 1024 * 1024,
            memtable_rotation_bytes: 512 * 1024 * 1024,
            memtable_max_age: Duration::from_mins(10),
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
            node_id,
            replay_decoded_bytes: 0,
        }
    }

    pub(crate) async fn stop(self) {
        self.scribe
            .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(1))
            .await;
        drop(self.wal_root);
    }

    pub(crate) async fn start_after_wal_restart(fail_replay_write: bool) -> Self {
        Self::start_after_wal_restart_with_keys(ReplayRestartSpec {
            fail_replay_write,
            table_names: &["restart_publish_events"],
            generations: 3,
            object_write_delays: &[
                Duration::from_millis(300),
                Duration::from_millis(1),
                Duration::from_millis(1),
            ],
            memory: persistence_test_roles(9 * 1024 * 1024 * 1024),
            wal_io_threads: 2,
            rows_per_generation: 50_000,
            wal_segment_bytes: None,
        })
        .await
        .expect("replay")
    }

    /// Starts a multi-table persistence fixture after seeding restart WAL state.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when replay seeding, Scribe construction,
    /// recovery, table registration, or configured persistence setup fails.
    pub(crate) async fn start_after_wal_restart_with_keys(
        spec: ReplayRestartSpec<'_>,
    ) -> Result<Self, ScribeError> {
        let ReplayRestartSpec {
            fail_replay_write,
            table_names,
            generations,
            object_write_delays,
            memory,
            wal_io_threads,
            rows_per_generation,
            wal_segment_bytes,
        } = spec;
        let database = PgFixture::start().await.expect("Postgres fixture");
        let tenant = database.data_tenant_id();
        let operator = Arc::new(
            opendal::Operator::new(Memory::default())
                .expect("memory operator")
                .finish(),
        );
        let warehouse = open_replay_catalog(&database, table_names, tenant).await;
        let wal_root = tempfile::tempdir().expect("WAL directory");
        let scratch_root = tempfile::tempdir().expect("scratch directory");
        let resources = compose_persistence_resources(&memory, &wal_root, &scratch_root);
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
                wal_segment_bytes
                    .map(WalConfig::new)
                    .transpose()?
                    .unwrap_or_default(),
            )
            .expect("WAL writer"),
        );
        let admission = persistence_admission_config();
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
        let wal = restarted_wal_writer(wal_root.path(), node_id);
        let scribe = restarted_scribe(RestartedScribeConfig {
            operator: Arc::clone(&operator),
            wal,
            node_id,
            admission,
            persistence,
            resources: scribe_resources,
            publisher: staging_file_publisher,
            wal_io_threads,
        });
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
            node_id,
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
    pub(crate) async fn assert_failed_replay(
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
        assert_eq!(
            published, 0,
            "failed replay must not publish later state; replay error: {error}"
        );
        error
    }
}

pub(crate) fn first_replay_scribe(
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
        catalog: None,
        operator,
        wal,
        stream: StreamIdentity::new(NodeId::new(node_id), WriterEpoch::new(1)),
        admission,
        coordination_runtime: tokio::runtime::Handle::current(),
        execution_pools: pools,
        persistence: None,
        resources: memory.scribe().expect("composed Scribe capability"),
        ingest_limits: vala_bifrost_redux::gate::limits::IngestLimits::default(),
        wal_rotation_bytes: 512 * 1024 * 1024,
        memtable_rotation_bytes: 512 * 1024 * 1024,
        memtable_max_age: Duration::from_mins(10),
        staging_file_publisher: None,
    }))
}

pub(crate) async fn register_replay_tables<T: AsRef<str>>(
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
                physical_layout: None,
                audit: None,
            })
            .await
            .expect("catalog table");
    }
}

/// Seeds complete one-slice WAL batches and returns their aggregate decoded Arrow ownership.
///
/// # Panics
///
/// Panics when a managed Arrow batch, audit envelope, seal key, or complete WAL
/// batch cannot be constructed and durably appended.
pub(crate) fn write_replay_records<T: AsRef<str>>(
    wal: &WalWriter,
    table_names: &[T],
    generations: i64,
    tenant: DataTenantId,
    rows_per_generation: usize,
) -> usize {
    let mut decoded_bytes = 0_usize;
    let replay_day = vala_bifrost_redux::catalog::layout::TimePartition::new(
        vala_bifrost_redux::catalog::layout::TimeGranularity::Day,
        chrono::Utc::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .expect("midnight")
            .and_utc(),
    )
    .expect("midnight is a daily partition boundary");
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
                replay_day,
            );
            wal.append_and_commit_for_replay_test(&key, batch_id, &audit, &data)
                .expect("complete replay WAL batch");
        }
    }
    decoded_bytes
}

pub(crate) fn table(name: &str) -> TableRef {
    TableRef::new(BifrostNamespace::Bifrost, name)
}

pub(crate) fn principal(tenant: DataTenantId) -> Principal {
    Principal {
        id: PrincipalId::new(uuid::Uuid::now_v7()),
        kind: PrincipalKind::User,
        tenant_id: tenant,
        roles: Vec::new(),
        effective_permissions: PermissionSet::new(),
    }
}

pub(crate) fn batch(value: i64) -> RecordBatch {
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

pub(crate) fn audit_event(operation: &str, request_id: RequestId) -> AuditEvent {
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

pub(crate) fn managed_batch_bytes(
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

pub(crate) async fn append_one(fixture: &PersistenceFixture, table_name: &str, value: i64) {
    append_with_batch_id(fixture, table_name, value, uuid::Uuid::now_v7()).await;
}

/// Registers the control row the seal path resolves its write recipe from.
///
/// These fixtures drive `ScribeImpl` directly rather than through the catalog,
/// so they own the one registration production requires before a table may be
/// sealed. The user projection is recovered from the batch about to be appended
/// — every managed column the ingest path stamps is filtered out and then
/// re-derived through the production helper — so the registered layout names
/// exactly the columns the seal will see.
///
/// Registration is skipped when a control row for the table already exists, so a
/// fixture that also registers through the catalog keeps that row rather than
/// racing a second one against the unique `(data_tenant_id, fqn)` constraint.
///
/// # Panics
///
/// Panics when the schema does not canonicalize, or when the tenant connection,
/// lookup, upsert, or commit fails.
pub(crate) async fn register_control_row(
    fixture: &PersistenceFixture,
    table_name: &str,
    rows: &RecordBatch,
) {
    let user_fields = rows
        .schema()
        .fields()
        .iter()
        .filter(|field| !crate::support::is_managed_column(field.name()))
        .map(|field| field.as_ref().clone())
        .collect::<Vec<_>>();
    let schema = Schema::new(vala_bifrost_redux::schema::with_managed_columns(
        user_fields,
    ));
    crate::support::register_control_row(
        fixture.database.vala_postgres(),
        fixture.tenant,
        BifrostNamespace::Bifrost.as_str(),
        table_name,
        &schema,
    )
    .await;
}

/// Fingerprints the user-visible half of a projected fixture schema.
///
/// Scribe identifies a table by the schema its client owns: server-managed
/// `wyrd_*` columns and the correlation columns Scribe stamps never take part
/// in schema identity. A fixture handing Scribe already-projected Arrow must
/// therefore declare the same projected identity a real client's IPC stream
/// would carry, which is what `expected_schema_fingerprint` on
/// [`ScribeIngressFrame`] states.
pub(crate) fn source_schema_fingerprint(schema: &Schema) -> SchemaFingerprint {
    let fields = schema
        .fields()
        .iter()
        .filter(|field| {
            !matches!(
                field.name().as_str(),
                "card_ref" | "card_uid" | "principal_id" | "run_id" | "data_tenant_id"
            ) && !field.name().starts_with("wyrd_")
        })
        .map(|field| field.as_ref().clone())
        .collect::<Vec<_>>();
    SchemaFingerprint::from_arrow_schema(&Schema::new(fields))
}

/// Builds the server-created audit event Gate owns for one authenticated frame.
///
/// `Scribe::ingest_frame` never mints an audit record of its own, so a fixture
/// driving that boundary directly supplies the allow/success event a real
/// authenticated write would have carried out of the transport layer.
pub(crate) fn frame_audit_event(
    principal: &Principal,
    table: &TableRef,
    request_id: &RequestId,
    rows: usize,
) -> AuditEvent {
    AuditEvent {
        request_id: request_id.clone(),
        trace_id: None,
        operation: "bifrost.append".to_owned(),
        resource: table.fqn(),
        card_ref: principal.card_ref().cloned(),
        principal_id: principal.id,
        principal_kind: principal.kind.tag(),
        auth_method: AuthMethod::Jwt,
        permission: "bifrost:append".to_owned(),
        decision: AuditDecision::Allow,
        result: AuditResult::Success,
        payload_summary: format!("{rows} rows"),
        detail: None,
    }
}

/// Ingests one already-projected fixture batch through the production boundary.
///
/// This is the canonical shape every scribe fixture uses: an explicit
/// authenticated tenant, an explicit Gate-owned audit event, the projected
/// source fingerprint, and the measured wire size, handed to
/// `Scribe::ingest_frame` so the fixture exercises the same admission path a
/// transport frame does.
///
/// # Errors
///
/// Returns the exact Scribe ingress error from the durable owner.
pub(crate) async fn ingest_projected_rows(
    scribe: &ScribeImpl,
    tenant: DataTenantId,
    table: TableRef,
    rows: RecordBatch,
    batch_id: uuid::Uuid,
    measured_wire_bytes: usize,
) -> Result<FrameAdmission, ScribeError> {
    let principal = principal(tenant);
    let request_id = RequestId::now_v7();
    let row_count = rows.num_rows();
    Scribe::ingest_frame(
        scribe,
        ScribeIngressFrame {
            authenticated_tenant: tenant,
            audit_event: frame_audit_event(&principal, &table, &request_id, row_count),
            principal,
            table,
            expected_schema_fingerprint: Some(source_schema_fingerprint(rows.schema().as_ref())),
            request_id,
            batch_id,
            measured_wire_bytes,
            payload: IngressPayload::ProjectedArrow(vec![rows]),
        },
    )
    .await
}

/// Append one generation carrying a caller-chosen `batch_id`.
///
/// [`append_one`] mints a fresh `now_v7` id per call, which spreads generations
/// across shard lanes via `shard_for`. Tests that must place two generations
/// on ONE shard lane instead choose their `batch_id`s explicitly through this
/// helper so the routing key is deterministic.
pub(crate) async fn append_with_batch_id(
    fixture: &PersistenceFixture,
    table_name: &str,
    value: i64,
    batch_id: uuid::Uuid,
) {
    let rows = batch(value);
    register_control_row(fixture, table_name, &rows).await;
    let admission = ingest_projected_rows(
        &fixture.scribe,
        fixture.tenant,
        table(table_name),
        rows,
        batch_id,
        0,
    )
    .await
    .expect("append to Scribe");
    assert_eq!(admission.batch_id, batch_id);
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
pub(crate) fn colocated_distinct_batch_id(
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

pub(crate) async fn wait_for_state(fixture: &PersistenceFixture, pending: usize) {
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

pub(crate) async fn retry_and_wait(fixture: &PersistenceFixture) {
    fixture.scribe.check_age(Instant::now());
    wait_for_state(fixture, 0).await;
}

pub(crate) async fn rows(fixture: &PersistenceFixture) -> Vec<(i64, i64, String)> {
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

pub(crate) async fn rows_for_table(
    fixture: &PersistenceFixture,
    table_name: &str,
) -> Vec<(i64, i64, String)> {
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
pub(crate) fn assert_wal_lsn_chain(ranges: &[(i64, i64, String)], expected_len: usize) {
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

/// Returns canonical ingest-control and visibility-publication audit cardinality.
///
/// # Panics
///
/// Panics when the repository-managed tenant connection or audit query fails.
pub(crate) async fn audit_counts(fixture: &PersistenceFixture) -> (i64, i64) {
    let mut conn = fixture
        .database
        .tenant_conn_for(fixture.tenant)
        .await
        .expect("tenant connection");
    sqlx::query_as(
        "SELECT count(*) FILTER (WHERE operation = 'bifrost.append')::bigint,
                count(*) FILTER (WHERE operation = 'bifrost.scribe.visibility.publish')::bigint
           FROM vala.audit_outbox",
    )
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("operation-specific audit counts")
}

pub(crate) async fn object_paths(fixture: &PersistenceFixture) -> Vec<String> {
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
pub(crate) async fn artifact_rows_for_table(
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
pub(crate) async fn assert_catalog_object_parity(
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
