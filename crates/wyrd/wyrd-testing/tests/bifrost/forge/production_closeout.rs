//! Public, cross-pod qualification of production Forge geometry and cleanup.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::public_support::{
    JourneyTable, ManagedRow, append_values, canonical_order, enable_compaction, read_managed_rows,
    register_table, set_table_properties, tenant_client, unique_table,
};
use arrow::array::{BinaryBuilder, Int64Array, RecordBatch, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use iceberg::spec::DataFile;
use iceberg::transaction::{ApplyTransactionAction, Transaction};
use rand::{RngCore, SeedableRng, rngs::StdRng};
use uuid::Uuid;
use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef, TenantTableBinding};
use vala_bifrost_redux::forge::{
    ForgeCommitNotice, ForgeCompactionDispatch, ForgeCompactionOutcome, ForgeCompactionType,
    ForgeConfig, ForgeError, ForgeHeldTerm, ForgeLifecycleEvent, ForgeTableKey, ForgeTableSettings,
    ForgeWorkerCompletionObserver,
};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::resources::{ResourceSource, SystemResourceSnapshot};
use vala_bifrost_redux::storage::{StorageOperation, StorageOperationBarrier};
use vala_sql::row_types::forge_tasks::{ForgeTaskStrategy, evidence_from_json};
use wyrd_client::WyrdClient;
use wyrd_server::config::BifrostRuntimeRole;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::NodeId;
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::write::RawIngest;
use wyrd_testing::bifrost::{
    BifrostClusterSpec, CommitUncertaintyCatalog, TestOracleResources, WyrdTestCluster,
};

/// Every Oracle-serving node's ranged-read pause, armed before a lazy public read.
///
/// The query's leader commits its active table read before any object IO, so
/// stalling the production storage owner at the first ranged read holds the
/// query at a point where that read is already durable and observable,
/// without adding any production seam.
struct OracleReadBarriers {
    /// One barrier per Oracle-serving node, paired with that node's identity.
    entries: Vec<(NodeId, Arc<StorageOperationBarrier>)>,
}

impl OracleReadBarriers {
    /// Installs one `ReadRange` barrier on every Oracle-serving node.
    ///
    /// # Panics
    /// Panics if no composed node owns both an Oracle and a storage owner.
    fn arm(cluster: &WyrdTestCluster) -> Self {
        let mut entries = Vec::new();
        for server in cluster.servers() {
            if server.state().bifrost.oracle().is_none() {
                continue;
            }
            let Some(storage) = server.state().bifrost_storage() else {
                continue;
            };
            let barrier = StorageOperationBarrier::new(StorageOperation::ReadRange);
            storage.install_operation_barrier_for_test(Arc::clone(&barrier));
            entries.push((server.node_id(), barrier));
        }
        assert!(
            !entries.is_empty(),
            "at least one Oracle node must be armed"
        );
        Self { entries }
    }

    /// Waits for the first node to stall a ranged read and releases the rest.
    ///
    /// Releasing the others is what keeps concurrent maintenance on unrelated
    /// pods running while exactly the reading node stays held.
    async fn first_reached(&self) -> NodeId {
        let reached =
            futures_util::future::select_all(self.entries.iter().map(|(node, barrier)| {
                Box::pin(async move {
                    barrier.wait_until_reached().await;
                    *node
                })
            }))
            .await
            .0;
        for (node, barrier) in &self.entries {
            if *node != reached {
                barrier.release();
            }
        }
        reached
    }

    /// Releases every remaining stall so the held query can finish.
    fn release(&self) {
        for (_, barrier) in &self.entries {
            barrier.release();
        }
    }
}

impl Drop for OracleReadBarriers {
    /// Releases every stall so an early exit cannot leave a query held.
    fn drop(&mut self) {
        self.release();
    }
}

/// Diagnostic limit for a scheduler pass; never the production ticker interval.
const PASS_BOUND: Duration = Duration::from_secs(15);
/// Production-sized rewrites may spend several minutes encoding physical bytes.
const REWRITE_BOUND: Duration = Duration::from_mins(10);
/// Maintenance passes driven while a reader is held; each must defer every
/// destructive route, so several passes prove the deferral is not one-shot.
const HELD_READER_PASSES: usize = 3;
/// Environment flag selecting the standard production geometry profile.
///
/// Journey-only: it changes nothing but the sizes this test's own setup
/// declares, so both modes run the identical body, workload, and assertions.
const PRODUCTION_GEOMETRY_FLAG: &str = "WYRD_FORGE_PRODUCTION_GEOMETRY";

/// Physical sizes one qualification mode declares for Scribe and Iceberg.
///
/// The proof is geometric — target rolling, exactly one residue, multi-row-group
/// outputs, replanned backlog — and every one of those properties is a ratio
/// rather than an absolute size. Scaling all of the sizes together therefore
/// keeps the assertions meaningful while letting the ordinary lane finish in
/// seconds; the production profile runs the same test at the standard sizes.
#[derive(Clone, Copy)]
struct GeometryProfile {
    /// Rows one public append request carries.
    rows_per_request: usize,
    /// Requests one flushed round issues.
    requests_per_flush: usize,
    /// Scribe assembled-object target the staging geometry rolls at.
    scribe_target_bytes: u64,
    /// Iceberg rolling target the table declares.
    iceberg_target_bytes: u64,
    /// Parquet row-group target, always below the file target so a rolled
    /// output necessarily contains more than one group.
    row_group_bytes: u64,
    /// Whether the qualification sizing of the worker and Oracle pods applies.
    production_resources: bool,
}

impl GeometryProfile {
    /// Scaled default: the same geometry two orders of magnitude smaller.
    const FAST: Self = Self {
        // One round carries the same multiple of the target as production
        // does, but in fewer, larger requests: a staging claim closes on the
        // smallest member prefix whose bytes reach the target, and the merged
        // object re-encodes a fraction below that sum. Coarse members keep the
        // crossing object above the target instead of a fraction under it.
        rows_per_request: 940,
        requests_per_flush: 6,
        scribe_target_bytes: 4 * 1024 * 1024,
        iceberg_target_bytes: 8 * 1024 * 1024,
        row_group_bytes: 1024 * 1024,
        production_resources: false,
    };

    /// Standard production sizing, selected only by the focused lane.
    const PRODUCTION: Self = Self {
        rows_per_request: 12_000,
        requests_per_flush: 52,
        scribe_target_bytes: 512 * 1024 * 1024,
        iceberg_target_bytes: 1024 * 1024 * 1024,
        row_group_bytes: 128 * 1024 * 1024,
        production_resources: true,
    };

    /// Reads the profile this process runs, defaulting to the fast one.
    fn selected() -> Self {
        if std::env::var_os(PRODUCTION_GEOMETRY_FLAG).is_some() {
            Self::PRODUCTION
        } else {
            Self::FAST
        }
    }
}
/// Incompressible payload width makes physical targets measurable.
const PAYLOAD_BYTES: usize = 1024;

/// Owns the real servers and passive completion evidence for one journey.
struct CloseoutJourney {
    /// Shared Postgres/storage with independent serving and maintenance nodes.
    cluster: WyrdTestCluster,
    /// Notification source emitted after production worker outcomes.
    observer: ForgeWorkerCompletionObserver,
    /// Stable identity of the initially delayed coordinator.
    coordinator_node: NodeId,
    /// Stable identity of the independent ingest process.
    scribe_node: NodeId,
    /// Stable identity of the independent query process.
    oracle_node: NodeId,
    /// Stable identity of the dedicated maintenance worker.
    worker_node: NodeId,
}

impl CloseoutJourney {
    /// Starts dedicated ingest, query, and worker nodes, retaining a delayed coordinator.
    ///
    /// # Panics
    /// Panics if the production topology cannot start or lacks its observer.
    async fn start() -> Self {
        Self::start_with_config(ForgeConfig::default()).await
    }

    /// Starts the same role topology with the journey's maintenance policy.
    ///
    /// # Panics
    /// Panics if the real role graph cannot start or its observer is absent.
    async fn start_with_config(config: ForgeConfig) -> Self {
        let profile = GeometryProfile::selected();
        // The one worker drains only the journey's tables, so the tenant's audit
        // table cannot take a slot a geometry or competing-table plan is owed.
        let mut spec = BifrostClusterSpec::dedicated_forge_workers().with_scribe_geometry_for_test(
            vala_bifrost_redux::scribe::geometry::ScribeGeometry::default()
                .with_staging_target_file_size_bytes(profile.scribe_target_bytes)
                .expect("the selected staging target is a valid geometry"),
        );
        let coordinator_node = spec.nodes[3].node_id;
        let scribe_node = spec.nodes[0].node_id;
        let oracle_node = spec.nodes[2].node_id;
        let worker_node = spec.nodes[1].node_id;
        let scribe_roles = spec.nodes[0].roles.clone();
        spec.nodes[3].roles = scribe_roles;
        spec.nodes[0].roles = [BifrostRuntimeRole::Scribe].into_iter().collect();
        spec.nodes[2].roles = [BifrostRuntimeRole::Oracle].into_iter().collect();
        // Only the qualification profile needs an oversized worker pod: its
        // 512 MiB inputs decode into a working set that would spill heavily
        // under an ordinary pod's shared cap. The scaled default runs on the
        // harness default snapshot.
        if profile.production_resources {
            spec.nodes[1].oracle = Some(TestOracleResources {
                data_root_parent: None,
                system_resources: Some(SystemResourceSnapshot {
                    memory_limit_bytes: 32 * 1024 * 1024 * 1024,
                    effective_cpu: 4,
                    scratch_capacity_bytes: 4 * 1024 * 1024 * 1024,
                    scratch_available_bytes: 4 * 1024 * 1024 * 1024,
                    memory_source: ResourceSource::Injected,
                    cpu_source: ResourceSource::Injected,
                }),
                oracle_query_slot_limit: None,
            });
        }
        if !profile.production_resources {
            // The scaled default runs the same journey on the resource plan
            // every node reports for itself: no injected snapshot anywhere. Each Oracle now derives its own local
            // capacity, so two replicas no longer have to agree on one durable
            // ceiling.
            for node in &spec.nodes {
                assert!(
                    node.oracle
                        .as_ref()
                        .is_none_or(|oracle| oracle.system_resources.is_none()),
                    "the fast profile injects no node resource snapshot"
                );
            }
        }
        let cluster = WyrdTestCluster::start_spec_with_forge_config_and_completion_observer(
            spec, config, true, true,
        )
        .await
        .expect("separate production roles start");
        let observer = cluster
            .forge_completion_observer()
            .expect("worker observer");
        assert_eq!(cluster.configured_node_ids().len(), 4);
        eprintln!(
            "closeout role/node identities: {:?}",
            cluster.configured_node_ids()
        );
        Self {
            cluster,
            observer,
            coordinator_node,
            scribe_node,
            oracle_node,
            worker_node,
        }
    }

    /// Advances only the retained deterministic maintenance clocks.
    ///
    /// # Panics
    /// Panics if the requested test time is unrepresentable.
    fn advance_maintenance(&self, duration: chrono::Duration) {
        for server in self.cluster.servers() {
            server
                .forge_clock()
                .advance(duration)
                .expect("maintenance time advances");
        }
    }

    /// Promotes owed debt, then waits for a pulled rewrite of this table's head.
    ///
    /// One scheduler pass promotes owed Scribe debt; its commit makes the table
    /// due on the leader, and a worker pulls it on its own interval. Progress
    /// is observed through settled attempts, never sleeps, until a snapshot
    /// newer than the one on entry names only Forge outputs.
    ///
    /// # Panics
    /// Panics if no such rewrite publishes within the rewrite bound, or on any
    /// worker or SQL failure observed while draining.
    async fn compact(&self, binding: &TenantTableBinding) -> (i64, BTreeMap<String, DataFile>) {
        let (entry, _) = self.live_files(binding).await;
        self.scheduler_pass().await;
        let current = tokio::time::timeout(REWRITE_BOUND, async {
            loop {
                let next = self.observer.attempts() + 1;
                let current = self.live_files(binding).await;
                if current.0 != entry && current.1.keys().all(|path| path.contains("/data/forge/"))
                {
                    return current;
                }
                self.observer.wait_for_attempts_at_least(next).await;
            }
        })
        .await
        .expect("a pulled rewrite publishes the table's current head");
        self.drain_tasks().await;
        current
    }

    /// Compacts this small fixed-hour test table into exactly one Forge file.
    ///
    /// # Panics
    /// Panics if [`Self::compact`] panics or the rewrite leaves more than one file.
    async fn compact_small_table(
        &self,
        binding: &TenantTableBinding,
    ) -> (i64, BTreeMap<String, DataFile>) {
        let current = self.compact(binding).await;
        assert_eq!(
            current.1.len(),
            1,
            "one partition rewrites into one small file"
        );
        current
    }

    /// Requires a named object to be physically absent, rejecting other IO errors.
    ///
    /// # Panics
    /// Panics on a storage error other than absence.
    async fn object_missing(&self, path: &str) -> bool {
        match self.cluster.storage_operator().stat(path).await {
            Ok(_) => false,
            Err(error) if error.kind() == opendal::ErrorKind::NotFound => true,
            Err(error) => panic!("object inspection failed: {error}"),
        }
    }

    /// Observes an exact production expired-cleanup delete with protection held.
    ///
    /// The elected leader's maintenance pass executes expired cleanup on its
    /// own executor, so the delete is paused on the leader node's real object
    /// store. The post-delete pause permits inspection of its still-prepared
    /// durable claim before settlement; no SQL transaction spans the storage
    /// effect.
    ///
    /// # Panics
    /// Panics if no node holds the leader term, deletion stalls, has the wrong
    /// route, loses a protected object, or lacks its terminal settlement.
    async fn collect_exact(
        &self,
        binding: &TenantTableBinding,
        path: &str,
        protected: &BTreeMap<String, DataFile>,
    ) {
        let leader = self.leader();
        let control = leader
            .forge_object_store_control_for_test()
            .expect("real storage control");
        control.pause_after_delete_for_path(path);
        let drive = async {
            for _ in 0..12 {
                self.maintenance_pass().await;
                self.drain_tasks().await;
                if self.object_missing(path).await {
                    return;
                }
            }
            panic!("production cleanup did not delete {path}");
        };
        let inspect = async {
            tokio::time::timeout(PASS_BOUND, control.wait_for_completed_delete())
                .await
                .expect("named object is actually deleted");
            assert!(self.object_missing(path).await);
            self.assert_objects(protected).await;
            let mut conn = self
                .coordinator()
                .tenant_conn_for(binding.tenant)
                .await
                .expect("tenant inspection");
            let rows: Vec<(Uuid, serde_json::Value)> = sqlx::query_as(
                "SELECT t.task_id, t.evidence FROM vala.forge_tasks t \
                 JOIN vala.forge_tasks s ON s.task_id=(t.plan->'parameters'->>'source_task_id')::uuid \
                 WHERE t.strategy='expired_cleanup' AND t.state='prepared' \
                 AND s.strategy='snapshot_expiry' AND s.state='succeeded'",
            ).fetch_all(&mut **conn.transaction()).await.expect("prepared cleanup claim");
            conn.commit().await.expect("inspection releases SQL");
            let matching: Vec<_> = rows
                .into_iter()
                .filter_map(|(task, raw)| {
                    let evidence = evidence_from_json(raw).expect("validated cleanup evidence");
                    let index = usize::try_from(evidence.prepared_candidate_index?)
                        .expect("candidate index");
                    (evidence.cleanup_candidates[index].path.as_str() == path).then_some(task)
                })
                .collect();
            assert_eq!(
                matching.len(),
                1,
                "exact expired-cleanup candidate owns the delete"
            );
            control.release_completed_delete();
            matching[0]
        };
        let ((), task) = tokio::join!(drive, inspect);
        let mut conn = self
            .coordinator()
            .tenant_conn_for(binding.tenant)
            .await
            .expect("tenant lineage");
        // Cleanup evaluates no principal permission, so its own task evidence —
        // not audit — counts the deletions it made.
        let deleted: i64 = sqlx::query_scalar(
            "SELECT COALESCE((evidence->>'deleted_candidate_count')::bigint, 0) \
             FROM vala.forge_tasks WHERE task_id=$1",
        )
        .bind(task)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("cleanup deletion lineage");
        conn.commit()
            .await
            .expect("lineage inspection releases SQL");
        assert!(deleted > 0);
        eprintln!(
            "expired cleanup task={task}, leader={}, physically deleted={path}",
            leader.node_id().as_uuid()
        );
    }

    /// Lists every physical object under this table's Forge output root.
    ///
    /// The listing is the same real prefix the collection route walks, so the
    /// difference between two listings is exactly what a rewrite closed, with
    /// no fixture ever naming an object on the writer's behalf.
    ///
    /// # Panics
    /// Panics when the storage listing fails.
    async fn forge_objects(&self, binding: &TenantTableBinding) -> BTreeSet<String> {
        let prefix = format!("{}/data/forge/", binding.object_prefix.as_str());
        self.cluster
            .storage_operator()
            .list_with(&prefix)
            .recursive(true)
            .await
            .expect("Forge output listing")
            .into_iter()
            .filter(|entry| entry.metadata().is_file())
            .map(|entry| entry.path().to_owned())
            .collect()
    }

    /// Asserts the coordinator's production orphan predicate for each object.
    ///
    /// The verdict comes from the retained protection loader on a node that is
    /// not the one executing the work, which is what makes it observable while
    /// a worker holds a catalog commit open.
    ///
    /// # Panics
    /// Panics when the production classifier fails or returns another verdict.
    async fn assert_eligibility(
        &self,
        binding: &TenantTableBinding,
        paths: &BTreeSet<String>,
        expected: &str,
        why: &str,
    ) {
        for path in paths {
            assert_eq!(
                self.coordinator()
                    .forge_gc_eligibility_for_test(binding, path)
                    .await
                    .expect("production eligibility classification"),
                expected,
                "{why}: {path}"
            );
        }
    }

    /// Corroborates collection with its own lineage operation identity.
    ///
    /// Forge evaluates no principal permission, so `vala.forge_operation_state`
    /// — not audit — is the authority this reads: the route owns an
    /// `orphan_gc` operation that reached a terminal phase.
    ///
    /// # Panics
    /// Panics when the route wrote no operation, its operation never settled,
    /// or it reported no physical deletion. Independently eligible sibling
    /// expiry or cleanup is permitted and is not evidence against it.
    async fn assert_orphan_evidence(&self, tenant: DataTenantId) {
        let mut conn = self
            .coordinator()
            .tenant_conn_for(tenant)
            .await
            .expect("tenant-scoped lineage inspection");
        let rows: Vec<(Uuid, String)> = sqlx::query_as(
            "SELECT operation_id, phase \
             FROM vala.forge_operation_state \
             WHERE family='orphan_gc'",
        )
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("orphan collection lineage evidence");
        conn.commit()
            .await
            .expect("read-only lineage inspection completes");
        assert!(
            !rows.is_empty(),
            "the collection route owns its own lineage operation identity"
        );
        for (operation, phase) in &rows {
            assert!(
                phase == "committed" || phase == "recovered",
                "collection left operation {operation} in phase {phase}"
            );
            eprintln!("orphan collection operation {operation}: {phase}");
        }
        let metrics = self
            .cluster
            .telemetry()
            .snapshot()
            .expect("production metrics");
        assert!(
            metrics.iter().any(
                |sample| sample.family == "bifrost_forge_deleted_objects_total"
                    && sample.value > 0.0
            ),
            "collection reported no physical deletion"
        );
        eprintln!(
            "collection worker identities={:?}",
            self.observer.completed_workers()
        );
    }

    /// Registers the payload schema through the retained server catalog harness.
    ///
    /// # Panics
    /// Panics if registration or tenant-qualified identity validation fails.
    async fn register_payload_table(&self, tenant: DataTenantId, name: String) -> JourneyTable {
        let table_ref = TableRef::new(BifrostNamespace::Datasets, &name);
        self.scribe()
            .create_bifrost_table_for_test(CreateTableRequest {
                table: table_ref.clone(),
                tenant,
                user_fields: vec![
                    Field::new("value", DataType::Int64, false),
                    Field::new("payload", DataType::Binary, false),
                ],
                physical_layout: None,
            })
            .await
            .expect("payload table registration");
        let binding = TenantTableBinding::resolve((tenant, table_ref)).expect("table binding");
        enable_compaction(self.scribe(), &binding).await;
        JourneyTable {
            qualified: format!("{}.{name}", BifrostNamespace::Datasets.as_str()),
            name,
            binding,
        }
    }

    /// Declares the qualification's 1 GiB policy before any data is written.
    ///
    /// This configures the real Iceberg table, whose undeclared dependency
    /// default is 512 MiB; it does not replace a planner or execute maintenance.
    ///
    /// # Panics
    /// Panics if catalog configuration fails or does not preserve the declared target.
    async fn declare_geometry(&self, binding: &TenantTableBinding) {
        let profile = GeometryProfile::selected();
        let catalog = self.scribe().bifrost_catalog().iceberg_catalog();
        let table = catalog
            .load_table(&binding.table_ident())
            .await
            .expect("qualification table");
        let tx = Transaction::new(&table);
        let tx = tx
            .update_table_properties()
            .set(
                "write.target-file-size-bytes".to_owned(),
                profile.iceberg_target_bytes.to_string(),
            )
            // Below the file target in both modes, so every output that rolled
            // at the target necessarily closed more than one row group.
            .set(
                "write.parquet.row-group-size-bytes".to_owned(),
                profile.row_group_bytes.to_string(),
            )
            .apply(tx)
            .expect("declared table geometry");
        let table = tx
            .commit_once(catalog.as_ref())
            .await
            .expect("table policy commits");
        assert_eq!(
            table
                .metadata()
                .properties()
                .get("write.target-file-size-bytes")
                .map(String::as_str),
            Some(profile.iceberg_target_bytes.to_string().as_str())
        );
    }

    /// Reads back the rolling target the table itself declares, in bytes.
    ///
    /// Geometry is judged against the published property rather than a
    /// hard-coded figure, so the assertions describe the policy the writer was
    /// actually given rather than a size the test happens to expect.
    ///
    /// # Panics
    /// Panics when the table is unreadable or declares no parsable target.
    async fn declared_target_bytes(&self, binding: &TenantTableBinding) -> u64 {
        self.scribe()
            .bifrost_catalog()
            .iceberg_catalog()
            .load_table(&binding.table_ident())
            .await
            .expect("geometry table")
            .metadata()
            .properties()
            .get("write.target-file-size-bytes")
            .expect("declared rolling target")
            .parse()
            .expect("declared rolling target is a byte count")
    }

    /// Borrows the live coordinator node.
    ///
    /// # Panics
    /// Panics if the retained node is absent.
    fn coordinator(&self) -> &WyrdTestServer {
        self.cluster
            .server_by_node(self.coordinator_node)
            .expect("coordinator node")
    }

    /// Borrows the dedicated Scribe serving node.
    ///
    /// # Panics
    /// Panics if the ingest node is absent.
    fn scribe(&self) -> &WyrdTestServer {
        self.cluster
            .server_by_node(self.scribe_node)
            .expect("Scribe node")
    }

    /// Borrows the separate Oracle serving node.
    ///
    /// # Panics
    /// Panics if the retained node is absent.
    fn oracle(&self) -> &WyrdTestServer {
        self.cluster
            .server_by_node(self.oracle_node)
            .expect("Oracle node")
    }

    /// Drains tasks while one deliberately abandoned rewrite is expected.
    ///
    /// The ordinary drain treats any returned worker error as a defect. One
    /// scenario needs a real attempt to be refused after it closed its outputs
    /// and before it prepared, so this variant accepts exactly the refusal that
    /// scenario injects at the publication reacquisition it drives, and still
    /// refuses every other worker error. The observer keeps every error it ever
    /// saw, so every drain after that injection has to use this variant.
    ///
    /// # Panics
    /// Panics on SQL failure, a stalled attempt, or any other worker error.
    async fn drain_tasks_allowing_injected_refusal(&self) {
        tokio::time::timeout(REWRITE_BOUND, async {
            loop {
                for error in self.observer.returned_errors() {
                    assert!(
                        error.contains("injected Forge catalog load failure"),
                        "worker failed for an unexpected reason: {error}"
                    );
                }
                let next = self.observer.attempts() + 1;
                let (pending, attempts): (i64, i64) = sqlx::query_as(
                    "SELECT count(*) FILTER (WHERE state NOT IN \
                     ('succeeded', 'failed', 'cancelled')), \
                     coalesce(sum(attempt_count), 0)::bigint FROM vala.forge_tasks",
                )
                .fetch_one(self.cluster.pg_fixture().operator_pool().pool())
                .await
                .expect("durable task and ownership inspection");
                if pending == 0
                    && self.observer.attempts()
                        >= usize::try_from(attempts).expect("nonnegative attempts")
                {
                    break;
                }
                self.observer.wait_for_attempts_at_least(next).await;
            }
        })
        .await
        .expect("the abandoned rewrite and its retry both settle");
    }

    /// Waits until the leader no longer owes `binding`'s table a compaction.
    ///
    /// A refused dispatch closes its row and leaves the retry to the leader,
    /// which re-dispatches the table on a later worker pull; until that retry
    /// settles the debt, planning admits no orphan cleanup for the table.
    ///
    /// # Panics
    /// Panics if the debt outlives the rewrite bound or a worker fails for an
    /// unexpected reason.
    async fn settle_owed_compaction(&self, binding: &TenantTableBinding) {
        let key = ForgeTableKey {
            tenant: binding.tenant,
            table: vala_sql::row_types::forge_tasks::ForgeTaskTableIdentity::new(
                vala_bifrost_redux::catalog::BIFROST_CATALOG_NAME,
                binding.logical_namespace.as_str(),
                binding.table_ref.name.as_str(),
            )
            .expect("table identity"),
        };
        let owed = || {
            self.coordinator()
                .state()
                .forge_coordinator()
                .and_then(|forge| forge.held_leader_term())
                .is_some_and(|term| term.schedule().owes_compaction(&key))
        };
        tokio::time::timeout(REWRITE_BOUND, async {
            while owed() {
                let next = self.observer.attempts() + 1;
                self.observer.wait_for_attempts_at_least(next).await;
                self.drain_tasks_allowing_injected_refusal().await;
            }
        })
        .await
        .expect("the leader's retry settles the refused table's debt");
    }

    /// Settles the coordinator's boot maintenance pass, then runs its first
    /// leader heartbeat.
    ///
    /// The maintenance timer ticks immediately on start, so a fixture that
    /// drives its own passes must settle that boot pass before arranging the
    /// world; otherwise it runs concurrently with the first driven pass. The
    /// first heartbeat waits a full period, so one driven heartbeat then takes
    /// the leader term and promotes owed Scribe debt before the journey reads
    /// the table's published snapshot.
    ///
    /// # Panics
    /// Panics if either pass does not complete within its bound.
    async fn await_boot_pass(&self) {
        tokio::time::timeout(
            PASS_BOUND,
            self.coordinator()
                .wait_for_forge_scheduler_passes_for_test(1),
        )
        .await
        .expect("a freshly started coordinator completes its boot maintenance pass");
        self.scheduler_pass().await;
    }

    /// Returns the running node whose Forge coordinator holds the leader term.
    ///
    /// # Panics
    /// Panics unless exactly one running node holds a term.
    fn leader(&self) -> &WyrdTestServer {
        let mut leaders = self.cluster.servers().filter(|server| {
            server
                .state()
                .forge_coordinator()
                .and_then(|forge| forge.held_leader_term())
                .is_some()
        });
        let leader = leaders
            .next()
            .expect("one node holds the Forge leader term");
        assert!(leaders.next().is_none(), "exactly one Forge leader");
        leader
    }

    /// Requests and observes one leader maintenance pass on every coordinator.
    ///
    /// Only the term holder runs manifest rewrite, expiry, and cleanup; every
    /// other coordinator skips the tick but still reports it, so waiting on
    /// each proves the leader's pass returned without guessing which leads.
    /// Nodes without a Forge coordinator run no maintenance loop.
    ///
    /// # Panics
    /// Panics if any node does not complete the requested pass in time.
    async fn maintenance_pass(&self) {
        let pending: Vec<_> = self
            .cluster
            .servers()
            .filter(|server| server.state().forge_coordinator().is_some())
            .map(|server| {
                let before = server.completed_forge_scheduler_passes_for_test();
                server.request_forge_maintenance_pass_for_test();
                (server, before)
            })
            .collect();
        for (server, before) in pending {
            tokio::time::timeout(
                PASS_BOUND,
                server.wait_for_forge_scheduler_passes_for_test(before + 1),
            )
            .await
            .expect("production maintenance responds to its trigger");
        }
    }

    /// Requests and observes one real scheduler pass before inspecting SQL.
    ///
    /// # Panics
    /// Panics if the supervisor does not complete the requested pass in 15 seconds.
    async fn scheduler_pass(&self) {
        let server = self.coordinator();
        let before = server.completed_forge_scheduler_passes_for_test();
        let started = Instant::now();
        server.request_forge_scheduler_pass_for_test();
        tokio::time::timeout(
            PASS_BOUND,
            server.wait_for_forge_scheduler_passes_for_test(before + 1),
        )
        .await
        .expect("production scheduler responds to its trigger within 15 seconds");
        eprintln!(
            "scheduler passes {before} -> {} in {:?}",
            server.completed_forge_scheduler_passes_for_test(),
            started.elapsed()
        );
    }

    /// Drains already-created tasks using completion notifications and durable state.
    ///
    /// # Panics
    /// Panics on SQL failure, a stalled attempt, or a production worker error.
    async fn drain_tasks(&self) {
        tokio::time::timeout(REWRITE_BOUND, async {
            loop {
                assert!(
                    self.observer.returned_errors().is_empty(),
                    "worker failed: {:?}",
                    self.observer.returned_errors()
                );
                let next = self.observer.attempts() + 1;
                let (pending, attempts): (i64, i64) = sqlx::query_as(
                    "SELECT count(*) FILTER (WHERE state NOT IN \
                     ('succeeded', 'failed', 'cancelled')), \
                     coalesce(sum(attempt_count), 0)::bigint FROM vala.forge_tasks",
                )
                .fetch_one(self.cluster.pg_fixture().operator_pool().pool())
                .await
                .expect("durable task and ownership inspection");
                if pending == 0
                    && self.observer.attempts()
                        >= usize::try_from(attempts).expect("nonnegative attempts")
                {
                    break;
                }
                self.observer.wait_for_attempts_at_least(next).await;
            }
        })
        .await
        .unwrap_or_else(|_| {
            panic!(
                "worker drain stalled: {:?}",
                self.observer.returned_errors()
            )
        });
        assert!(
            self.observer.returned_errors().is_empty(),
            "{:?}",
            self.observer.returned_errors()
        );
    }

    /// Reads exact live files from the current catalog snapshot's manifests.
    ///
    /// # Panics
    /// Panics on missing catalog authority, unreadable manifests, or duplicate files.
    async fn live_files(&self, binding: &TenantTableBinding) -> (i64, BTreeMap<String, DataFile>) {
        let table = self
            .coordinator()
            .bifrost_catalog()
            .iceberg_catalog()
            .load_table(&binding.table_ident())
            .await
            .expect("catalog table");
        let snapshot = table
            .metadata()
            .current_snapshot()
            .expect("published snapshot");
        let manifests = table
            .manifest_list_reader(snapshot)
            .load()
            .await
            .expect("manifest list");
        let mut files = BTreeMap::new();
        for manifest in manifests.entries() {
            let manifest = manifest
                .load_manifest(table.file_io())
                .await
                .expect("manifest");
            for entry in manifest.entries().iter().filter(|entry| entry.is_alive()) {
                let file = entry.data_file();
                let start = file
                    .file_path()
                    .find(binding.object_prefix.as_str())
                    .expect("file belongs to this tenant/table prefix");
                assert!(
                    files
                        .insert(file.file_path()[start..].to_owned(), file.clone())
                        .is_none()
                );
            }
        }
        (snapshot.snapshot_id(), files)
    }

    /// Counts the snapshots the table currently retains.
    ///
    /// Publication is per plan, so the retained snapshot count — not the task
    /// count — is what an attempt's operations have to line up against.
    ///
    /// # Panics
    /// Panics when the catalog cannot load the table.
    async fn snapshot_count(&self, binding: &TenantTableBinding) -> usize {
        self.coordinator()
            .bifrost_catalog()
            .iceberg_catalog()
            .load_table(&binding.table_ident())
            .await
            .expect("catalog table")
            .metadata()
            .snapshots()
            .count()
    }

    /// Corroborates every completed rewrite of one table with its lineage and
    /// metrics.
    ///
    /// Only `binding`'s own operations are read: the tenant's audit table is
    /// maintained alongside it and settles rewrites of its own.
    ///
    /// An attempt publishes each of its admitted plans independently, so a
    /// completed rewrite settles *one operation per plan*, not one per task —
    /// and a task whose planning finds nothing to rewrite self-settles with
    /// none. The task count therefore bounds nothing; each plan holds its own
    /// operation in `vala.forge_operation_state`, settled into a terminal
    /// phase. Forge evaluates no principal permission, so that projection — not
    /// audit — is the authority read here.
    ///
    /// Returns how many operations that evidence covers, so the caller can hold
    /// it against the snapshots the same passes published.
    ///
    /// # Panics
    /// Panics on unsettled operation evidence, on two plans sharing one
    /// operation identity, or on missing physical data-flow counters.
    async fn assert_rewrite_evidence(&self, binding: &TenantTableBinding) -> usize {
        let tenant = binding.tenant;
        let mut conn = self
            .coordinator()
            .tenant_conn_for(tenant)
            .await
            .expect("tenant-scoped lineage inspection");
        let rows: Vec<(Uuid, String, serde_json::Value)> = sqlx::query_as(
            "SELECT operation_id, phase, prepared_detail \
             FROM vala.forge_operation_state \
             WHERE family='iceberg_rewrite' AND resource = $1",
        )
        .bind(format!(
            "bifrost://{}/{}/{}",
            binding.tenant, binding.table_ref.namespace, binding.table_ref.name
        ))
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("rewrite lineage evidence");
        conn.commit()
            .await
            .expect("read-only lineage inspection completes");
        assert_eq!(
            rows.iter()
                .map(|(operation, ..)| *operation)
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            rows.len(),
            "no two published plans settled under one operation identity"
        );
        let operations = rows.len();
        for (operation, phase, detail) in rows {
            assert_eq!(
                phase, "committed",
                "rewrite operation {operation} did not settle"
            );
            eprintln!("rewrite {operation}, phase {phase}, fenced preparation {detail}");
        }
        let metrics = self
            .cluster
            .telemetry()
            .snapshot()
            .expect("production metrics");
        for family in [
            "bifrost_forge_input_files_total",
            "bifrost_forge_input_bytes_total",
            "bifrost_forge_output_files_total",
            "bifrost_forge_output_bytes_total",
        ] {
            assert!(
                metrics.iter().any(|sample| sample.family == family
                    && sample.value > 0.0
                    && sample
                        .labels
                        .get("task_type")
                        .is_some_and(|kind| kind == "small_files")),
                "{family}"
            );
        }
        eprintln!("worker identities={:?}", self.observer.completed_workers());
        operations
    }

    /// Proves each named object still physically exists at its committed size.
    ///
    /// # Panics
    /// Panics if an object disappeared or its physical size differs from Iceberg.
    async fn assert_objects(&self, files: &BTreeMap<String, DataFile>) {
        for (path, file) in files {
            let metadata = self
                .cluster
                .storage_operator()
                .stat(path)
                .await
                .expect("retained object");
            assert_eq!(metadata.content_length(), file.file_size_in_bytes());
        }
    }
}

/// Owns bounded, deterministic incompressible data and its acknowledged row set.
struct GeometryWorkload {
    /// Pinned event time keeps every request in one physical partition.
    event_time: chrono::DateTime<chrono::Utc>,
    /// Installed standard generator avoids a separate random-byte implementation.
    random: StdRng,
    /// Exact acknowledged managed identities and payload values.
    expected: Vec<ManagedRow>,
    /// Sizes this process runs the qualification at.
    profile: GeometryProfile,
}

impl GeometryWorkload {
    /// Starts one reproducible workload with no acknowledged rows.
    fn new() -> Self {
        Self {
            event_time: chrono::Utc::now(),
            random: StdRng::seed_from_u64(0x5eed),
            expected: Vec::new(),
            profile: GeometryProfile::selected(),
        }
    }

    /// Appends one target-sized round through authenticated public gRPC.
    ///
    /// # Panics
    /// Panics on malformed local Arrow data or any refused public append.
    async fn append_round(&mut self, client: &WyrdClient, table: &str) {
        let transport = RawIngest::connect(client).await.expect("ingest transport");
        for _ in 0..self.profile.requests_per_flush {
            let first = i64::try_from(self.expected.len()).expect("bounded row count");
            let values: Vec<i64> = (first
                ..first + i64::try_from(self.profile.rows_per_request).expect("bounded request"))
                .collect();
            self.append_batch(&transport, table, &values).await;
        }
    }

    /// Appends exact values with the workload's fixed event-time partition.
    ///
    /// # Panics
    /// Panics on malformed Arrow data or a refused public append.
    async fn append_batch(&mut self, transport: &RawIngest, table: &str, values: &[i64]) {
        let batch = self.batch(values);
        let mut ipc = Vec::new();
        {
            let mut writer =
                arrow::ipc::writer::StreamWriter::try_new(&mut ipc, batch.schema().as_ref())
                    .expect("IPC writer");
            writer.write(&batch).expect("IPC batch");
            writer.finish().expect("IPC terminal");
        }
        let request_id = transport
            .insert(table, Uuid::now_v7(), ipc)
            .await
            .expect("public append acknowledged");
        let request_id = Uuid::parse_str(request_id.as_str()).expect("request identity is a UUID");
        self.expected.extend(values.iter().map(|value| ManagedRow {
            request_id,
            value: *value,
        }));
    }

    /// Builds the next bounded batch without changing production object geometry.
    ///
    /// # Panics
    /// Panics if the locally constructed arrays disagree with their schema.
    fn batch(&mut self, values: &[i64]) -> RecordBatch {
        let mut payload = BinaryBuilder::with_capacity(values.len(), values.len() * PAYLOAD_BYTES);
        let mut block = [0_u8; PAYLOAD_BYTES];
        for _ in values {
            self.random.fill_bytes(&mut block);
            payload.append_value(block);
        }
        RecordBatch::try_new(
            Arc::new(Schema::new(vec![
                Field::new("value", DataType::Int64, false),
                Field::new("payload", DataType::Binary, false),
                Field::new(
                    "wyrd_event_time",
                    DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                    false,
                ),
            ])),
            vec![
                Arc::new(Int64Array::from(values.to_vec())),
                Arc::new(payload.finish()),
                Arc::new(
                    TimestampMicrosecondArray::from(vec![
                        self.event_time.timestamp_micros();
                        values.len()
                    ])
                    .with_timezone("UTC"),
                ),
            ],
        )
        .expect("payload batch")
    }
}

/// Proves the rewrite's balanced outputs are well-formed replacements.
///
/// The full-table rewrite plans one task per partition and writes it through
/// `max_output_parallelism` balanced writers, as RisingWave does, so outputs
/// are split evenly rather than rolled at the target. Each output must stay at
/// or under the declared target and report ascending row groups inside its
/// physical file.
///
/// # Panics
/// Panics when no output exists, an output exceeds the declared target, or a
/// row group escapes its physical file.
fn assert_output_geometry(outputs: &BTreeMap<String, DataFile>, rows: usize, target: u64) {
    let output_sizes: Vec<_> = outputs.values().map(DataFile::file_size_in_bytes).collect();
    eprintln!("Forge physical bytes: {output_sizes:?}; exact rows: {rows}");
    assert!(!outputs.is_empty(), "the rewrite publishes outputs");
    for (path, file) in outputs {
        assert!(
            file.file_size_in_bytes() <= target,
            "{path} stays within the declared target {target}: {output_sizes:?}"
        );
        let offsets = file.split_offsets().expect("row-group offsets");
        assert!(!offsets.is_empty(), "{path} reports its row groups");
        assert!(
            offsets.windows(2).all(|pair| pair[0] < pair[1]),
            "{path} row-group offsets ascend: {offsets:?}"
        );
        assert!(
            offsets.iter().all(|offset| {
                u64::try_from(*offset).is_ok_and(|offset| offset < file.file_size_in_bytes())
            }),
            "{path} row groups stay inside the physical file"
        );
    }
}

/// Qualifies real 512 MiB Scribe inputs and balanced Forge outputs within the 1 GiB target.
///
/// Public exact rows, manifest membership, old-object presence and a second
/// unchanged pass jointly distinguish publication from destructive cleanup.
///
/// # Panics
/// Panics on any route, geometry, exactness, tenancy, or convergence violation.
#[tokio::test]
#[ignore = "requires Postgres and production-sized object storage"]
#[expect(
    clippy::float_cmp,
    reason = "Prometheus renders this gauge as whole numbers, so f64 equality is exact"
)]
async fn compaction_geometry_exact_rows_and_non_destructive_second_pass() {
    let mut journey = CloseoutJourney::start().await;
    let tenant = journey.cluster.data_tenant_id();
    let neighbour = journey
        .cluster
        .add_tenant(&unique_table("geometry_neighbour"))
        .await
        .expect("second tenant");
    let name = unique_table("geometry");
    let table = journey.register_payload_table(tenant, name).await;
    let neighbour_table = register_table(journey.scribe(), neighbour, &table.name).await;
    enable_compaction(journey.scribe(), &neighbour_table.binding).await;
    journey.declare_geometry(&table.binding).await;
    journey.declare_geometry(&neighbour_table.binding).await;
    let target = journey.declared_target_bytes(&table.binding).await;
    let writer = tenant_client(journey.scribe(), tenant).await;
    let reader = tenant_client(journey.oracle(), tenant).await;
    let neighbour_writer = tenant_client(journey.scribe(), neighbour).await;
    let neighbour_reader = tenant_client(journey.oracle(), neighbour).await;
    // The neighbour is written in separate flushed rounds so it owns several
    // hot objects of its own. That gives it real maintenance demand, and its
    // tasks then compete for the same worker as the geometry table's plans
    // rather than sitting idle beside them.
    let mut neighbour_rows = Vec::new();
    for values in [[9001, 9002, 9003], [9004, 9005, 9006], [9007, 9008, 9009]] {
        neighbour_rows.extend(
            append_values(
                &neighbour_writer,
                &neighbour_table.qualified,
                Uuid::now_v7(),
                &values,
            )
            .await,
        );
        journey
            .scribe()
            .flush_bifrost()
            .await
            .expect("the competing tenant's rows flush");
    }
    let neighbour_expected = canonical_order(neighbour_rows);
    let mut workload = GeometryWorkload::new();
    for round in 0..2 {
        let started = Instant::now();
        workload.append_round(&writer, &table.qualified).await;
        journey
            .scribe()
            .flush_bifrost()
            .await
            .expect("publicly acknowledged rows flush");
        eprintln!(
            "geometry input round {round} flushed in {:?}",
            started.elapsed()
        );
    }
    let hot = journey
        .scribe()
        .published_hot_files_for_test(tenant, BifrostNamespace::Datasets.as_str(), &table.name)
        .await
        .expect("physical Scribe inputs");
    let sizes: Vec<_> = hot.iter().map(|file| file.file_size).collect();
    eprintln!("Scribe physical bytes: {sizes:?}");
    assert!(
        hot.iter()
            .filter(|file| file.file_size >= workload.profile.scribe_target_bytes)
            .count()
            >= 2,
        "multiple inputs at the selected staging target required: {sizes:?}"
    );
    workload.expected = canonical_order(workload.expected);
    assert_eq!(
        read_managed_rows(&reader, &table.qualified).await,
        workload.expected
    );
    assert_eq!(
        read_managed_rows(&neighbour_reader, &table.qualified).await,
        neighbour_expected
    );
    journey
        .cluster
        .restart_node(journey.coordinator_node)
        .await
        .expect("coordinator starts after complete ingestion");
    // The new leader promotes all owed debt inside its boot pass, so the
    // promoted cut is read before any driven pass can rewrite it.
    journey.await_boot_pass().await;
    let (promoted_snapshot, inputs) = journey.live_files(&table.binding).await;
    assert_eq!(
        inputs.len(),
        hot.len(),
        "promotion retains every physical Scribe object"
    );
    let (neighbour_promoted, _) = journey.live_files(&neighbour_table.binding).await;
    let published_before = journey.snapshot_count(&table.binding).await;
    for server in journey.cluster.servers().iter() {
        server
            .forge_clock()
            .advance(chrono::Duration::days(1))
            .expect("closed partition");
    }
    // One dispatched full rewrite covers the closed partition's whole head.
    let (replacement_snapshot, outputs) = journey.compact(&table.binding).await;
    assert_ne!(replacement_snapshot, promoted_snapshot);
    let (neighbour_snapshot, neighbour_files) = journey.live_files(&neighbour_table.binding).await;
    assert_ne!(
        neighbour_snapshot, neighbour_promoted,
        "the competing table's own maintenance drained through the same worker \
         as the geometry table's plans"
    );
    assert!(
        outputs.keys().all(|path| !inputs.contains_key(path)),
        "new cuts use replacements"
    );
    assert_output_geometry(&outputs, workload.expected.len(), target);
    journey.assert_objects(&inputs).await;
    journey.assert_objects(&outputs).await;
    // Pause the follower at its first ranged read: the query's active table
    // read must already be durable there, because acquisition commits it
    // before any object IO.
    let barriers = OracleReadBarriers::arm(&journey.cluster);
    let paused_read = read_managed_rows(&reader, &table.qualified);
    let inspect_protection = async {
        let reader_node = tokio::time::timeout(PASS_BOUND, barriers.first_reached())
            .await
            .expect("lazy query reaches its first ranged read");
        let active = journey
            .cluster
            .server_by_node(reader_node)
            .expect("the stalled reader is a composed node")
            .oracle_active_table_reads_for_test(&table.binding)
            .await
            .expect("active read inspection");
        assert_eq!(active, 1, "the held query owns one active table read");
        barriers.release();
    };
    let (actual, ()) = tokio::join!(paused_read, inspect_protection);
    assert_eq!(actual, workload.expected);
    let rewrites = journey
        .observer
        .completed_strategies()
        .iter()
        .filter(|strategy| {
            **strategy
                == vala_sql::row_types::forge_tasks::ForgeClaimStrategy::Known(
                    ForgeTaskStrategy::SmallFiles,
                )
        })
        .count();
    assert!(rewrites > 0);
    journey.scheduler_pass().await;
    journey.drain_tasks().await;
    let (second_snapshot, second_files) = journey.live_files(&table.binding).await;
    assert_eq!(
        second_snapshot, replacement_snapshot,
        "unchanged pass publishes no snapshot"
    );
    assert_eq!(
        second_files.keys().collect::<Vec<_>>(),
        outputs.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        journey.live_files(&neighbour_table.binding).await.0,
        neighbour_snapshot
    );
    journey.assert_objects(&neighbour_files).await;
    assert_eq!(
        read_managed_rows(&neighbour_reader, &table.qualified).await,
        neighbour_expected
    );
    journey.assert_objects(&inputs).await;
    // Each admitted plan publishes on its own, so the rewrite passes owe one
    // operation per snapshot they added — not one per completed task.
    let operations = journey.assert_rewrite_evidence(&table.binding).await;
    assert_eq!(
        operations,
        journey.snapshot_count(&table.binding).await - published_before,
        "every snapshot the rewrite passes published carries its own operation"
    );
    // Ownership is judged once every role has drained, when every attempt
    // guard must have returned its increment.
    let telemetry = journey.cluster.telemetry().clone();
    journey.cluster.shutdown().await.expect("all roles drain");
    for sample in telemetry
        .snapshot()
        .expect("production metrics")
        .iter()
        .filter(|sample| sample.family == "bifrost_forge_active_tasks")
    {
        assert_eq!(sample.value, 0.0, "settled ownership: {sample:?}");
    }
}

/// Owns two tenants and the real old cut held across destructive maintenance.
struct ReaderCleanupJourney {
    /// Independent serving and maintenance nodes.
    roles: CloseoutJourney,
    /// Fixed-hour data whose successive snapshots the reader protects.
    table: JourneyTable,
    /// Same public name in a different tenant.
    neighbour_table: JourneyTable,
    /// Authenticated public query client on the dedicated Oracle node.
    reader: WyrdClient,
    /// Authenticated neighbor query client on that same public endpoint.
    neighbour_reader: WyrdClient,
    /// Public ingest transport retained across successive appends.
    transport: RawIngest,
    /// Fixed partition, deterministic payload and exact acknowledged identities.
    workload: GeometryWorkload,
    /// Neighbor identities that must remain unchanged throughout cleanup.
    neighbour_expected: Vec<ManagedRow>,
    /// Snapshot selected before the paused query starts.
    old_snapshot: i64,
    /// Exact data files the paused query must retain.
    old_files: BTreeMap<String, DataFile>,
    /// Earlier replaced data object collected only after the reader releases.
    earlier_object: String,
}

impl ReaderCleanupJourney {
    /// Writes two real hot files, then publishes and compacts the reader's cut.
    ///
    /// # Panics
    /// Panics if public setup, promotion, or compaction fails.
    async fn start() -> Self {
        let mut roles = CloseoutJourney::start_with_config(ForgeConfig::default()).await;
        let tenant = roles.cluster.data_tenant_id();
        let neighbour = roles
            .cluster
            .add_tenant(&unique_table("reader_neighbour"))
            .await
            .expect("neighbor tenant");
        let table = roles
            .register_payload_table(tenant, unique_table("retained_reader"))
            .await;
        let neighbour_table = register_table(roles.scribe(), neighbour, &table.name).await;
        enable_compaction(roles.scribe(), &neighbour_table.binding).await;
        let writer = tenant_client(roles.scribe(), tenant).await;
        let reader = tenant_client(roles.oracle(), tenant).await;
        let neighbour_writer = tenant_client(roles.scribe(), neighbour).await;
        let neighbour_reader = tenant_client(roles.oracle(), neighbour).await;
        let transport = RawIngest::connect(&writer).await.expect("public ingest");
        let neighbour_expected = canonical_order(
            append_values(
                &neighbour_writer,
                &table.qualified,
                Uuid::now_v7(),
                &[9001, 9002],
            )
            .await,
        );
        let mut workload = GeometryWorkload::new();
        for values in [&[1, 2][..], &[3, 4]] {
            workload
                .append_batch(&transport, &table.qualified, values)
                .await;
            roles
                .scribe()
                .flush_bifrost()
                .await
                .expect("acknowledged inputs flush");
        }
        roles
            .cluster
            .restart_node(roles.coordinator_node)
            .await
            .expect("coordinator starts");
        // The boot pass promotes the owed debt, so the promoted inputs are
        // read before any driven pass can rewrite them.
        roles.await_boot_pass().await;
        let (_, earlier_files) = roles.live_files(&table.binding).await;
        assert!(earlier_files.len() >= 2, "two real promoted inputs");
        roles.advance_maintenance(chrono::Duration::hours(2));
        let earlier_object = earlier_files.keys().next().expect("earlier object").clone();
        let (old_snapshot, old_files) = roles.compact_small_table(&table.binding).await;
        assert!(!old_files.contains_key(&earlier_object));
        roles.assert_objects(&earlier_files).await;
        assert_eq!(
            read_managed_rows(&reader, &table.qualified).await,
            canonical_order(workload.expected.clone())
        );
        Self {
            roles,
            table,
            neighbour_table,
            reader,
            neighbour_reader,
            transport,
            workload,
            neighbour_expected,
            old_snapshot,
            old_files,
            earlier_object,
        }
    }

    /// Waits for the terminal query to release its active table read.
    ///
    /// # Panics
    /// Panics if the read survives the pass bound or inspection fails.
    async fn wait_for_reader_release(&self) {
        tokio::time::timeout(PASS_BOUND, async {
            loop {
                let active = self
                    .roles
                    .coordinator()
                    .oracle_active_table_reads_for_test(&self.table.binding)
                    .await
                    .expect("active read inspection");
                if active == 0 {
                    return;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("terminal query releases its active table read");
    }

    /// Holds a public reader across a rewrite and destructive passes, then
    /// collects the earlier replaced object once the reader releases.
    ///
    /// While the query owns its active table read, another pod still rewrites
    /// the table, but every maintenance pass defers expiry and cleanup, so the
    /// earlier object, the reader's files, and its snapshot all survive.
    ///
    /// # Panics
    /// Panics if the query was not protected, a held pass deletes anything, its
    /// terminal rows change, or the earlier object is not collected after
    /// release.
    async fn protect_during_cleanup(&mut self) {
        let expected = canonical_order(self.workload.expected.clone());
        let barriers = OracleReadBarriers::arm(&self.roles.cluster);
        let query = read_managed_rows(&self.reader, &self.table.qualified);
        let maintenance = async {
            let reader_node = tokio::time::timeout(PASS_BOUND, barriers.first_reached())
                .await
                .expect("lazy query reaches its first ranged read");
            assert_ne!(reader_node, self.roles.worker_node);
            let active = self
                .roles
                .cluster
                .server_by_node(reader_node)
                .expect("the stalled reader is a composed node")
                .oracle_active_table_reads_for_test(&self.table.binding)
                .await
                .expect("active read inspection");
            assert_eq!(active, 1, "the held query owns one active table read");
            self.workload
                .append_batch(&self.transport, &self.table.qualified, &[5, 6])
                .await;
            self.roles
                .scribe()
                .flush_bifrost()
                .await
                .expect("new rows flush");
            let (current, _) = self.roles.compact_small_table(&self.table.binding).await;
            assert_ne!(current, self.old_snapshot);
            for _ in 0..HELD_READER_PASSES {
                self.roles.maintenance_pass().await;
                self.roles.drain_tasks().await;
            }
            assert!(
                !self.roles.object_missing(&self.earlier_object).await,
                "an active table read defers every destructive pass"
            );
            self.roles.assert_objects(&self.old_files).await;
            let metadata = self
                .roles
                .coordinator()
                .bifrost_catalog()
                .iceberg_catalog()
                .load_table(&self.table.binding.table_ident())
                .await
                .expect("retained snapshot inspection");
            assert!(
                metadata
                    .metadata()
                    .snapshot_by_id(self.old_snapshot)
                    .is_some()
            );
            barriers.release();
        };
        let (actual, ()) = tokio::join!(query, maintenance);
        assert_eq!(
            actual, expected,
            "old reader returns exactly its protected cut"
        );
        self.wait_for_reader_release().await;
        // With the last reader gone the earlier object is collected without an
        // age wait; the reader's own files stay live through the head's
        // rewrite lineage until `finish` moves the head on.
        self.roles
            .collect_exact(&self.table.binding, &self.earlier_object, &self.old_files)
            .await;
    }

    /// Moves the rewrite head onward and observes exact old-object deletion.
    ///
    /// # Panics
    /// Panics if released inputs cannot be collected, current or neighbor rows
    /// change, or production roles fail to drain.
    async fn finish(mut self) {
        self.workload
            .append_batch(&self.transport, &self.table.qualified, &[7, 8])
            .await;
        self.roles
            .scribe()
            .flush_bifrost()
            .await
            .expect("later rows flush");
        let (_, current) = self.roles.compact_small_table(&self.table.binding).await;
        self.roles.advance_maintenance(chrono::Duration::days(2));
        let old_path = self.old_files.keys().next().expect("old compacted object");
        self.roles
            .collect_exact(&self.table.binding, old_path, &current)
            .await;
        for path in self.old_files.keys() {
            assert!(
                self.roles.object_missing(path).await,
                "released old object is deleted"
            );
        }
        assert_eq!(
            read_managed_rows(&self.reader, &self.table.qualified).await,
            canonical_order(self.workload.expected)
        );
        assert_eq!(
            read_managed_rows(&self.neighbour_reader, &self.neighbour_table.qualified).await,
            self.neighbour_expected
        );
        self.roles.assert_objects(&current).await;
        self.roles
            .cluster
            .shutdown()
            .await
            .expect("all production roles drain");
    }
}

/// An old public reader defers cross-pod expiration and physical cleanup.
///
/// # Panics
/// Panics when durable protection, exact deletion ownership, or tenant rows fail.
#[tokio::test]
#[ignore = "requires Postgres and independent Oracle/Forge roles"]
async fn lazy_old_reader_survives_cross_pod_expiration_and_physical_cleanup() {
    let mut journey = ReaderCleanupJourney::start().await;
    journey.protect_during_cleanup().await;
    journey.finish().await;
}

/// Owns two tenants and the real output one refused publication leaves behind.
struct OrphanJourney {
    /// Independent serving and maintenance nodes.
    roles: CloseoutJourney,
    /// Table whose rewrite is refused at the real catalog boundary.
    table: JourneyTable,
    /// Same public name in a different tenant.
    neighbour_table: JourneyTable,
    /// Authenticated public query client on the dedicated Oracle node.
    reader: WyrdClient,
    /// Authenticated neighbor query client on that same public endpoint.
    neighbour_reader: WyrdClient,
    /// Public ingest transport retained across successive appends.
    transport: RawIngest,
    /// Fixed partition, deterministic payload and exact acknowledged identities.
    workload: GeometryWorkload,
    /// Neighbor identities that must remain unchanged throughout collection.
    neighbour_expected: Vec<ManagedRow>,
    /// Shared control over the real Forge catalog publication boundary.
    catalog: Arc<CommitUncertaintyCatalog>,
    /// Exact objects the refused rewrite closed and never published.
    orphans: BTreeSet<String>,
}

impl OrphanJourney {
    /// Publishes a real compacted cut, then refuses the next rewrite's commit.
    ///
    /// # Panics
    /// Panics if public setup, promotion, or the refused publication does not
    /// leave at least one closed output that no snapshot names.
    async fn start() -> Self {
        // The leader's collection pass ages objects against the Forge clock it
        // reads when the pass runs, and this journey never advances that clock,
        // so the terminal floor has to be a real interval this journey can
        // outlive. Two seconds is long enough that the object is provably
        // young while its own rewrite is still open and short enough that the
        // bounded collection loop below crosses it.
        let mut roles = CloseoutJourney::start_with_config(ForgeConfig {
            orphan_gc_ttl: std::time::Duration::from_secs(2),
            ..ForgeConfig::default()
        })
        .await;
        let catalog = roles
            .cluster
            .commit_uncertainty_catalog()
            .expect("the topology wraps the real Forge catalog boundary");
        let tenant = roles.cluster.data_tenant_id();
        let neighbour = roles
            .cluster
            .add_tenant(&unique_table("orphan_neighbour"))
            .await
            .expect("neighbor tenant");
        let table = roles
            .register_payload_table(tenant, unique_table("never_published"))
            .await;
        // Collection must be proved by difference: it removes the tracked
        // generation and nothing else. Snapshot expiry would legitimately
        // delete replaced outputs once the clock advances, so this table opts
        // out of it and keeps leader-maintenance membership through manifest
        // rewrite, which deletes no data object.
        set_table_properties(
            roles.scribe(),
            &table.binding,
            &[
                ("wyrd.forge.enable-snapshot-expiration", "false"),
                ("wyrd.forge.enable-manifest-rewrite", "true"),
            ],
        )
        .await;
        let neighbour_table = register_table(roles.scribe(), neighbour, &table.name).await;
        enable_compaction(roles.scribe(), &neighbour_table.binding).await;
        let writer = tenant_client(roles.scribe(), tenant).await;
        let reader = tenant_client(roles.oracle(), tenant).await;
        let neighbour_writer = tenant_client(roles.scribe(), neighbour).await;
        let neighbour_reader = tenant_client(roles.oracle(), neighbour).await;
        let transport = RawIngest::connect(&writer).await.expect("public ingest");
        let neighbour_expected = canonical_order(
            append_values(
                &neighbour_writer,
                &table.qualified,
                Uuid::now_v7(),
                &[7001, 7002],
            )
            .await,
        );
        let mut workload = GeometryWorkload::new();
        for values in [&[1, 2][..], &[3, 4]] {
            workload
                .append_batch(&transport, &table.qualified, values)
                .await;
            roles
                .scribe()
                .flush_bifrost()
                .await
                .expect("acknowledged inputs flush");
        }
        roles
            .cluster
            .restart_node(roles.coordinator_node)
            .await
            .expect("coordinator starts");
        roles.await_boot_pass().await;
        // No maintenance-clock advance here. This journey's terminal age floor
        // is a real interval measured by the leader's collection pass, so a
        // manual clock running ahead of storage would report every object as
        // already old and erase the young window the scenario has to observe.
        roles.compact_small_table(&table.binding).await;

        // One more acknowledged flush, promoted on its own pass. Promotion is a
        // catalog commit too, so it has to be finished and live before the seam
        // is armed; otherwise the held commit would be the promotion's, which
        // writes no Forge output and therefore strands nothing.
        workload
            .append_batch(&transport, &table.qualified, &[5, 6])
            .await;
        roles
            .scribe()
            .flush_bifrost()
            .await
            .expect("later rows flush");
        roles.scheduler_pass().await;
        roles.drain_tasks().await;
        let (_, promoted) = roles.live_files(&table.binding).await;
        assert!(
            promoted.len() >= 2,
            "the promoted input is live and a rewrite is now due"
        );

        Self::prove_commit_window_retention(&roles, &table, &catalog).await;

        // A further acknowledged flush, promoted on its own pass, leaves the
        // table owing one more real rewrite: the attempt that will be refused.
        workload
            .append_batch(&transport, &table.qualified, &[7, 8])
            .await;
        roles
            .scribe()
            .flush_bifrost()
            .await
            .expect("later rows flush");
        roles.scheduler_pass().await;
        roles.drain_tasks().await;
        let orphans = Self::strand_one_generation(&roles, &table, &catalog).await;
        Self {
            roles,
            table,
            neighbour_table,
            reader,
            neighbour_reader,
            transport,
            workload,
            neighbour_expected,
            catalog,
            orphans,
        }
    }

    /// Proves an open and a commit-uncertain rewrite both retain their outputs.
    ///
    /// Two production windows, observed from the coordinator because the worker
    /// is the process inside the catalog call: a rewrite whose operation row is
    /// prepared and whose commit has not been delegated, and one whose commit
    /// the catalog accepted but has not acknowledged. Collection must refuse
    /// the outputs in both, and neither verdict may come from a fixture.
    ///
    /// # Panics
    /// Panics if either window is never reached, the rewrite closed no output,
    /// or the production predicate does not protect it.
    async fn prove_commit_window_retention(
        roles: &CloseoutJourney,
        table: &JourneyTable,
        catalog: &Arc<CommitUncertaintyCatalog>,
    ) {
        let before = roles.forge_objects(&table.binding).await;
        catalog.pause_before_commit();
        let drive = async {
            roles.scheduler_pass().await;
            roles.drain_tasks().await;
        };
        let inspect = async {
            tokio::time::timeout(REWRITE_BOUND, catalog.wait_for_before_commit())
                .await
                .expect("a real rewrite reaches the catalog publication boundary");
            let open: BTreeSet<String> = roles
                .forge_objects(&table.binding)
                .await
                .difference(&before)
                .cloned()
                .collect();
            assert!(
                !open.is_empty(),
                "the held rewrite closed at least one real output"
            );
            roles
                .assert_eligibility(&table.binding, &open, "Protected", "open rewrite")
                .await;
            // Arm the post-acceptance hold before releasing this one, so the
            // retry's own commit cannot slip past the uncertain window.
            catalog.pause_after_commit();
            catalog.reject_paused_before_commit();
            tokio::time::timeout(REWRITE_BOUND, catalog.wait_for_commit())
                .await
                .expect("the retry's commit is held after the catalog accepted it");
            roles
                .assert_eligibility(&table.binding, &open, "Protected", "uncertain commit")
                .await;
            catalog.release_paused_commit();
        };
        tokio::join!(drive, inspect);
    }

    /// Refuses one real rewrite after it closed its outputs and before it prepared.
    ///
    /// The refusal is a production one. The rewrite is held at the point its
    /// managed execution is finished and its publication has not yet reacquired
    /// authoritative metadata, and that reacquisition is then made to fail. The
    /// objects the attempt already closed are therefore named by no snapshot,
    /// no operation row, and the retry runs under a
    /// new attempt identity that cannot reuse them. While the attempt is still
    /// open those same objects must still be retained, but the age floor is the
    /// only authority that can retain them: a protection root requires the
    /// prepared operation row this attempt never reaches. The retention is
    /// observed from the coordinator because the worker is the process holding
    /// the rewrite.
    ///
    /// # Panics
    /// Panics if the barrier is never reached, the refused rewrite closed no
    /// output, or the production predicate does not retain it.
    async fn strand_one_generation(
        roles: &CloseoutJourney,
        table: &JourneyTable,
        catalog: &Arc<CommitUncertaintyCatalog>,
    ) -> BTreeSet<String> {
        let before = roles.forge_objects(&table.binding).await;
        roles.observer.hold_after_next_rewrite_handoff_for_test();
        let drive = async {
            roles.scheduler_pass().await;
            roles.drain_tasks_allowing_injected_refusal().await;
            roles.settle_owed_compaction(&table.binding).await;
        };
        let inspect = async {
            tokio::time::timeout(
                REWRITE_BOUND,
                roles.observer.wait_for_held_rewrite_handoff_for_test(),
            )
            .await
            .expect("a real rewrite reaches its post-execution barrier");
            let orphans: BTreeSet<String> = roles
                .forge_objects(&table.binding)
                .await
                .difference(&before)
                .cloned()
                .collect();
            assert!(
                !orphans.is_empty(),
                "the held rewrite closed at least one real output"
            );
            // The attempt admits every eligible plan, and a sibling plan can
            // already have prepared and published while this one is held. Its
            // outputs are live and protected by that prepared operation, so the
            // stranded generation is the held plan's own outputs: the ones no
            // operation row names, retained by the age floor alone.
            // They must still be retained while the attempt is open. Which
            // authority answers is not fixed: the age floor retains an object
            // no operation names, and a sibling plan of the same attempt that
            // has already prepared blocks destructive maintenance for the whole
            // table until it settles. Either verdict is retention; only
            // `Eligible` would be a collectible live attempt's output.
            for path in &orphans {
                let verdict = roles
                    .coordinator()
                    .forge_gc_eligibility_for_test(&table.binding, path)
                    .await
                    .expect("the production orphan predicate answers");
                assert!(
                    verdict == "TooYoung" || verdict == "Protected",
                    "an open attempt's closed output must be retained: {path} => {verdict}"
                );
            }
            // Refuse the metadata reacquisition this rewrite performs next,
            // which is the last authority it consults before preparing.
            catalog.fail_next_load_table();
            roles.observer.release_held_rewrite_handoff_for_test();
            orphans
        };
        let ((), orphans) = tokio::join!(drive, inspect);

        // Sibling plans of the same attempt publish on their own operations, so
        // the stranded generation is the closed outputs no snapshot names once
        // the refusal has settled the attempt.
        let (_, live) = roles.live_files(&table.binding).await;
        let orphans: BTreeSet<String> = orphans
            .into_iter()
            .filter(|path| !live.contains_key(path))
            .collect();
        assert!(
            !orphans.is_empty(),
            "the refused rewrite left at least one output no snapshot names"
        );
        let named: BTreeSet<String> = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT prepared_detail FROM vala.forge_operation_state \
             WHERE data_tenant_id = $1 AND family = 'iceberg_rewrite'",
        )
        .bind(table.binding.tenant.as_uuid())
        .fetch_all(roles.cluster.pg_fixture().operator_pool().pool())
        .await
        .expect("Forge operation-state inspection")
        .iter()
        .flat_map(|detail| {
            detail
                .get("output_paths")
                .and_then(serde_json::Value::as_array)
                .cloned()
                .unwrap_or_default()
        })
        .filter_map(|path| path.as_str().map(str::to_owned))
        .collect();
        assert!(
            orphans
                .iter()
                .all(|path| !named.iter().any(|output| output.ends_with(path))),
            "the refused plan never prepared, so no operation row names its \
             outputs: {orphans:?} in {named:?}"
        );
        orphans
    }

    /// Crosses the terminal age floor and collects exactly the stranded output.
    ///
    /// # Panics
    /// Panics if collection removes a protected object, misses the orphan, or
    /// either tenant's exact rows or production evidence change.
    async fn finish(self) {
        self.roles
            .assert_eligibility(
                &self.table.binding,
                &self.orphans,
                "TooYoung",
                "young output",
            )
            .await;
        let (_, live) = self.roles.live_files(&self.table.binding).await;
        self.roles.advance_maintenance(chrono::Duration::days(2));
        // Deletion ownership is an equality, not a membership test: the pass
        // must remove the tracked generation and nothing else this table owns.
        let before_objects = self.roles.forge_objects(&self.table.binding).await;
        assert!(
            self.orphans.is_subset(&before_objects),
            "the tracked outputs are still present before collection"
        );
        self.roles
            .assert_eligibility(
                &self.table.binding,
                &self.orphans,
                "Eligible",
                "aged output",
            )
            .await;

        let before = self
            .roles
            .coordinator()
            .completed_forge_scheduler_passes_for_test();
        for _ in 0..12 {
            self.roles.maintenance_pass().await;
            self.roles.drain_tasks_allowing_injected_refusal().await;
            let mut remaining = false;
            for path in &self.orphans {
                remaining |= !self.roles.object_missing(path).await;
            }
            if !remaining {
                break;
            }
        }
        for path in &self.orphans {
            assert!(
                self.roles.object_missing(path).await,
                "the never-published output survived its own collection route: {path}"
            );
        }
        assert!(
            self.roles
                .coordinator()
                .completed_forge_scheduler_passes_for_test()
                > before,
            "collection ran through the leader maintenance trigger"
        );
        // Ownership is proved by difference, not by membership: every object
        // this table owned before the pass must survive it except the tracked
        // generation. The passes driven above can publish their own new
        // outputs, so the surviving set is a superset of that remainder rather
        // than equal to it; what may not happen is any other removal.
        let after_objects = self.roles.forge_objects(&self.table.binding).await;
        let survivors: BTreeSet<String> =
            before_objects.difference(&self.orphans).cloned().collect();
        assert!(
            survivors.is_subset(&after_objects),
            "collection removed an object outside the tracked generation: {:?}",
            survivors.difference(&after_objects).collect::<Vec<_>>()
        );
        assert!(
            after_objects.is_disjoint(&self.orphans),
            "the tracked generation survived its own collection route"
        );
        self.roles.assert_objects(&live).await;
        self.roles
            .assert_orphan_evidence(self.table.binding.tenant)
            .await;
        assert_eq!(
            read_managed_rows(&self.reader, &self.table.qualified).await,
            canonical_order(self.workload.expected.clone())
        );
        assert_eq!(
            read_managed_rows(&self.neighbour_reader, &self.neighbour_table.qualified).await,
            self.neighbour_expected
        );
        drop(self.transport);
        drop(self.catalog);
        self.roles
            .cluster
            .shutdown()
            .await
            .expect("all production roles drain");
    }
}

/// A real never-published Forge output is collected only after terminal age.
///
/// # Panics
/// Panics when protection, exact deletion ownership, or tenant rows fail.
#[tokio::test]
#[ignore = "requires Postgres and independent Oracle/Forge roles"]
async fn failed_never_published_output_is_collected_after_terminal_age() {
    let journey = OrphanJourney::start().await;
    journey.finish().await;
}

/// Two coordinators sharing one election row, composed only from cluster owners.
///
/// The fixture adds no replica runner, clock or election harness: every
/// coordinator is a real server node of the shared-Postgres cluster, passes
/// are the production loop's own, and leadership is read from each node's
/// coordinator.
struct LeaderJourney {
    /// Real nodes sharing Postgres, storage and the election row.
    cluster: WyrdTestCluster,
    /// Notification source for every executed Forge attempt in the cluster.
    observer: ForgeWorkerCompletionObserver,
    /// Tenant every table in the journey belongs to.
    tenant: DataTenantId,
}

impl LeaderJourney {
    /// Starts `spec` with the default Forge policy and a completion observer.
    ///
    /// `inject_uncertainty` routes every node's Forge catalog through the
    /// cluster's shared [`CommitUncertaintyCatalog`], which passes commits
    /// through unchanged until a scenario arms one of its holds.
    ///
    /// # Panics
    /// Panics if the cluster cannot start or composes no observer.
    async fn start(spec: BifrostClusterSpec, inject_uncertainty: bool) -> Self {
        let cluster = WyrdTestCluster::start_spec_with_forge_config_and_completion_observer(
            spec,
            ForgeConfig::default(),
            false,
            inject_uncertainty,
        )
        .await
        .expect("leader journey cluster starts");
        let observer = cluster
            .forge_completion_observer()
            .expect("completion observer");
        let tenant = cluster.data_tenant_id();
        Self {
            cluster,
            observer,
            tenant,
        }
    }

    /// Borrows one running node.
    ///
    /// # Panics
    /// Panics if the node is unknown or stopped.
    fn node(&self, node: NodeId) -> &WyrdTestServer {
        self.cluster.server_by_node(node).expect("running node")
    }

    /// Runs one production planning pass on `node` and waits for it.
    ///
    /// # Panics
    /// Panics if the pass does not complete within the diagnostic bound.
    async fn pass(&self, node: NodeId) {
        let server = self.node(node);
        let before = server.completed_forge_scheduler_passes_for_test();
        server.request_forge_scheduler_pass_for_test();
        tokio::time::timeout(
            PASS_BOUND,
            server.wait_for_forge_scheduler_passes_for_test(before + 1),
        )
        .await
        .expect("the production loop completes the requested pass");
    }

    /// Runs one leader maintenance tick on `node` and waits for it.
    ///
    /// The tick is the production timer's own pass; a node without the
    /// leader term returns from it without touching any table.
    ///
    /// # Panics
    /// Panics if the pass does not complete within the diagnostic bound.
    async fn maintain(&self, node: NodeId) {
        let server = self.node(node);
        let before = server.completed_forge_scheduler_passes_for_test();
        server.request_forge_maintenance_pass_for_test();
        tokio::time::timeout(
            PASS_BOUND,
            server.wait_for_forge_scheduler_passes_for_test(before + 1),
        )
        .await
        .expect("the production maintenance loop completes the requested tick");
    }

    /// Advances every running node's Forge clock by `duration`.
    ///
    /// # Panics
    /// Panics if the requested test time is unrepresentable.
    fn advance(&self, duration: chrono::Duration) {
        for server in self.cluster.servers() {
            server
                .forge_clock()
                .advance(duration)
                .expect("maintenance time advances");
        }
    }

    /// Returns the snapshot ids `table` currently retains.
    ///
    /// # Panics
    /// Panics if the catalog cannot load the table.
    async fn snapshots(&self, via: NodeId, table: &JourneyTable) -> BTreeSet<i64> {
        self.node(via)
            .bifrost_catalog()
            .iceberg_catalog()
            .load_table(&table.binding.table_ident())
            .await
            .expect("journey table")
            .metadata()
            .snapshots()
            .map(|snapshot| snapshot.snapshot_id())
            .collect()
    }

    /// Counts `table`'s durable expiry, expired-cleanup and orphan attempts.
    ///
    /// # Panics
    /// Panics when the read-only inspection fails.
    async fn maintenance_rows(&self, table: &JourneyTable) -> i64 {
        sqlx::query_scalar(
            "SELECT count(*) FROM vala.forge_tasks WHERE data_tenant_id = $1 \
             AND table_name = $2 AND strategy IN \
             ('snapshot_expiry', 'expired_cleanup', 'orphan_cleanup')",
        )
        .bind(self.tenant.as_uuid())
        .bind(&table.name)
        .fetch_one(self.cluster.pg_fixture().operator_pool().pool())
        .await
        .expect("forge_tasks inspection")
    }

    /// Whether `path` still exists in shared storage.
    ///
    /// # Panics
    /// Panics on a storage error other than absence.
    async fn exists(&self, path: &str) -> bool {
        match self.cluster.storage_operator().stat(path).await {
            Ok(_) => true,
            Err(error) if error.kind() == opendal::ErrorKind::NotFound => false,
            Err(error) => panic!("object inspection failed: {error}"),
        }
    }

    /// Classifies `path` with `via`'s production orphan predicate.
    ///
    /// # Panics
    /// Panics when the production classifier fails.
    async fn eligibility(&self, via: NodeId, table: &JourneyTable, path: &str) -> String {
        self.node(via)
            .forge_gc_eligibility_for_test(&table.binding, path)
            .await
            .expect("production eligibility classification")
    }

    /// Returns the leader term `node` holds, if any.
    fn held(&self, node: NodeId) -> Option<Arc<ForgeHeldTerm>> {
        self.node(node)
            .state()
            .forge_coordinator()
            .and_then(|forge| forge.held_leader_term())
    }

    /// Returns every running node that holds a leader term.
    fn leaders(&self) -> Vec<(NodeId, i64)> {
        self.cluster
            .servers()
            .filter_map(|server| {
                self.held(server.node_id())
                    .map(|term| (server.node_id(), term.fencing_token()))
            })
            .collect()
    }

    /// Registers a table that enables compaction and manifest rewriting.
    ///
    /// # Panics
    /// Panics if registration or the catalog property commit fails.
    async fn register_scheduled_table(&self, via: NodeId, prefix: &str) -> JourneyTable {
        self.register_table_with(via, prefix, &[]).await
    }

    /// Registers a scheduled table with additional Forge table properties.
    ///
    /// # Panics
    /// Panics if registration or the catalog property commit fails.
    async fn register_table_with(
        &self,
        via: NodeId,
        prefix: &str,
        properties: &[(&str, &str)],
    ) -> JourneyTable {
        let table = register_table(self.node(via), self.tenant, &unique_table(prefix)).await;
        let catalog = self.node(via).bifrost_catalog().iceberg_catalog();
        let loaded = catalog
            .load_table(&table.binding.table_ident())
            .await
            .expect("scheduled table");
        let tx = Transaction::new(&loaded);
        let tx = properties
            .iter()
            .fold(
                tx.update_table_properties()
                    .set("wyrd.forge.enable-compaction".to_owned(), "true".to_owned())
                    .set(
                        "wyrd.forge.enable-manifest-rewrite".to_owned(),
                        "true".to_owned(),
                    ),
                |update, (key, value)| update.set((*key).to_owned(), (*value).to_owned()),
            )
            .apply(tx)
            .expect("Forge table settings");
        tx.commit_once(catalog.as_ref())
            .await
            .expect("Forge table settings commit");
        table
    }

    /// Appends rows through `via`'s public ingest and seals them as hot objects.
    ///
    /// # Panics
    /// Panics if the append is refused or the Scribe flush fails.
    async fn write_hot(&self, via: NodeId, table: &JourneyTable, values: &[i64]) {
        let client = tenant_client(self.node(via), self.tenant).await;
        append_values(&client, &table.qualified, Uuid::now_v7(), values).await;
        self.node(via)
            .flush_bifrost()
            .await
            .expect("acknowledged rows seal as hot objects");
    }

    /// Counts the table's hot objects that still owe an Iceberg promotion.
    ///
    /// # Panics
    /// Panics when the read-only inspection fails.
    async fn unpromoted(&self, table: &JourneyTable) -> i64 {
        sqlx::query_scalar(
            "SELECT count(*) FROM vala.file_list WHERE data_tenant_id = $1 \
             AND table_name = $2 AND committed_snapshot_id IS NULL",
        )
        .bind(self.tenant.as_uuid())
        .bind(&table.name)
        .fetch_one(self.cluster.pg_fixture().operator_pool().pool())
        .await
        .expect("file_list inspection")
    }

    /// Waits until every hot object of `table` has been promoted.
    ///
    /// Each wait is one completed Forge attempt, never a timer.
    ///
    /// # Panics
    /// Panics if the bound elapses or an attempt returns an error.
    async fn await_promoted(&self, table: &JourneyTable) {
        tokio::time::timeout(PASS_BOUND, async {
            while self.unpromoted(table).await > 0 {
                let next = self.observer.attempts() + 1;
                self.observer.wait_for_attempts_at_least(next).await;
            }
        })
        .await
        .expect("hot objects are promoted by a coordinator");
        let failures = self.failures();
        assert!(failures.is_empty(), "{failures:?}");
    }

    /// Returns every attempt error except a stopping node's pre-effect
    /// [`ForgeError::Shutdown`].
    ///
    /// A node the journey stops seals the tenant's `audit_log` rows during its
    /// drain, so it can be promoting that table as it stops. The attempt it
    /// abandons before any durable effect is released for a successor, which
    /// is a clean stop rather than a failure.
    fn failures(&self) -> Vec<String> {
        let shutdown = ForgeError::Shutdown.to_string();
        self.observer
            .returned_errors()
            .into_iter()
            .filter(|error| *error != shutdown)
            .collect()
    }

    /// Builds the leader's key for one journey table.
    ///
    /// # Panics
    /// Panics if the journey table name is not a valid Forge table identity.
    fn key(&self, table: &JourneyTable) -> ForgeTableKey {
        self.key_in(BifrostNamespace::Datasets, &table.name)
    }

    /// Builds the leader's key for one table of the journey tenant.
    ///
    /// # Panics
    /// Panics if the name is not a valid Forge table identity.
    fn key_in(&self, namespace: BifrostNamespace, name: &str) -> ForgeTableKey {
        ForgeTableKey {
            tenant: self.tenant,
            table: vala_sql::row_types::forge_tasks::ForgeTaskTableIdentity::new(
                vala_bifrost_redux::catalog::BIFROST_CATALOG_NAME,
                namespace.as_str(),
                name,
            )
            .expect("table identity"),
        }
    }
}

/// One coordinator leads; its successor starts with an empty volatile schedule.
///
/// Mirrors RisingWave's meta election: only the elected node builds the
/// Iceberg compaction manager, a replacement is elected through the shared
/// SQL row, and the replacement's tracks and maintenance sets start empty.
///
/// # Panics
/// Panics when two coordinators lead, a commit is not counted by the leader,
/// or the successor inherits any pending count or maintenance membership.
#[tokio::test]
#[ignore = "requires Postgres and two coordinator replicas"]
async fn one_leader_failover_volatile_state() {
    let spec = BifrostClusterSpec::two_mixed();
    let (first, second) = (spec.nodes[0].node_id, spec.nodes[1].node_id);
    let mut journey = LeaderJourney::start(spec, false).await;
    journey.pass(first).await;
    journey.pass(second).await;
    let leaders = journey.leaders();
    assert_eq!(
        leaders.len(),
        1,
        "exactly one coordinator leads: {leaders:?}"
    );
    let (leader, first_token) = leaders[0];
    let standby = if leader == first { second } else { first };

    // A commit promoted on the standby reaches the leader over the peer route.
    let table = journey
        .register_scheduled_table(standby, "leader_failover")
        .await;
    journey.write_hot(standby, &table, &[1, 2, 3]).await;
    journey.await_promoted(&table).await;
    let key = journey.key(&table);
    let counted = journey
        .held(leader)
        .expect("leader still holds its term")
        .schedule()
        .track_for_test(&key)
        .expect("the leader tracks the promoted table");
    assert_eq!(
        counted.pending_commits, 1,
        "one promotion is one Iceberg commit"
    );
    assert_eq!(journey.held(standby).map(|term| term.fencing_token()), None);

    // Graceful stop resigns; the standby takes over on its next pass.
    let stopped = Instant::now();
    journey
        .cluster
        .stop_node(leader)
        .await
        .expect("leader stops");
    journey.pass(standby).await;
    let successor = journey.held(standby).expect("the standby takes over");
    eprintln!("Forge leader failover took {:?}", stopped.elapsed());
    assert!(
        successor.fencing_token() > first_token,
        "a new term is minted"
    );
    // The stopped leader drained its audit rows, so the successor may already
    // track the audit table from a commit it observed itself; nothing else.
    assert!(
        successor.schedule().track_for_test(&key).is_none(),
        "the successor inherits no pending count"
    );
    let audit = usize::from(
        successor
            .schedule()
            .track_for_test(&journey.key_in(BifrostNamespace::Audit, "audit_log"))
            .is_some(),
    );
    assert_eq!(
        successor.schedule().sizes_for_test(),
        (audit, audit, 0),
        "the successor starts with no inherited tracks or maintenance membership"
    );

    // The successor counts only commits it observes, through the local route.
    journey.write_hot(standby, &table, &[4, 5]).await;
    journey.await_promoted(&table).await;
    let recounted = successor
        .schedule()
        .track_for_test(&key)
        .expect("the successor tracks the table after a new commit");
    assert_eq!(recounted.pending_commits, 1, "no pending count is copied");

    // The restarted former leader is a standby while the successor's term lives.
    journey.cluster.restart_node(leader).await.expect("restart");
    journey.pass(leader).await;
    assert_eq!(
        journey.leaders(),
        vec![(standby, successor.fencing_token())]
    );
    journey.cluster.shutdown().await.expect("cluster drains");
}

/// Ends the live election term the way a lapsed lease does.
///
/// The row stays owned by the old term but is no longer live, which is
/// exactly what PostgreSQL holds once a renewal fails to land in time. The
/// replica that held it is not told; it must discover the loss itself.
///
/// # Panics
/// Panics when the election row cannot be updated.
async fn lapse_leader_term(journey: &LeaderJourney) {
    sqlx::query(
        "UPDATE vala.forge_scheduler_state SET expires_at = statement_timestamp() WHERE singleton",
    )
    .execute(journey.cluster.pg_fixture().operator_pool().pool())
    .await
    .expect("election row lapses");
}

/// Waits for `term` to be revoked by its own replica's renewal loop.
///
/// # Panics
/// Panics if the replica keeps the term past the diagnostic bound.
async fn await_revoked(term: &ForgeHeldTerm) {
    tokio::time::timeout(PASS_BOUND, term.revoked())
        .await
        .expect("the replaced term is revoked by its renewal loop");
}

/// A lost term is revoked while promotion and maintenance are parked.
///
/// Renewal runs beside hinted promotion, so a promotion held at the catalog
/// cannot keep a replaced term alive: the old replica discovers the loss on
/// its own heartbeat and refuses notify, pull and report under that term.
/// The same revocation stops a paused maintenance pass, which then performs
/// no cleanup or orphan effect, and the next leader settles what it left.
///
/// # Panics
/// Panics when a replaced term is not revoked, still serves a leader handler,
/// or its maintenance pass makes a durable effect after revocation, or when
/// the successor leaves an attempt unsettled.
#[tokio::test]
#[ignore = "requires Postgres and two coordinator replicas"]
async fn revoked_term_stops_promotion_dispatch_and_maintenance() {
    let spec = BifrostClusterSpec::two_mixed();
    let (first, second) = (spec.nodes[0].node_id, spec.nodes[1].node_id);
    let journey = LeaderJourney::start(spec, true).await;
    let catalog = journey
        .cluster
        .commit_uncertainty_catalog()
        .expect("the topology wraps the real Forge catalog");
    journey.pass(first).await;
    journey.pass(second).await;
    let (old_leader, _) = journey.leaders()[0];
    let successor = if old_leader == first { second } else { first };
    let forge = |node: NodeId| {
        Arc::clone(
            journey
                .node(node)
                .state()
                .forge_coordinator()
                .expect("coordinator"),
        )
    };

    // A hinted promotion on the leader is parked at its catalog commit.
    let table = journey
        .register_scheduled_table(old_leader, "revoked_promotion")
        .await;
    let old_term = journey.held(old_leader).expect("leader term");
    catalog.pause_before_commit();
    journey.write_hot(old_leader, &table, &[1, 2]).await;
    tokio::time::timeout(PASS_BOUND, catalog.wait_for_before_commit())
        .await
        .expect("the hinted promotion reaches the catalog");

    // The term lapses; the standby takes it, and the parked replica's own
    // renewal loop revokes the old term while the promotion is still held.
    lapse_leader_term(&journey).await;
    journey.pass(successor).await;
    let new_term = journey.held(successor).expect("the standby takes over");
    assert!(new_term.fencing_token() > old_term.fencing_token());
    await_revoked(&old_term).await;
    assert!(journey.held(old_leader).is_none());
    let old = forge(old_leader);
    let key = journey.key(&table);
    let refused = |result: Result<(), ForgeError>, handler: &str| {
        assert!(
            matches!(result, Err(ForgeError::FenceLost { .. })),
            "{handler} under a revoked term: {result:?}"
        );
    };
    refused(
        old.accept_commit_notice(
            old_term.fencing_token(),
            ForgeCommitNotice {
                key: key.clone(),
                snapshot_id: 1,
                settings: ForgeTableSettings::default(),
            },
        ),
        "notify",
    );
    refused(
        old.serve_compaction_pull(old_term.fencing_token(), 4)
            .map(|_| ()),
        "pull",
    );
    refused(
        old.serve_compaction_report(
            old_term.fencing_token(),
            &key,
            Uuid::now_v7(),
            ForgeCompactionOutcome::Failed,
        )
        .map(|_| ()),
        "report",
    );

    // Released, the promotion lands once and its notice reaches the successor.
    catalog.release_paused_before_commit();
    journey.await_promoted(&table).await;
    assert_eq!(
        new_term
            .schedule()
            .track_for_test(&key)
            .map(|track| track.pending_commits),
        Some(1),
        "the successor counts the parked promotion exactly once"
    );

    // A cold table the successor maintains: expiry is due and an aged
    // rowless output waits for the orphan sweep that follows it.
    let cold = register_table(
        journey.node(successor),
        journey.tenant,
        &unique_table("revoked_maintenance"),
    )
    .await;
    set_table_properties(
        journey.node(successor),
        &cold.binding,
        &[("wyrd.forge.enable-compaction", "false")],
    )
    .await;
    for values in [&[1, 2][..], &[3, 4]] {
        journey.write_hot(successor, &cold, values).await;
        journey.await_promoted(&cold).await;
    }
    let orphan = format!(
        "{}/data/forge/v2/{}-00000-{}.parquet",
        cold.binding.object_prefix.trim_end_matches('/'),
        Uuid::now_v7(),
        Uuid::now_v7()
    );
    journey
        .cluster
        .storage_operator()
        .write(&orphan, b"never published".to_vec())
        .await
        .expect("rowless output");
    journey.advance(chrono::Duration::days(2));
    assert_eq!(
        journey.eligibility(successor, &cold, &orphan).await,
        "Eligible"
    );

    // The pass is held after the catalog accepts the expiry; the term is
    // replaced and revoked there, and the pass then starts nothing more.
    let controls = forge(successor).expiry_controls_for_test();
    controls.arm_expiry_accepted();
    let server = journey.node(successor);
    let passes = server.completed_forge_scheduler_passes_for_test();
    server.request_forge_maintenance_pass_for_test();
    tokio::time::timeout(PASS_BOUND, controls.wait_expiry_accepted())
        .await
        .expect("the successor's expiry is accepted");
    let accepted = (
        journey.snapshots(successor, &cold).await,
        journey.maintenance_rows(&cold).await,
    );
    lapse_leader_term(&journey).await;
    journey.pass(old_leader).await;
    let third = journey.held(old_leader).expect("a third term is acquired");
    assert!(third.fencing_token() > new_term.fencing_token());
    await_revoked(&new_term).await;
    // The accepted expiry is one table-fenced effect that completes through
    // loss of the leader term; nothing after it may start.
    controls.release_expiry_accepted();
    tokio::time::timeout(
        PASS_BOUND,
        server.wait_for_forge_scheduler_passes_for_test(passes + 1),
    )
    .await
    .expect("the revoked maintenance pass returns");
    assert_eq!(
        (
            journey.snapshots(successor, &cold).await,
            journey.maintenance_rows(&cold).await,
        ),
        accepted,
        "a revoked pass makes no later expiry or cleanup effect"
    );
    assert!(
        journey.exists(&orphan).await,
        "a revoked pass sweeps nothing"
    );
    assert!(journey.held(successor).is_none());

    // The third term rejoins the table on its next commit and settles the
    // revoked pass's accepted expiry before its own cleanup.
    journey.write_hot(old_leader, &cold, &[5]).await;
    tokio::time::timeout(PASS_BOUND, async {
        while journey.unpromoted(&cold).await > 0 {
            let next = journey.observer.attempts() + 1;
            journey.observer.wait_for_attempts_at_least(next).await;
        }
    })
    .await
    .expect("the rejoining commit is promoted");
    journey.advance(chrono::Duration::minutes(1));
    journey.maintain(old_leader).await;
    assert!(!journey.exists(&orphan).await, "the next leader sweeps it");
    let unsettled: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.forge_tasks WHERE data_tenant_id = $1 \
         AND table_name = $2 AND state NOT IN ('succeeded', 'failed', 'cancelled')",
    )
    .bind(journey.tenant.as_uuid())
    .bind(&cold.name)
    .fetch_one(journey.cluster.pg_fixture().operator_pool().pool())
    .await
    .expect("forge_tasks inspection");
    assert_eq!(unsettled, 0, "every maintenance attempt settled");
    journey.cluster.shutdown().await.expect("cluster drains");
}

/// A restarted leader recovers lost-hint promotion debt with an empty schedule.
///
/// The second node runs Scribe without a coordinator, so its hot objects have
/// no hint consumer: only the leader's `file_list` sweep can promote them.
///
/// # Panics
/// Panics if hot objects stay unpromoted after the restart, or the restarted
/// leader carries any track or membership from its previous term.
#[tokio::test]
#[ignore = "requires Postgres and two replicas"]
async fn restart_recovers_hot_promotion_with_empty_schedule() {
    let mut spec = BifrostClusterSpec::two_mixed();
    spec.nodes[1].roles = [BifrostRuntimeRole::Scribe].into_iter().collect();
    let (leader, scribe) = (spec.nodes[0].node_id, spec.nodes[1].node_id);
    let mut journey = LeaderJourney::start(spec, false).await;
    journey.pass(leader).await;
    let first_token = journey
        .held(leader)
        .expect("the only coordinator leads")
        .fencing_token();

    let before = journey
        .register_scheduled_table(leader, "restart_before")
        .await;
    journey.write_hot(leader, &before, &[1, 2]).await;
    journey.await_promoted(&before).await;
    assert!(
        journey
            .held(leader)
            .expect("term")
            .schedule()
            .track_for_test(&journey.key(&before))
            .is_some(),
        "the first term tracks its table"
    );

    journey
        .cluster
        .stop_node(leader)
        .await
        .expect("leader stops");
    let lost = journey
        .register_scheduled_table(scribe, "restart_lost")
        .await;
    journey.write_hot(scribe, &lost, &[7, 8, 9]).await;
    assert!(
        journey.unpromoted(&lost).await > 0,
        "no coordinator promoted the Scribe-only objects"
    );

    journey.cluster.restart_node(leader).await.expect("restart");
    journey.pass(leader).await;
    assert_eq!(
        journey.unpromoted(&lost).await,
        0,
        "the sweep promoted the debt"
    );
    let term = journey
        .held(leader)
        .expect("the restarted coordinator leads");
    assert!(term.fencing_token() > first_token, "a new term is minted");
    assert!(
        term.schedule()
            .track_for_test(&journey.key(&before))
            .is_none(),
        "no track survives the restart"
    );
    assert_eq!(
        term.schedule()
            .track_for_test(&journey.key(&lost))
            .map(|track| track.pending_commits),
        Some(1),
        "the recovered promotion is the only counted commit"
    );
    // The stopping leader's drain sealed the tenant's audit_log rows, so the
    // new term's sweep may promote that debt too; it uses no rewrite setting.
    let audit = usize::from(
        term.schedule()
            .track_for_test(&journey.key_in(BifrostNamespace::Audit, "audit_log"))
            .is_some(),
    );
    assert_eq!(
        term.schedule().sizes_for_test(),
        (1 + audit, 1 + audit, 1),
        "membership holds only the recovered table and the audit debt it swept"
    );
    journey.cluster.shutdown().await.expect("cluster drains");
}

/// A new leader maintains nothing until a commit, then cleans only safe orphans.
///
/// Mirrors `RisingWave`'s Iceberg GC loop at e23ddf95: the elected node's
/// maintenance sets are volatile, so after failover a cold table is not
/// expired, rewritten or swept until its next commit notice makes it a member
/// again. Wyrd's never-published orphan sweep has no `RisingWave` equivalent;
/// it reuses the protected deletion boundary, so an unresolved expiry
/// operation on the table protects even an aged rowless output.
///
/// # Panics
/// Panics if the successor or the standby maintains a cold table, the rejoined
/// table is not expired and swept, the unresolved operation leaves the orphan
/// collectable, or a repeated sweep over the deleted object fails.
#[tokio::test]
#[ignore = "requires Postgres and two replicas"]
async fn empty_maintenance_restart_protects_orphans() {
    let spec = BifrostClusterSpec::two_mixed();
    let (first, second) = (spec.nodes[0].node_id, spec.nodes[1].node_id);
    let mut journey = LeaderJourney::start(spec, true).await;
    let catalog = journey
        .cluster
        .commit_uncertainty_catalog()
        .expect("the topology wraps the real Forge catalog");
    journey.pass(first).await;
    journey.pass(second).await;
    let (leader, _) = journey.leaders()[0];
    let successor = if leader == first { second } else { first };

    // Snapshot expiration is on by default; compaction is opted out, because
    // the leader skips the orphan sweep for a table that owes a rewrite. The
    // table's only leader work is maintenance and it never owes a rewrite.
    let table = register_table(
        journey.node(leader),
        journey.tenant,
        &unique_table("cold_restart"),
    )
    .await;
    set_table_properties(
        journey.node(leader),
        &table.binding,
        &[("wyrd.forge.enable-compaction", "false")],
    )
    .await;
    for values in [&[1, 2][..], &[3, 4]] {
        journey.write_hot(leader, &table, values).await;
        journey.await_promoted(&table).await;
    }
    let key = journey.key(&table);
    assert!(
        journey
            .held(leader)
            .expect("leader term")
            .schedule()
            .maintenance_tables()
            .1
            .contains(&key),
        "the first term counts the table for snapshot expiration"
    );
    // A rowless output in the writer's canonical grammar: what a rewrite that
    // died before preparing leaves behind, and what only the sweep can reach.
    let orphan = format!(
        "{}/data/forge/v2/{}-00000-{}.parquet",
        table.binding.object_prefix.trim_end_matches('/'),
        Uuid::now_v7(),
        Uuid::now_v7()
    );
    journey
        .cluster
        .storage_operator()
        .write(&orphan, b"never published".to_vec())
        .await
        .expect("rowless output");

    journey
        .cluster
        .stop_node(leader)
        .await
        .expect("leader stops");
    journey.pass(successor).await;
    journey.cluster.restart_node(leader).await.expect("restart");
    journey.pass(leader).await;
    assert_eq!(journey.leaders().len(), 1);
    let term = journey.held(successor).expect("the successor leads");
    // The successor may legitimately schedule the tenant's audit table, whose
    // retained reads keep promoting; this table must start with no membership.
    let schedule = term.schedule();
    let (manifest_rewrite, snapshot_expiration) = schedule.maintenance_tables();
    assert!(
        schedule.track_for_test(&key).is_none()
            && !manifest_rewrite.contains(&key)
            && !snapshot_expiration.contains(&key),
        "the successor starts with empty maintenance membership for this table"
    );

    // Past retention and the orphan floor, both replicas tick and nothing runs.
    journey.advance(chrono::Duration::days(2));
    let retained = journey.snapshots(successor, &table).await;
    let rows = journey.maintenance_rows(&table).await;
    assert!(retained.len() > 1, "expiry is due: {retained:?}");
    assert_eq!(
        journey.eligibility(successor, &table, &orphan).await,
        "Eligible",
        "the orphan is collectable, so only membership withholds the sweep"
    );
    journey.maintain(successor).await;
    journey.maintain(leader).await;
    assert_eq!(journey.snapshots(successor, &table).await, retained);
    assert_eq!(journey.maintenance_rows(&table).await, rows);
    assert!(journey.exists(&orphan).await, "a cold table is not swept");

    // A new commit rejoins the table; the standby still maintains nothing.
    journey.write_hot(successor, &table, &[5]).await;
    journey.await_promoted(&table).await;
    let rejoined = journey.snapshots(successor, &table).await;
    let rows = journey.maintenance_rows(&table).await;
    journey.maintain(leader).await;
    assert_eq!(journey.held(leader).map(|term| term.fencing_token()), None);
    assert_eq!(journey.snapshots(successor, &table).await, rejoined);
    assert_eq!(journey.maintenance_rows(&table).await, rows);
    assert!(journey.exists(&orphan).await, "a standby sweeps nothing");

    // The leader's expiry is held after the catalog accepted it: that
    // unresolved operation protects the aged orphan until it settles.
    catalog.pause_after_snapshot_removal();
    let drive = journey.maintain(successor);
    let inspect = async {
        tokio::time::timeout(PASS_BOUND, catalog.wait_for_commit())
            .await
            .expect("the rejoined table's expiry reaches the catalog");
        assert_eq!(
            journey.eligibility(successor, &table, &orphan).await,
            "Protected",
            "an unresolved expiry protects every object of its table"
        );
        assert!(journey.exists(&orphan).await);
        catalog.release_paused_commit();
    };
    tokio::join!(drive, inspect);
    let expired = journey.snapshots(successor, &table).await;
    assert_eq!(expired.len(), 1, "retain-last keeps only the head");
    assert!(expired.is_subset(&rejoined));
    assert!(
        !journey.exists(&orphan).await,
        "the settled pass swept the rowless output"
    );

    // A later sweep, at a new cut, settles idempotently without the object.
    let swept = journey.maintenance_rows(&table).await;
    journey.advance(chrono::Duration::minutes(1));
    journey.maintain(successor).await;
    assert!(!journey.exists(&orphan).await);
    assert!(
        journey.maintenance_rows(&table).await > swept,
        "the repeated pass ran its own recorded sweep"
    );
    let unsettled: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.forge_tasks WHERE data_tenant_id = $1 \
         AND table_name = $2 AND state NOT IN ('succeeded', 'failed', 'cancelled')",
    )
    .bind(journey.tenant.as_uuid())
    .bind(&table.name)
    .fetch_one(journey.cluster.pg_fixture().operator_pool().pool())
    .await
    .expect("forge_tasks inspection");
    assert_eq!(unsettled, 0, "every maintenance attempt settled");
    let failures = journey.failures();
    assert!(failures.is_empty(), "{failures:?}");
    journey.cluster.shutdown().await.expect("cluster drains");
}

/// Compactors pull the oldest due tables on either route and both replicas work.
///
/// Mirrors RisingWave's compactor pull (`compactor/mod.rs:1606-1640`) and
/// oldest-due dispatch (`schedule.rs:428-490,915-993`) at e23ddf95: a pull
/// names only table identities, a failed delivery returns the table to Idle
/// with its commits, and success consumes only the commits counted at
/// dispatch. The first cluster runs coordinators without workers so every
/// pull is the test's own; the second lets both replicas' workers pull.
///
/// # Panics
/// Panics when a pull returns tables out of due order or beyond its limit,
/// a route disagrees with the other, a report loses a commit, or the workers
/// of only one replica execute the dispatched backlog.
#[tokio::test]
#[ignore = "requires Postgres and two replicas"]
async fn compactors_pull_oldest_due_with_capacity() {
    let due_now = [("wyrd.forge.compaction.trigger-snapshot-count", "1")];
    let mut spec = BifrostClusterSpec::two_mixed();
    for node in &mut spec.nodes {
        node.roles.remove(&BifrostRuntimeRole::ForgeWorker);
    }
    let (first, second) = (spec.nodes[0].node_id, spec.nodes[1].node_id);
    let journey = LeaderJourney::start(spec, false).await;
    journey.pass(first).await;
    journey.pass(second).await;
    let (leader, _) = journey.leaders()[0];
    let standby = if leader == first { second } else { first };
    let forge = |node: NodeId| {
        Arc::clone(
            journey
                .node(node)
                .state()
                .forge_coordinator()
                .expect("coordinator"),
        )
    };
    let mut keys = Vec::new();
    let mut tables = Vec::new();
    for index in 0..5_i64 {
        let table = journey
            .register_table_with(leader, "pull_due", &due_now)
            .await;
        journey.write_hot(leader, &table, &[index]).await;
        journey.await_promoted(&table).await;
        keys.push(journey.key(&table));
        tables.push(table);
    }
    let pulled_keys = |dispatches: &[ForgeCompactionDispatch]| {
        dispatches
            .iter()
            .map(|dispatch| dispatch.key.clone())
            .collect::<Vec<_>>()
    };

    // The peer route and the in-process route share one oldest-due order.
    let peer = forge(standby).pull_compaction(2).await.expect("peer pull");
    assert_eq!(pulled_keys(&peer), keys[..2], "the two oldest due tables");
    assert!(
        peer.iter()
            .all(|dispatch| dispatch.compaction_type == ForgeCompactionType::SmallFiles),
        "a dispatch names the table and its task type, never files: {peer:?}"
    );
    let local = forge(leader).pull_compaction(2).await.expect("local pull");
    assert_eq!(pulled_keys(&local), keys[2..4], "the next two due tables");

    // A failed delivery returns the table to Idle, due now, with its commit.
    forge(standby)
        .report_compaction(&peer[0], ForgeCompactionOutcome::NotStarted)
        .await
        .expect("peer report");
    let term = journey.held(leader).expect("leader term");
    let reverted = term.schedule().track_for_test(&keys[0]).expect("track");
    assert_eq!((reverted.pending_commits, reverted.in_flight), (1, None));
    let again = forge(leader).pull_compaction(4).await.expect("local pull");
    assert_eq!(
        pulled_keys(&again),
        vec![keys[0].clone(), keys[4].clone()],
        "only due Idle tables are offered, oldest due first, within the limit"
    );

    // A commit during execution survives success; a stale report is ignored.
    journey.write_hot(leader, &tables[1], &[10]).await;
    journey.await_promoted(&tables[1]).await;
    forge(standby)
        .report_compaction(&peer[1], ForgeCompactionOutcome::Succeeded)
        .await
        .expect("peer report");
    let finished = term.schedule().track_for_test(&keys[1]).expect("track");
    assert_eq!(
        (finished.pending_commits, finished.in_flight),
        (1, None),
        "success consumes only the commit counted at dispatch"
    );
    forge(leader)
        .report_compaction(&peer[0], ForgeCompactionOutcome::Failed)
        .await
        .expect("local report");
    assert_eq!(
        term.schedule()
            .track_for_test(&keys[0])
            .expect("track")
            .in_flight,
        Some(again[0].task_id),
        "a report for an earlier task changes nothing"
    );
    journey.cluster.shutdown().await.expect("cluster drains");

    // Both replicas' workers pull from the one leader and execute the backlog.
    let journey = LeaderJourney::start(BifrostClusterSpec::two_mixed(), false).await;
    let nodes = journey
        .cluster
        .servers()
        .map(wyrd_testing::WyrdTestServer::node_id)
        .collect::<Vec<_>>();
    for node in &nodes {
        journey.pass(*node).await;
    }
    let (leader, _) = journey.leaders()[0];
    journey.observer.hold_after_claims_for_test(2);
    let mut tables = Vec::new();
    for index in 0..9_i64 {
        let table = journey
            .register_table_with(leader, "pull_work", &due_now)
            .await;
        journey.write_hot(leader, &table, &[index]).await;
        tables.push(table);
    }
    tokio::time::timeout(PASS_BOUND, journey.observer.wait_for_claims_for_test())
        .await
        .expect("each replica's worker claims pulled work");
    journey.observer.release_claims_for_test();
    let names = tables
        .iter()
        .map(|table| table.name.clone())
        .collect::<Vec<_>>();
    let term = journey.held(leader).expect("leader term");
    tokio::time::timeout(REWRITE_BOUND, async {
        loop {
            let next = journey.observer.attempts() + 1;
            let idle = tables.iter().all(|table| {
                term.schedule()
                    .track_for_test(&journey.key(table))
                    .is_some_and(|track| track.pending_commits == 0 && track.in_flight.is_none())
            });
            if idle {
                break;
            }
            journey.observer.wait_for_attempts_at_least(next).await;
        }
    })
    .await
    .expect("the pulled backlog drains");
    let dispatched: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT task_id, state FROM vala.forge_tasks WHERE data_tenant_id = $1 \
         AND table_name = ANY($2) AND plan->'parameters' ? 'compaction_type'",
    )
    .bind(journey.tenant.as_uuid())
    .bind(&names)
    .fetch_all(journey.cluster.pg_fixture().operator_pool().pool())
    .await
    .expect("dispatched task inspection");
    assert_eq!(
        dispatched.len(),
        tables.len(),
        "one dispatched attempt per table"
    );
    assert!(
        dispatched.iter().all(|(_, state)| state == "succeeded"),
        "{dispatched:?}"
    );
    let dispatched = dispatched
        .into_iter()
        .map(|(task_id, _)| task_id)
        .collect::<BTreeSet<_>>();
    let workers = journey
        .observer
        .lifecycle_events()
        .into_iter()
        .filter_map(|event| match event {
            ForgeLifecycleEvent::Claimed { task_id, worker_id }
                if dispatched.contains(&task_id) =>
            {
                Some(worker_id)
            }
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        workers.len(),
        2,
        "both replicas' workers executed pulled work"
    );
    let failures = journey.failures();
    assert!(failures.is_empty(), "{failures:?}");
    journey.cluster.shutdown().await.expect("cluster drains");
}
