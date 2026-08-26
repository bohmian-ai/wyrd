//! Shared fixtures for the oracle modules.
//!
//! Every item here is used by more than one sibling module. A helper
//! used by exactly one module lives in that module instead. Contains
//! no tests.

use arrow::array::{
    Array, ArrayRef, FixedSizeBinaryBuilder, Int32Array, Int64Array, StringArray,
    TimestampMicrosecondArray,
};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use chrono::Utc;
use futures_util::StreamExt;
use iceberg::spec::{DataContentType, DataFileBuilder, DataFileFormat, Literal, Struct};
use iceberg::transaction::{ApplyTransactionAction, Transaction};
use opendal::Buffer;
use parquet::arrow::ArrowWriter;
use secrecy::ExposeSecret;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::catalog::{
    BifrostCatalog, CreateTableRequest, TableRef, TenantTableBinding,
};
use vala_bifrost_redux::cluster::{ClusterRegistry, RegisteredRole};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::oracle::dispatcher::{
    LocalOraclePeerTransport, OraclePeerTransportDirectory, OraclePeerWorker,
    OraclePeerWorkerConfig, ReservationRegistry, TonicOraclePeerTransport,
};
use vala_bifrost_redux::oracle::peer::{
    NoopPeerSecurityAudit, PeerSecurityError, PeerTicketClaims, PeerTicketVerifier,
    VerifiedClaimsBytes,
};
use vala_bifrost_redux::oracle::{
    AuthorizedQueryContext, BifrostQueryReadDecision, BifrostSecurityViolation,
    DelegatedOracleAdmissionConfig, Oracle, OracleAudit, OracleBuildConfig, OracleConfig,
    OracleMemoryResources, OracleSlotManager, QueryIpcDecoder, TailTransportDirectory,
    TestPostgresOracleAudit, VerifiedSecurityContext,
};
use vala_bifrost_redux::schema::with_managed_columns;
use vala_bifrost_redux::scribe::file_list_writer::{FileListInsert, insert_and_audit};
use vala_bifrost_redux::scribe::tail_rpc::{
    FenceRelease, LocalTailPage, TailReadError, TailReadTransport,
};
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_runtime::permission::{Permission, PermissionSet};
use wyrd_runtime::{Principal, PrincipalKind};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::PrincipalId;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::api::{
    AuditDecision, AuditEvent, AuditResult, AuthMethod, BifrostQueryRequest, FreshnessPolicy,
    OracleCapabilitiesV1, QueryClass, QueryStreamFrame, QueryTerminalOutcome, VisibilityMode,
};
use wyrd_spec::vala::api::{NodeId as OracleNodeId, SignedPeerTicket};
use wyrd_storage::{BackendConfig, StorageHandle, StorageSettings};

/// Verifies deterministic fixture tickets while preserving audience and fence checks.
pub(crate) struct DeterministicTestVerifier;

#[async_trait]
impl PeerTicketVerifier for DeterministicTestVerifier {
    /// Authenticates the fixture ticket and returns its opaque verified claims.
    ///
    /// # Errors
    /// Returns a closed peer-security failure for tamper, malformed claims, audience, or fence.
    async fn verify_peer_ticket(
        &self,
        ticket: &SignedPeerTicket,
        expected_worker: OracleNodeId,
        expected_worker_fence: u64,
        _now: chrono::DateTime<Utc>,
    ) -> Result<VerifiedClaimsBytes, PeerSecurityError> {
        use wyrd_tonic::prost::Message as _;

        if ticket.key_id != "test" || ticket.signature != ticket.claims_bytes {
            return Err(PeerSecurityError::InvalidSignature);
        }
        let claims = PeerTicketClaims::decode(ticket.claims_bytes.as_slice())
            .map_err(|_| PeerSecurityError::Claims)?;
        if claims.audience != expected_worker.as_uuid().as_bytes() {
            return Err(PeerSecurityError::Audience);
        }
        if claims.worker_fence != expected_worker_fence {
            return Err(PeerSecurityError::Fence);
        }
        Ok(VerifiedClaimsBytes(ticket.claims_bytes.clone()))
    }
}

/// Composes Oracle capabilities from one injected raw observation.
///
/// The fixture supplies only raw observations and the enabled role; the
/// production policy and role-composition stages produce every grant, so no
/// test path constructs a sibling root or a raw pool.
pub(crate) fn composed_oracle_roles() -> vala_bifrost_redux::resources::BifrostRoleResources {
    vala_bifrost_redux::resources::BifrostRuntimeResources::from_snapshot(
        vala_bifrost_redux::resources::SystemResourceSnapshot {
            memory_limit_bytes: 1024 * 1024 * 1024,
            effective_cpu: 4,
            scratch_capacity_bytes: 2 * 1024 * 1024 * 1024,
            scratch_available_bytes: 2 * 1024 * 1024 * 1024,
            memory_source: vala_bifrost_redux::resources::ResourceSource::Injected,
            cpu_source: vala_bifrost_redux::resources::ResourceSource::Injected,
        },
        vala_bifrost_redux::resources::BifrostResourcePolicy {
            roles: [vala_bifrost_redux::resources::BifrostRole::Oracle]
                .into_iter()
                .collect(),
            memory_limit_bytes: None,
            unmanaged_reserve_bytes: None,
            scratch_limit_bytes: Some(1024 * 1024 * 1024),
            effective_cpu: None,
            oracle_query_slot_limit: None,
            scratch_root: std::path::PathBuf::new(),
            volume_roots: None,
        },
    )
    .expect("injected observation must satisfy the resource policy")
    .compose_roles()
    .expect("composition must be issued from an unpoisoned root")
}

/// Deterministic role-lifecycle clock used without wall-clock sleeps.
pub(crate) struct InjectedRoleClock {
    /// Total logical time advanced by the shutdown proof.
    pub(crate) elapsed: Duration,
}

impl InjectedRoleClock {
    /// Creates a clock at the role shutdown boundary.
    pub(crate) fn new() -> Self {
        Self {
            elapsed: Duration::ZERO,
        }
    }

    /// Advances logical time by one lifecycle interval.
    pub(crate) fn advance(&mut self, duration: Duration) {
        self.elapsed += duration;
    }
}

/// Dependencies retained for one real Postgres/catalog/Oracle integration.
pub(crate) struct OracleFixture {
    /// Managed Postgres fixture.
    pub(crate) pg: PgFixture,
    /// Authenticated tenant.
    pub(crate) tenant: DataTenantId,
    /// Registered logical table.
    pub(crate) table: TableRef,
    /// Physical table binding.
    pub(crate) binding: TenantTableBinding,
    /// Redux catalog.
    pub(crate) catalog: Arc<BifrostCatalog>,
    /// Local object store.
    pub(crate) storage: Arc<StorageHandle>,
    /// Registered Oracle membership owner.
    pub(crate) cluster: Arc<ClusterRegistry>,
    /// Exact local Oracle role fence.
    pub(crate) role: RegisteredRole,
    /// Warehouse lifetime.
    pub(crate) _warehouse: tempfile::TempDir,
    /// Pod-local Oracle spill root lifetime.
    pub(crate) spill_root: tempfile::TempDir,
}

/// One persisted hot fixture and the exact physical batch written to Parquet.
pub(crate) struct SeededHotRows {
    /// Decoded Arrow memory used to configure a forced spill.
    pub(crate) memory_bytes: usize,
    /// Physical batch reused to create real cross-tier overlap.
    pub(crate) batch: RecordBatch,
    /// Tenant-qualified object key recorded in the hot manifest.
    pub(crate) file_path: String,
    /// Exact persisted Parquet size used by the Iceberg data-file identity.
    pub(crate) file_size: u64,
}

impl OracleFixture {
    /// Counts every owned process/query scratch descendant beneath this fixture.
    ///
    /// # Panics
    ///
    /// Panics when a scratch directory cannot be inspected during a lifecycle assertion.
    #[cfg(feature = "test-support")]
    pub(crate) fn spill_descendant_count(&self) -> usize {
        /// Recursively counts descendants without following symlinks.
        ///
        /// # Panics
        ///
        /// Panics when the test-owned directory cannot be read or classified.
        fn count(path: &std::path::Path) -> usize {
            if !path.exists() {
                return 0;
            }
            std::fs::read_dir(path)
                .expect("test spill directory must remain readable")
                .map(|entry| {
                    let entry = entry.expect("test spill entry must be readable");
                    let nested = if entry
                        .file_type()
                        .expect("test spill entry type must be readable")
                        .is_dir()
                    {
                        count(&entry.path())
                    } else {
                        0
                    };
                    1 + nested
                })
                .sum()
        }

        count(&self.spill_root.path().join("oracle-spill"))
    }

    /// Creates a real tenant-qualified empty Iceberg table and Oracle role.
    ///
    /// # Panics
    ///
    /// Panics when managed Postgres, storage, catalog, or membership setup fails.
    pub(crate) async fn new(table_name: &str) -> Self {
        let pg = PgFixture::start().await.expect("managed Postgres fixture");
        let tenant = pg.data_tenant_id();
        vala_sql::queries::oracle_admission::OracleAdmissionBlocks::new(pg.operator_pool())
            .ensure_canonical_policies(8, 4, 8, 4)
            .await
            .expect("canonical Oracle admission policies");
        let warehouse = tempfile::tempdir().expect("warehouse");
        let spill_root = tempfile::tempdir().expect("Oracle spill root");
        let storage = StorageHandle::from_settings(StorageSettings {
            backend: BackendConfig::Local {
                root: warehouse.path().to_path_buf(),
            },
            require_encryption: false,
            presign_ttl: Duration::from_mins(10),
            part_size_bytes: 16 * 1024 * 1024,
            multipart_threshold_bytes: 100 * 1024 * 1024,
            public_base_url: None,
        })
        .await
        .expect("local storage");
        let catalog = Arc::new(
            BifrostCatalog::new(
                pg.catalog_dsn().expose_secret(),
                storage.backend_config(),
                pg.vala_postgres().clone(),
            )
            .await
            .expect("Redux catalog"),
        );
        let table = TableRef::new(BifrostNamespace::Datasets, table_name);
        catalog
            .create_table(CreateTableRequest {
                table: table.clone(),
                user_fields: vec![Field::new("value", DataType::Int64, false)],
                tenant,
                physical_layout: None,
                audit: None,
            })
            .await
            .expect("registered table");
        let binding =
            TenantTableBinding::resolve((tenant, table.clone())).expect("tenant table binding");
        let node_id = wyrd_spec::vala::api::NodeId::new(uuid::Uuid::now_v7());
        let cluster = Arc::new(ClusterRegistry::new(pg.vala_postgres().clone(), node_id));
        let role = cluster
            .register_oracle(
                "127.0.0.1:0",
                OracleCapabilitiesV1 {
                    storage_protocol_version: 1,
                    cpu_cores: 4.0,
                    memory_budget_bytes: 512 * 1024 * 1024,
                    cpu_cores_per_slot: 1.0,
                    memory_bytes_per_slot: 64 * 1024 * 1024,
                    raw_slots: 4,
                    usable_slots: 4,
                    supported_classes: vec![QueryClass::Interactive, QueryClass::Analytical],
                    max_workers_per_query: 0,
                },
            )
            .await
            .expect("Oracle role");
        cluster
            .refresh_snapshot()
            .await
            .expect("membership snapshot");
        Self {
            pg,
            tenant,
            table,
            binding,
            catalog,
            storage,
            cluster,
            role,
            _warehouse: warehouse,
            spill_root,
        }
    }

    /// Builds one retained Oracle and waits for startup reconciliation.
    ///
    /// # Panics
    ///
    /// Panics when construction or readiness does not complete.
    pub(crate) async fn oracle(
        &self,
        audit: Arc<dyn OracleAudit>,
        tails: Arc<TailTransportDirectory>,
        config: OracleConfig,
    ) -> Oracle {
        self.oracle_with_memory(audit, tails, config, 64 * 1024 * 1024)
            .await
    }

    /// Builds one retained Oracle with an explicit reconciliation ceiling.
    ///
    /// # Panics
    ///
    /// Panics when construction or startup reconciliation does not complete.
    pub(crate) async fn oracle_with_memory(
        &self,
        audit: Arc<dyn OracleAudit>,
        tails: Arc<TailTransportDirectory>,
        config: OracleConfig,
        reconciliation_limit_bytes: usize,
    ) -> Oracle {
        let oracle = self.build_oracle(audit, tails, config, reconciliation_limit_bytes);
        tokio::time::timeout(Duration::from_secs(2), async {
            while !oracle.is_ready() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("Oracle readiness");
        oracle
    }

    /// Builds an Oracle whose leader-local peer path reads the pinned table through `FileIO`.
    ///
    /// # Panics
    ///
    /// Panics when the table, worker, Oracle, or startup readiness cannot be constructed.
    pub(crate) async fn oracle_with_local_fragments(
        &self,
        audit: Arc<dyn OracleAudit>,
        config: OracleConfig,
    ) -> Oracle {
        let reservations = Arc::new(ReservationRegistry::new(
            Arc::new(OracleSlotManager::new(16, 16)),
            16,
        ));
        let worker_resources = composed_oracle_roles();
        let worker = Arc::new(OraclePeerWorker::new_physical_with_resources(
            OraclePeerWorkerConfig {
                worker_node_id: self.role.key.node_id,
                oracle_fence: self.role.fencing_token,
                verifier: Arc::new(DeterministicTestVerifier),
                security_audit: Arc::new(NoopPeerSecurityAudit),
                reservations,
                oracle_resources: worker_resources
                    .oracle()
                    .expect("production-shaped worker Oracle capability"),
                resolver: Arc::new(
                    vala_bifrost_redux::oracle::follower::OracleCatalogResolver::new(Arc::clone(
                        &self.catalog,
                    )),
                ),
                audit: Arc::clone(&audit),
            },
        ));
        let transports = OraclePeerTransportDirectory::new(
            self.role.key.node_id,
            Arc::new(LocalOraclePeerTransport::new(worker)),
            Arc::new(TonicOraclePeerTransport::with_credentials(
                Arc::clone(&self.cluster),
                Arc::new(
                    vala_bifrost_redux::oracle::dispatcher::StaticOraclePeerCredentials::new(
                        secrecy::SecretString::from(String::new()),
                    ),
                ),
            )),
        );
        let oracle = self.build_oracle_with_transport(
            audit,
            Arc::new(TailTransportDirectory::default()),
            config,
            64 * 1024 * 1024,
            Some(transports),
        );
        tokio::time::timeout(Duration::from_secs(2), async {
            while !oracle.is_ready() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("Oracle readiness");
        oracle
    }

    /// Constructs Oracle without waiting for startup reconciliation.
    ///
    /// # Panics
    ///
    /// Panics when the supplied configuration violates Oracle invariants.
    pub(crate) fn build_oracle(
        &self,
        audit: Arc<dyn OracleAudit>,
        tails: Arc<TailTransportDirectory>,
        config: OracleConfig,
        reconciliation_limit_bytes: usize,
    ) -> Oracle {
        self.build_oracle_with_transport(audit, tails, config, reconciliation_limit_bytes, None)
    }

    /// Constructs Oracle with an optional immutable-fragment transport.
    ///
    /// # Panics
    ///
    /// Panics when the supplied configuration violates Oracle invariants.
    pub(crate) fn build_oracle_with_transport(
        &self,
        audit: Arc<dyn OracleAudit>,
        tails: Arc<TailTransportDirectory>,
        config: OracleConfig,
        reconciliation_limit_bytes: usize,
        peer_transports: Option<OraclePeerTransportDirectory>,
    ) -> Oracle {
        Oracle::new(OracleBuildConfig {
            shutdown: CancellationToken::new(),
            catalog: Arc::clone(&self.catalog),
            vala: self.pg.vala_postgres().clone(),
            operator_pool: self.pg.operator_pool().clone(),
            cluster: Arc::clone(&self.cluster),
            local_role: self.role.clone(),
            local_slots: Arc::new(OracleSlotManager::new(16, 16)),
            memory: {
                let roles = composed_oracle_roles();
                OracleMemoryResources {
                    resources: roles
                        .oracle()
                        .expect("composition must issue the Oracle capability"),
                    reconciliation_limit_bytes,
                }
            },
            spill_runtime: vala_bifrost_redux::oracle::OracleSpillRuntime::new(
                &self.spill_root.path().join("oracle-spill"),
                1024 * 1024 * 1024,
            )
            .expect("Oracle spill runtime"),
            tails,
            audit,
            peer_ticket_minter: Arc::new(
                vala_bifrost_redux::oracle::peer::DeterministicTestSigner {
                    key_id: "test".to_owned(),
                },
            ),
            tail_ticket_minter: None,
            tail_discovery: None,
            peer_transports,
            delegated_admission_config: DelegatedOracleAdmissionConfig::default(),
            config,
        })
        .expect("Oracle")
    }

    /// Returns a fresh authenticated request context.
    ///
    /// # Panics
    ///
    /// Panics if the fixture's principal and tenant unexpectedly diverge.
    pub(crate) fn context(&self) -> AuthorizedQueryContext {
        let principal = Principal::new(
            PrincipalId::new(uuid::Uuid::now_v7()),
            PrincipalKind::User,
            self.tenant,
            Vec::new(),
            PermissionSet::from_iter([Permission::bifrost_query_read()]),
        );
        AuthorizedQueryContext::try_new(
            principal,
            self.tenant,
            RequestId::now_v7(),
            None,
            AuthMethod::Internal,
            "bifrost_query:read",
        )
        .expect("authorized context")
    }

    /// Writes one hot Parquet object with explicit row tenants.
    ///
    /// This helper intentionally permits a foreign tenant value so the real
    /// physical tripwire and durable security-audit failure paths can be
    /// exercised without bypassing the Oracle table provider.
    ///
    /// # Panics
    ///
    /// Panics when no rows are supplied or Arrow, storage, or SQL persistence
    /// fails.
    #[must_use]
    pub(crate) async fn seed_hot_rows(&self, rows: &[(i64, DataTenantId)]) -> SeededHotRows {
        self.seed_hot_rows_at("hot.parquet", rows).await
    }

    /// Writes one named hot Parquet object with explicit row tenants.
    ///
    /// The caller-provided basename allows one fixture to retain an
    /// Iceberg-overlapping object and a distinct hot-only object.
    ///
    /// # Panics
    ///
    /// Panics when the basename is not a single safe path segment, no rows are
    /// supplied, or Arrow, storage, or SQL persistence fails.
    #[must_use]
    pub(crate) async fn seed_hot_rows_at(
        &self,
        basename: &str,
        rows: &[(i64, DataTenantId)],
    ) -> SeededHotRows {
        self.seed_hot_rows_at_epoch(basename, rows, 1).await
    }

    /// Writes one named hot Parquet object for a selected Scribe writer epoch.
    #[must_use]
    pub(crate) async fn seed_hot_rows_at_epoch(
        &self,
        basename: &str,
        rows: &[(i64, DataTenantId)],
        writer_epoch: i64,
    ) -> SeededHotRows {
        validate_hot_basename(basename);
        assert!(!rows.is_empty(), "hot fixture requires rows");
        let schema = Arc::new(Schema::new(with_managed_columns(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )])));
        let mut batch_ids = FixedSizeBinaryBuilder::with_capacity(rows.len(), 16);
        for row in 0..rows.len() {
            let mut batch_id = *uuid::Uuid::now_v7().as_bytes();
            batch_id[0] = u8::try_from(row % 32).expect("spill partition identity");
            batch_ids.append_value(batch_id).expect("batch id");
        }
        let row_count = rows.len();
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![
                Arc::new(Int64Array::from(
                    rows.iter().map(|(value, _)| *value).collect::<Vec<_>>(),
                )) as ArrayRef,
                Arc::new(StringArray::from(vec![None::<String>; row_count])),
                Arc::new(StringArray::from(vec![None::<String>; row_count])),
                Arc::new(StringArray::from(
                    (0..row_count)
                        .map(|_| uuid::Uuid::now_v7().to_string())
                        .collect::<Vec<_>>(),
                )),
                Arc::new(StringArray::from(
                    (0..row_count)
                        .map(|_| RequestId::now_v7().to_string())
                        .collect::<Vec<_>>(),
                )),
                Arc::new(
                    TimestampMicrosecondArray::from(vec![1_000_000_i64; row_count])
                        .with_timezone("UTC"),
                ),
                Arc::new(
                    TimestampMicrosecondArray::from(vec![1_000_001_i64; row_count])
                        .with_timezone("UTC"),
                ),
                Arc::new(batch_ids.finish()),
                Arc::new(Int32Array::from_value(0, row_count)),
                Arc::new(StringArray::from(
                    rows.iter()
                        .map(|(_, tenant)| tenant.to_string())
                        .collect::<Vec<_>>(),
                )),
            ],
        )
        .expect("physical batch");
        let memory_bytes = batch.get_array_memory_size();
        let mut bytes = Vec::new();
        let mut writer =
            ArrowWriter::try_new(&mut bytes, Arc::clone(&schema), None).expect("Parquet writer");
        writer.write(&batch).expect("Parquet batch");
        writer.close().expect("Parquet close");
        let node_id = uuid::Uuid::now_v7();
        let path = format!("{}/{basename}", self.binding.object_prefix);
        self.storage
            .operator()
            .write(&path, Buffer::from(bytes.clone()))
            .await
            .expect("hot object");
        let audit = audit_event("oracle.fixture.hot");
        let mut conn = self
            .pg
            .tenant_conn_for(self.tenant)
            .await
            .expect("tenant conn");
        insert_and_audit(
            &mut conn,
            &FileListInsert {
                id: uuid::Uuid::now_v7(),
                data_tenant_id: self.tenant,
                namespace: &self.binding.logical_namespace,
                table_name: &self.binding.table_name,
                file_path: &path,
                file_size: i64::try_from(bytes.len()).expect("file size"),
                row_count: i64::try_from(row_count).expect("row count"),
                min_event_time: Utc::now(),
                max_event_time: Utc::now(),
                partition: vala_bifrost_redux::partition_fixtures::day_partition(1970, 1, 1),
                node_id,
                writer_epoch,
                wal_lsn_min: 1,
                wal_lsn_max: 1,
            },
            std::slice::from_ref(&audit),
        )
        .await
        .expect("file list");
        conn.commit().await.expect("manifest commit");
        SeededHotRows {
            memory_bytes,
            batch,
            file_path: path,
            file_size: u64::try_from(bytes.len()).expect("file size"),
        }
    }

    /// Fast-appends one existing hot object to the table's current Iceberg snapshot.
    ///
    /// The exact object remains in the hot manifest, creating the transition
    /// overlap that Oracle must subtract before building its fused scan.
    ///
    /// # Panics
    ///
    /// Panics when the hot key escapes this table, Iceberg rejects the
    /// data-file identity, or the catalog commit fails.
    pub(crate) async fn append_hot_to_iceberg_snapshot(&self, seeded: &SeededHotRows) {
        let catalog = self.catalog.iceberg_catalog();
        let table = catalog
            .load_table(&self.binding.table_ident())
            .await
            .expect("Iceberg table");
        let relative = seeded
            .file_path
            .strip_prefix(&format!("{}/", self.binding.object_prefix))
            .expect("hot object belongs to the table binding");
        let catalog_path = format!(
            "{}/{}",
            table.metadata().location().trim_end_matches('/'),
            relative
        );
        let partition = Struct::from_iter([Some(Literal::date(0))]);
        let data_file = DataFileBuilder::default()
            .content(DataContentType::Data)
            .file_path(catalog_path)
            .file_format(DataFileFormat::Parquet)
            .partition(partition)
            .record_count(u64::try_from(seeded.batch.num_rows()).expect("row count"))
            .file_size_in_bytes(seeded.file_size)
            .sort_order_id(
                i32::try_from(table.metadata().default_sort_order_id())
                    .expect("sort order ID fits i32"),
            )
            .partition_spec_id(table.metadata().default_partition_spec_id())
            .build()
            .expect("overlapping data file");
        let append = Transaction::new(&table)
            .fast_append()
            .add_data_files([data_file]);
        ApplyTransactionAction::apply(append, Transaction::new(&table))
            .expect("overlap append action")
            .commit(catalog.as_ref())
            .await
            .expect("overlap snapshot commit");
    }

    /// Inserts a valid manifest identity whose object intentionally does not exist.
    ///
    /// # Panics
    ///
    /// Panics when tenant-scoped SQL persistence fails.
    pub(crate) async fn seed_missing_hot_row(&self) {
        let path = format!("{}/missing.parquet", self.binding.object_prefix);
        let mut conn = self
            .pg
            .tenant_conn_for(self.tenant)
            .await
            .expect("tenant conn");
        insert_and_audit(
            &mut conn,
            &FileListInsert {
                id: uuid::Uuid::now_v7(),
                data_tenant_id: self.tenant,
                namespace: &self.binding.logical_namespace,
                table_name: &self.binding.table_name,
                file_path: &path,
                file_size: 128,
                row_count: 1,
                min_event_time: Utc::now(),
                max_event_time: Utc::now(),
                partition: vala_bifrost_redux::partition_fixtures::day_partition(1970, 1, 1),
                node_id: uuid::Uuid::now_v7(),
                writer_epoch: 1,
                wal_lsn_min: 1,
                wal_lsn_max: 1,
            },
            &[audit_event("oracle.fixture.missing")],
        )
        .await
        .expect("file list");
        conn.commit().await.expect("manifest commit");
    }
}

/// Rejects unsafe fixture basenames before constructing a tenant object path.
///
/// # Panics
///
/// Panics when the basename is empty or contains a path separator.
pub(crate) fn validate_hot_basename(basename: &str) {
    assert!(
        !basename.is_empty() && !basename.contains(['/', '\\']),
        "hot fixture basename must be one safe segment"
    );
}

/// Audit owner that deterministically refuses the mandatory read decision.
pub(crate) struct FailingAudit;

#[async_trait]
impl OracleAudit for FailingAudit {
    /// Refuses the read decision before any source provider may execute.
    async fn append_read_decision(
        &self,
        _context: &AuthorizedQueryContext,
        _decision: BifrostQueryReadDecision,
    ) -> Result<(), BifrostError> {
        Err(BifrostError::QueryAuditUnavailable)
    }

    /// Refuses any security event in the same fail-closed test sink.
    async fn append_security_violation(
        &self,
        _context: VerifiedSecurityContext,
        _violation: BifrostSecurityViolation,
    ) -> Result<(), BifrostError> {
        Err(BifrostError::QueryAuditUnavailable)
    }
}

/// Audit owner that commits reads but refuses the mandatory security event.
pub(crate) struct SecurityFailingAudit {
    /// Real SQL audit delegate for the initial immutable read decision.
    pub(crate) reads: TestPostgresOracleAudit,
}

/// Audit probe that signals post-acquisition entry and remains pending until timeout.
pub(crate) struct BlockingAudit {
    /// Notification proving Oracle reached audit only after acquiring the full cut.
    pub(crate) entered: Arc<tokio::sync::Notify>,
}

/// Audit probe that counts read decisions and optionally refuses them.
pub(crate) struct CountingAudit {
    /// Number of read decisions presented after a complete visibility cut.
    pub(crate) decisions: Arc<AtomicUsize>,
    /// Whether the read decision append fails closed.
    pub(crate) fail: bool,
}

#[async_trait]
impl OracleAudit for CountingAudit {
    /// Counts the decision and returns the configured durable outcome.
    async fn append_read_decision(
        &self,
        _context: &AuthorizedQueryContext,
        _decision: BifrostQueryReadDecision,
    ) -> Result<(), BifrostError> {
        self.decisions.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            Err(BifrostError::QueryAuditUnavailable)
        } else {
            Ok(())
        }
    }

    /// Accepts unrelated security events; focused typed plans remain tenant-correct.
    async fn append_security_violation(
        &self,
        _context: VerifiedSecurityContext,
        _violation: BifrostSecurityViolation,
    ) -> Result<(), BifrostError> {
        Ok(())
    }
}

#[async_trait]
impl OracleAudit for BlockingAudit {
    /// Signals audit entry and waits forever so the query's absolute deadline wins.
    async fn append_read_decision(
        &self,
        _context: &AuthorizedQueryContext,
        _decision: BifrostQueryReadDecision,
    ) -> Result<(), BifrostError> {
        self.entered.notify_one();
        std::future::pending().await
    }

    /// Accepts unrelated security events; the timeout proof never emits one.
    async fn append_security_violation(
        &self,
        _context: VerifiedSecurityContext,
        _violation: BifrostSecurityViolation,
    ) -> Result<(), BifrostError> {
        Ok(())
    }
}

#[async_trait]
impl OracleAudit for SecurityFailingAudit {
    /// Commits the read decision through the standard SQL-backed owner.
    async fn append_read_decision(
        &self,
        context: &AuthorizedQueryContext,
        decision: BifrostQueryReadDecision,
    ) -> Result<(), BifrostError> {
        self.reads.append_read_decision(context, decision).await
    }

    /// Deterministically refuses the security append.
    async fn append_security_violation(
        &self,
        _context: VerifiedSecurityContext,
        _violation: BifrostSecurityViolation,
    ) -> Result<(), BifrostError> {
        Err(BifrostError::QueryAuditUnavailable)
    }
}

/// Tail transport probe that either acquires or fails while counting releases.
pub(crate) struct FenceProbeTransport {
    /// Stream identity returned by successful acquisition.
    pub(crate) node_id: wyrd_spec::vala::api::NodeId,
    /// Whether acquisition fails with capacity exhaustion.
    pub(crate) fail_acquire: bool,
    /// Whether the awaited release reports a transport failure.
    pub(crate) fail_release: bool,
    /// Shared successful-release count.
    pub(crate) releases: Arc<AtomicUsize>,
    /// Physical batches returned by every complete page.
    pub(crate) batches: Vec<RecordBatch>,
    /// Shared page call count used to return configured overlap exactly once.
    pub(crate) page_reads: Arc<AtomicUsize>,
}

#[async_trait]
impl TailReadTransport for FenceProbeTransport {
    /// Returns request-consistent metadata or the configured acquisition failure.
    async fn acquire_fence(
        &self,
        request: wyrd_spec::vala::api::AcquireTailFenceRequest,
    ) -> Result<wyrd_spec::vala::api::TailReadFence, TailReadError> {
        if self.fail_acquire {
            return Err(TailReadError::Capacity);
        }
        Ok(wyrd_spec::vala::api::TailReadFence {
            fence_id: wyrd_spec::vala::api::TailFenceId::new(uuid::Uuid::now_v7()),
            binding: request.binding,
            time_partition: request.time_partition,
            stream: wyrd_spec::vala::api::TailStreamIdentity {
                node_id: self.node_id,
                writer_epoch: request.exclusive_sealed.writer_epoch,
            },
            inclusive_live: request.exclusive_sealed.clone(),
            exclusive_sealed: request.exclusive_sealed,
            schema_fingerprint: request.schema_fingerprint,
            tail_protocol_version: request.tail_protocol_version,
            expires_at: request.deadline,
        })
    }

    /// Returns one configured complete page; partial-acquire tests never call it.
    async fn read_page(
        &self,
        _request: wyrd_spec::vala::api::TailPageRequest,
    ) -> Result<LocalTailPage, TailReadError> {
        let batches = if self.page_reads.fetch_add(1, Ordering::SeqCst) == 0 {
            self.batches.iter().cloned().map(Arc::new).collect()
        } else {
            Vec::new()
        };
        Ok(LocalTailPage {
            batches,
            next: None,
            complete: true,
        })
    }

    /// Counts the required release of each successfully acquired fence.
    fn release_fence(
        &self,
        _fence_id: wyrd_spec::vala::api::TailFenceId,
    ) -> Result<FenceRelease, TailReadError> {
        self.releases.fetch_add(1, Ordering::SeqCst);
        if self.fail_release {
            return Err(TailReadError::State {
                detail: "injected release failure".to_owned(),
            });
        }
        Ok(FenceRelease { released: true })
    }
}

/// Async release probe that records first polls and can remain pending forever.
pub(crate) struct CleanupReleaseProbeTransport {
    /// Stream identity returned by successful acquisition.
    pub(crate) node_id: wyrd_spec::vala::api::NodeId,
    /// Whether the async release remains pending until its cleanup timeout.
    pub(crate) block_release: bool,
    /// Shared count incremented on the first poll of each async release body.
    pub(crate) release_polls: Arc<AtomicUsize>,
    /// Shared count incremented only when a release future finishes normally.
    pub(crate) release_completions: Arc<AtomicUsize>,
}

#[async_trait]
impl TailReadTransport for CleanupReleaseProbeTransport {
    /// Returns one valid metadata-only fence for the requested stream.
    async fn acquire_fence(
        &self,
        request: wyrd_spec::vala::api::AcquireTailFenceRequest,
    ) -> Result<wyrd_spec::vala::api::TailReadFence, TailReadError> {
        Ok(wyrd_spec::vala::api::TailReadFence {
            fence_id: wyrd_spec::vala::api::TailFenceId::new(uuid::Uuid::now_v7()),
            binding: request.binding,
            time_partition: request.time_partition,
            stream: wyrd_spec::vala::api::TailStreamIdentity {
                node_id: self.node_id,
                writer_epoch: request.exclusive_sealed.writer_epoch,
            },
            inclusive_live: request.exclusive_sealed.clone(),
            exclusive_sealed: request.exclusive_sealed,
            schema_fingerprint: request.schema_fingerprint,
            tail_protocol_version: request.tail_protocol_version,
            expires_at: request.deadline,
        })
    }

    /// Returns an empty complete page; this probe is used only before draining.
    async fn read_page(
        &self,
        _request: wyrd_spec::vala::api::TailPageRequest,
    ) -> Result<LocalTailPage, TailReadError> {
        Ok(LocalTailPage {
            batches: Vec::new(),
            next: None,
            complete: true,
        })
    }

    /// Rejects the synchronous path because cleanup must exercise the async future.
    fn release_fence(
        &self,
        _fence_id: wyrd_spec::vala::api::TailFenceId,
    ) -> Result<FenceRelease, TailReadError> {
        Err(TailReadError::State {
            detail: "cleanup probe requires async release".to_owned(),
        })
    }

    /// Records first poll, then either blocks or completes with an injected failure.
    async fn release_fence_async(
        &self,
        _fence_id: wyrd_spec::vala::api::TailFenceId,
    ) -> Result<FenceRelease, TailReadError> {
        self.release_polls.fetch_add(1, Ordering::SeqCst);
        if self.block_release {
            std::future::pending().await
        } else {
            self.release_completions.fetch_add(1, Ordering::SeqCst);
            Err(TailReadError::State {
                detail: "injected async release failure".to_owned(),
            })
        }
    }
}

/// One production span and its recorded closed fields.
#[derive(Clone, Debug)]
pub(crate) struct CapturedSpan {
    /// Stable instrumentation span name.
    pub(crate) name: String,
    /// Field values recorded at creation or later updates.
    pub(crate) fields: HashMap<String, String>,
}

/// Minimal in-process subscriber retaining Oracle span names and fields.
#[derive(Clone, Default)]
pub(crate) struct SpanProbe {
    /// Monotonic tracing span identity source.
    pub(crate) next_id: Arc<AtomicU64>,
    /// Captured spans keyed by tracing identity.
    pub(crate) spans: Arc<Mutex<HashMap<u64, CapturedSpan>>>,
}

impl SpanProbe {
    /// Returns a stable snapshot of all captured spans.
    ///
    /// # Panics
    ///
    /// Panics when a prior test poisoned the probe lock.
    pub(crate) fn snapshot(&self) -> Vec<CapturedSpan> {
        self.spans
            .lock()
            .expect("span probe")
            .values()
            .cloned()
            .collect()
    }
}

/// Field visitor retaining closed scalar values without payload data.
pub(crate) struct SpanFieldVisitor<'a> {
    /// Mutable captured field map.
    pub(crate) fields: &'a mut HashMap<String, String>,
}

impl tracing::field::Visit for SpanFieldVisitor<'_> {
    /// Records debug-only fields such as closed enums.
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.fields
            .insert(field.name().to_owned(), format!("{value:?}"));
    }

    /// Records string fields without debug quoting.
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.fields
            .insert(field.name().to_owned(), value.to_owned());
    }

    /// Records unsigned count fields.
    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.fields
            .insert(field.name().to_owned(), value.to_string());
    }
}

impl tracing::Subscriber for SpanProbe {
    /// Enables every callsite used by the focused Oracle proof.
    fn enabled(&self, _metadata: &tracing::Metadata<'_>) -> bool {
        true
    }

    /// Captures one span and its initial fields.
    fn new_span(&self, attributes: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let mut fields = HashMap::new();
        attributes.record(&mut SpanFieldVisitor {
            fields: &mut fields,
        });
        self.spans.lock().expect("span probe").insert(
            id,
            CapturedSpan {
                name: attributes.metadata().name().to_owned(),
                fields,
            },
        );
        tracing::span::Id::from_u64(id)
    }

    /// Applies fields recorded after span construction.
    fn record(&self, span: &tracing::span::Id, values: &tracing::span::Record<'_>) {
        if let Some(captured) = self
            .spans
            .lock()
            .expect("span probe")
            .get_mut(&span.into_u64())
        {
            values.record(&mut SpanFieldVisitor {
                fields: &mut captured.fields,
            });
        }
    }

    /// Ignores follows-from edges because this proof asserts field contracts.
    fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}

    /// Captures scrubbed diagnostic events to aid failed proof diagnosis.
    fn event(&self, event: &tracing::Event<'_>) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let mut fields = HashMap::new();
        event.record(&mut SpanFieldVisitor {
            fields: &mut fields,
        });
        self.spans.lock().expect("span probe").insert(
            id,
            CapturedSpan {
                name: format!("event:{}", event.metadata().name()),
                fields,
            },
        );
    }

    /// Accepts span entry without retaining execution-stack state.
    fn enter(&self, _span: &tracing::span::Id) {}

    /// Accepts span exit without retaining execution-stack state.
    fn exit(&self, _span: &tracing::span::Id) {}
}

/// Asserts that the standard recorder observed a positive matching counter.
///
/// # Panics
///
/// Panics when no counter series contains the required closed fragment.
pub(crate) fn assert_counter(snapshot: &wyrd_bench::BenchmarkMetricSnapshot, fragment: &str) {
    assert!(
        snapshot
            .counters
            .iter()
            .any(|(series, value)| series.contains(fragment) && *value > 0),
        "missing counter {fragment}: {snapshot:?}"
    );
}

/// Asserts one exact canonical counter value, including zero-valued series.
///
/// # Panics
///
/// Panics when the exact series is absent or differs from `expected`.
pub(crate) fn assert_counter_value(
    snapshot: &wyrd_bench::BenchmarkMetricSnapshot,
    series: &str,
    expected: u64,
) {
    assert_eq!(
        snapshot.counters.get(series).copied(),
        Some(expected),
        "unexpected counter {series}: {snapshot:?}"
    );
}

/// Asserts that one canonical counter is absent or has the exact zero value.
///
/// Metrics may omit a series when no recording site was reached. This helper
/// preserves the stronger pre-byte invariant by rejecting every positive
/// value without requiring a synthetic zero observation.
///
/// # Panics
///
/// Panics when the named series has a nonzero value.
pub(crate) fn assert_counter_absent_or_zero(
    snapshot: &wyrd_bench::BenchmarkMetricSnapshot,
    series: &str,
) {
    assert_eq!(
        snapshot.counters.get(series).copied().unwrap_or(0),
        0,
        "unexpected counter {series}: {snapshot:?}"
    );
}

/// Asserts one exact canonical histogram has at least one observation.
///
/// # Panics
///
/// Panics when the exact series is absent or empty.
pub(crate) fn assert_histogram(snapshot: &wyrd_bench::BenchmarkMetricSnapshot, series: &str) {
    assert!(
        snapshot
            .histograms
            .get(series)
            .is_some_and(|value| value.count > 0),
        "missing histogram {series}: {snapshot:?}"
    );
}

/// Asserts one exact canonical gauge reached a positive production value.
///
/// # Panics
///
/// Panics when the exact series is absent or never became positive.
pub(crate) fn assert_gauge_peak(snapshot: &wyrd_bench::BenchmarkMetricSnapshot, series: &str) {
    assert!(
        snapshot
            .gauge_peaks
            .get(series)
            .is_some_and(|value| *value > 0.0),
        "missing positive gauge peak {series}: {snapshot:?}"
    );
}

/// Asserts one exact canonical gauge's final value.
///
/// # Panics
///
/// Panics when the exact series is absent or differs from `expected`.
pub(crate) fn assert_gauge_value(
    snapshot: &wyrd_bench::BenchmarkMetricSnapshot,
    series: &str,
    expected: f64,
) {
    assert_eq!(
        snapshot.gauges.get(series).copied(),
        Some(expected),
        "unexpected final gauge {series}: {snapshot:?}"
    );
}

/// Creates one scrubbed fixture audit event.
pub(crate) fn audit_event(operation: &str) -> AuditEvent {
    AuditEvent::new(
        RequestId::now_v7(),
        None,
        operation.to_owned(),
        "bifrost.oracle.integration".to_owned(),
        None,
        PrincipalId::new(uuid::Uuid::now_v7()),
        wyrd_spec::auth::PrincipalKindTag::User,
        AuthMethod::Internal,
        "bifrost_query:read".to_owned(),
        AuditDecision::Allow,
        AuditResult::Success,
        "oracle integration fixture".to_owned(),
    )
}

/// Drains a query and returns its sole terminal frame.
///
/// # Panics
///
/// Panics for stream errors or a missing terminal.
pub(crate) async fn terminal(
    mut query: vala_bifrost_redux::oracle::OracleQueryStream,
) -> wyrd_spec::vala::api::QueryTerminalFrame {
    while let Some(frame) = query.frames.next().await {
        if let QueryStreamFrame::Terminal(terminal) = frame.expect("query frame") {
            return terminal;
        }
    }
    panic!("query stream ended without terminal")
}

/// Fully decoded Arrow result from one Oracle query stream.
pub(crate) struct DecodedQuery {
    /// Schema carried by the exactly-once schema frame.
    pub(crate) schema: Arc<Schema>,
    /// Record batches decoded from every bounded batch frame.
    pub(crate) batches: Vec<RecordBatch>,
    /// Sole closed terminal frame.
    pub(crate) terminal: wyrd_spec::vala::api::QueryTerminalFrame,
}

/// Decodes and validates every schema, batch, and terminal frame.
///
/// The stream is one Arrow IPC stream split across Wyrd frames, so a single
/// decoder consumes the schema prefix, each batch delta, and the terminal
/// end-of-stream delta. A non-failed terminal must close the stream.
///
/// # Panics
///
/// Panics when IPC is malformed, schema frames are missing or duplicated,
/// batch schemas diverge, frame ordering is invalid, the stream is not closed
/// by a successful terminal, or no terminal arrives.
pub(crate) async fn decoded_query(
    mut query: vala_bifrost_redux::oracle::OracleQueryStream,
) -> DecodedQuery {
    let mut ipc = QueryIpcDecoder::new();
    let mut schema = None;
    let mut batches = Vec::new();
    let mut terminal = None;
    let mut largest_fragment = 0_usize;
    while let Some(frame) = query.frames.next().await {
        match frame.expect("query frame") {
            QueryStreamFrame::Schema(frame) => {
                assert!(schema.is_none(), "query emitted duplicate schema");
                largest_fragment = largest_fragment.max(frame.arrow_ipc_schema.len());
                schema = Some(
                    ipc.accept_schema(&frame.arrow_ipc_schema)
                        .expect("schema IPC"),
                );
            }
            QueryStreamFrame::Batch(frame) => {
                assert!(schema.is_some(), "batch preceded schema");
                largest_fragment = largest_fragment.max(frame.arrow_ipc_batch.len());
                let batch = ipc.accept_batch(&frame.arrow_ipc_batch).expect("batch IPC");
                assert_eq!(
                    batch.schema().as_ref(),
                    schema.as_ref().expect("schema").as_ref()
                );
                batches.push(batch);
            }
            QueryStreamFrame::Terminal(frame) => {
                assert!(terminal.is_none(), "query emitted duplicate terminal");
                if frame.outcome != QueryTerminalOutcome::Failed {
                    largest_fragment = largest_fragment.max(frame.arrow_ipc_eos.len());
                    ipc.accept_eos(&frame.arrow_ipc_eos)
                        .expect("terminal closes the query IPC stream");
                }
                terminal = Some(frame);
            }
        }
    }
    assert!(
        terminal
            .as_ref()
            .is_some_and(|frame| frame.outcome == QueryTerminalOutcome::Failed)
            || ipc.eos_accepted(),
        "a non-failed query stream must be explicitly closed"
    );
    assert!(
        ipc.peak_pending_frame_bytes() <= largest_fragment,
        "the decoder retains at most one fragment at a time"
    );

    DecodedQuery {
        schema: schema.expect("query schema"),
        batches,
        terminal: terminal.expect("query terminal"),
    }
}

/// Returns all non-null `Int64` values for one named result column.
///
/// # Panics
///
/// Panics when the column is absent, has a different Arrow type, or contains
/// nulls.
pub(crate) fn int64_values(result: &DecodedQuery, column: &str) -> Vec<i64> {
    let index = result.schema.index_of(column).expect("result column");
    result
        .batches
        .iter()
        .flat_map(|batch| {
            let values = batch
                .column(index)
                .as_any()
                .downcast_ref::<Int64Array>()
                .expect("Int64 result");
            assert_eq!(values.null_count(), 0);
            values.values().iter().copied().collect::<Vec<_>>()
        })
        .collect()
}

/// Executes one strict `PublishedOnly` SQL statement and decodes all frames.
///
/// # Panics
///
/// Panics when Oracle rejects the query or any returned Arrow frame is invalid.
pub(crate) async fn published_query(
    oracle: &Oracle,
    fixture: &OracleFixture,
    sql: String,
) -> DecodedQuery {
    let stream = oracle
        .query_sql(
            fixture.context(),
            BifrostQueryRequest {
                sql,
                visibility: VisibilityMode::PublishedOnly,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(5_000),
            },
        )
        .await
        .expect("Oracle SQL");
    decoded_query(stream).await
}

/// COUNT and JOIN cannot observe a foreign row before the physical tripwire.
#[tokio::test(flavor = "current_thread")]
pub(crate) async fn pg_bifrost_oracle_multitenant_isolation_and_fairness_journey_tripwire_count_join()
 {
    let fixture = OracleFixture::new("oracle_tripwire").await;
    let foreign = DataTenantId::new_v7();
    let _ = fixture.seed_hot_rows(&[(5, foreign)]).await;
    let oracle = fixture
        .oracle(
            Arc::new(TestPostgresOracleAudit::new(
                fixture.pg.vala_postgres().clone(),
            )),
            Arc::new(TailTransportDirectory::default()),
            OracleConfig::default(),
        )
        .await;
    let recorder = wyrd_bench::BenchmarkRecorder::new();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    let table = fixture.table.fqn();
    for sql in [
        format!("SELECT count(*) AS total FROM {table}"),
        format!("SELECT a.value FROM {table} a JOIN {table} b ON a.value = b.value"),
    ] {
        let error = oracle
            .query_sql(
                fixture.context(),
                BifrostQueryRequest {
                    sql,
                    visibility: VisibilityMode::PublishedOnly,
                    freshness: FreshnessPolicy::Strict,
                    deadline_ms: Some(5_000),
                },
            )
            .await
            .expect_err("tenant tripwire must reject before stream framing");
        assert_eq!(error, BifrostError::QueryTenantInvariant);
    }
    let mut conn = fixture
        .pg
        .tenant_conn_for(fixture.tenant)
        .await
        .expect("tenant conn");
    let envelopes: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT decision,result,detail FROM vala.audit_outbox \
         WHERE data_tenant_id = wyrd.current_tenant() \
           AND operation = 'bifrost.query.security_violation' \
         ORDER BY seq",
    )
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("security envelopes");
    conn.commit().await.expect("security audit read commit");
    assert!(
        envelopes.len() >= 2,
        "COUNT and JOIN each require a durable security event: {envelopes:?}"
    );
    for (decision, result, detail) in envelopes {
        assert_eq!(decision, "allow");
        assert_eq!(result, "failure");
        assert!(detail.contains("\"violation\":\"tenant_row\""));
        assert!(detail.contains("\"phase\":\"source\""));
    }
    let snapshot = recorder.snapshot();
    assert_counter(
        &snapshot,
        "bifrost_oracle_security_events_total{event_class=\"tenant_row\"}",
    );
    oracle
        .shutdown(Instant::now() + Duration::from_secs(1))
        .await;
}

/// Shuts down a test Oracle under the standard bounded drain deadline.
pub(crate) async fn shutdown_oracle(oracle: &Oracle) {
    oracle
        .shutdown(Instant::now() + Duration::from_secs(1))
        .await;
}

/// Real provider scans preserve projection, predicate, limit, and Iceberg
/// reader accounting through the Oracle wrapper used by `TableProvider::scan`.
#[tokio::test(flavor = "current_thread")]
pub(crate) async fn oracle_provider_scan_preserves_projection_predicate_limit_and_metrics() {
    let fixture = OracleFixture::new("oracle_provider_scan").await;
    let seeded = fixture
        .seed_hot_rows(&[
            (3, fixture.tenant),
            (7, fixture.tenant),
            (11, fixture.tenant),
        ])
        .await;
    fixture.append_hot_to_iceberg_snapshot(&seeded).await;
    let oracle = fixture
        .oracle_with_local_fragments(
            Arc::new(TestPostgresOracleAudit::new(
                fixture.pg.vala_postgres().clone(),
            )),
            OracleConfig::default(),
        )
        .await;
    let recorder = wyrd_bench::BenchmarkRecorder::new();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    let result = published_query(
        &oracle,
        &fixture,
        format!(
            "SELECT value FROM {} WHERE value >= 7 ORDER BY value LIMIT 1",
            fixture.table.fqn()
        ),
    )
    .await;
    assert_eq!(result.terminal.outcome, QueryTerminalOutcome::Success);
    assert_eq!(int64_values(&result, "value"), [7]);
    let snapshot = recorder.snapshot();
    assert_counter_value(
        &snapshot,
        "oracle_query_files_scanned_total{class=\"interactive\"}",
        1,
    );
    assert_counter_value(
        &snapshot,
        "oracle_query_partitions_scanned_total{class=\"interactive\"}",
        1,
    );
    oracle
        .shutdown(Instant::now() + Duration::from_secs(1))
        .await;
}

/// Real execution emits required closed metrics and scrubbed span fields.
#[tokio::test(flavor = "current_thread")]
pub(crate) async fn pg_bifrost_oracle_multitenant_isolation_and_fairness_journey_telemetry() {
    let fixture = OracleFixture::new("oracle_telemetry").await;
    let rows = (0_i64..1_024)
        .map(|value| (value, fixture.tenant))
        .collect::<Vec<_>>();
    let seeded = fixture.seed_hot_rows(&rows).await;
    let decoded_batch_bytes = seeded.memory_bytes;
    fixture.append_hot_to_iceberg_snapshot(&seeded).await;
    let _hot_only = fixture
        .seed_hot_rows_at("hot-only.parquet", &[(1_024, fixture.tenant)])
        .await;
    let tails = Arc::new(TailTransportDirectory::default());
    let releases = Arc::new(AtomicUsize::new(0));
    let node_id = wyrd_spec::vala::api::NodeId::new(uuid::Uuid::now_v7());
    let transport: Arc<dyn TailReadTransport> = Arc::new(FenceProbeTransport {
        node_id,
        fail_acquire: false,
        fail_release: false,
        releases: Arc::clone(&releases),
        batches: Vec::new(),
        page_reads: Arc::new(AtomicUsize::new(0)),
    });
    tails.insert(fixture.table.fqn(), Arc::clone(&transport));
    tails.insert_live_stream(
        fixture.table.fqn(),
        node_id,
        1,
        vala_bifrost_redux::partition_fixtures::day_partition(1970, 1, 1).to_wire(),
        transport,
    );
    let oracle = fixture
        .oracle_with_memory(
            Arc::new(TestPostgresOracleAudit::new(
                fixture.pg.vala_postgres().clone(),
            )),
            tails,
            OracleConfig::default(),
            decoded_batch_bytes.saturating_mul(3) / 4,
        )
        .await;
    let recorder = wyrd_bench::BenchmarkRecorder::new();
    let spans = SpanProbe::default();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    let _span_guard = tracing::subscriber::set_default(spans.clone());
    let query = oracle
        .query_sql(
            fixture.context(),
            BifrostQueryRequest {
                sql: format!("SELECT count(*) AS total FROM {}", fixture.table.fqn()),
                visibility: VisibilityMode::Fused,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(5_000),
            },
        )
        .await
        .expect("telemetry query");
    let result = decoded_query(query).await;
    let captured = spans.snapshot();
    assert_eq!(
        result.terminal.outcome,
        QueryTerminalOutcome::Success,
        "{:?}; {captured:?}",
        result.terminal,
    );
    assert_eq!(
        int64_values(&result, "total"),
        [1_025],
        "the Iceberg-overlapping hot object must not be scanned twice"
    );
    assert_eq!(
        releases.load(Ordering::SeqCst),
        3,
        "complete sealed lineage and the live-only stream release every fence"
    );

    let snapshot = recorder.snapshot();
    assert_telemetry_counters(&snapshot);
    assert_telemetry_histograms_and_gauges(&snapshot);
    assert_telemetry_spans(&captured);
    oracle
        .shutdown(Instant::now() + Duration::from_secs(1))
        .await;
}

/// Verifies canonical counter families and exact values for the telemetry journey.
pub(crate) fn assert_telemetry_counters(snapshot: &wyrd_bench::BenchmarkMetricSnapshot) {
    // Physical demand is captured independently from logical selection and
    // returned Arrow bytes; all three dimensions must remain positive.
    for fragment in [
        "oracle_query_logical_bytes_selected_total{class=\"analytical\"}",
        "oracle_query_bytes_scanned_total{class=\"analytical\"}",
        "oracle_query_bytes_returned_total{class=\"analytical\"}",
        "oracle_query_files_scanned_total{class=\"analytical\"}",
        "oracle_query_partitions_scanned_total{class=\"analytical\"}",
    ] {
        assert_counter(snapshot, fragment);
    }
    assert_counter_value(
        snapshot,
        "bifrost_oracle_files_pruned_total{reason=\"snapshot_overlap\",source=\"hot_sealed\"}",
        1,
    );
    assert_counter_value(
        snapshot,
        "bifrost_oracle_tail_pages_total{locality=\"local\",outcome=\"success\"}",
        3,
    );
    assert_counter_value(
        snapshot,
        "bifrost_oracle_tail_fences_total{locality=\"local\",outcome=\"success\"}",
        3,
    );
    assert_counter_value(snapshot, "oracle_query_rows_total{class=\"analytical\"}", 1);
    assert_counter_absent_or_zero(
        snapshot,
        "oracle_query_spill_bytes_total{class=\"analytical\"}",
    );
}

/// Verifies histogram presence, gauge lifetimes, and low-cardinality labels.
pub(crate) fn assert_telemetry_histograms_and_gauges(
    snapshot: &wyrd_bench::BenchmarkMetricSnapshot,
) {
    for series in [
        "oracle_query_duration_seconds{class=\"analytical\",outcome=\"success\"}",
        "oracle_query_time_to_first_batch_seconds{class=\"analytical\"}",
        "oracle_admission_queue_duration_seconds{class=\"analytical\"}",
        "bifrost_oracle_tail_page_seconds{locality=\"local\",outcome=\"success\"}",
        "bifrost_oracle_tail_fence_hold_seconds{locality=\"local\",outcome=\"success\"}",
    ] {
        assert_histogram(snapshot, series);
    }
    for series in [
        "oracle_queries_active{class=\"analytical\"}",
        "oracle_queries_queued{class=\"analytical\"}",
    ] {
        assert_gauge_peak(snapshot, series);
    }
    for series in [
        "oracle_queries_active{class=\"analytical\"}",
        "oracle_queries_queued{class=\"analytical\"}",
    ] {
        assert_gauge_value(snapshot, series, 0.0);
    }
    assert!(
        snapshot
            .counters
            .keys()
            .chain(snapshot.gauges.keys())
            .chain(snapshot.histograms.keys())
            .all(|series| ![
                "tenant",
                "principal",
                "request_id",
                "query_digest",
                "file_path",
                "sql"
            ]
            .iter()
            .any(|forbidden| series.contains(&format!("{forbidden}=")))),
        "high-cardinality label leaked: {snapshot:?}"
    );
}

/// Verifies scrubbed production span names and closed fields.
pub(crate) fn assert_telemetry_spans(captured: &[CapturedSpan]) {
    let query_span = captured
        .iter()
        .find(|span| span.name == "bifrost.oracle.query")
        .expect("query span");
    assert_eq!(
        query_span.fields.get("visibility").map(String::as_str),
        Some("fused")
    );
    assert_eq!(
        query_span.fields.get("search_role").map(String::as_str),
        Some("oracle")
    );
    assert!(query_span.fields.contains_key("request_node_id"));
    assert!(query_span.fields.contains_key("leader_node_id"));
    let admit_span = captured
        .iter()
        .find(|span| span.name == "bifrost.oracle.admission")
        .expect("admit span");
    assert!(
        admit_span
            .fields
            .keys()
            .all(|field| !["tenant", "principal", "request_id", "sql"].contains(&field.as_str()))
    );
    let acquire_span = captured
        .iter()
        .find(|span| span.name == "bifrost.oracle.tail_fence")
        .expect("tail acquire span");
    assert_eq!(
        acquire_span.fields.get("table_count").map(String::as_str),
        Some("1")
    );
    let drain_span = captured
        .iter()
        .find(|span| span.name == "bifrost.oracle.tail")
        .expect("tail drain span");
    assert_eq!(
        drain_span.fields.get("fence_count").map(String::as_str),
        Some("3")
    );
    assert_eq!(
        drain_span.fields.get("freshness").map(String::as_str),
        Some("Strict")
    );
    for name in ["bifrost.oracle.plan", "bifrost.oracle.audit"] {
        assert!(
            captured.iter().any(|span| span.name == name),
            "missing production span {name}: {captured:?}"
        );
    }
    let audit_span = captured
        .iter()
        .find(|span| {
            span.name == "bifrost.oracle.audit"
                && span.fields.get("audit_kind").map(String::as_str) == Some("read_decision")
        })
        .expect("read-decision audit span");
    assert_eq!(
        audit_span.fields.get("audit_kind").map(String::as_str),
        Some("read_decision")
    );
    assert_eq!(
        audit_span.fields.get("query_class").map(String::as_str),
        Some("analytical")
    );
}

/// A missing pinned file triggers exactly one whole-cut pre-byte retry.
#[tokio::test(flavor = "current_thread")]
pub(crate) async fn pg_bifrost_oracle_multitenant_isolation_and_fairness_journey_stale_file_replan()
{
    let fixture = OracleFixture::new("oracle_stale").await;
    fixture.seed_missing_hot_row().await;
    let oracle = fixture
        .oracle(
            Arc::new(TestPostgresOracleAudit::new(
                fixture.pg.vala_postgres().clone(),
            )),
            Arc::new(TailTransportDirectory::default()),
            OracleConfig::default(),
        )
        .await;
    let recorder = wyrd_bench::BenchmarkRecorder::new();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    let error = oracle
        .query_sql(
            fixture.context(),
            BifrostQueryRequest {
                sql: format!("SELECT * FROM {}", fixture.table.fqn()),
                visibility: VisibilityMode::PublishedOnly,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(5_000),
            },
        )
        .await
        .expect_err("second stale attempt fails before stream framing");
    assert_eq!(error, BifrostError::QueryExecutionFailed);
    let mut conn = fixture
        .pg
        .tenant_conn_for(fixture.tenant)
        .await
        .expect("tenant conn");
    let details: Vec<String> = sqlx::query_scalar(
        "SELECT detail::text FROM vala.audit_outbox \
         WHERE data_tenant_id = wyrd.current_tenant() \
           AND operation = 'bifrost.query.read_decision' \
         ORDER BY seq",
    )
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("retry audit details");
    conn.commit().await.expect("audit read commit");
    assert_eq!(details.len(), 2);
    assert!(details[0].contains("\"retry_ordinal\":0"));
    assert!(details[1].contains("\"retry_ordinal\":1"));
    let snapshot = recorder.snapshot();
    assert_counter_absent_or_zero(
        &snapshot,
        "oracle_query_bytes_scanned_total{class=\"interactive\"}",
    );
    assert_counter_value(
        &snapshot,
        "oracle_query_files_scanned_total{class=\"interactive\"}",
        0,
    );
    assert_counter_value(
        &snapshot,
        "oracle_query_partitions_scanned_total{class=\"interactive\"}",
        0,
    );
    assert!(
        snapshot
            .histograms
            .keys()
            .any(|series| series.contains("oracle_query_duration_seconds")),
        "stale retry must record terminal query duration"
    );
    oracle
        .shutdown(Instant::now() + Duration::from_secs(1))
        .await;
}
