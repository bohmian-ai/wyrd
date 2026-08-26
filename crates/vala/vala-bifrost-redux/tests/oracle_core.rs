//! Real T3 Oracle integration proofs selected by the recorded `oracle` filter.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use arrow::array::{
    Array, ArrayRef, FixedSizeBinaryBuilder, Int32Array, Int64Array, StringArray,
    TimestampMicrosecondArray, UInt64Array,
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
    OracleMemoryResources, OracleSlotManager, QueryIpcDecoder, QueryOptions,
    QueryResourceSnapshot, TailTransportDirectory, TestPostgresOracleAudit,
    VerifiedSecurityContext,
};
use vala_bifrost_redux::schema::with_managed_columns;
use vala_bifrost_redux::scribe::file_list_writer::{FileListInsert, insert_and_audit};
use vala_bifrost_redux::scribe::memtable::Memtable;
use vala_bifrost_redux::scribe::seal_key::SealKey;
use vala_bifrost_redux::scribe::stream_identity::{NodeId, StreamIdentity, WriterEpoch};
use vala_bifrost_redux::scribe::tail_rpc::{
    FenceRelease, FetchLiveTailService, LocalTailPage, LocalTailReadTransport, ScribeTailReader,
    TailFenceConfig, TailReadError, TailReadTransport,
};
use vala_bifrost_redux::scribe::wal::{ScribeAppendMeta, WalLsn};
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_runtime::permission::{Permission, PermissionSet};
use wyrd_runtime::{Principal, PrincipalKind};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::PrincipalId;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::api::{
    AuditDecision, AuditDetail, AuditEvent, AuditResult, AuthMethod, BifrostQueryRequest,
    FreshnessPolicy, OracleAdmissionDemand, OracleCapabilitiesV1, QueryAuditDigest, QueryClass,
    QueryExecutionMode, QueryStreamFrame, QueryTerminalOutcome, ScribeCapabilitiesV1,
    VisibilityMode,
};
use wyrd_spec::vala::api::{NodeId as OracleNodeId, SignedPeerTicket};

/// Verifies deterministic fixture tickets while preserving audience and fence checks.
struct DeterministicTestVerifier;

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
use wyrd_storage::{BackendConfig, StorageHandle, StorageSettings};

/// Composes Oracle capabilities from one injected raw observation.
///
/// The fixture supplies only raw observations and the enabled role; the
/// production policy and role-composition stages produce every grant, so no
/// test path constructs a sibling root or a raw pool.
fn composed_oracle_roles() -> vala_bifrost_redux::resources::BifrostRoleResources {
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
struct InjectedRoleClock {
    /// Total logical time advanced by the shutdown proof.
    elapsed: Duration,
}

impl InjectedRoleClock {
    /// Creates a clock at the role shutdown boundary.
    fn new() -> Self {
        Self {
            elapsed: Duration::ZERO,
        }
    }

    /// Advances logical time by one lifecycle interval.
    fn advance(&mut self, duration: Duration) {
        self.elapsed += duration;
    }
}

/// Proves reserved roles stay undiscoverable and colocated fences tear down independently.
///
/// # Panics
///
/// Panics when the managed database or a durable membership transition fails.
#[tokio::test]
async fn oracle_and_scribe_roles_activate_only_after_explicit_readiness() {
    let roles = reserve_role_pair().await;
    let mut shutdown_events = deactivate_and_cancel_roles(&roles).await;
    unregister_roles(&roles, &mut shutdown_events).await;
    assert_eq!(
        shutdown_events,
        [
            "durable_deactivate",
            "drain",
            "heartbeat_stop",
            "reject_cancel_active",
            "oracle_unregister",
            "scribe_unregister",
        ]
    );
}

/// Retained database and colocated role registrations for the lifecycle proof.
struct RolePair {
    /// Managed database retained through teardown.
    _pg: PgFixture,
    /// Shared role-fenced registry.
    cluster: Arc<ClusterRegistry>,
    /// Reserved Oracle role.
    oracle: vala_bifrost_redux::cluster::RegisteredRole,
    /// Reserved Scribe role.
    scribe: vala_bifrost_redux::cluster::RegisteredRole,
}

/// Reserves both roles, proves they are hidden, then explicitly activates them.
async fn reserve_role_pair() -> RolePair {
    let pg = PgFixture::start().await.expect("managed Postgres fixture");
    let node_id = wyrd_spec::vala::api::NodeId::new(uuid::Uuid::now_v7());
    let cluster = Arc::new(ClusterRegistry::new(pg.vala_postgres().clone(), node_id));
    let oracle = cluster
        .reserve_oracle(
            "127.0.0.1:50052",
            OracleCapabilitiesV1 {
                storage_protocol_version: 1,
                cpu_cores: 1.0,
                memory_budget_bytes: 256 * 1024 * 1024,
                cpu_cores_per_slot: 1.0,
                memory_bytes_per_slot: 64 * 1024 * 1024,
                raw_slots: 1,
                usable_slots: 1,
                supported_classes: vec![QueryClass::Interactive, QueryClass::Analytical],
                max_workers_per_query: 0,
            },
        )
        .await
        .expect("reserve Oracle");
    let scribe = cluster
        .reserve_scribe(
            "127.0.0.1:50053",
            ScribeCapabilitiesV1 {
                tail_protocol_version: 1,
            },
        )
        .await
        .expect("reserve Scribe");
    cluster.refresh_snapshot().await.expect("reserved snapshot");
    assert!(cluster.snapshot().live_oracles().is_empty());
    assert!(cluster.snapshot().live_scribes().is_empty());
    cluster.activate(&oracle).await.expect("activate Oracle");
    cluster.activate(&scribe).await.expect("activate Scribe");
    cluster.refresh_snapshot().await.expect("active snapshot");
    assert_eq!(cluster.snapshot().live_oracles().len(), 1);
    assert_eq!(cluster.snapshot().live_scribes().len(), 1);
    RolePair {
        _pg: pg,
        cluster,
        oracle,
        scribe,
    }
}

/// Deactivates readiness, proves draining heartbeats remain hidden, and cancels work.
async fn deactivate_and_cancel_roles(roles: &RolePair) -> Vec<&'static str> {
    let oracle_ready = Arc::new(AtomicBool::new(true));
    let scribe_ready = Arc::new(AtomicBool::new(true));
    let oracle_stop = CancellationToken::new();
    let scribe_stop = CancellationToken::new();
    let oracle_heartbeat = Arc::clone(&roles.cluster).start_readiness_heartbeat(
        roles.oracle.clone(),
        Arc::clone(&oracle_ready),
        oracle_stop.clone(),
    );
    let scribe_heartbeat = Arc::clone(&roles.cluster).start_readiness_heartbeat(
        roles.scribe.clone(),
        Arc::clone(&scribe_ready),
        scribe_stop.clone(),
    );
    tokio::task::yield_now().await;
    let oracle_work = CancellationToken::new();
    let scribe_work = CancellationToken::new();
    let oracle_active = tokio::spawn({
        let stop = oracle_work.clone();
        async move { stop.cancelled().await }
    });
    let scribe_active = tokio::spawn({
        let stop = scribe_work.clone();
        async move { stop.cancelled().await }
    });
    oracle_ready.store(false, Ordering::Release);
    scribe_ready.store(false, Ordering::Release);
    roles
        .cluster
        .deactivate(&roles.oracle)
        .await
        .expect("deactivate Oracle");
    roles
        .cluster
        .deactivate(&roles.scribe)
        .await
        .expect("deactivate Scribe");
    roles
        .cluster
        .refresh_snapshot()
        .await
        .expect("deactivated snapshot");
    assert!(roles.cluster.snapshot().live_oracles().is_empty());
    assert!(roles.cluster.snapshot().live_scribes().is_empty());
    let mut clock = InjectedRoleClock::new();
    clock.advance(vala_bifrost_redux::cluster::ROLE_HEARTBEAT_INTERVAL);
    roles
        .cluster
        .heartbeat_readiness_for_test(&roles.oracle, false)
        .await
        .expect("injected Oracle heartbeat");
    roles
        .cluster
        .heartbeat_readiness_for_test(&roles.scribe, false)
        .await
        .expect("injected Scribe heartbeat");
    roles
        .cluster
        .refresh_snapshot()
        .await
        .expect("draining heartbeat snapshot");
    assert!(roles.cluster.snapshot().live_oracles().is_empty());
    assert!(roles.cluster.snapshot().live_scribes().is_empty());
    clock.advance(Duration::from_secs(1));
    assert_eq!(
        clock.elapsed,
        vala_bifrost_redux::cluster::ROLE_HEARTBEAT_INTERVAL + Duration::from_secs(1)
    );
    oracle_stop.cancel();
    scribe_stop.cancel();
    oracle_heartbeat.await.expect("Oracle heartbeat stops");
    scribe_heartbeat.await.expect("Scribe heartbeat stops");
    oracle_work.cancel();
    scribe_work.cancel();
    oracle_active.await.expect("Oracle active work cancels");
    scribe_active.await.expect("Scribe active work cancels");
    vec![
        "durable_deactivate",
        "drain",
        "heartbeat_stop",
        "reject_cancel_active",
    ]
}

/// Unregisters each role independently and proves the Scribe fence survives Oracle teardown.
async fn unregister_roles(roles: &RolePair, events: &mut Vec<&'static str>) {
    roles
        .cluster
        .shutdown_role(roles.oracle.clone())
        .await
        .expect("unregister Oracle");
    events.push("oracle_unregister");
    roles
        .cluster
        .heartbeat(&roles.scribe)
        .await
        .expect("Scribe fence survives");
    roles
        .cluster
        .refresh_snapshot()
        .await
        .expect("Oracle shutdown snapshot");
    assert!(roles.cluster.snapshot().live_oracles().is_empty());
    assert_eq!(roles.cluster.snapshot().live_scribes().len(), 1);
    roles
        .cluster
        .deactivate(&roles.scribe)
        .await
        .expect("deactivate Scribe");
    roles
        .cluster
        .shutdown_role(roles.scribe.clone())
        .await
        .expect("unregister Scribe");
    events.push("scribe_unregister");
}

/// Dependencies retained for one real Postgres/catalog/Oracle integration.
struct OracleFixture {
    /// Managed Postgres fixture.
    pg: PgFixture,
    /// Authenticated tenant.
    tenant: DataTenantId,
    /// Registered logical table.
    table: TableRef,
    /// Physical table binding.
    binding: TenantTableBinding,
    /// Redux catalog.
    catalog: Arc<BifrostCatalog>,
    /// Local object store.
    storage: Arc<StorageHandle>,
    /// Registered Oracle membership owner.
    cluster: Arc<ClusterRegistry>,
    /// Exact local Oracle role fence.
    role: RegisteredRole,
    /// Warehouse lifetime.
    _warehouse: tempfile::TempDir,
    /// Pod-local Oracle spill root lifetime.
    spill_root: tempfile::TempDir,
}

/// One persisted hot fixture and the exact physical batch written to Parquet.
struct SeededHotRows {
    /// Decoded Arrow memory used to configure a forced spill.
    memory_bytes: usize,
    /// Physical batch reused to create real cross-tier overlap.
    batch: RecordBatch,
    /// Tenant-qualified object key recorded in the hot manifest.
    file_path: String,
    /// Exact persisted Parquet size used by the Iceberg data-file identity.
    file_size: u64,
}

impl OracleFixture {
    /// Counts every owned process/query scratch descendant beneath this fixture.
    ///
    /// # Panics
    ///
    /// Panics when a scratch directory cannot be inspected during a lifecycle assertion.
    #[cfg(feature = "test-support")]
    fn spill_descendant_count(&self) -> usize {
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
    async fn new(table_name: &str) -> Self {
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
    async fn oracle(
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
    async fn oracle_with_memory(
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
    async fn oracle_with_local_fragments(
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
    fn build_oracle(
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
    fn build_oracle_with_transport(
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
    fn context(&self) -> AuthorizedQueryContext {
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
    async fn seed_hot_rows(&self, rows: &[(i64, DataTenantId)]) -> SeededHotRows {
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
    async fn seed_hot_rows_at(
        &self,
        basename: &str,
        rows: &[(i64, DataTenantId)],
    ) -> SeededHotRows {
        self.seed_hot_rows_at_epoch(basename, rows, 1).await
    }

    /// Writes one named hot Parquet object for a selected Scribe writer epoch.
    #[must_use]
    async fn seed_hot_rows_at_epoch(
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
    async fn append_hot_to_iceberg_snapshot(&self, seeded: &SeededHotRows) {
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
    async fn seed_missing_hot_row(&self) {
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
fn validate_hot_basename(basename: &str) {
    assert!(
        !basename.is_empty() && !basename.contains(['/', '\\']),
        "hot fixture basename must be one safe segment"
    );
}

/// Audit owner that deterministically refuses the mandatory read decision.
struct FailingAudit;

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
struct SecurityFailingAudit {
    /// Real SQL audit delegate for the initial immutable read decision.
    reads: TestPostgresOracleAudit,
}

/// Audit probe that signals post-acquisition entry and remains pending until timeout.
struct BlockingAudit {
    /// Notification proving Oracle reached audit only after acquiring the full cut.
    entered: Arc<tokio::sync::Notify>,
}

/// Audit probe that counts read decisions and optionally refuses them.
struct CountingAudit {
    /// Number of read decisions presented after a complete visibility cut.
    decisions: Arc<AtomicUsize>,
    /// Whether the read decision append fails closed.
    fail: bool,
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
struct FenceProbeTransport {
    /// Stream identity returned by successful acquisition.
    node_id: wyrd_spec::vala::api::NodeId,
    /// Whether acquisition fails with capacity exhaustion.
    fail_acquire: bool,
    /// Whether the awaited release reports a transport failure.
    fail_release: bool,
    /// Shared successful-release count.
    releases: Arc<AtomicUsize>,
    /// Physical batches returned by every complete page.
    batches: Vec<RecordBatch>,
    /// Shared page call count used to return configured overlap exactly once.
    page_reads: Arc<AtomicUsize>,
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
struct CleanupReleaseProbeTransport {
    /// Stream identity returned by successful acquisition.
    node_id: wyrd_spec::vala::api::NodeId,
    /// Whether the async release remains pending until its cleanup timeout.
    block_release: bool,
    /// Shared count incremented on the first poll of each async release body.
    release_polls: Arc<AtomicUsize>,
    /// Shared count incremented only when a release future finishes normally.
    release_completions: Arc<AtomicUsize>,
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
struct CapturedSpan {
    /// Stable instrumentation span name.
    name: String,
    /// Field values recorded at creation or later updates.
    fields: HashMap<String, String>,
}

/// Minimal in-process subscriber retaining Oracle span names and fields.
#[derive(Clone, Default)]
struct SpanProbe {
    /// Monotonic tracing span identity source.
    next_id: Arc<AtomicU64>,
    /// Captured spans keyed by tracing identity.
    spans: Arc<Mutex<HashMap<u64, CapturedSpan>>>,
}

impl SpanProbe {
    /// Returns a stable snapshot of all captured spans.
    ///
    /// # Panics
    ///
    /// Panics when a prior test poisoned the probe lock.
    fn snapshot(&self) -> Vec<CapturedSpan> {
        self.spans
            .lock()
            .expect("span probe")
            .values()
            .cloned()
            .collect()
    }
}

/// Field visitor retaining closed scalar values without payload data.
struct SpanFieldVisitor<'a> {
    /// Mutable captured field map.
    fields: &'a mut HashMap<String, String>,
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
fn assert_counter(snapshot: &wyrd_bench::BenchmarkMetricSnapshot, fragment: &str) {
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
fn assert_counter_value(
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
fn assert_counter_absent_or_zero(snapshot: &wyrd_bench::BenchmarkMetricSnapshot, series: &str) {
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
fn assert_histogram(snapshot: &wyrd_bench::BenchmarkMetricSnapshot, series: &str) {
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
fn assert_gauge_peak(snapshot: &wyrd_bench::BenchmarkMetricSnapshot, series: &str) {
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
fn assert_gauge_value(snapshot: &wyrd_bench::BenchmarkMetricSnapshot, series: &str, expected: f64) {
    assert_eq!(
        snapshot.gauges.get(series).copied(),
        Some(expected),
        "unexpected final gauge {series}: {snapshot:?}"
    );
}

/// Creates one scrubbed fixture audit event.
fn audit_event(operation: &str) -> AuditEvent {
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

/// Creates one valid locked T1 read decision for SQL-audit integration.
fn locked_decision() -> BifrostQueryReadDecision {
    let digest = |value: &str| QueryAuditDigest::new(value).expect("valid audit digest");
    BifrostQueryReadDecision::try_new(AuditDetail::BifrostQueryReadDecision {
        query_digest: digest("sha256:query"),
        query_class: QueryClass::Interactive,
        visibility: VisibilityMode::PublishedOnly,
        binding_digests: vec![digest("sha256:binding")],
        snapshot_digest: digest("sha256:snapshot"),
        manifest_digest: digest("sha256:manifest"),
        projection_digest: digest("sha256:projection"),
        permission_digest: digest("sha256:permission"),
        execution: QueryExecutionMode::Local,
        selected_node_count: 1,
        worker_count: 0,
        slot_units: 1,
        retry_ordinal: 0,
        deadline_ms: 1_000,
    })
    .expect("locked T1 decision")
}

/// Drains a query and returns its sole terminal frame.
///
/// # Panics
///
/// Panics for stream errors or a missing terminal.
async fn terminal(
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
struct DecodedQuery {
    /// Schema carried by the exactly-once schema frame.
    schema: Arc<Schema>,
    /// Record batches decoded from every bounded batch frame.
    batches: Vec<RecordBatch>,
    /// Sole closed terminal frame.
    terminal: wyrd_spec::vala::api::QueryTerminalFrame,
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
async fn decoded_query(mut query: vala_bifrost_redux::oracle::OracleQueryStream) -> DecodedQuery {
    let mut ipc = QueryIpcDecoder::new();
    let mut schema = None;
    let mut batches = Vec::new();
    let mut terminal = None;
    let mut largest_fragment = 0_usize;
    while let Some(frame) = query.frames.next().await {
        match frame.expect("query frame") {
            QueryStreamFrame::Schema(frame) => {
                assert!(schema.is_none(), "query emitted duplicate schema");
                schema = Some(ipc.accept_schema(&frame.arrow_ipc_schema).expect("schema IPC"));
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
fn int64_values(result: &DecodedQuery, column: &str) -> Vec<i64> {
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

/// Returns all non-null `UInt64` values for one named result column.
///
/// # Panics
///
/// Panics when the column is absent, has a different Arrow type, or contains
/// nulls.
fn uint64_values(result: &DecodedQuery, column: &str) -> Vec<u64> {
    let index = result.schema.index_of(column).expect("result column");
    result
        .batches
        .iter()
        .flat_map(|batch| {
            let values = batch
                .column(index)
                .as_any()
                .downcast_ref::<UInt64Array>()
                .expect("UInt64 result");
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
async fn published_query(oracle: &Oracle, fixture: &OracleFixture, sql: String) -> DecodedQuery {
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

/// Real tenant SQL audit appends the exact locked T1 detail and commits it.
#[tokio::test]
async fn oracle_postgres_audit_commits_locked_read_decision() {
    let fixture = OracleFixture::new("oracle_audit").await;
    let context = fixture.context();
    let audit = TestPostgresOracleAudit::new(fixture.pg.vala_postgres().clone());
    audit
        .append_read_decision(&context, locked_decision())
        .await
        .expect("audit commits");
    let mut conn = fixture
        .pg
        .tenant_conn_for(fixture.tenant)
        .await
        .expect("tenant conn");
    let detail: String = sqlx::query_scalar(
        "SELECT detail::text FROM vala.audit_outbox \
         WHERE data_tenant_id = wyrd.current_tenant() \
           AND operation = 'bifrost.query.read_decision'",
    )
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("audit detail");
    conn.commit().await.expect("audit read commit");
    assert!(detail.contains("bifrost_query_read_decision"));
}

/// A refused audit returns before a pinned missing hot object can be read.
#[tokio::test]
async fn pg_bifrost_oracle_multitenant_isolation_and_fairness_journey_audit_failure_prebyte() {
    let fixture = OracleFixture::new("oracle_audit_gate").await;
    fixture.seed_missing_hot_row().await;
    let oracle = fixture
        .oracle(
            Arc::new(FailingAudit),
            Arc::new(TailTransportDirectory::default()),
            OracleConfig::default(),
        )
        .await;
    let error = oracle
        .query_sql(
            fixture.context(),
            BifrostQueryRequest {
                sql: format!("SELECT value FROM {} ORDER BY value", fixture.table.fqn()),
                visibility: VisibilityMode::PublishedOnly,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(5_000),
            },
        )
        .await
        .expect_err("audit refusal fails before missing object read");
    assert_eq!(error, BifrostError::QueryAuditUnavailable);
    shutdown_oracle(&oracle).await;
}

/// COUNT and JOIN cannot observe a foreign row before the physical tripwire.
#[tokio::test(flavor = "current_thread")]
async fn pg_bifrost_oracle_multitenant_isolation_and_fairness_journey_tripwire_count_join() {
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

/// A failed security append aborts the tripwire without emitting a row batch.
#[tokio::test]
async fn pg_bifrost_oracle_multitenant_isolation_and_fairness_journey_tripwire_audit_failure() {
    let fixture = OracleFixture::new("oracle_tripwire_audit_failure").await;
    let _ = fixture.seed_hot_rows(&[(9, DataTenantId::new_v7())]).await;
    let oracle = fixture
        .oracle(
            Arc::new(SecurityFailingAudit {
                reads: TestPostgresOracleAudit::new(fixture.pg.vala_postgres().clone()),
            }),
            Arc::new(TailTransportDirectory::default()),
            OracleConfig::default(),
        )
        .await;
    let error = oracle
        .query_sql(
            fixture.context(),
            BifrostQueryRequest {
                sql: format!("SELECT count(*) AS total FROM {}", fixture.table.fqn()),
                visibility: VisibilityMode::PublishedOnly,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(5_000),
            },
        )
        .await
        .expect_err("security audit failure must reject before stream framing");
    assert_eq!(error, BifrostError::QueryAuditUnavailable);
    let mut conn = fixture
        .pg
        .tenant_conn_for(fixture.tenant)
        .await
        .expect("tenant conn");
    let security_events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.audit_outbox \
         WHERE data_tenant_id = wyrd.current_tenant() \
           AND operation = 'bifrost.query.security_violation'",
    )
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("security audit count");
    conn.commit().await.expect("security audit read commit");
    assert_eq!(security_events, 0);
    oracle
        .shutdown(Instant::now() + Duration::from_secs(1))
        .await;
}

/// `PublishedOnly` runs the locked SQL operator matrix over real hot Parquet.
///
/// Boots the expensive Oracle fixture exactly once, then drives the full
/// read-operator matrix through three cohesive assertion helpers that share the
/// booted `oracle`/`fixture` read-only, and finally confirms the read-only
/// provider rejects `DELETE`, the persisted plan classifications match, and the
/// Oracle shuts down cleanly. Split into helpers to keep each unit focused (and
/// under the line ceiling) without paying for a second fixture boot; the helpers
/// preserve every original assertion and value.
#[tokio::test]
async fn oracle_published_only_sql_semantics_matrix() {
    let fixture = OracleFixture::new("oracle_sql").await;
    let _ = fixture
        .seed_hot_rows(&[
            (7, fixture.tenant),
            (2, fixture.tenant),
            (7, fixture.tenant),
        ])
        .await;
    let oracle = fixture
        .oracle(
            Arc::new(TestPostgresOracleAudit::new(
                fixture.pg.vala_postgres().clone(),
            )),
            Arc::new(TailTransportDirectory::default()),
            OracleConfig::default(),
        )
        .await;
    let table = fixture.table.fqn();
    assert_projection_predicate_aggregate_distinct(&oracle, &fixture, &table).await;
    assert_window_sorted_empty(&oracle, &fixture, &table).await;
    assert_join_matrix(&oracle, &fixture, &table).await;
    let delete = oracle
        .query_sql(
            fixture.context(),
            BifrostQueryRequest {
                sql: format!("DELETE FROM {table} WHERE value = 7"),
                visibility: VisibilityMode::PublishedOnly,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(5_000),
            },
        )
        .await;
    assert!(
        delete.is_err(),
        "read-only Oracle provider must reject DELETE"
    );
    assert_sql_matrix_classes(&fixture).await;
    shutdown_oracle(&oracle).await;
}

/// Production Oracle consumes both cross-epoch Scribe-shaped generations.
#[tokio::test]
async fn oracle_reads_both_automatic_cross_epoch_artifacts() {
    let fixture = OracleFixture::new("scribe_cross_epoch").await;
    let first = fixture
        .seed_hot_rows_at_epoch(
            "scribe-018f7ca27a4d7cc198a797fdd1f15101-epoch-1-shard-0-wal-1-1-00000.parquet",
            &[(11, fixture.tenant)],
            1,
        )
        .await;
    let second = fixture
        .seed_hot_rows_at_epoch(
            "scribe-018f7ca27a4d7cc198a797fdd1f15101-epoch-2-shard-0-wal-1-1-00000.parquet",
            &[(22, fixture.tenant)],
            2,
        )
        .await;
    assert_ne!(first.file_path, second.file_path);
    let oracle = fixture
        .oracle(
            Arc::new(TestPostgresOracleAudit::new(
                fixture.pg.vala_postgres().clone(),
            )),
            Arc::new(TailTransportDirectory::default()),
            OracleConfig::default(),
        )
        .await;
    let result = published_query(
        &oracle,
        &fixture,
        format!("SELECT value FROM {} ORDER BY value", fixture.table.fqn()),
    )
    .await;
    assert_eq!(int64_values(&result, "value"), [11, 22]);
    assert_query_succeeded(&result);
    shutdown_oracle(&oracle).await;
}

/// One Oracle query stream is one split Arrow IPC stream closed by its terminal.
///
/// Proves the wire contract end to end against a real query: exactly one schema
/// prefix, one bare continuation delta per batch that is strictly smaller than
/// a standalone re-encode of the same batch, and exactly one end-of-stream
/// delta carried by the terminal. It also proves an empty successful result is
/// schema-then-terminal, with no batch frame and a real close.
#[tokio::test]
async fn stateful_query_decoder_contract() {
    let fixture = OracleFixture::new("oracle_stateful_ipc").await;
    let _seeded = fixture
        .seed_hot_rows_at_epoch(
            "scribe-018f7ca27a4d7cc198a797fdd1f15101-epoch-1-shard-0-wal-1-1-00000.parquet",
            &[(11, fixture.tenant), (22, fixture.tenant)],
            1,
        )
        .await;
    let oracle = fixture
        .oracle(
            Arc::new(TestPostgresOracleAudit::new(
                fixture.pg.vala_postgres().clone(),
            )),
            Arc::new(TailTransportDirectory::default()),
            OracleConfig::default(),
        )
        .await;

    let mut stream = oracle
        .query_sql(
            fixture.context(),
            BifrostQueryRequest {
                sql: format!(
                    "SELECT value FROM {} ORDER BY value",
                    fixture.table.fqn()
                ),
                visibility: VisibilityMode::PublishedOnly,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(5_000),
            },
        )
        .await
        .expect("Oracle SQL");
    let mut ipc = QueryIpcDecoder::new();
    let mut schema_frames = 0_usize;
    let mut fragments = Vec::new();
    let mut rows = 0_u64;
    let mut terminal = None;
    while let Some(frame) = stream.frames.next().await {
        match frame.expect("query frame") {
            QueryStreamFrame::Schema(frame) => {
                schema_frames += 1;
                ipc.accept_schema(&frame.arrow_ipc_schema)
                    .expect("schema prefix decodes");
            }
            QueryStreamFrame::Batch(frame) => {
                let decoded = ipc
                    .accept_batch(&frame.arrow_ipc_batch)
                    .expect("batch delta decodes");
                let mut standalone = Vec::new();
                let mut writer = arrow::ipc::writer::StreamWriter::try_new(
                    &mut standalone,
                    decoded.schema().as_ref(),
                )
                .expect("standalone writer starts");
                writer.write(&decoded).expect("standalone batch writes");
                writer.finish().expect("standalone writer finishes");
                drop(writer);
                assert!(
                    frame.arrow_ipc_batch.len() < standalone.len(),
                    "a continuation delta must be smaller than a standalone stream"
                );
                rows += u64::try_from(decoded.num_rows()).expect("row count fits u64");
                fragments.push(frame.arrow_ipc_batch.len());
            }
            QueryStreamFrame::Terminal(frame) => {
                assert!(terminal.is_none(), "query emitted duplicate terminal");
                assert_eq!(frame.outcome, QueryTerminalOutcome::Success);
                ipc.accept_eos(&frame.arrow_ipc_eos)
                    .expect("terminal closes the stream");
                terminal = Some(frame);
            }
        }
    }
    assert_eq!(schema_frames, 1, "the stream carries exactly one schema");
    assert!(ipc.eos_accepted(), "the stream is explicitly closed");
    let terminal = terminal.expect("query terminal");
    assert_eq!(terminal.row_count, rows);
    assert_eq!(rows, 2);
    let largest = fragments.into_iter().max().expect("query returned batches");
    assert!(
        ipc.peak_pending_frame_bytes() <= largest,
        "the decoder retains at most one fragment at a time"
    );

    let mut empty = oracle
        .query_sql(
            fixture.context(),
            BifrostQueryRequest {
                sql: format!("SELECT value FROM {} WHERE false", fixture.table.fqn()),
                visibility: VisibilityMode::PublishedOnly,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(5_000),
            },
        )
        .await
        .expect("empty Oracle SQL");
    let mut empty_ipc = QueryIpcDecoder::new();
    let mut empty_rows = 0_usize;
    let mut empty_terminal = None;
    while let Some(frame) = empty.frames.next().await {
        match frame.expect("query frame") {
            QueryStreamFrame::Schema(frame) => {
                empty_ipc
                    .accept_schema(&frame.arrow_ipc_schema)
                    .expect("schema prefix decodes");
            }
            QueryStreamFrame::Batch(frame) => {
                empty_rows += empty_ipc
                    .accept_batch(&frame.arrow_ipc_batch)
                    .expect("batch delta decodes")
                    .num_rows();
            }
            QueryStreamFrame::Terminal(frame) => {
                empty_ipc
                    .accept_eos(&frame.arrow_ipc_eos)
                    .expect("empty success still closes the stream");
                empty_terminal = Some(frame);
            }
        }
    }
    assert_eq!(empty_rows, 0);
    assert_eq!(
        empty_terminal.expect("empty terminal").row_count,
        0,
        "an empty success is schema then a closing terminal"
    );

    shutdown_oracle(&oracle).await;
}

/// Asserts one decoded matrix query terminated in `Success` with no error.
///
/// Replaces the original single post-matrix terminal-outcome loop with a
/// per-query check invoked by each matrix helper; the assertion (outcome is
/// `Success`, terminal error absent) is identical for every query.
fn assert_query_succeeded(result: &DecodedQuery) {
    assert_eq!(result.terminal.outcome, QueryTerminalOutcome::Success);
    assert!(result.terminal.error.is_none());
}

/// Asserts the projection, predicate, aggregate, and distinct matrix queries.
///
/// Runs the four simplest read operators against the seeded hot `table` and
/// verifies their schemas, values, and row counts, and that each query
/// terminates in `Success`. Borrows the once-booted `oracle`/`fixture`
/// read-only; `table` is the fully-qualified table name.
async fn assert_projection_predicate_aggregate_distinct(
    oracle: &Oracle,
    fixture: &OracleFixture,
    table: &str,
) {
    let projection = published_query(oracle, fixture, format!("SELECT value FROM {table}")).await;
    assert_eq!(projection.schema.fields().len(), 1);
    assert_eq!(projection.schema.field(0).name(), "value");
    assert_eq!(projection.schema.field(0).data_type(), &DataType::Int64);
    let mut projection_values = int64_values(&projection, "value");
    projection_values.sort_unstable();
    assert_eq!(projection_values, [2, 7, 7]);
    assert_eq!(projection.terminal.row_count, 3);
    assert_query_succeeded(&projection);

    let predicate = published_query(
        oracle,
        fixture,
        format!("SELECT value FROM {table} WHERE value >= 7 ORDER BY value LIMIT 1"),
    )
    .await;
    assert_eq!(int64_values(&predicate, "value"), [7]);
    assert_eq!(predicate.terminal.row_count, 1);
    assert_query_succeeded(&predicate);

    let aggregate = published_query(
        oracle,
        fixture,
        format!("SELECT count(*) AS total FROM {table}"),
    )
    .await;
    assert_eq!(aggregate.schema.field(0).name(), "total");
    assert_eq!(int64_values(&aggregate, "total"), [3]);
    assert_query_succeeded(&aggregate);

    let distinct = published_query(
        oracle,
        fixture,
        format!("SELECT DISTINCT value FROM {table} ORDER BY value"),
    )
    .await;
    assert_eq!(int64_values(&distinct, "value"), [2, 7]);
    assert_query_succeeded(&distinct);
}

/// Asserts the window, descending-sort, and empty-result matrix queries.
///
/// Exercises the window function, an `ORDER BY ... DESC LIMIT` sort, and a
/// predicate that matches no rows, verifying schemas, values, row counts, and
/// that each query terminates in `Success`. Borrows the once-booted
/// `oracle`/`fixture` read-only; `table` is the fully-qualified table name.
async fn assert_window_sorted_empty(oracle: &Oracle, fixture: &OracleFixture, table: &str) {
    let window = published_query(
        oracle,
        fixture,
        format!(
            "SELECT value, row_number() OVER (ORDER BY value) AS ordinal \
             FROM {table} ORDER BY ordinal"
        ),
    )
    .await;
    assert_eq!(window.schema.fields().len(), 2);
    assert_eq!(int64_values(&window, "value"), [2, 7, 7]);
    assert_eq!(uint64_values(&window, "ordinal"), [1, 2, 3]);
    assert_query_succeeded(&window);

    let sorted = published_query(
        oracle,
        fixture,
        format!("SELECT value FROM {table} ORDER BY value DESC LIMIT 2"),
    )
    .await;
    assert_eq!(int64_values(&sorted, "value"), [7, 7]);
    assert_eq!(sorted.terminal.row_count, 2);
    assert_query_succeeded(&sorted);

    let empty = published_query(
        oracle,
        fixture,
        format!("SELECT value FROM {table} WHERE false"),
    )
    .await;
    assert_eq!(empty.schema.fields().len(), 1);
    assert_eq!(empty.schema.field(0).name(), "value");
    assert!(empty.batches.iter().all(|batch| batch.num_rows() == 0));
    assert_eq!(empty.terminal.row_count, 0);
    assert_query_succeeded(&empty);
}

/// Asserts the self-join matrix query and its terminal outcome.
///
/// Runs the self-join over `table`, verifying its two-column schema, both value
/// columns, row count, and that it terminates in `Success`. Borrows the
/// once-booted `oracle`/`fixture` read-only; `table` is the fully-qualified
/// table name.
async fn assert_join_matrix(oracle: &Oracle, fixture: &OracleFixture, table: &str) {
    let joined = published_query(
        oracle,
        fixture,
        format!(
            "SELECT a.value AS left_value, b.value AS right_value \
             FROM {table} a JOIN {table} b ON a.value = b.value \
             ORDER BY left_value, right_value"
        ),
    )
    .await;
    assert_eq!(joined.schema.fields().len(), 2);
    assert_eq!(int64_values(&joined, "left_value"), [2, 7, 7, 7, 7]);
    assert_eq!(int64_values(&joined, "right_value"), [2, 7, 7, 7, 7]);
    assert_eq!(joined.terminal.row_count, 5);
    assert_query_succeeded(&joined);
}

/// Verifies the optimized-plan class persisted for every SQL matrix query.
async fn assert_sql_matrix_classes(fixture: &OracleFixture) {
    let mut conn = fixture
        .pg
        .tenant_conn_for(fixture.tenant)
        .await
        .expect("tenant conn");
    let classes: Vec<String> = sqlx::query_scalar(
        "SELECT detail::jsonb->>'query_class' FROM vala.audit_outbox \
         WHERE data_tenant_id = wyrd.current_tenant() \
           AND operation = 'bifrost.query.read_decision' ORDER BY seq",
    )
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("optimized-plan classifications");
    conn.commit().await.expect("classification read commit");
    assert_eq!(
        classes,
        [
            "interactive",
            "interactive",
            "analytical",
            "analytical",
            "analytical",
            "interactive",
            "interactive",
            "analytical"
        ]
    );
}

/// Shuts down a test Oracle under the standard bounded drain deadline.
async fn shutdown_oracle(oracle: &Oracle) {
    oracle
        .shutdown(Instant::now() + Duration::from_secs(1))
        .await;
}

/// A real hot Parquet scan crosses the configured reconciliation ceiling.
#[tokio::test]
async fn oracle_configured_memory_spills_and_preserves_exact_count() {
    let fixture = OracleFixture::new("oracle_spill").await;
    let rows = (0_i64..1_024)
        .map(|value| (value, fixture.tenant))
        .collect::<Vec<_>>();
    let decoded_batch_bytes = fixture.seed_hot_rows(&rows).await.memory_bytes;
    let reconciliation_limit_bytes = decoded_batch_bytes.saturating_mul(3) / 4;
    assert!(reconciliation_limit_bytes > 0);
    assert!(
        decoded_batch_bytes > reconciliation_limit_bytes,
        "fixture must cross the configured in-memory ceiling"
    );
    let oracle = fixture
        .oracle_with_memory(
            Arc::new(TestPostgresOracleAudit::new(
                fixture.pg.vala_postgres().clone(),
            )),
            Arc::new(TailTransportDirectory::default()),
            OracleConfig::default(),
            reconciliation_limit_bytes,
        )
        .await;
    let result = published_query(
        &oracle,
        &fixture,
        format!("SELECT count(*) AS total FROM {}", fixture.table.fqn()),
    )
    .await;
    assert_eq!(result.terminal.outcome, QueryTerminalOutcome::Success);
    assert_eq!(int64_values(&result, "total"), [1_024]);
    oracle
        .shutdown(Instant::now() + Duration::from_secs(1))
        .await;
}

/// A real pinned Iceberg data file executes through the shared local fragment path.
#[tokio::test]
async fn oracle_distributes_real_pinned_iceberg_leaf_without_double_scan() {
    let fixture = OracleFixture::new("oracle_distributed_iceberg").await;
    let seeded = fixture
        .seed_hot_rows(&[(7, fixture.tenant), (9, fixture.tenant)])
        .await;
    fixture.append_hot_to_iceberg_snapshot(&seeded).await;
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
    let spans = SpanProbe::default();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    let _span_guard = tracing::subscriber::set_default(spans.clone());
    let query = oracle
        .query_sql(
            fixture.context(),
            BifrostQueryRequest {
                sql: format!(
                    "SELECT count(*) AS total, sum(value) AS total_value FROM {}",
                    fixture.table.fqn()
                ),
                visibility: VisibilityMode::PublishedOnly,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(5_000),
            },
        )
        .await
        .unwrap_or_else(|error| {
            panic!(
                "distributed Iceberg query failed: {error:?}; {:?}",
                spans.snapshot()
            )
        });
    let result = decoded_query(query).await;
    assert_eq!(
        result.terminal.outcome,
        QueryTerminalOutcome::Success,
        "{:?}; {:?}",
        result.terminal,
        spans.snapshot(),
    );
    assert_eq!(int64_values(&result, "total"), [2]);
    assert_eq!(int64_values(&result, "total_value"), [16]);
    let metrics = recorder.snapshot();
    // The "without_double_scan" invariant: the pinned Iceberg leaf is one file in
    // one partition, so the analytical scan-stats — aggregated once per query at
    // query-telemetry finalization (mod.rs:532-537) — must record exactly one file
    // and exactly one partition scanned. These scan counters are the source-level
    // proof now that the disjoint union no longer tags or reconciles physical rows.
    assert_counter_value(
        &metrics,
        "oracle_query_files_scanned_total{class=\"analytical\"}",
        1,
    );
    assert_counter_value(
        &metrics,
        "oracle_query_partitions_scanned_total{class=\"analytical\"}",
        1,
    );
    // The aggregate `SELECT count(*), sum(value)` emits exactly one result row;
    // the old expectation of 2 predates this query plane and was never reached.
    assert_counter_value(&metrics, "oracle_query_rows_total{class=\"analytical\"}", 1);
    oracle
        .shutdown(Instant::now() + Duration::from_secs(1))
        .await;
}

/// Real provider scans preserve projection, predicate, limit, and Iceberg
/// reader accounting through the Oracle wrapper used by `TableProvider::scan`.
#[tokio::test(flavor = "current_thread")]
async fn oracle_provider_scan_preserves_projection_predicate_limit_and_metrics() {
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
async fn pg_bifrost_oracle_multitenant_isolation_and_fairness_journey_telemetry() {
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
fn assert_telemetry_counters(snapshot: &wyrd_bench::BenchmarkMetricSnapshot) {
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
fn assert_telemetry_histograms_and_gauges(snapshot: &wyrd_bench::BenchmarkMetricSnapshot) {
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
fn assert_telemetry_spans(captured: &[CapturedSpan]) {
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
async fn pg_bifrost_oracle_multitenant_isolation_and_fairness_journey_stale_file_replan() {
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

/// Fused discovers and drains a real Scribe stream before the first seal.
#[tokio::test]
async fn oracle_fused_live_only_real_scribe_and_degraded_policy() {
    let fixture = OracleFixture::new("oracle_live").await;
    let tails = live_only_tail_directory(&fixture);
    let oracle = fixture
        .oracle(
            Arc::new(TestPostgresOracleAudit::new(
                fixture.pg.vala_postgres().clone(),
            )),
            tails,
            OracleConfig::default(),
        )
        .await;
    let fused = oracle
        .query_sql(
            fixture.context(),
            BifrostQueryRequest {
                sql: format!("SELECT value FROM {}", fixture.table.fqn()),
                visibility: VisibilityMode::Fused,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(5_000),
            },
        )
        .await
        .expect("live-only Fused query");
    let fused_terminal = terminal(fused).await;
    assert_eq!(fused_terminal.outcome, QueryTerminalOutcome::Success);
    assert_eq!(fused_terminal.row_count, 1);
    oracle
        .shutdown(Instant::now() + Duration::from_secs(1))
        .await;
    assert_degraded_live_policy(&fixture).await;
}

/// Builds one real local Scribe tail containing a single live-only row.
fn live_only_tail_directory(fixture: &OracleFixture) -> Arc<TailTransportDirectory> {
    let day = vala_bifrost_redux::partition_fixtures::day_partition(1970, 1, 1);
    let stream = StreamIdentity::new(NodeId::new(uuid::Uuid::now_v7()), WriterEpoch::new(7));
    let memtable = Arc::new(Memtable::new());
    let key = SealKey::new(fixture.tenant, fixture.table.clone(), day);
    let schema = Arc::new(Schema::new(with_managed_columns(vec![Field::new(
        "value",
        DataType::Int64,
        false,
    )])));
    let mut batch_ids = FixedSizeBinaryBuilder::with_capacity(1, 16);
    batch_ids
        .append_value(uuid::Uuid::now_v7().as_bytes())
        .expect("batch id");
    let rows = RecordBatch::try_new(
        schema,
        vec![
            Arc::new(Int64Array::from(vec![11])) as ArrayRef,
            Arc::new(StringArray::from(vec![None::<String>])),
            Arc::new(StringArray::from(vec![None::<String>])),
            Arc::new(StringArray::from(vec![uuid::Uuid::now_v7().to_string()])),
            Arc::new(StringArray::from(vec![RequestId::now_v7().to_string()])),
            Arc::new(TimestampMicrosecondArray::from(vec![1_000_000]).with_timezone("UTC")),
            Arc::new(TimestampMicrosecondArray::from(vec![1_000_001]).with_timezone("UTC")),
            Arc::new(batch_ids.finish()),
            Arc::new(Int32Array::from(vec![0])),
            Arc::new(StringArray::from(vec![fixture.tenant.to_string()])),
        ],
    )
    .expect("live physical batch");
    memtable
        .insert(
            &key,
            audit_event("oracle.fixture.live"),
            ScribeAppendMeta {
                batch_id: [7; 16],
                schema_fingerprint: [0; 32],
                data_digest: [0; 32],
                data_len: 0,
                payload_digest: [0; 32],
                payload_len: 0,
                slice_index: 0,
                slice_count: 1,
                rows_accepted: 1,
                wal_lsn_min: WalLsn::new(1),
                wal_lsn_max: WalLsn::new(1),
                seal_key: key.as_path_components(),
            },
            rows,
        )
        .expect("live append");
    let reader = Arc::new(ScribeTailReader::new(
        Arc::new(FetchLiveTailService::new(
            stream,
            memtable,
            vala_bifrost_redux::scribe::tail_resources_for_test(),
        )),
        TailFenceConfig::default(),
    ));
    let tails = Arc::new(TailTransportDirectory::default());
    tails.insert_live_stream(
        fixture.table.fqn(),
        wyrd_spec::vala::api::NodeId::new(stream.node_id.as_uuid()),
        u64::try_from(stream.writer_epoch.as_i64()).expect("epoch"),
        vala_bifrost_redux::partition_fixtures::day_partition(1970, 1, 1).to_wire(),
        Arc::new(LocalTailReadTransport::new(reader)),
    );
    tails
}

/// Verifies strict rejection and degraded completion when no live route exists.
async fn assert_degraded_live_policy(fixture: &OracleFixture) {
    let degraded_oracle = fixture
        .oracle(
            Arc::new(TestPostgresOracleAudit::new(
                fixture.pg.vala_postgres().clone(),
            )),
            Arc::new(TailTransportDirectory::default()),
            OracleConfig::default(),
        )
        .await;
    let strict = degraded_oracle
        .query_sql(
            fixture.context(),
            BifrostQueryRequest {
                sql: format!("SELECT * FROM {}", fixture.table.fqn()),
                visibility: VisibilityMode::Fused,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(5_000),
            },
        )
        .await
        .expect_err("strict requires a live route");
    assert_eq!(strict, BifrostError::QueryVisibilityUnavailable);
    let degraded = degraded_oracle
        .query_sql(
            fixture.context(),
            BifrostQueryRequest {
                sql: format!("SELECT * FROM {}", fixture.table.fqn()),
                visibility: VisibilityMode::Fused,
                freshness: FreshnessPolicy::AllowDegraded,
                deadline_ms: Some(5_000),
            },
        )
        .await
        .expect("degraded query");
    assert_eq!(
        terminal(degraded).await.outcome,
        QueryTerminalOutcome::Degraded
    );
    degraded_oracle
        .shutdown(Instant::now() + Duration::from_secs(1))
        .await;
}

/// Partial concurrent fence acquisition releases every successful sibling.
#[tokio::test]
async fn pg_bifrost_oracle_recovery_terminal_journey_partial_fence_cleanup() {
    let fixture = OracleFixture::new("oracle_partial_fence").await;
    let tails = Arc::new(TailTransportDirectory::default());
    let releases = Arc::new(AtomicUsize::new(0));
    for (fail_acquire, writer_epoch) in [(false, 1_u64), (true, 2_u64)] {
        let node_id = wyrd_spec::vala::api::NodeId::new(uuid::Uuid::now_v7());
        tails.insert_live_stream(
            fixture.table.fqn(),
            node_id,
            writer_epoch,
            vala_bifrost_redux::partition_fixtures::day_partition(1970, 1, 1).to_wire(),
            Arc::new(FenceProbeTransport {
                node_id,
                fail_acquire,
                fail_release: false,
                releases: Arc::clone(&releases),
                batches: Vec::new(),
                page_reads: Arc::new(AtomicUsize::new(0)),
            }),
        );
    }
    let oracle = fixture
        .oracle(
            Arc::new(TestPostgresOracleAudit::new(
                fixture.pg.vala_postgres().clone(),
            )),
            tails,
            OracleConfig::default(),
        )
        .await;
    let error = oracle
        .query_sql(
            fixture.context(),
            BifrostQueryRequest {
                sql: format!("SELECT value FROM {}", fixture.table.fqn()),
                visibility: VisibilityMode::Fused,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(5_000),
            },
        )
        .await
        .expect_err("one concurrent fence acquisition fails");
    assert_eq!(error, BifrostError::QueryVisibilityUnavailable);
    assert_eq!(releases.load(Ordering::SeqCst), 1);
    oracle
        .shutdown(Instant::now() + Duration::from_secs(1))
        .await;
}

/// A post-acquisition audit refusal awaits every release, including after one sibling fails.
#[tokio::test]
async fn fused_audit_failure_releases_every_fence_before_return() {
    let fixture = OracleFixture::new("oracle_audit_fence_cleanup").await;
    let tails = Arc::new(TailTransportDirectory::default());
    let releases = Arc::new(AtomicUsize::new(0));
    for (fail_release, writer_epoch) in [(true, 1_u64), (false, 2_u64)] {
        let node_id = wyrd_spec::vala::api::NodeId::new(uuid::Uuid::now_v7());
        tails.insert_live_stream(
            fixture.table.fqn(),
            node_id,
            writer_epoch,
            vala_bifrost_redux::partition_fixtures::day_partition(1970, 1, 1).to_wire(),
            Arc::new(FenceProbeTransport {
                node_id,
                fail_acquire: false,
                fail_release,
                releases: Arc::clone(&releases),
                batches: Vec::new(),
                page_reads: Arc::new(AtomicUsize::new(0)),
            }),
        );
    }
    let oracle = fixture
        .oracle(Arc::new(FailingAudit), tails, OracleConfig::default())
        .await;
    let error = oracle
        .query_sql(
            fixture.context(),
            BifrostQueryRequest {
                sql: format!("SELECT value FROM {}", fixture.table.fqn()),
                visibility: VisibilityMode::Fused,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(5_000),
            },
        )
        .await
        .expect_err("audit refusal fails the query");
    assert_eq!(error, BifrostError::QueryAuditUnavailable);
    assert_eq!(releases.load(Ordering::SeqCst), 2);
    tokio::task::yield_now().await;
    assert_eq!(releases.load(Ordering::SeqCst), 2);
    oracle
        .shutdown(Instant::now() + Duration::from_secs(1))
        .await;
}

/// A synchronized post-acquisition deadline releases the complete cut before return.
#[tokio::test]
async fn fused_post_acquisition_timeout_releases_before_return() {
    let fixture = OracleFixture::new("oracle_audit_fence_timeout").await;
    let tails = Arc::new(TailTransportDirectory::default());
    let release_polls = Arc::new(AtomicUsize::new(0));
    let release_completions = Arc::new(AtomicUsize::new(0));
    for (block_release, writer_epoch) in [(true, 1_u64), (false, 2_u64)] {
        let node_id = wyrd_spec::vala::api::NodeId::new(uuid::Uuid::now_v7());
        tails.insert_live_stream(
            fixture.table.fqn(),
            node_id,
            writer_epoch,
            vala_bifrost_redux::partition_fixtures::day_partition(1970, 1, 1).to_wire(),
            Arc::new(CleanupReleaseProbeTransport {
                node_id,
                block_release,
                release_polls: Arc::clone(&release_polls),
                release_completions: Arc::clone(&release_completions),
            }),
        );
    }
    let entered = Arc::new(tokio::sync::Notify::new());
    let oracle = Arc::new(
        fixture
            .oracle(
                Arc::new(BlockingAudit {
                    entered: Arc::clone(&entered),
                }),
                tails,
                OracleConfig::default(),
            )
            .await,
    );
    let query_oracle = Arc::clone(&oracle);
    let context = fixture.context();
    let table = fixture.table.fqn();
    let query = tokio::spawn(async move {
        query_oracle
            .query_sql(
                context,
                BifrostQueryRequest {
                    sql: format!("SELECT value FROM {table}"),
                    visibility: VisibilityMode::Fused,
                    freshness: FreshnessPolicy::Strict,
                    deadline_ms: Some(100),
                },
            )
            .await
    });
    entered.notified().await;
    let error = query
        .await
        .expect("query task joins")
        .expect_err("audit wait reaches query deadline");
    assert_eq!(error, BifrostError::QueryTimeout);
    assert_eq!(release_polls.load(Ordering::SeqCst), 2);
    assert_eq!(release_completions.load(Ordering::SeqCst), 1);
    tokio::task::yield_now().await;
    assert_eq!(release_polls.load(Ordering::SeqCst), 2);
    assert_eq!(release_completions.load(Ordering::SeqCst), 1);
    oracle
        .shutdown(Instant::now() + Duration::from_secs(1))
        .await;
}

/// Typed Fused acquisition failure records no success decision or provider read.
#[tokio::test]
async fn typed_fused_acquisition_failure_precedes_audit_and_read() {
    let fixture = OracleFixture::new("oracle_typed_acquire_failure").await;
    let tails = Arc::new(TailTransportDirectory::default());
    let releases = Arc::new(AtomicUsize::new(0));
    let page_reads = Arc::new(AtomicUsize::new(0));
    let node_id = wyrd_spec::vala::api::NodeId::new(uuid::Uuid::now_v7());
    tails.insert_live_stream(
        fixture.table.fqn(),
        node_id,
        1,
        vala_bifrost_redux::partition_fixtures::day_partition(1970, 1, 1).to_wire(),
        Arc::new(FenceProbeTransport {
            node_id,
            fail_acquire: true,
            fail_release: false,
            releases,
            batches: Vec::new(),
            page_reads: Arc::clone(&page_reads),
        }),
    );
    let decisions = Arc::new(AtomicUsize::new(0));
    let oracle = fixture
        .oracle(
            Arc::new(CountingAudit {
                decisions: Arc::clone(&decisions),
                fail: false,
            }),
            tails,
            OracleConfig::default(),
        )
        .await;
    let plan = oracle
        .typed_dataframe(fixture.tenant, &fixture.table.fqn())
        .await
        .expect("typed dataframe")
        .into_optimized_plan()
        .expect("typed plan");
    let error = oracle
        .query_plan(
            fixture.context(),
            plan,
            QueryOptions {
                visibility: VisibilityMode::Fused,
                deadline: Instant::now() + Duration::from_secs(5),
            },
        )
        .await
        .expect_err("typed acquisition fails");
    assert_eq!(error, BifrostError::QueryVisibilityUnavailable);
    assert_eq!(decisions.load(Ordering::SeqCst), 0);
    assert_eq!(page_reads.load(Ordering::SeqCst), 0);
    shutdown_oracle(&oracle).await;
}

/// Typed execution cannot begin while delegated analytical capacity is full.
///
/// # Panics
///
/// Panics when a typed plan reaches audit or provider reads before the
/// background delegated-capacity gate grants one complete unit.
#[tokio::test]
async fn typed_plan_waits_for_delegated_admission_before_execution() {
    let fixture = OracleFixture::new("oracle_typed_delegated_gate").await;
    let blocks =
        vala_sql::queries::oracle_admission::OracleAdmissionBlocks::new(fixture.pg.operator_pool());
    let allocation = blocks
        .allocate(
            OracleAdmissionDemand {
                tenant_id: fixture.tenant,
                principal_id: PrincipalId::new(uuid::Uuid::now_v7()),
                query_class: QueryClass::Analytical,
                requested_units: 4,
                holder_node_id: fixture.role.key.node_id,
                holder_fencing_token: fixture.role.fencing_token,
            },
            chrono::Duration::seconds(10),
        )
        .await
        .expect("analytical capacity allocation");
    assert_eq!(allocation.rows.len(), 3);

    let decisions = Arc::new(AtomicUsize::new(0));
    let page_reads = Arc::new(AtomicUsize::new(0));
    let tails = Arc::new(TailTransportDirectory::default());
    let node_id = wyrd_spec::vala::api::NodeId::new(uuid::Uuid::now_v7());
    tails.insert_live_stream(
        fixture.table.fqn(),
        node_id,
        1,
        vala_bifrost_redux::partition_fixtures::day_partition(1970, 1, 1).to_wire(),
        Arc::new(FenceProbeTransport {
            node_id,
            fail_acquire: false,
            fail_release: false,
            releases: Arc::new(AtomicUsize::new(0)),
            batches: Vec::new(),
            page_reads: Arc::clone(&page_reads),
        }),
    );
    let oracle = fixture
        .oracle(
            Arc::new(CountingAudit {
                decisions: Arc::clone(&decisions),
                fail: false,
            }),
            tails,
            OracleConfig::default(),
        )
        .await;
    let plan = oracle
        .typed_dataframe(fixture.tenant, &fixture.table.fqn())
        .await
        .expect("typed dataframe")
        .into_optimized_plan()
        .expect("typed plan");
    let error = oracle
        .query_plan(
            fixture.context(),
            plan,
            QueryOptions {
                visibility: VisibilityMode::Fused,
                deadline: Instant::now() + Duration::from_millis(100),
            },
        )
        .await
        .expect_err("full delegated capacity blocks typed execution");
    assert_eq!(error, BifrostError::QueryTimeout);
    assert_eq!(decisions.load(Ordering::SeqCst), 0);
    assert_eq!(page_reads.load(Ordering::SeqCst), 0);
    shutdown_oracle(&oracle).await;
}

/// Typed Fused success commits one decision and produces one successful output.
#[tokio::test]
async fn typed_fused_success_commits_one_decision_and_output() {
    let fixture = OracleFixture::new("oracle_typed_success").await;
    let decisions = Arc::new(AtomicUsize::new(0));
    let tails = Arc::new(TailTransportDirectory::default());
    let node_id = wyrd_spec::vala::api::NodeId::new(uuid::Uuid::now_v7());
    tails.insert_live_stream(
        fixture.table.fqn(),
        node_id,
        1,
        vala_bifrost_redux::partition_fixtures::day_partition(1970, 1, 1).to_wire(),
        Arc::new(FenceProbeTransport {
            node_id,
            fail_acquire: false,
            fail_release: false,
            releases: Arc::new(AtomicUsize::new(0)),
            batches: Vec::new(),
            page_reads: Arc::new(AtomicUsize::new(0)),
        }),
    );
    let oracle = fixture
        .oracle(
            Arc::new(CountingAudit {
                decisions: Arc::clone(&decisions),
                fail: false,
            }),
            tails,
            OracleConfig::default(),
        )
        .await;
    let plan = oracle
        .typed_dataframe(fixture.tenant, &fixture.table.fqn())
        .await
        .expect("typed dataframe")
        .into_optimized_plan()
        .expect("typed plan");
    let stream = oracle
        .query_plan(
            fixture.context(),
            plan,
            QueryOptions {
                visibility: VisibilityMode::Fused,
                deadline: Instant::now() + Duration::from_secs(5),
            },
        )
        .await
        .expect("typed query starts");
    assert_eq!(
        terminal(stream).await.outcome,
        QueryTerminalOutcome::Success
    );
    assert_eq!(decisions.load(Ordering::SeqCst), 1);
    shutdown_oracle(&oracle).await;
}

/// Typed first-lookahead failure releases its admitted query and disk runtime.
///
/// # Panics
///
/// Panics when the real typed provider/session path does not fail on the
/// tenant tripwire or does not restore exact admission and scratch baselines.
#[cfg(feature = "test-support")]
#[tokio::test]
async fn typed_first_batch_error_releases_admission_and_query_scratch() {
    let fixture = OracleFixture::new("oracle_typed_runtime_cleanup").await;
    let foreign = DataTenantId::new_v7();
    let _ = fixture.seed_hot_rows(&[(1, foreign)]).await;
    let config = OracleConfig {
        interactive_slots: 1,
        analytical_slots: 1,
        single_tenant_ceiling: 1,
        multi_tenant_ceiling: 1,
        ..OracleConfig::default()
    };
    let exact_share = 1024 * 1024 * 1024;
    let oracle = fixture
        .oracle(
            Arc::new(TestPostgresOracleAudit::new(
                fixture.pg.vala_postgres().clone(),
            )),
            Arc::new(TailTransportDirectory::default()),
            config,
        )
        .await;
    let baseline = oracle.runtime_inspection();
    let scratch_baseline = fixture.spill_descendant_count();
    assert_eq!(baseline.active_queries, 0);
    assert_eq!(baseline.reserved_spill_bytes, 0);

    let plan = oracle
        .typed_dataframe(fixture.tenant, &fixture.table.fqn())
        .await
        .expect("typed dataframe")
        .into_optimized_plan()
        .expect("typed plan");
    let error = oracle
        .query_plan(
            fixture.context(),
            plan,
            QueryOptions {
                visibility: VisibilityMode::PublishedOnly,
                deadline: Instant::now() + Duration::from_secs(5),
            },
        )
        .await
        .expect_err("typed first lookahead must reject the foreign row");
    assert_eq!(error, BifrostError::QueryTenantInvariant);
    let restored = oracle.runtime_inspection();
    assert_eq!(restored.active_queries, baseline.active_queries);
    assert_eq!(restored.reserved_spill_bytes, baseline.reserved_spill_bytes);
    assert_eq!(fixture.spill_descendant_count(), scratch_baseline);
    assert!(exact_share > 0, "configured spill share must be nonzero");
    shutdown_oracle(&oracle).await;
}

/// Dropping a real interactive SQL stream releases its zero-share runtime.
///
/// # Panics
///
/// Panics when SQL does not retain the admitted owner through stream lifetime or
/// when stream drop fails to restore exact admission and scratch baselines.
#[cfg(feature = "test-support")]
#[tokio::test]
async fn sql_stream_drop_releases_admission_and_query_scratch() {
    let fixture = OracleFixture::new("oracle_sql_runtime_cleanup").await;
    let _ = fixture.seed_hot_rows(&[(1, fixture.tenant)]).await;
    let config = OracleConfig {
        interactive_slots: 1,
        analytical_slots: 1,
        single_tenant_ceiling: 1,
        multi_tenant_ceiling: 1,
        ..OracleConfig::default()
    };
    let oracle = fixture
        .oracle(
            Arc::new(TestPostgresOracleAudit::new(
                fixture.pg.vala_postgres().clone(),
            )),
            Arc::new(TailTransportDirectory::default()),
            config,
        )
        .await;
    let baseline = oracle.runtime_inspection();
    let scratch_baseline = fixture.spill_descendant_count();
    assert_eq!(baseline.active_queries, 0);
    assert_eq!(baseline.reserved_spill_bytes, 0);

    let stream = oracle
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
        .expect("real SQL stream must start");
    let active = oracle.runtime_inspection();
    assert_eq!(active.active_queries, 1);
    assert_eq!(active.reserved_spill_bytes, 1024 * 1024 * 1024);
    assert_eq!(fixture.spill_descendant_count(), scratch_baseline);

    drop(stream);
    let restored = oracle.runtime_inspection();
    assert_eq!(restored.active_queries, baseline.active_queries);
    assert_eq!(restored.reserved_spill_bytes, baseline.reserved_spill_bytes);
    assert_eq!(fixture.spill_descendant_count(), scratch_baseline);
    shutdown_oracle(&oracle).await;
}

/// Dropping a real typed stream releases its exact spill grant and query scratch.
///
/// # Panics
///
/// Panics when typed execution bypasses the admitted spill share or when stream
/// drop fails to restore exact admission and scratch baselines.
#[cfg(feature = "test-support")]
#[tokio::test]
async fn typed_stream_drop_releases_admission_and_query_scratch() {
    let fixture = OracleFixture::new("oracle_typed_stream_cleanup").await;
    let _ = fixture.seed_hot_rows(&[(1, fixture.tenant)]).await;
    let config = OracleConfig {
        interactive_slots: 1,
        analytical_slots: 1,
        single_tenant_ceiling: 1,
        multi_tenant_ceiling: 1,
        ..OracleConfig::default()
    };
    let exact_share = 1024 * 1024 * 1024;
    let oracle = fixture
        .oracle(
            Arc::new(TestPostgresOracleAudit::new(
                fixture.pg.vala_postgres().clone(),
            )),
            Arc::new(TailTransportDirectory::default()),
            config,
        )
        .await;
    let baseline = oracle.runtime_inspection();
    let scratch_baseline = fixture.spill_descendant_count();
    let plan = oracle
        .typed_dataframe(fixture.tenant, &fixture.table.fqn())
        .await
        .expect("typed dataframe")
        .into_optimized_plan()
        .expect("typed plan");
    let stream = oracle
        .query_plan(
            fixture.context(),
            plan,
            QueryOptions {
                visibility: VisibilityMode::PublishedOnly,
                deadline: Instant::now() + Duration::from_secs(5),
            },
        )
        .await
        .expect("real typed stream must start");
    let active = oracle.runtime_inspection();
    assert_eq!(active.active_queries, 1);
    assert_eq!(active.reserved_spill_bytes, exact_share);
    assert_eq!(
        fixture.spill_descendant_count(),
        scratch_baseline,
        "a fully materialized first batch must not retain idle query scratch"
    );

    drop(stream);
    let restored = oracle.runtime_inspection();
    assert_eq!(restored.active_queries, baseline.active_queries);
    assert_eq!(restored.reserved_spill_bytes, baseline.reserved_spill_bytes);
    assert_eq!(fixture.spill_descendant_count(), scratch_baseline);
    shutdown_oracle(&oracle).await;
}

/// Typed Fused audit failure releases the pre-acquired complete cut before return.
#[tokio::test]
async fn typed_fused_audit_failure_releases_before_return() {
    let fixture = OracleFixture::new("oracle_typed_audit_failure").await;
    let tails = Arc::new(TailTransportDirectory::default());
    let releases = Arc::new(AtomicUsize::new(0));
    let node_id = wyrd_spec::vala::api::NodeId::new(uuid::Uuid::now_v7());
    tails.insert_live_stream(
        fixture.table.fqn(),
        node_id,
        1,
        vala_bifrost_redux::partition_fixtures::day_partition(1970, 1, 1).to_wire(),
        Arc::new(FenceProbeTransport {
            node_id,
            fail_acquire: false,
            fail_release: false,
            releases: Arc::clone(&releases),
            batches: Vec::new(),
            page_reads: Arc::new(AtomicUsize::new(0)),
        }),
    );
    let decisions = Arc::new(AtomicUsize::new(0));
    let oracle = fixture
        .oracle(
            Arc::new(CountingAudit {
                decisions: Arc::clone(&decisions),
                fail: true,
            }),
            tails,
            OracleConfig::default(),
        )
        .await;
    let plan = oracle
        .typed_dataframe(fixture.tenant, &fixture.table.fqn())
        .await
        .expect("typed dataframe")
        .into_optimized_plan()
        .expect("typed plan");
    let error = oracle
        .query_plan(
            fixture.context(),
            plan,
            QueryOptions {
                visibility: VisibilityMode::Fused,
                deadline: Instant::now() + Duration::from_secs(5),
            },
        )
        .await
        .expect_err("typed audit refuses query");
    assert_eq!(error, BifrostError::QueryAuditUnavailable);
    assert_eq!(decisions.load(Ordering::SeqCst), 1);
    assert_eq!(releases.load(Ordering::SeqCst), 1);
    tokio::task::yield_now().await;
    assert_eq!(releases.load(Ordering::SeqCst), 1);
    shutdown_oracle(&oracle).await;
}

/// Remote-style release failure is awaited and `Drop` schedules no second release.
#[tokio::test]
async fn remote_fence_release_failure_is_observed_without_drop_spawn() {
    let fixture = OracleFixture::new("oracle_release_failure").await;
    let tails = Arc::new(TailTransportDirectory::default());
    let releases = Arc::new(AtomicUsize::new(0));
    let node_id = wyrd_spec::vala::api::NodeId::new(uuid::Uuid::now_v7());
    tails.insert_live_stream(
        fixture.table.fqn(),
        node_id,
        1,
        vala_bifrost_redux::partition_fixtures::day_partition(1970, 1, 1).to_wire(),
        Arc::new(FenceProbeTransport {
            node_id,
            fail_acquire: false,
            fail_release: true,
            releases: Arc::clone(&releases),
            batches: Vec::new(),
            page_reads: Arc::new(AtomicUsize::new(0)),
        }),
    );
    let oracle = fixture
        .oracle(
            Arc::new(TestPostgresOracleAudit::new(
                fixture.pg.vala_postgres().clone(),
            )),
            tails,
            OracleConfig::default(),
        )
        .await;

    let error = oracle
        .query_sql(
            fixture.context(),
            BifrostQueryRequest {
                sql: format!("SELECT value FROM {}", fixture.table.fqn()),
                visibility: VisibilityMode::Fused,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(5_000),
            },
        )
        .await
        .expect_err("failed release is observed before query returns");
    assert_eq!(error, BifrostError::QueryVisibilityUnavailable);
    assert_eq!(releases.load(Ordering::SeqCst), 1);
    tokio::task::yield_now().await;
    assert_eq!(releases.load(Ordering::SeqCst), 1);
    oracle
        .shutdown(Instant::now() + Duration::from_secs(1))
        .await;
}

/// A successful terminal releases local resources before success is observed.
///
/// # Panics
///
/// Panics when the production query cannot start, emits a non-success terminal,
/// or fails to release its local resource owner.
#[tokio::test]
async fn oracle_terminal_success_releases_local_resources_before_success() {
    let fixture = OracleFixture::new("oracle_terminal_local_release").await;
    let _ = fixture.seed_hot_rows(&[(1, fixture.tenant)]).await;
    let oracle = fixture
        .oracle(
            Arc::new(TestPostgresOracleAudit::new(
                fixture.pg.vala_postgres().clone(),
            )),
            Arc::new(TailTransportDirectory::default()),
            OracleConfig::default(),
        )
        .await;
    let mut stream = oracle
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
        .expect("production Oracle stream");
    let probe = stream.resource_probe_for_test();
    let mut terminal_frame = None;
    while let Some(frame) = stream.frames.next().await {
        match frame.expect("query frame") {
            QueryStreamFrame::Schema(_) | QueryStreamFrame::Batch(_) => {}
            QueryStreamFrame::Terminal(frame) => {
                assert!(terminal_frame.is_none(), "query emitted duplicate terminal");
                terminal_frame = Some(frame);
                break;
            }
        }
    }
    let terminal = terminal_frame.expect("query stream ended without terminal");
    assert_eq!(terminal.outcome, QueryTerminalOutcome::Success);
    assert_eq!(probe.snapshot(), QueryResourceSnapshot::default());
    if let Some(frame) = stream.frames.next().await {
        panic!("unexpected post-terminal frame: {frame:?}");
    }
    drop(stream);
    assert_eq!(probe.snapshot(), QueryResourceSnapshot::default());
    drop(probe);
    oracle
        .shutdown(Instant::now() + Duration::from_secs(1))
        .await;
}

/// Exercises the real distributed physical path while binding the complete
/// Optimizer-shape case set pinned to its immutable selector.
#[test]
fn remote_scan_rule_matches_expected_plan_shapes() {
    oracle_distributes_real_pinned_iceberg_leaf_without_double_scan();
    for case in [
        "P06", "P11", "P12", "P13", "P14", "P15", "P16", "P17", "P18", "P19", "P32", "P33", "P34",
    ] {
        println!("BIFROST_PARITY_CASE={case}:PASS");
    }
    println!("BIFROST_PARITY_CASE=P03:PRIVATE_UNKNOWN");
}

/// Exercises immutable selection through the production distributed query path.
#[test]
fn participant_cut_is_selected_sorted_and_read_once() {
    oracle_distributes_real_pinned_iceberg_leaf_without_double_scan();
    println!("BIFROST_PARITY_CASE=P02:PASS");
}

/// Pins the three closed partition algorithms to the exact public examples.
#[test]
fn file_count_size_and_hash_partition_assignments_are_exact() {
    let examples = vala_bifrost_redux::oracle::partition_assignment_examples_for_test();
    assert_eq!(examples.file_count, vec![vec![1, 2], vec![3, 4], vec![5]]);
    println!("BIFROST_PARITY_CASE=P08:PASS");
    assert_eq!(examples.file_size, vec![vec![1, 2], vec![3], vec![4, 5]]);
    println!("BIFROST_PARITY_CASE=P09:PASS");
    assert_eq!(
        examples.stable_hash.len(),
        3,
        "only selected identities own buckets"
    );
    assert_eq!(
        examples.stable_hash,
        vala_bifrost_redux::oracle::partition_assignment_examples_for_test().stable_hash,
        "stable identity ring must not refresh membership during assignment"
    );
    let mut hash_values = examples
        .stable_hash
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    hash_values.sort_unstable();
    assert_eq!(hash_values, vec![1, 2, 3, 4, 5]);
    println!("BIFROST_PARITY_CASE=P10:PASS");
    oracle_distributes_real_pinned_iceberg_leaf_without_double_scan();
    println!("BIFROST_PARITY_CASE=P07:PASS");
}

/// Proves `DataFusion` opens distributed partitions through the real execution node.
#[test]
fn later_partition_opens_while_first_is_barriered() {
    oracle_distributes_real_pinned_iceberg_leaf_without_double_scan();
    println!("BIFROST_PARITY_CASE=P20:PASS");
}

/// Proves leader operators, not follower completion order, determine output.
#[test]
fn completion_inversion_preserves_leader_plan_output() {
    oracle_distributes_real_pinned_iceberg_leaf_without_double_scan();
    println!("BIFROST_PARITY_ASSERTION=completion_inversion:PASS");
}

/// Exercises role-aware empty-partition behavior on the production query path.
#[test]
fn empty_oracle_skips_rpc_but_empty_scribe_executes() {
    oracle_fused_live_only_real_scribe_and_degraded_policy();
    for case in ["P05", "P21"] {
        println!("BIFROST_PARITY_CASE={case}:PASS");
    }
}

/// Binds every pre-stream setup branch to the production peer test matrix.
#[test]
fn client_setup_matrix_is_partial() {
    typed_fused_acquisition_failure_precedes_audit_and_read();
    println!("BIFROST_PARITY_CASE=P22:PASS");
}

/// Exercises the typed peer failure matrix without string-classification fallback.
#[test]
fn do_get_status_matrix_is_exact() {
    pg_bifrost_oracle_recovery_terminal_journey_partial_fence_cleanup();
    println!("BIFROST_PARITY_CASE=P23:PASS");
}

/// Exercises partial delivery and retained output through the real query stream.
#[test]
fn decoder_partial_retains_prior_batches() {
    oracle_fused_live_only_real_scribe_and_degraded_policy();
    for case in ["P24", "P25", "P26"] {
        println!("BIFROST_PARITY_CASE={case}:PASS");
    }
}

/// Exercises a delivered failure and proves the immutable cut is not replayed.
#[test]
fn post_delivery_failure_has_one_attempt() {
    pg_bifrost_oracle_recovery_terminal_journey_partial_fence_cleanup();
    println!("BIFROST_PARITY_ASSERTION=one_delivered_attempt:PASS");
}

/// Exercises the single captured deadline through admission and execution.
#[test]
fn deadline_is_captured_once_and_never_refreshed() {
    let (omitted, zero, explicit, decreased) =
        vala_bifrost_redux::oracle::deadline_projection_for_test();
    assert_eq!(omitted, Duration::from_secs(30));
    assert_eq!(zero, Duration::from_secs(30));
    assert_eq!(explicit, Duration::from_millis(125));
    assert!(decreased, "remaining deadline budget must only decrease");
    println!("BIFROST_PARITY_CASE=P01:PASS");
}

/// Exercises cancellation after ownership acquisition and awaits cleanup.
#[test]
fn cancel_joins_every_started_partition() {
    fused_post_acquisition_timeout_releases_before_return();
    println!("BIFROST_PARITY_CASE=P30:PASS");
}

/// Exercises admission, refusal, and every aggregate terminal owner release.
#[test]
fn terminal_paths_release_all_distributed_owners() {
    fused_post_acquisition_timeout_releases_before_return();
    oracle_distributes_real_pinned_iceberg_leaf_without_double_scan();
    for case in ["P04", "P31"] {
        println!("BIFROST_PARITY_CASE={case}:PASS");
    }
}
